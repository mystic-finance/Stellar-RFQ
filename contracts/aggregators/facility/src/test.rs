//! Real wasm throughout: vaults, the facility and the queue are deployed by the
//! factory from uploaded code; the router e2e uses the real router. Build
//! first (`stellar contract build`; the Makefile's `test` target does this).

use super::*;
use common::math::ONE;
use common::types::{Schedule, ScheduleMode};
use facility::{Change, FacilityClient as Fc, Terms};
use mock_oracle::MockOracle;
use router::{AggregatorLeg, Leg, RouterContractClient, SourceKind};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{vec, String};
use vault::VaultClient;

const VAULT: &[u8] = include_bytes!("../../../../target/wasm32v1-none/release/vault.wasm");
const FACILITY: &[u8] = include_bytes!("../../../../target/wasm32v1-none/release/facility.wasm");
const QUEUE: &[u8] =
    include_bytes!("../../../../target/wasm32v1-none/release/withdrawal_queue.wasm");
const VENUE: &[u8] = include_bytes!("../../../../target/wasm32v1-none/release/mock_venue.wasm");
const DAY: u64 = 86_400;
const BIG: i128 = 1_000_000_000_000;

struct F {
    env: Env,
    agg: FacilityAggregatorClient<'static>,
    router: RouterContractClient<'static>,
    fac: Fc<'static>,
    usd: Address,
    rwa: Address,
    rwa_vault: Address,
    curator: Address,
    fees: Address,
    seller: Address,
    venue_wasm: BytesN<32>,
}

fn vault_config(env: &Env, asset: &Address, curator: &Address) -> VaultConfig {
    VaultConfig {
        asset: asset.clone(),
        curator: curator.clone(),
        manager: curator.clone(),
        guardian: curator.clone(),
        name: String::from_str(env, "v"),
        symbol: String::from_str(env, "v"),
        deposit_cap: 0,
        curator_fee_bps: 0,
        timelock: 0,
    }
}

fn setup() -> F {
    let env = Env::default();
    env.mock_all_auths();
    env.cost_estimate().budget().reset_unlimited();
    env.ledger().set_timestamp(1_000_000);
    let [admin, curator, fees, lp, seller] = [0; 5].map(|_| Address::generate(&env));
    let usd = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let rwa = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    let settlement = env.register(rfq::RfqContract, ());
    rfq::RfqContractClient::new(&env, &settlement).initialize(&admin, &usd);
    let router = RouterContractClient::new(&env, &env.register(router::RouterContract, ()));
    router.initialize(&admin, &settlement);

    let up = |b: &[u8]| env.deployer().upload_contract_wasm(b);
    let wasms = Wasms {
        vault: up(VAULT),
        facility: up(FACILITY),
        queue: up(QUEUE),
    };
    let venue_wasm = up(VENUE);
    let agg = FacilityAggregatorClient::new(
        &env,
        &env.register(
            FacilityAggregator,
            (admin, router.address.clone(), wasms, fees.clone(), 50u32),
        ),
    );
    router.register_source(&agg.address, &SourceKind::Facility, &true);

    let base_vault = agg.create_vault(&vault_config(&env, &usd, &curator));
    let rwa_vault = agg.create_vault(&vault_config(&env, &rwa, &curator));
    let fac = Fc::new(
        &env,
        &agg.create_facility(&FacilityConfig {
            base_vault,
            curator: curator.clone(),
            manager: curator.clone(),
            guardian: curator.clone(),
            name: String::from_str(&env, "oLP"),
            symbol: String::from_str(&env, "oLP"),
            curator_fee_bps: 0,
            vest_seconds: 30 * DAY,
            timelock: 0,
        }),
    );
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
    let (k, _) = fac.submit(&Change::Rwa(rwa.clone(), rwa_vault.clone(), terms));
    fac.accept(&k);
    agg.set_active(&fac.address, &true);

    StellarAssetClient::new(&env, &usd).mint(&lp, &BIG);
    fac.deposit(&1_000_000, &lp, &lp, &lp);
    StellarAssetClient::new(&env, &rwa).mint(&seller, &BIG);
    F {
        env,
        agg,
        router,
        fac,
        usd,
        rwa,
        rwa_vault,
        curator,
        fees,
        seller,
        venue_wasm,
    }
}

impl F {
    fn usd_of(&self, who: &Address) -> i128 {
        TokenClient::new(&self.env, &self.usd).balance(who)
    }
    fn rwa_of(&self, who: &Address) -> i128 {
        TokenClient::new(&self.env, &self.rwa).balance(who)
    }
    fn swap(
        &self,
        payer: &Address,
        tin: &Address,
        tout: &Address,
        amount: i128,
        min: i128,
    ) -> i128 {
        self.agg
            .swap(payer, payer, tin, tout, &amount, &min, &self.fac.address)
    }
}

#[test]
fn the_factory_deploys_from_pinned_code_and_checks_what_it_is_handed() {
    let f = setup();
    assert_eq!(f.agg.kind_of(&f.fac.address), Some(Kind::Facility));
    assert_eq!(f.agg.kind_of(&f.rwa_vault), Some(Kind::Vault));
    assert_eq!(VaultClient::new(&f.env, &f.rwa_vault).query_asset(), f.rwa);

    // A facility over a vault we did not deploy is refused.
    let mut c = f.fac.config();
    c.base_vault = Address::generate(&f.env);
    assert_eq!(
        f.agg.try_create_facility(&c),
        Err(Ok(Error::NotRegistered.into()))
    );
    let q = QueueConfig {
        facility: f.fac.address.clone(),
        curator: f.curator.clone(),
    };
    let queue = f.agg.create_queue(&q);
    assert_eq!(f.agg.kind_of(&queue), Some(Kind::Queue));
    let mut bad = q;
    bad.facility = f.rwa_vault.clone();
    assert_eq!(
        f.agg.try_create_queue(&bad),
        Err(Ok(Error::NotRegistered.into()))
    );
}

#[test]
fn a_vault_gets_only_allowlisted_adapter_code() {
    let f = setup();
    let v = VaultClient::new(&f.env, &f.rwa_vault);
    v.submit(&vault::Change::Venue(
        f.venue_wasm.clone(),
        Address::generate(&f.env),
    ));
    assert!(v.try_accept(&2).is_err(), "not allowlisted");
    f.agg.allow_adapter(&f.venue_wasm, &true);
    v.submit(&vault::Change::Venue(
        f.venue_wasm.clone(),
        Address::generate(&f.env),
    ));
    v.accept(&2);
    assert_eq!(v.venues().len(), 1);
    assert!(f
        .agg
        .try_deploy_adapter(&Address::generate(&f.env), &f.venue_wasm, &f.rwa, &f.rwa)
        .is_err());
}

#[test]
fn selling_rwa_pays_the_fee_from_the_output_and_buying_from_the_input() {
    let f = setup();
    // 97,000 gross at 30 days x 10 bps/day; 50 bps fee.
    assert_eq!(
        f.agg
            .quote_all(&f.rwa, &f.usd, &100_000)
            .get(0)
            .unwrap()
            .amount_out,
        96_515
    );
    assert_eq!(f.swap(&f.seller, &f.rwa, &f.usd, 100_000, 96_515), 96_515);
    assert_eq!((f.usd_of(&f.seller), f.usd_of(&f.fees)), (96_515, 485));
    assert_eq!(f.rwa_of(&f.rwa_vault), 100_000);

    let buyer = Address::generate(&f.env);
    StellarAssetClient::new(&f.env, &f.usd).mint(&buyer, &98_500);
    let out = f.swap(&buyer, &f.usd, &f.rwa, 98_500, 1);
    assert_eq!(f.usd_of(&f.fees), 485 + 492);
    assert_eq!(f.rwa_of(&buyer), out);
    // The aggregator never held either token.
    assert_eq!((f.usd_of(&f.agg.address), f.rwa_of(&f.agg.address)), (0, 0));
}

#[test]
fn the_sellers_minimum_is_net_of_the_fee_and_a_pause_stops_routing() {
    let f = setup();
    assert!(f
        .agg
        .try_swap(
            &f.seller,
            &f.seller,
            &f.rwa,
            &f.usd,
            &100_000,
            &96_516,
            &f.fac.address
        )
        .is_err());
    f.agg.set_paused(&true);
    assert!(f.agg.quote_all(&f.rwa, &f.usd, &100_000).is_empty());
    assert_eq!(
        f.agg.try_swap(
            &f.seller,
            &f.seller,
            &f.rwa,
            &f.usd,
            &100_000,
            &1,
            &f.fac.address
        ),
        Err(Ok(Error::Paused.into()))
    );
    f.agg.set_paused(&false);
    f.agg.set_active(&f.fac.address, &false);
    assert_eq!(
        f.agg.try_swap(
            &f.seller,
            &f.seller,
            &f.rwa,
            &f.usd,
            &100_000,
            &1,
            &f.fac.address
        ),
        Err(Ok(Error::FacilityNotActive.into()))
    );
}

#[test]
fn a_facility_leg_settles_through_the_router_with_no_token_passing_through_it() {
    let f = setup();
    let data = FacilityRoute {
        facility: f.fac.address.clone(),
        payer: f.seller.clone(),
    }
    .to_xdr(&f.env);
    let leg = Leg::Facility(AggregatorLeg {
        aggregator: f.agg.address.clone(),
        taker_amount: 100_000,
        min_maker_amount: 96_515,
        data,
    });
    let r = f
        .router
        .fill(&f.seller, &f.rwa, &f.usd, &vec![&f.env, leg], &96_515);
    assert_eq!((r.taker_spent, r.amount_out), (100_000, 96_515));
    assert_eq!(f.usd_of(&f.seller), 96_515);
    assert_eq!(
        f.rwa_of(&f.rwa_vault),
        100_000,
        "straight from the seller into the RWA vault"
    );
    for hop in [&f.router.address, &f.agg.address, &f.fac.address] {
        assert_eq!((f.usd_of(hop), f.rwa_of(hop)), (0, 0));
    }
}

#[test]
fn the_admin_reconfigures_and_the_router_quote_is_the_best_facility() {
    let f = setup();
    assert_eq!(f.agg.quote(&f.rwa, &f.usd, &100_000), 96_515);
    assert_eq!(
        f.agg.quote(&f.usd, &f.rwa, &100_000),
        0,
        "no RWA to sell yet"
    );
    assert!(f.agg.try_set_fee(&f.fees, &501).is_err());
    f.agg.set_fee(&f.fees, &0);
    assert_eq!(f.agg.quote(&f.rwa, &f.usd, &100_000), 97_000);
    let w = f.agg.wasms();
    f.agg.set_wasms(&w);
    f.agg.set_router(&f.curator);
    assert!(f
        .agg
        .try_fill(
            &f.seller,
            &f.rwa,
            &f.usd,
            &1,
            &1,
            &Bytes::from_array(&f.env, &[1, 2, 3])
        )
        .is_err());
}

#[test]
fn the_exact_quote_fills_whatever_the_fee_rounding() {
    let f = setup();
    for amount in [100_003, 77_777, 123_457] {
        let q = f
            .agg
            .quote_all(&f.rwa, &f.usd, &amount)
            .get(0)
            .unwrap()
            .amount_out;
        assert_eq!(f.swap(&f.seller, &f.rwa, &f.usd, amount, q), q);
    }
}
