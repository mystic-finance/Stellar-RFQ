//! Against a real facility and vaults: the facility buys RWA until its cash
//! runs short, and queued LPs are served in order as cash returns.

use super::*;
use common::interfaces::VaultHost;
use common::math::ONE;
use common::types::{Schedule, ScheduleMode, VaultConfig};
use facility::{Change as FChange, Facility, FacilityClient as Fc, FacilityConfig, Terms};
use mock_oracle::MockOracle;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{BytesN, String, Symbol};
use vault::Vault;

#[contract]
pub struct MockHost;

#[contractimpl]
impl VaultHost for MockHost {
    fn admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "a"))
            .unwrap_or(env.current_contract_address())
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

const DAY: u64 = 86_400;

struct F {
    env: Env,
    q: QueueClient<'static>,
    fac: Fc<'static>,
    usd: Address,
    alice: Address,
    bob: Address,
}

fn vault(env: &Env, host: &Address, asset: &Address, owner: &Address) -> Address {
    let c = VaultConfig {
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
    env.register(Vault, (host.clone(), c))
}

/// Alice and Bob each hold 500k of oLP; the facility then spends 873k on RWA,
/// leaving 127k liquid.
fn setup() -> F {
    let env = Env::default();
    env.mock_all_auths();
    env.cost_estimate().budget().reset_unlimited();
    env.ledger().set_timestamp(1_000_000);
    let [curator, alice, bob, seller] = [0; 4].map(|_| Address::generate(&env));
    let host = env.register(MockHost, ());
    let usd = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let rwa = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let base_vault = vault(&env, &host, &usd, &curator);
    let rwa_vault = vault(&env, &host, &rwa, &curator);
    let fac_id = env.register(
        Facility,
        (
            host.clone(),
            FacilityConfig {
                base_vault: base_vault.clone(),
                curator: curator.clone(),
                manager: curator.clone(),
                guardian: curator.clone(),
                name: String::from_str(&env, "oLP"),
                symbol: String::from_str(&env, "oLP"),
                curator_fee_bps: 0,
                vest_seconds: 0,
                timelock: 0,
            },
        ),
    );
    let fac = Fc::new(&env, &fac_id);
    let oracle = env.register(MockOracle, (curator.clone(), ONE));
    let terms = Terms {
        accepted: true,
        bid_rate: 1_000,
        ask_rate: 500,
        max_bps: 10_000,
        oracle,
        max_price_age: 365 * DAY,
        schedule: Schedule {
            mode: ScheduleMode::Rolling,
            rolling_seconds: (30 * DAY) as u32,
            next_redemption_at: 0,
            cycle_seconds: 0,
            max_bps_per_day: 2_000,
        },
        redemption: curator.clone(),
    };
    let (k, _) = fac.submit(&FChange::Rwa(rwa.clone(), rwa_vault, terms));
    fac.accept(&k);
    let q_id = env.register(
        Queue,
        (QueueConfig {
            facility: fac_id.clone(),
            curator: curator.clone(),
        },),
    );
    let (k, _) = fac.submit(&FChange::Queue(q_id.clone()));
    fac.accept(&k);
    for who in [&alice, &bob] {
        StellarAssetClient::new(&env, &usd).mint(who, &500_000);
        fac.deposit(&500_000, who, who, who);
    }
    StellarAssetClient::new(&env, &rwa).mint(&seller, &900_000);
    fac.trade(&seller, &seller, &rwa, &usd, &900_000, &1);
    F {
        q: QueueClient::new(&env, &q_id),
        env,
        fac,
        usd,
        alice,
        bob,
    }
}

impl F {
    fn queue_all(&self, who: &Address) -> u64 {
        self.q.request(who, who, &self.fac.balance(who))
    }
    fn usd(&self, who: &Address) -> i128 {
        TokenClient::new(&self.env, &self.usd).balance(who)
    }
}

#[test]
fn positions_are_served_first_come_first_served_and_a_part_fill_keeps_its_place() {
    let f = setup();
    let a = f.queue_all(&f.alice);
    f.queue_all(&f.bob);
    assert_eq!(f.q.bounds(), (0, 2));
    // 127k liquid: Alice, first in, takes it all; Bob waits.
    assert_eq!(f.q.process(&10), 1);
    assert!((126_999..=127_000).contains(&f.usd(&f.alice)));
    assert!(f.q.position(&a).unwrap().shares > 0);
    assert_eq!(f.usd(&f.bob), 0);
    assert_eq!(f.q.bounds(), (0, 2));
}

#[test]
fn as_liquidity_arrives_the_head_finishes_first_then_the_next() {
    let f = setup();
    let a = f.queue_all(&f.alice);
    let b = f.queue_all(&f.bob);
    f.q.process(&10);
    StellarAssetClient::new(&f.env, &f.usd).mint(&f.alice, &2_000_000);
    f.fac.deposit(&2_000_000, &f.alice, &f.alice, &f.alice);
    assert_eq!(f.q.process(&10), 2);
    assert!(f.q.position(&a).is_none() && f.q.position(&b).is_none());
    assert_eq!(f.q.bounds(), (2, 2));
    assert!(f.usd(&f.bob) >= 500_000);
}

#[test]
fn queued_shares_are_reserved_and_can_be_cancelled() {
    let f = setup();
    let b = f.queue_all(&f.bob);
    // Bob's queued claim outranks Alice's synchronous exit.
    assert_eq!(f.fac.max_withdraw(&f.alice), 0);
    let shares = f.q.position(&b).unwrap().shares;
    f.q.cancel(&b);
    assert_eq!(f.fac.balance(&f.bob), shares);
    assert!(f.q.position(&b).is_none());
    assert!(f.fac.max_withdraw(&f.alice) > 0);
    // Serving skips the cancelled slot.
    assert_eq!(f.q.process(&10), 0);
    assert_eq!(f.q.bounds(), (1, 1));
}

#[test]
fn only_the_owner_queues_or_cancels() {
    let f = setup();
    let b = f.queue_all(&f.bob);
    assert_eq!(
        f.q.try_request(&f.alice, &f.alice, &0),
        Err(Ok(Error::InvalidAmount.into()))
    );
    f.env.set_auths(&[]);
    assert!(f.q.try_cancel(&b).is_err());
    assert!(f.q.try_request(&f.alice, &f.alice, &1).is_err());
}
