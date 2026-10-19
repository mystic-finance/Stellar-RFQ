use super::*;
use common::interfaces::VaultHost;
use mock_venue::{MockVenue, MockVenueClient};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{String, Symbol};

/// Stands in for the facility aggregator: protocol admin, pinned vault code,
/// and adapter deployment (it hands back the adapter the test registered).
#[contract]
pub struct MockHost;

#[contractimpl]
impl VaultHost for MockHost {
    fn admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "admin"))
            .unwrap()
    }
    fn vault_wasm(env: Env) -> BytesN<32> {
        BytesN::from_array(&env, &[9u8; 32])
    }
    fn deploy_adapter(
        _env: Env,
        _vault: Address,
        _wasm: BytesN<32>,
        _asset: Address,
        venue: Address,
    ) -> Address {
        venue
    }
}

#[contractimpl]
impl MockHost {
    pub fn __constructor(env: Env, admin: Address) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "admin"), &admin);
    }
}

const BIG: i128 = 1_000_000_000_000;
const DAY: u64 = 86_400;

struct F {
    env: Env,
    vault: VaultClient<'static>,
    asset: Address,
    curator: Address,
    manager: Address,
    guardian: Address,
    alice: Address,
    bob: Address,
}

fn setup(fee_bps: u32) -> F {
    let env = Env::default();
    env.mock_all_auths();
    env.cost_estimate().budget().reset_unlimited();
    env.ledger().set_timestamp(1_000_000);
    let [curator, manager, guardian, alice, bob] = [0; 5].map(|_| Address::generate(&env));
    let asset = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let host = env.register(MockHost, (Address::generate(&env),));
    let config = VaultConfig {
        asset: asset.clone(),
        curator: curator.clone(),
        manager: manager.clone(),
        guardian: guardian.clone(),
        name: String::from_str(&env, "Octarine USDC"),
        symbol: String::from_str(&env, "oUSDC"),
        deposit_cap: 0,
        curator_fee_bps: fee_bps,
        timelock: 2 * DAY,
    };
    let id = env.register(Vault, (host, config));
    for who in [&alice, &bob] {
        StellarAssetClient::new(&env, &asset).mint(who, &BIG);
    }
    F {
        vault: VaultClient::new(&env, &id),
        env,
        asset,
        curator,
        manager,
        guardian,
        alice,
        bob,
    }
}

impl F {
    fn deposit(&self, who: &Address, a: i128) -> i128 {
        self.vault.deposit(&a, who, who, who)
    }
    fn redeem_all(&self, who: &Address) -> i128 {
        self.vault.redeem(&self.vault.balance(who), who, who, who)
    }
    fn base(&self, who: &Address) -> i128 {
        TokenClient::new(&self.env, &self.asset).balance(who)
    }
    fn gift(&self, a: i128) {
        StellarAssetClient::new(&self.env, &self.asset).mint(&self.vault.address, &a);
    }
    fn wait(&self) {
        let now = self.env.ledger().timestamp();
        self.env.ledger().set_timestamp(now + 2 * DAY);
    }
    /// A venue holding `a`, owned by the vault, enabled through the timelock.
    fn venue(&self, a: i128) -> MockVenueClient<'static> {
        let id = self.env.register(
            MockVenue,
            (
                self.vault.address.clone(),
                self.asset.clone(),
                Address::generate(&self.env),
            ),
        );
        self.vault.submit(&Change::Venue(
            BytesN::from_array(&self.env, &[7u8; 32]),
            id.clone(),
        ));
        self.wait();
        self.vault.accept(&2);
        if a > 0 {
            self.vault.allocate(&id, &a);
        }
        MockVenueClient::new(&self.env, &id)
    }
}

#[test]
fn shares_carry_the_offset_and_round_trip_exactly() {
    let f = setup(0);
    assert_eq!(f.vault.decimals(), 7 + 6);
    assert_eq!(f.deposit(&f.alice, 1_000_000), 1_000_000 * 1_000_000);
    assert_eq!(f.redeem_all(&f.alice), 1_000_000);
    assert_eq!(f.base(&f.alice), BIG);
}

#[test]
fn yield_accrues_to_share_price_and_a_donation_cannot_zero_the_next_depositor() {
    let f = setup(0);
    f.deposit(&f.alice, 1);
    f.gift(100_000_000);
    let shares = f.deposit(&f.bob, 1_000_000);
    assert!(shares > 0);
    let back = f.vault.redeem(&shares, &f.bob, &f.bob, &f.bob);
    assert!(back >= 999_900, "victim lost {}", 1_000_000 - back);
}

#[test]
fn the_cap_and_the_allowlist_gate_entry_and_share_transfers() {
    let f = setup(0);
    f.vault.submit(&Change::DepositCap(1_000));
    f.wait();
    f.vault.accept(&1);
    assert_eq!(f.vault.max_deposit(&f.alice), 1_000);
    assert_eq!(
        f.vault.try_deposit(&1_001, &f.alice, &f.alice, &f.alice),
        Err(Ok(Error::DepositCapExceeded.into()))
    );

    f.vault.set_allowlist(&true);
    assert_eq!(
        f.vault.try_deposit(&10, &f.alice, &f.alice, &f.alice),
        Err(Ok(Error::NotAllowlisted.into()))
    );
    f.vault.set_member(&f.alice, &true);
    f.deposit(&f.alice, 10);
    assert!(f.vault.try_transfer(&f.alice, &f.bob, &1).is_err());
}

#[test]
fn the_fee_is_charged_only_above_the_high_water_mark() {
    let f = setup(2_000);
    f.deposit(&f.alice, 1_000_000);
    f.gift(100_000);
    f.vault.accrue();
    let fee = f.vault.balance(&f.curator);
    assert!(fee > 0);
    // 20% of the 100k gain, give or take rounding.
    assert!((19_990..=20_000).contains(&f.vault.convert_to_assets(&fee)));

    TokenClient::new(&f.env, &f.asset).burn(&f.vault.address, &200_000);
    f.gift(100_000);
    f.vault.accrue();
    assert_eq!(
        f.vault.balance(&f.curator),
        fee,
        "no fee while recovering a loss"
    );
}

#[test]
fn an_operator_redeems_only_against_an_allowance() {
    let f = setup(0);
    let shares = f.deposit(&f.alice, 1_000);
    assert!(f
        .vault
        .try_redeem(&shares, &f.bob, &f.alice, &f.bob)
        .is_err());
    f.vault.approve(&f.alice, &f.bob, &shares, &1_000);
    f.vault.redeem(&shares, &f.bob, &f.alice, &f.bob);
    assert_eq!(f.base(&f.bob), BIG + 1_000);
}

#[test]
fn exits_draw_on_venues_in_order_and_a_short_venue_reverts() {
    let f = setup(0);
    f.deposit(&f.alice, 1_000_000);
    let a = f.venue(600_000);
    let b = f.venue(300_000);
    assert_eq!(f.vault.total_assets(), 1_000_000);

    a.set_liquidity(&Some(100_000));
    assert_eq!(f.vault.max_withdraw(&f.alice), 500_000);
    assert_eq!(
        f.vault.try_withdraw(&500_001, &f.alice, &f.alice, &f.alice),
        Err(Ok(Error::InsufficientLiquidity.into()))
    );
    f.vault.withdraw(&500_000, &f.alice, &f.alice, &f.alice);
    assert_eq!((a.total_assets(), b.total_assets()), (500_000, 0));

    f.vault.set_venue_order(&soroban_sdk::vec![
        &f.env,
        b.address.clone(),
        a.address.clone()
    ]);
    assert!(f
        .vault
        .try_set_venue_order(&soroban_sdk::vec![&f.env, a.address.clone()])
        .is_err());
    assert_eq!(
        f.vault.try_disable_venue(&a.address),
        Err(Ok(Error::VenueNotEmpty.into()))
    );
    f.vault.disable_venue(&b.address);
}

#[test]
fn governed_changes_wait_out_the_timelock_and_the_guardian_can_revoke() {
    let f = setup(0);
    let at = f.vault.submit(&Change::CuratorFee(1_000));
    assert!(matches!(f.vault.pending(&0), Some((Change::CuratorFee(1_000), t)) if t == at));
    assert!(f.vault.try_accept(&0).is_err(), "not ready");
    f.vault.revoke(&0, &f.guardian);
    f.wait();
    assert!(f.vault.try_accept(&0).is_err(), "revoked");

    f.vault.submit(&Change::CuratorFee(1_000));
    assert!(f.vault.try_revoke(&0, &f.alice).is_err());
    f.wait();
    f.vault.accept(&0);
    assert_eq!(f.vault.config().curator_fee_bps, 1_000);
    assert!(f.vault.try_submit(&Change::CuratorFee(5_001)).is_err());

    f.env.set_auths(&[]);
    assert!(
        f.vault.try_submit(&Change::DepositCap(1)).is_err(),
        "curator only"
    );
}

#[test]
fn a_frozen_holder_cannot_move_or_redeem_and_can_be_seized() {
    let f = setup(0);
    let shares = f.deposit(&f.alice, 1_000);
    f.vault.freeze(&f.alice, &true);
    assert!(f.vault.try_transfer(&f.alice, &f.bob, &1).is_err());
    assert!(f
        .vault
        .try_redeem(&shares, &f.alice, &f.alice, &f.alice)
        .is_err());
    f.vault.seize(&f.alice, &f.manager, &shares);
    assert_eq!(f.vault.balance(&f.manager), shares);
    assert_eq!(f.vault.balance(&f.alice), 0);
}

#[test]
fn only_the_manager_allocates_and_upgrades_need_the_registered_code() {
    let f = setup(0);
    f.deposit(&f.alice, 1_000);
    let v = f.venue(0);
    assert_eq!(
        f.vault.try_allocate(&v.address, &1_001),
        Err(Ok(Error::InvalidAmount.into()))
    );
    assert!(f
        .vault
        .try_upgrade(&BytesN::from_array(&f.env, &[1u8; 32]))
        .is_err());
    f.env.set_auths(&[]);
    assert!(f.vault.try_allocate(&v.address, &1).is_err());
}

#[test]
fn previews_match_what_each_entry_point_does() {
    let f = setup(0);
    f.deposit(&f.alice, 1_000_000);
    f.gift(10_000);
    let v = &f.vault;
    assert_eq!(v.max_deposit(&f.bob), i128::MAX);
    assert_eq!(v.max_mint(&f.bob), i128::MAX);
    let shares = v.preview_deposit(&5_000);
    assert_eq!(shares, v.convert_to_shares(&5_000));
    let cost = v.preview_mint(&shares);
    assert_eq!(v.mint(&shares, &f.bob, &f.bob, &f.bob), cost);
    assert_eq!(v.balance(&f.bob), shares);
    assert!(cost >= 5_000, "minting rounds up");
    assert_eq!(v.max_redeem(&f.bob), shares);
    assert_eq!(
        v.preview_redeem(&shares),
        v.redeem(&shares, &f.bob, &f.bob, &f.bob)
    );
    let burn = v.preview_withdraw(&1_000);
    assert_eq!(v.withdraw(&1_000, &f.alice, &f.alice, &f.alice), burn);
    assert_eq!(v.total_supply(), v.balance(&f.alice));
    assert!(v.high_water_mark() > 0, "tracked even with no fee");
    v.set_manager(&f.bob);
    assert_eq!(v.config().manager, f.bob);
}
