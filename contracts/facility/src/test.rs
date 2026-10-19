use super::*;
use common::interfaces::VaultHost;
use common::types::{Schedule, ScheduleMode, VaultConfig};
use mock_oracle::{MockOracle, MockOracleClient};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{BytesN, String, Symbol};
use vault::Vault;

#[contract]
pub struct MockHost;

#[contractimpl]
impl MockHost {
    pub fn __constructor(env: Env, admin: Address) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "admin"), &admin);
    }
}

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
        _e: Env,
        _v: Address,
        _w: BytesN<32>,
        _a: Address,
        venue: Address,
    ) -> Address {
        venue
    }
}

const BIG: i128 = 1_000_000_000_000;
const DAY: u64 = 86_400;
const TEN_BPS: u32 = 1_000;

struct F {
    env: Env,
    fac: FacilityClient<'static>,
    host: Address,
    usd: Address,
    curator: Address,
    manager: Address,
    lp: Address,
    seller: Address,
    oracle: MockOracleClient<'static>,
}

fn sac(env: &Env) -> Address {
    env.register_stellar_asset_contract_v2(Address::generate(env))
        .address()
}

fn vault(env: &Env, host: &Address, asset: &Address, owner: &Address) -> Address {
    let config = VaultConfig {
        asset: asset.clone(),
        curator: owner.clone(),
        manager: owner.clone(),
        guardian: owner.clone(),
        name: String::from_str(env, "v"),
        symbol: String::from_str(env, "v"),
        deposit_cap: 0,
        curator_fee_bps: 0,
        timelock: 0,
    };
    env.register(Vault, (host.clone(), config))
}

fn setup() -> F {
    let env = Env::default();
    env.mock_all_auths();
    env.cost_estimate().budget().reset_unlimited();
    env.ledger().set_timestamp(1_000_000);
    let [curator, manager, lp, seller] = [0; 4].map(|_| Address::generate(&env));
    let host = env.register(MockHost, (Address::generate(&env),));
    let usd = sac(&env);
    let base_vault = vault(&env, &host, &usd, &curator);
    let config = FacilityConfig {
        base_vault,
        curator: curator.clone(),
        manager: manager.clone(),
        guardian: curator.clone(),
        name: String::from_str(&env, "Octarine LP"),
        symbol: String::from_str(&env, "oLP"),
        curator_fee_bps: 0,
        vest_seconds: 30 * DAY,
        timelock: DAY,
    };
    let fac = FacilityClient::new(&env, &env.register(Facility, (host.clone(), config)));
    let oracle = MockOracleClient::new(&env, &env.register(MockOracle, (curator.clone(), ONE)));
    StellarAssetClient::new(&env, &usd).mint(&lp, &BIG);
    fac.deposit(&1_000_000, &lp, &lp, &lp);
    F {
        env,
        fac,
        host,
        usd,
        curator,
        manager,
        lp,
        seller,
        oracle,
    }
}

impl F {
    fn wait(&self, secs: u64) {
        let now = self.env.ledger().timestamp();
        self.env.ledger().set_timestamp(now + secs);
    }
    fn terms(&self, max_bps: u32) -> Terms {
        Terms {
            accepted: true,
            bid_rate: TEN_BPS,
            ask_rate: TEN_BPS / 2,
            max_bps,
            oracle: self.oracle.address.clone(),
            max_price_age: 365 * DAY,
            schedule: Schedule {
                mode: ScheduleMode::Rolling,
                rolling_seconds: (30 * DAY) as u32,
                next_redemption_at: 0,
                cycle_seconds: 0,
                max_bps_per_day: 2 * TEN_BPS,
            },
            redemption: Address::generate(&self.env),
        }
    }
    /// Lists a new RWA (its own vault) through the timelock.
    fn listed(&self, max_bps: u32) -> (Address, Address) {
        let rwa = sac(&self.env);
        let v = vault(&self.env, &self.host, &rwa, &self.curator);
        let (kind, _) = self
            .fac
            .submit(&Change::Rwa(rwa.clone(), v.clone(), self.terms(max_bps)));
        self.wait(DAY);
        self.fac.accept(&kind);
        StellarAssetClient::new(&self.env, &rwa).mint(&self.seller, &BIG);
        (rwa, v)
    }
    fn bal(&self, t: &Address, who: &Address) -> i128 {
        TokenClient::new(&self.env, t).balance(who)
    }
}

#[test]
fn lps_enter_and_exit_through_the_base_vault() {
    let f = setup();
    assert_eq!(f.fac.decimals(), 7 + 6);
    assert_eq!(f.fac.total_assets(), 1_000_000);
    let shares = f.fac.balance(&f.lp);
    assert_eq!(f.fac.redeem(&shares, &f.lp, &f.lp, &f.lp), 1_000_000);
    assert_eq!(f.bal(&f.usd, &f.lp), BIG);
}

#[test]
fn a_sale_lands_the_rwa_in_its_vault_pays_from_the_base_vault_and_vests_the_discount() {
    let f = setup();
    let (rwa, rwa_vault) = f.listed(10_000);
    // 10 bps/day over 30 days: 3%.
    assert_eq!(f.fac.quote(&rwa, &f.usd, &100_000), 97_000);
    let out = f
        .fac
        .trade(&f.seller, &f.seller, &rwa, &f.usd, &100_000, &97_000);
    assert_eq!(out, 97_000);
    assert_eq!(f.bal(&f.usd, &f.seller), 97_000);
    assert_eq!(
        f.bal(&rwa, &rwa_vault),
        100_000,
        "the RWA sits in its own vault"
    );
    assert_eq!(
        (f.bal(&rwa, &f.fac.address), f.bal(&f.usd, &f.fac.address)),
        (0, 0)
    );

    // The 3,000 discount is locked, so NAV does not jump at the fill...
    assert_eq!(f.fac.locked(), 3_000);
    assert_eq!(f.fac.total_assets(), 1_000_000);
    // ...and reaches LPs linearly.
    f.wait(15 * DAY);
    assert_eq!(f.fac.total_assets(), 1_001_500);
    f.wait(15 * DAY);
    assert_eq!(f.fac.total_assets(), 1_003_000);
}

#[test]
fn buying_rwa_from_the_facility_prices_at_the_ask_and_nets_against_locked_gain() {
    let f = setup();
    let (rwa, _) = f.listed(10_000);
    f.fac
        .trade(&f.seller, &f.seller, &rwa, &f.usd, &100_000, &1);
    let buyer = Address::generate(&f.env);
    StellarAssetClient::new(&f.env, &f.usd).mint(&buyer, &98_500);
    // 5 bps/day over 30 days: 98,500 buys 100,000 units.
    assert_eq!(f.fac.quote(&f.usd, &rwa, &98_500), 100_000);
    f.fac.trade(&buyer, &buyer, &f.usd, &rwa, &98_500, &100_000);
    assert_eq!(f.bal(&rwa, &buyer), 100_000);
    // Bought at 97,000, sold at 98,500: the 1,500 spread stays locked.
    assert_eq!(f.fac.locked(), 1_500);
    assert_eq!(f.fac.total_assets(), 1_000_000);
    // Nothing left to sell.
    assert_eq!(f.fac.quote(&f.usd, &rwa, &1_000), 0);
}

#[test]
fn rwa_swaps_through_the_base_value() {
    let f = setup();
    let (a, _) = f.listed(10_000);
    let (b, _) = f.listed(10_000);
    let holder = Address::generate(&f.env);
    StellarAssetClient::new(&f.env, &b).mint(&holder, &200_000);
    f.fac.trade(&holder, &holder, &b, &f.usd, &200_000, &1);
    // A at the bid (97,000), into B at the ask.
    let before = f.bal(&b, &f.seller);
    let out = f.fac.trade(&f.seller, &f.seller, &a, &b, &100_000, &1);
    assert_eq!(out, 98_477);
    assert_eq!(f.bal(&b, &f.seller) - before, 98_477);
}

#[test]
fn no_fill_without_a_fresh_price_acceptance_or_room_under_the_cap() {
    let f = setup();
    let (rwa, _) = f.listed(500);
    // 5% of NAV is 50,000 of RWA value.
    assert!(f
        .fac
        .try_trade(&f.seller, &f.seller, &rwa, &f.usd, &60_000, &1)
        .is_err());
    f.fac.trade(&f.seller, &f.seller, &rwa, &f.usd, &40_000, &1);

    f.oracle.set_price(&0, &None);
    assert_eq!(f.fac.quote(&rwa, &f.usd, &1_000), 0);
    assert!(
        f.fac.try_total_assets().is_err(),
        "NAV stops pricing while it holds the RWA"
    );
    f.oracle.set_price(&ONE, &None);

    let mut t = f.terms(500);
    t.accepted = false;
    let (kind, _) = f
        .fac
        .submit(&Change::Rwa(rwa.clone(), f.fac.rwa(&rwa).unwrap().vault, t));
    f.wait(DAY);
    f.fac.accept(&kind);
    assert_eq!(f.fac.quote(&rwa, &f.usd, &1_000), 0);
    assert_eq!(
        f.fac
            .try_trade(&f.seller, &f.seller, &rwa, &f.usd, &1_000, &1),
        Err(Ok(Error::NotAccepted.into()))
    );
}

#[test]
fn the_queue_is_owed_first_and_only_it_can_take_that_liquidity() {
    let f = setup();
    let queue = Address::generate(&f.env);
    let (kind, _) = f.fac.submit(&Change::Queue(queue.clone()));
    f.wait(DAY);
    f.fac.accept(&kind);
    let half = f.fac.balance(&f.lp) / 2;
    f.fac.transfer(&f.lp, &queue, &half);
    assert_eq!(f.fac.max_withdraw(&f.lp), 500_000);
    assert!(f.fac.try_withdraw(&500_001, &f.lp, &f.lp, &f.lp).is_err());
    assert_eq!(f.fac.max_withdraw(&queue), 500_000);
    f.fac.redeem(&half, &queue, &queue, &queue);
}

#[test]
fn a_redemption_is_carried_at_the_mark_and_settles_in_full_or_not_at_all() {
    let f = setup();
    let (rwa, _) = f.listed(10_000);
    f.fac
        .trade(&f.seller, &f.seller, &rwa, &f.usd, &100_000, &1);
    let nav = f.fac.total_assets();
    f.fac.book(&rwa, &100_000);
    let to = f.fac.rwa(&rwa).unwrap().terms.redemption;
    assert_eq!(f.bal(&rwa, &to), 100_000);
    assert_eq!(f.fac.total_assets(), nav);
    assert!(f.fac.try_book(&rwa, &1).is_err(), "one open per RWA");

    StellarAssetClient::new(&f.env, &f.usd).mint(&f.manager, &200_000);
    assert_eq!(
        f.fac.try_settle(&rwa, &99_999),
        Err(Ok(Error::NotFullyRepaid.into()))
    );
    f.fac.settle(&rwa, &100_500);
    assert!(f.fac.redemption(&rwa).is_none());
    f.wait(30 * DAY);
    assert_eq!(f.fac.total_assets(), 1_003_500);
}

#[test]
fn listings_wait_out_the_timelock_and_the_payer_must_sign() {
    let f = setup();
    let rwa = sac(&f.env);
    let v = vault(&f.env, &f.host, &rwa, &f.curator);
    let (kind, _) = f
        .fac
        .submit(&Change::Rwa(rwa.clone(), v.clone(), f.terms(10_000)));
    assert!(f.fac.try_accept(&kind).is_err());
    f.fac.revoke(&kind, &f.curator);
    assert!(f.fac.pending(&kind).is_none());
    f.wait(DAY);
    assert!(f.fac.try_accept(&kind).is_err());
    let wrong = vault(&f.env, &f.host, &f.usd, &f.curator);
    assert!(f
        .fac
        .try_submit(&Change::Rwa(rwa.clone(), wrong, f.terms(10_000)))
        .is_err());

    let (rwa, _) = f.listed(10_000);
    f.env.set_auths(&[]);
    assert!(f
        .fac
        .try_trade(&f.seller, &f.seller, &rwa, &f.usd, &1_000, &1)
        .is_err());
}

#[test]
fn a_write_down_takes_the_loss_and_a_delisting_needs_an_empty_vault() {
    let f = setup();
    let (rwa, rwa_vault) = f.listed(10_000);
    f.fac
        .trade(&f.seller, &f.seller, &rwa, &f.usd, &100_000, &1);
    assert_eq!(f.fac.try_remove_rwa(&rwa), Err(Ok(Error::StillHeld.into())));
    f.fac.book(&rwa, &100_000);
    assert_eq!(
        f.fac.try_remove_rwa(&rwa),
        Err(Ok(Error::StillHeld.into())),
        "a redemption is open"
    );

    let admin = common::interfaces::VaultHostClient::new(&f.env, &f.host).admin();
    StellarAssetClient::new(&f.env, &f.usd).mint(&admin, &40_000);
    assert!(f.fac.try_write_down(&rwa, &100_000).is_err(), "not a loss");
    f.fac.write_down(&rwa, &40_000);
    // 60,000 short of the 100,000 mark: the 3,000 locked gain absorbs part of it.
    assert_eq!(f.fac.locked(), 0);
    assert_eq!(f.fac.total_assets(), 1_000_000 - 57_000);
    assert_eq!(
        TokenClient::new(&f.env, &rwa_vault).balance(&f.fac.address),
        0
    );
    f.fac.remove_rwa(&rwa);
    assert!(f.fac.rwa(&rwa).is_none());
    assert!(f.fac.rwas().is_empty());
}

#[test]
fn an_allowlisted_facility_admits_only_members() {
    let f = setup();
    f.fac.set_allowlist(&true);
    let outsider = Address::generate(&f.env);
    StellarAssetClient::new(&f.env, &f.usd).mint(&outsider, &1_000);
    assert_eq!(
        f.fac.try_deposit(&1_000, &outsider, &outsider, &outsider),
        Err(Ok(Error::NotAllowlisted.into()))
    );
    f.fac.set_member(&outsider, &true);
    f.fac.deposit(&1_000, &outsider, &outsider, &outsider);
    assert_eq!(f.fac.convert_to_assets(&f.fac.balance(&outsider)), 1_000);
}

#[test]
fn a_pause_stops_trading_but_not_exits() {
    let f = setup();
    let (rwa, _) = f.listed(10_000);
    assert!(
        f.fac.try_set_paused(&f.lp, &true).is_err(),
        "curator or guardian only"
    );
    f.fac.set_paused(&f.curator, &true);
    assert!(f.fac.paused());
    assert_eq!(f.fac.quote(&rwa, &f.usd, &1_000), 0);
    assert_eq!(
        f.fac
            .try_trade(&f.seller, &f.seller, &rwa, &f.usd, &1_000, &1),
        Err(Ok(Error::Paused.into()))
    );
    let shares = f.fac.balance(&f.lp);
    f.fac.redeem(&shares, &f.lp, &f.lp, &f.lp);
    f.fac.set_paused(&f.curator, &false);
    assert_eq!(f.fac.quote(&rwa, &f.usd, &1_000), 970, "trading resumes");
}
