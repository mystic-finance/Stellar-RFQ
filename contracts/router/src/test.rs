use super::*;
use ed25519_dalek::{Signer as _, SigningKey};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{contract, contractimpl, vec, BytesN};

const HUGE: i128 = 1_000_000_000_000_000;
const DAY: u64 = 86_400;
const ONE: i128 = 1_000_000_000_000_000_000;

#[contract]
pub struct MockAggregator;

#[contractimpl]
impl MockAggregator {
    pub fn init(env: Env, token_in: Address, bps: i128, shortfall: i128) {
        env.storage().instance().set(&symbol_short!("i"), &token_in);
        env.storage().instance().set(&symbol_short!("b"), &bps);
        env.storage()
            .instance()
            .set(&symbol_short!("s"), &shortfall);
    }

    pub fn quote(env: Env, _token_in: Address, _token_out: Address, amount_in: i128) -> i128 {
        let bps: i128 = env.storage().instance().get(&symbol_short!("b")).unwrap();
        amount_in * bps / 10_000
    }

    pub fn fill(
        env: Env,
        recipient: Address,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        _min_amount_out: i128,
        _data: soroban_sdk::Bytes,
    ) -> i128 {
        let me = env.current_contract_address();
        let held: i128 = env
            .storage()
            .instance()
            .get(&symbol_short!("h"))
            .unwrap_or(0);
        let arrived = token::Client::new(&env, &token_in).balance(&me) - held;
        assert!(arrived >= amount_in, "router did not push the input");
        env.storage()
            .instance()
            .set(&symbol_short!("h"), &(held + arrived));

        let shortfall: i128 = env.storage().instance().get(&symbol_short!("s")).unwrap();
        let pay = Self::quote(env.clone(), token_in, token_out.clone(), amount_in) - shortfall;
        token::Client::new(&env, &token_out).transfer(&me, &recipient, &pay);
        pay
    }
}

struct Fixture {
    env: Env,
    router: RouterContractClient<'static>,
    settlement: rfq::RfqContractClient<'static>,
    admin: Address,
    maker: Address,
    taker: Address,
    collector: Address,
    rwa: Address,
    usd: Address,
    key: SigningKey,
    taker_key: SigningKey,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let admin = Address::generate(&env);
    let maker = Address::generate(&env);
    let taker = Address::generate(&env);
    let collector = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let rwa = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let usd = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let settlement_id = env.register(rfq::RfqContract, ());
    let settlement = rfq::RfqContractClient::new(&env, &settlement_id);
    settlement.initialize(&admin, &usd);

    let router_id = env.register(RouterContract, ());
    let router = RouterContractClient::new(&env, &router_id);
    router.initialize(&admin, &settlement_id);

    StellarAssetClient::new(&env, &rwa).mint(&taker, &HUGE);
    StellarAssetClient::new(&env, &usd).mint(&maker, &HUGE);

    let exp = env.ledger().sequence() + 1_000_000;
    TokenClient::new(&env, &rwa).approve(&taker, &settlement_id, &HUGE, &exp);
    TokenClient::new(&env, &rwa).approve(&taker, &router_id, &HUGE, &exp);
    TokenClient::new(&env, &usd).approve(&maker, &settlement_id, &HUGE, &exp);
    let _ = &collector;

    let key = SigningKey::from_bytes(&[7u8; 32]);
    settlement.register_order_signer(
        &maker,
        &BytesN::from_array(&env, &key.verifying_key().to_bytes()),
        &true,
    );
    let taker_key = SigningKey::from_bytes(&[11u8; 32]);
    settlement.register_order_signer(
        &taker,
        &BytesN::from_array(&env, &taker_key.verifying_key().to_bytes()),
        &true,
    );

    settlement.set_schedule(
        &admin,
        &rwa,
        &rfq::Schedule {
            mode: rfq::ScheduleMode::Rolling,
            rolling_seconds: (30 * DAY) as u32,
            next_redemption_at: 0,
            cycle_seconds: 0,
            max_bps_per_day: 1_000,
        },
    );
    settlement.set_config(&rfq::Config {
        min_fee_bps: 0,
        max_fee_bps: 1_000,
        fallback_max_age: 3_600,
        max_deviation_bps: 5_000,
        min_push_interval: 300,
        max_shift_seconds: 0,
        decay_seconds: (7 * DAY) as u32,
    });
    settlement.push_price(&admin, &rwa, &ONE);

    Fixture {
        env,
        router,
        settlement,
        admin,
        maker,
        taker,
        collector,
        rwa,
        usd,
        key,
        taker_key,
    }
}

impl Fixture {
    fn signed(&self, taker_amount: i128, bps_per_day: u32, salt: u64) -> Leg {
        let order = RfqOrder {
            maker_token: self.usd.clone(),
            taker_token: self.rwa.clone(),
            taker_amount,
            min_received_amount: 1,
            fee_bps: 0,
            taker: Some(self.taker.clone()),
            sender: Some(self.router.address.clone()),
            fee_recipient: self.admin.clone(),
            expiry: self.env.ledger().timestamp() + 1_000,
            salt,
            taker_max_bps_per_day: 1_000,
            maker_bps_per_day: bps_per_day,
            max_maker_amount: taker_amount,
            maker: self.maker.clone(),
        };
        let maker_signature = self.sign_as(&self.key, &self.settlement.hash_rfq_order(&order));
        let taker_signature = vec![
            &self.env,
            self.sign_as(
                &self.taker_key,
                &self.settlement.hash_request(&order.request()),
            ),
        ];
        Leg::Rfq(RfqLeg {
            order: SignedOrder::Rfq(order),
            maker_signature,
            taker_signature,
            taker_amount,
        })
    }

    fn signed_open(&self, taker_amount: i128, bps_per_day: u32, salt: u64) -> Leg {
        let mut order = match self.signed(taker_amount, bps_per_day, salt) {
            Leg::Rfq(l) => match l.order {
                SignedOrder::Rfq(o) => o,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        };
        order.taker = None;
        Leg::Rfq(RfqLeg {
            maker_signature: self.sign_as(&self.key, &self.settlement.hash_rfq_order(&order)),
            order: SignedOrder::Rfq(order),
            taker_signature: vec![&self.env],
            taker_amount,
        })
    }

    fn sign_as(&self, key: &SigningKey, hash: &BytesN<32>) -> Signature {
        let digest = self.sep53(hash);
        Signature {
            signer: BytesN::from_array(&self.env, &key.verifying_key().to_bytes()),
            signature: BytesN::from_array(&self.env, &key.sign(&digest).to_bytes()),
        }
    }

    fn sep53(&self, hash: &BytesN<32>) -> [u8; 32] {
        let mut buf = soroban_sdk::Bytes::from_slice(&self.env, b"Stellar Signed Message:\n");
        buf.append(&soroban_sdk::Bytes::from_array(&self.env, &hash.to_array()));
        self.env.crypto().sha256(&buf).to_bytes().to_array()
    }

    fn aggregator(&self, kind: SourceKind, bps: i128, shortfall: i128) -> Address {
        let id = self.env.register(MockAggregator, ());
        StellarAssetClient::new(&self.env, &self.usd).mint(&id, &HUGE);
        MockAggregatorClient::new(&self.env, &id).init(&self.rwa, &bps, &shortfall);
        self.router.register_source(&id, &kind, &true);
        id
    }

    fn agg_leg(&self, kind: SourceKind, at: &Address, taker_amount: i128, min_out: i128) -> Leg {
        let leg = AggregatorLeg {
            aggregator: at.clone(),
            taker_amount,
            min_maker_amount: min_out,
            data: soroban_sdk::Bytes::new(&self.env),
        };
        match kind {
            SourceKind::Dex => Leg::Dex(leg),
            SourceKind::Facility => Leg::Facility(leg),
        }
    }

    fn usd_of(&self, who: &Address) -> i128 {
        TokenClient::new(&self.env, &self.usd).balance(who)
    }
    fn rwa_of(&self, who: &Address) -> i128 {
        TokenClient::new(&self.env, &self.rwa).balance(who)
    }
}

#[test]
fn routes_a_signed_bid_through_the_settlement_contract() {
    let f = setup();
    let route = vec![&f.env, f.signed(1_000_000, 1_000, 1)];

    let r = f.router.fill(&f.taker, &f.rwa, &f.usd, &route, &970_000);
    assert_eq!(r.taker_spent, 1_000_000);
    assert_eq!(r.amount_out, 970_000);
    assert_eq!(f.usd_of(&f.taker), 970_000);
    assert_eq!(f.rwa_of(&f.maker), 1_000_000);
    assert_eq!(f.usd_of(&f.router.address), 0);
    assert_eq!(f.rwa_of(&f.router.address), 0);
}

#[test]
fn quote_ranks_every_registered_source() {
    let f = setup();
    let dex = f.aggregator(SourceKind::Dex, 9_500, 0);
    let facility = f.aggregator(SourceKind::Facility, 9_900, 0);

    let quotes = f.router.quote(&f.rwa, &f.usd, &1_000_000);
    assert_eq!(quotes.len(), 2);
    assert_eq!(quotes.get(0).unwrap().kind, SourceKind::Dex);
    assert_eq!(quotes.get(1).unwrap().kind, SourceKind::Facility);
    assert_eq!(quotes.get(0).unwrap().source, dex);

    let best = f.router.best_quote(&f.rwa, &f.usd, &1_000_000).unwrap();
    assert_eq!(best.source, facility);
    assert_eq!(best.maker_amount, 990_000);
}

#[test]
fn the_route_the_taker_picked_is_the_route_that_settles() {
    let f = setup();
    let dex = f.aggregator(SourceKind::Dex, 9_500, 0);
    let facility = f.aggregator(SourceKind::Facility, 9_900, 0);

    let r = f.router.fill(
        &f.taker,
        &f.rwa,
        &f.usd,
        &vec![&f.env, f.agg_leg(SourceKind::Dex, &dex, 1_000_000, 950_000)],
        &950_000,
    );
    assert_eq!(r.amount_out, 950_000);
    assert_eq!(f.rwa_of(&dex), 1_000_000);
    assert_eq!(f.rwa_of(&facility), 0);
}

#[test]
fn a_blended_route_sums_both_bid_channels() {
    let f = setup();
    let facility = f.aggregator(SourceKind::Facility, 9_900, 0);
    let dex = f.aggregator(SourceKind::Dex, 9_800, 0);

    let route = vec![
        &f.env,
        f.signed(400_000, 1_000, 1),
        f.agg_leg(SourceKind::Facility, &facility, 300_000, 297_000),
        f.agg_leg(SourceKind::Dex, &dex, 300_000, 294_000),
    ];
    let r = f.router.fill(&f.taker, &f.rwa, &f.usd, &route, &979_000);

    assert_eq!(r.taker_spent, 1_000_000);
    assert_eq!(r.amount_out, 388_000 + 297_000 + 294_000);
    assert_eq!(f.rwa_of(&f.maker), 400_000);
    assert_eq!(f.rwa_of(&facility), 300_000);
    assert_eq!(f.rwa_of(&dex), 300_000);
    assert_eq!(f.usd_of(&f.router.address), 0);
    assert_eq!(f.rwa_of(&f.router.address), 0);
}

#[test]
fn reverts_whole_route_when_output_misses_the_minimum() {
    let f = setup();
    let before = (f.usd_of(&f.taker), f.rwa_of(&f.taker));
    let route = vec![&f.env, f.signed(1_000_000, 1_000, 1)];

    let err = f
        .router
        .try_fill(&f.taker, &f.rwa, &f.usd, &route, &970_001)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::BelowMinOut.into());
    assert_eq!((f.usd_of(&f.taker), f.rwa_of(&f.taker)), before);
    assert_eq!(f.rwa_of(&f.maker), 0);
    let order = match route.get(0).unwrap() {
        Leg::Rfq(l) => match l.order {
            SignedOrder::Rfq(o) => o,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    assert_eq!(
        f.settlement
            .filled_amount(&f.settlement.hash_rfq_order(&order)),
        0
    );
}

#[test]
fn an_open_bid_is_routed_with_the_router_as_taker_of_record() {
    let f = setup();
    TokenClient::new(&f.env, &f.rwa).approve(
        &f.taker,
        &f.settlement.address,
        &0i128,
        &(f.env.ledger().sequence() + 1_000_000),
    );

    let route = vec![&f.env, f.signed_open(1_000_000, 1_000, 7)];
    let r = f.router.fill(&f.taker, &f.rwa, &f.usd, &route, &970_000);

    assert_eq!(r.amount_out, 970_000);
    assert_eq!(f.usd_of(&f.taker), 970_000);
    assert_eq!(f.rwa_of(&f.maker), 1_000_000);
    assert_eq!(f.rwa_of(&f.router.address), 0);
    assert_eq!(f.usd_of(&f.router.address), 0);
}

#[test]
fn a_leg_whose_signature_contradicts_its_order_is_rejected() {
    let f = setup();
    let open_with_sig = match (f.signed_open(1_000, 1_000, 8), f.signed(1_000, 1_000, 9)) {
        (Leg::Rfq(open), Leg::Rfq(named)) => Leg::Rfq(RfqLeg {
            taker_signature: named.taker_signature,
            ..open
        }),
        _ => unreachable!(),
    };
    assert_eq!(
        f.router
            .try_fill(&f.taker, &f.rwa, &f.usd, &vec![&f.env, open_with_sig], &1)
            .err()
            .unwrap()
            .unwrap(),
        Error::LegSignatureMismatch.into()
    );

    let named_without_sig = match f.signed(1_000, 1_000, 10) {
        Leg::Rfq(l) => Leg::Rfq(RfqLeg {
            taker_signature: vec![&f.env],
            ..l
        }),
        _ => unreachable!(),
    };
    assert_eq!(
        f.router
            .try_fill(
                &f.taker,
                &f.rwa,
                &f.usd,
                &vec![&f.env, named_without_sig],
                &1
            )
            .err()
            .unwrap()
            .unwrap(),
        Error::LegSignatureMismatch.into()
    );
}

#[test]
fn a_bid_quoted_to_someone_else_cannot_be_routed() {
    let f = setup();
    let mallory = Address::generate(&f.env);
    TokenClient::new(&f.env, &f.rwa).approve(
        &mallory,
        &f.router.address,
        &HUGE,
        &(f.env.ledger().sequence() + 1_000_000),
    );

    let route = vec![&f.env, f.signed(1_000_000, 1_000, 1)];
    let err = f
        .router
        .try_fill(&mallory, &f.rwa, &f.usd, &route, &1)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::LegTakerMismatch.into());
    assert_eq!(f.usd_of(&f.taker), 0);
    assert_eq!(f.rwa_of(&f.maker), 0);
}

#[test]
fn a_signed_leg_settles_between_the_counterparties_only() {
    let f = setup();
    let route = vec![&f.env, f.signed(1_000_000, 1_000, 1)];
    f.router.fill(&f.taker, &f.rwa, &f.usd, &route, &970_000);

    assert_eq!(f.rwa_of(&f.maker), 1_000_000);
    assert_eq!(f.usd_of(&f.taker), 970_000);
    assert_eq!(f.rwa_of(&f.router.address), 0);
    assert_eq!(f.usd_of(&f.router.address), 0);
}

#[test]
fn the_router_takes_no_fee_of_its_own() {
    let f = setup();
    let route = vec![&f.env, f.signed(1_000_000, 1_000, 1)];

    let r = f.router.fill(&f.taker, &f.rwa, &f.usd, &route, &970_000);
    assert_eq!(r.amount_out, 970_000);
    assert_eq!(f.usd_of(&f.taker), 970_000);
    assert_eq!(f.usd_of(&f.collector), 0);
}

#[test]
fn a_source_that_under_delivers_its_quote_reverts() {
    let f = setup();
    let facility = f.aggregator(SourceKind::Facility, 9_900, 1);

    let err = f
        .router
        .try_fill(
            &f.taker,
            &f.rwa,
            &f.usd,
            &vec![
                &f.env,
                f.agg_leg(SourceKind::Facility, &facility, 1_000_000, 990_000),
            ],
            &1,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::SourceUnderDelivered.into());
    assert_eq!(f.usd_of(&f.taker), 0);
}

#[test]
fn unregistered_sources_and_mismatched_legs_are_rejected() {
    let f = setup();
    let rogue = f.aggregator(SourceKind::Facility, 9_900, 0);
    f.router
        .register_source(&rogue, &SourceKind::Facility, &false);
    assert_eq!(f.router.sources().len(), 0);

    let err = f
        .router
        .try_fill(
            &f.taker,
            &f.rwa,
            &f.usd,
            &vec![&f.env, f.agg_leg(SourceKind::Facility, &rogue, 1_000, 900)],
            &1,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::SourceNotRegistered.into());

    let err = f
        .router
        .try_fill(
            &f.taker,
            &f.usd,
            &f.rwa,
            &vec![&f.env, f.signed(1_000, 1_000, 1)],
            &1,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::TokenMismatch.into());
}

#[test]
fn empty_routes_and_pause_are_enforced() {
    let f = setup();
    assert_eq!(
        f.router
            .try_fill(&f.taker, &f.rwa, &f.usd, &vec![&f.env], &1)
            .err()
            .unwrap()
            .unwrap(),
        Error::EmptyRoute.into()
    );

    f.router.set_paused(&true);
    assert_eq!(
        f.router
            .try_fill(
                &f.taker,
                &f.rwa,
                &f.usd,
                &vec![&f.env, f.signed(1_000, 1_000, 1)],
                &1
            )
            .err()
            .unwrap()
            .unwrap(),
        Error::Paused.into()
    );
}

#[test]
fn a_source_is_held_to_the_payout_the_taker_was_shown() {
    let f = setup();
    let facility = f.aggregator(SourceKind::Facility, 9_900, 0);

    let err = f
        .router
        .try_fill(
            &f.taker,
            &f.rwa,
            &f.usd,
            &vec![
                &f.env,
                f.agg_leg(SourceKind::Facility, &facility, 1_000_000, 990_001),
            ],
            &1,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::SourceUnderDelivered.into());
    assert_eq!(f.usd_of(&f.taker), 0);
}
