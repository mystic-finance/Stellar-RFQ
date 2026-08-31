use super::*;
use ed25519_dalek::{Signer as _, SigningKey};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{Address, BytesN, Env};

pub(crate) const HUGE: i128 = 1_000_000_000_000_000;
pub(crate) const DAY: u64 = 86_400;
pub(crate) const ONE: i128 = 1_000_000_000_000_000_000;

pub(crate) struct Fixture {
    pub env: Env,
    pub client: RfqContractClient<'static>,
    pub admin: Address,
    pub maker: Address,
    pub taker: Address,
    pub rwa: Address,
    pub usd: Address,
    pub key: SigningKey,
    pub pubkey: BytesN<32>,
    pub taker_key: SigningKey,
}

pub(crate) fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let admin = Address::generate(&env);
    let maker = Address::generate(&env);
    let taker = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let rwa = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let usd = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let contract_id = env.register(RfqContract, ());
    let client = RfqContractClient::new(&env, &contract_id);
    client.initialize(&admin, &usd);

    StellarAssetClient::new(&env, &rwa).mint(&taker, &HUGE);
    StellarAssetClient::new(&env, &usd).mint(&maker, &HUGE);
    let exp = env.ledger().sequence() + 1_000_000;
    TokenClient::new(&env, &rwa).approve(&taker, &contract_id, &HUGE, &exp);
    TokenClient::new(&env, &usd).approve(&maker, &contract_id, &HUGE, &exp);

    let key = SigningKey::from_bytes(&[7u8; 32]);
    let pubkey = BytesN::from_array(&env, &key.verifying_key().to_bytes());
    client.register_order_signer(&maker, &pubkey, &true);

    let taker_key = SigningKey::from_bytes(&[11u8; 32]);
    client.register_order_signer(
        &taker,
        &BytesN::from_array(&env, &taker_key.verifying_key().to_bytes()),
        &true,
    );

    client.set_schedule(
        &admin,
        &rwa,
        &Schedule {
            mode: ScheduleMode::Rolling,
            rolling_seconds: (30 * DAY) as u32,
            next_redemption_at: 0,
            cycle_seconds: 0,
            max_bps_per_day: 10,
        },
    );
    client.set_config(&Config {
        min_fee_bps: 0,
        max_fee_bps: 1_000,
        fallback_max_age: 3_600,
        max_deviation_bps: 5_000,
        min_push_interval: 300,
        max_shift_seconds: 0,
        decay_seconds: (7 * DAY) as u32,
    });
    client.push_price(&admin, &rwa, &ONE);

    Fixture {
        env,
        client,
        admin,
        maker,
        taker,
        rwa,
        usd,
        key,
        pubkey,
        taker_key,
    }
}

impl Fixture {
    pub fn rfq(&self) -> RfqOrder {
        RfqOrder {
            maker_token: self.usd.clone(),
            taker_token: self.rwa.clone(),
            taker_amount: 1_000_000,
            min_received_amount: 1,
            fee_bps: 0,
            taker: None,
            sender: None,
            fee_recipient: self.admin.clone(),
            expiry: self.env.ledger().timestamp() + 1_000,
            salt: 1,
            taker_max_bps_per_day: 10,
            maker_bps_per_day: 10,
            max_maker_amount: 1_000_000,
            maker: self.maker.clone(),
        }
    }

    pub fn fixed(&self) -> FixedOrder {
        FixedOrder {
            maker_token: self.usd.clone(),
            taker_token: self.rwa.clone(),
            taker_amount: 1_000_000,
            min_received_amount: 1,
            fee_bps: 0,
            taker: None,
            sender: None,
            fee_recipient: self.admin.clone(),
            expiry: self.env.ledger().timestamp() + 1_000,
            salt: 1,
            maker_amount: 900_000,
            maker: self.maker.clone(),
        }
    }

    pub fn dutch(&self) -> DutchOrder {
        DutchOrder {
            maker_token: self.usd.clone(),
            taker_token: self.rwa.clone(),
            taker_amount: 1_000_000,
            start_maker_amount: 1_000_000,
            min_maker_amount: 800_000,
            fee_bps: 0,
            fee_recipient: self.admin.clone(),
            expiry: 0,
        }
    }

    pub fn sign(&self, hash: &BytesN<32>) -> Signature {
        self.sign_as(&self.key, hash)
    }

    pub fn sign_as(&self, key: &SigningKey, hash: &BytesN<32>) -> Signature {
        let digest = crate::hash::sep53(&self.env, hash);
        Signature {
            signer: BytesN::from_array(&self.env, &key.verifying_key().to_bytes()),
            signature: BytesN::from_array(&self.env, &key.sign(&digest.to_array()).to_bytes()),
        }
    }

    pub fn usd_of(&self, who: &Address) -> i128 {
        TokenClient::new(&self.env, &self.usd).balance(who)
    }
    pub fn rwa_of(&self, who: &Address) -> i128 {
        TokenClient::new(&self.env, &self.rwa).balance(who)
    }
}

#[test]
fn rfq_discount_is_bps_per_day_over_the_horizon() {
    let f = setup();
    let order = f.rfq();
    let q = f.client.quote_rfq_order(&order, &1_000_000);
    assert_eq!(q.horizon_seconds, (30 * DAY) as u32);
    assert_eq!(q.maker_amount, 970_000);

    let r = f.client.fill_rfq_order(
        &order,
        &f.sign(&f.client.hash_rfq_order(&order)),
        &None,
        &f.taker,
        &1_000_000,
    );
    assert_eq!(r.maker_filled, 970_000);
    assert_eq!(f.usd_of(&f.taker), 970_000);
    assert_eq!(f.rwa_of(&f.maker), 1_000_000);
}

#[test]
fn horizon_nets_off_the_maker_leg_schedule() {
    let f = setup();
    f.client.set_schedule(
        &f.admin,
        &f.usd,
        &Schedule {
            mode: ScheduleMode::Rolling,
            rolling_seconds: (10 * DAY) as u32,
            next_redemption_at: 0,
            cycle_seconds: 0,
            max_bps_per_day: 10,
        },
    );
    let q = f.client.quote_rfq_order(&f.rfq(), &1_000_000);
    assert_eq!(q.horizon_seconds, (20 * DAY) as u32);
    assert_eq!(q.maker_amount, 980_000);
}

#[test]
fn fixed_schedule_prices_to_the_second() {
    let f = setup();
    let now = f.env.ledger().timestamp();
    f.client.set_schedule(
        &f.admin,
        &f.rwa,
        &Schedule {
            mode: ScheduleMode::Cyclical,
            rolling_seconds: 0,
            next_redemption_at: now + 12 * 3_600,
            cycle_seconds: (30 * DAY) as u32,
            max_bps_per_day: 10,
        },
    );
    assert_eq!(f.client.seconds_to_redemption(&f.rwa), 12 * 3_600);

    f.env.ledger().set_timestamp(now + 13 * 3_600);
    assert_eq!(
        f.client.seconds_to_redemption(&f.rwa),
        (30 * DAY - 3_600) as u32
    );
}

#[test]
fn rfq_rejects_rate_above_the_schedule_or_taker_cap() {
    let f = setup();
    let mut order = f.rfq();
    order.maker_bps_per_day = 11;
    let err = f
        .client
        .try_quote_rfq_order(&order, &1_000_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::BpsPerDayTooHigh.into());

    let mut order = f.rfq();
    order.taker_max_bps_per_day = 5;
    let err = f
        .client
        .try_quote_rfq_order(&order, &1_000_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::BpsPerDayTooHigh.into());
}

#[test]
fn rfq_partial_fills_accumulate_and_cannot_overfill() {
    let f = setup();
    let order = f.rfq();
    let sig = f.sign(&f.client.hash_rfq_order(&order));

    f.client
        .fill_rfq_order(&order, &sig, &None, &f.taker, &400_000);
    assert_eq!(
        f.client.filled_amount(&f.client.hash_rfq_order(&order)),
        400_000
    );
    f.client
        .fill_rfq_order(&order, &sig, &None, &f.taker, &600_000);
    assert_eq!(f.usd_of(&f.taker), 970_000);

    let err = f
        .client
        .try_fill_rfq_order(&order, &sig, &None, &f.taker, &1)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::OrderNotFillable.into());
}

#[test]
fn rfq_enforces_maker_cap_and_taker_floor() {
    let f = setup();
    let mut order = f.rfq();
    order.max_maker_amount = 960_000;
    let err = f
        .client
        .try_fill_rfq_order(
            &order,
            &f.sign(&f.client.hash_rfq_order(&order)),
            &None,
            &f.taker,
            &1_000_000,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::MakerAmountTooHigh.into());

    let mut order = f.rfq();
    order.min_received_amount = 980_000;
    let err = f
        .client
        .try_fill_rfq_order(
            &order,
            &f.sign(&f.client.hash_rfq_order(&order)),
            &None,
            &f.taker,
            &1_000_000,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::BelowMinReceived.into());
}

#[test]
fn fee_is_skimmed_from_the_maker_output() {
    let f = setup();
    let recipient = Address::generate(&f.env);
    let mut order = f.rfq();
    order.fee_bps = 100;
    order.fee_recipient = recipient.clone();

    let r = f.client.fill_rfq_order(
        &order,
        &f.sign(&f.client.hash_rfq_order(&order)),
        &None,
        &f.taker,
        &1_000_000,
    );
    assert_eq!(r.maker_filled, 970_000);
    assert_eq!(r.fee, 9_700);
    assert_eq!(f.usd_of(&recipient), 9_700);
    assert_eq!(f.usd_of(&f.taker), 960_300);
}

#[test]
fn fixed_order_ignores_the_rate_model() {
    let f = setup();
    let order = f.fixed();
    let r = f.client.fill_fixed_order(
        &order,
        &f.sign(&f.client.hash_fixed_order(&order)),
        &None,
        &f.taker,
        &500_000,
    );
    assert_eq!(r.maker_filled, 450_000);
    assert_eq!(f.usd_of(&f.taker), 450_000);
}

#[test]
fn dutch_ask_decays_and_fills_against_escrow() {
    let f = setup();
    let buyer = Address::generate(&f.env);
    StellarAssetClient::new(&f.env, &f.usd).mint(&buyer, &HUGE);
    TokenClient::new(&f.env, &f.usd).approve(
        &buyer,
        &f.client.address,
        &HUGE,
        &(f.env.ledger().sequence() + 1_000_000),
    );

    let id = f.client.create_dutch_order(&f.taker, &f.dutch());
    assert_eq!(f.rwa_of(&f.client.address), 1_000_000);
    assert_eq!(f.client.current_ask(&id), 1_000_000);

    f.env
        .ledger()
        .set_timestamp(f.env.ledger().timestamp() + 3 * DAY + DAY / 2);
    assert_eq!(f.client.current_ask(&id), 900_000);

    let r = f.client.fill_dutch_order(&id, &buyer, &900_000);
    assert_eq!(r.maker_filled, 900_000);
    assert_eq!(f.rwa_of(&buyer), 1_000_000);
    assert_eq!(f.usd_of(&f.taker), 900_000);
    assert_eq!(f.rwa_of(&f.client.address), 0);

    let err = f
        .client
        .try_fill_dutch_order(&id, &buyer, &900_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::ListingNotActive.into());
}

#[test]
fn dutch_ask_floors_after_the_decay_and_cancel_returns_escrow() {
    let f = setup();
    let id = f.client.create_dutch_order(&f.taker, &f.dutch());
    f.env
        .ledger()
        .set_timestamp(f.env.ledger().timestamp() + 30 * DAY);
    assert_eq!(f.client.current_ask(&id), 800_000);

    f.client.cancel_dutch_order(&id);
    assert_eq!(f.rwa_of(&f.taker), HUGE);
    assert_eq!(f.rwa_of(&f.client.address), 0);
}

#[test]
fn cancelling_a_salt_voids_both_sides_book_under_it() {
    let f = setup();
    let order = f.rfq();
    let sig = f.sign(&f.client.hash_rfq_order(&order));

    f.client.cancel_salt(&f.maker, &f.maker, &order.salt);
    assert!(f.client.is_salt_cancelled(&f.maker, &order.salt));
    let err = f
        .client
        .try_fill_rfq_order(&order, &sig, &None, &f.taker, &1)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::SaltIsCancelled.into());

    let mut other = f.rfq();
    other.salt = 2;
    let sig = f.sign(&f.client.hash_rfq_order(&other));
    assert_eq!(
        f.client
            .fill_rfq_order(&other, &sig, &None, &f.taker, &1_000)
            .taker_filled,
        1_000
    );

    let stranger = Address::generate(&f.env);
    assert!(f
        .client
        .try_cancel_salt(&stranger, &f.maker, &3u64)
        .is_err());
}

#[test]
fn unregistered_signer_and_expiry_are_rejected() {
    let f = setup();
    let order = f.rfq();
    let rogue = SigningKey::from_bytes(&[9u8; 32]);
    let hash = f.client.hash_rfq_order(&order);
    let digest = crate::hash::sep53(&f.env, &hash);
    let sig = Signature {
        signer: BytesN::from_array(&f.env, &rogue.verifying_key().to_bytes()),
        signature: BytesN::from_array(&f.env, &rogue.sign(&digest.to_array()).to_bytes()),
    };
    let err = f
        .client
        .try_fill_rfq_order(&order, &sig, &None, &f.taker, &1_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::SignerNotAuthorized.into());

    assert!(f.client.is_order_signer(&f.maker, &f.pubkey));

    let good = f.sign(&hash);
    f.env.ledger().set_timestamp(order.expiry + 1);
    let err = f
        .client
        .try_fill_rfq_order(&order, &good, &None, &f.taker, &1_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::OrderNotFillable.into());
}

#[test]
fn pause_blocks_fills() {
    let f = setup();
    f.client.set_paused(&true);
    let order = f.rfq();
    let err = f
        .client
        .try_fill_rfq_order(
            &order,
            &f.sign(&f.client.hash_rfq_order(&order)),
            &None,
            &f.taker,
            &1_000,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::Paused.into());
}

#[test]
fn pushed_price_needs_a_live_epoch_and_bounded_deviation() {
    let f = setup();
    assert_eq!(f.client.price_of(&f.rwa, &f.usd), ONE);

    let err = f
        .client
        .try_push_price(&f.admin, &f.rwa, &(ONE * 3))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::PriceDeviation.into());

    f.client.set_reference(&Address::generate(&f.env));
    let err = f
        .client
        .try_price_of(&f.rwa, &f.usd)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::NoPrice.into());
}

#[test]
fn keeper_schedule_shift_is_bounded() {
    let f = setup();
    let keeper = Address::generate(&f.env);
    f.client.set_keeper(&keeper, &true);
    f.client.set_config(&Config {
        min_fee_bps: 0,
        max_fee_bps: 1_000,
        fallback_max_age: 3_600,
        max_deviation_bps: 5_000,
        min_push_interval: 300,
        max_shift_seconds: DAY as u32,
        decay_seconds: (7 * DAY) as u32,
    });

    let mut schedule = f.client.get_schedule(&f.rwa).unwrap();
    schedule.rolling_seconds = (25 * DAY) as u32;
    let err = f
        .client
        .try_set_schedule(&keeper, &f.rwa, &schedule)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::ScheduleShiftTooLarge.into());

    f.client.set_schedule(&f.admin, &f.rwa, &schedule);
    assert_eq!(f.client.seconds_to_redemption(&f.rwa), (25 * DAY) as u32);
}

#[soroban_sdk::contract]
pub struct MockFeed;

#[soroban_sdk::contractimpl]
impl MockFeed {
    pub fn set(env: Env, asset: oracle::Asset, price: i128, timestamp: u64) {
        env.storage().persistent().set(&asset, &(price, timestamp));
    }
    pub fn decimals(_env: Env) -> u32 {
        14
    }
    pub fn lastprice(env: Env, asset: oracle::Asset) -> Option<oracle::PriceData> {
        env.storage()
            .persistent()
            .get::<oracle::Asset, (i128, u64)>(&asset)
            .map(|(price, timestamp)| oracle::PriceData { price, timestamp })
    }
}

#[test]
fn registered_oracle_prices_the_fill_and_staleness_falls_back() {
    let f = setup();
    let now = f.env.ledger().timestamp();

    let feed = f.env.register(MockFeed, ());
    MockFeedClient::new(&f.env, &feed).set(
        &oracle::Asset::Stellar(f.rwa.clone()),
        &(2 * 100_000_000_000_000),
        &now,
    );
    let adapter = f.env.register(oracle::OctarineOracle, ());
    oracle::OctarineOracleClient::new(&f.env, &adapter).initialize(
        &f.admin,
        &oracle::Config {
            source: feed.clone(),
            base: f.rwa.clone(),
            quote: f.usd.clone(),
            base_asset: oracle::Asset::Stellar(f.rwa.clone()),
            quote_asset: oracle::Asset::Stellar(f.rwa.clone()),
            cross: false,
            base_decimals: 7,
            quote_decimals: 7,
            max_age: 3_600,
            invert: false,
        },
    );
    f.client.set_oracle(
        &f.rwa,
        &f.usd,
        &Some(OracleCfg {
            oracle: adapter,
            max_age: 600,
        }),
    );

    assert_eq!(f.client.price_of(&f.rwa, &f.usd), ONE * 2);
    let mut order = f.rfq();
    order.max_maker_amount = 2_000_000;
    assert_eq!(
        f.client.quote_rfq_order(&order, &1_000_000).maker_amount,
        1_940_000
    );

    f.env.ledger().set_timestamp(now + 1_000);
    assert_eq!(f.client.price_of(&f.rwa, &f.usd), ONE);

    f.env.ledger().set_timestamp(now + 4_000);
    assert_eq!(
        f.client
            .try_price_of(&f.rwa, &f.usd)
            .err()
            .unwrap()
            .unwrap(),
        Error::NoPrice.into()
    );
}

#[test]
fn sep53_digest_matches_reference() {
    let env = Env::default();
    let hash = BytesN::from_array(
        &env,
        &[
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb,
            0xcc, 0xdd, 0xee, 0xff,
        ],
    );
    assert_eq!(
        crate::hash::sep53(&env, &hash).to_array(),
        [
            0x09, 0xa6, 0xf2, 0x36, 0x99, 0xc9, 0xfd, 0x76, 0xed, 0x9e, 0x0d, 0x6c, 0x32, 0xff,
            0x3e, 0x6c, 0x62, 0xbf, 0xe6, 0xc5, 0x04, 0x6a, 0xca, 0x5c, 0x72, 0xce, 0x6d, 0x88,
            0xba, 0xe1, 0x59, 0x14,
        ],
    );
}

use crate::mock_token::{SkewToken, SkewTokenClient};

fn skewed(tax_bps: i128, bonus_bps: i128) -> (Fixture, SkewTokenClient<'static>) {
    let f = setup();
    let token = f.env.register(SkewToken, ());
    let client = SkewTokenClient::new(&f.env, &token);
    client.init(&tax_bps, &bonus_bps);
    client.mint(&f.taker, &HUGE);
    client.approve(&f.taker, &f.client.address, &HUGE, &0u32);

    f.client.set_schedule(
        &f.admin,
        &token,
        &Schedule {
            mode: ScheduleMode::Rolling,
            rolling_seconds: (30 * DAY) as u32,
            next_redemption_at: 0,
            cycle_seconds: 0,
            max_bps_per_day: 10,
        },
    );
    f.client.push_price(&f.admin, &token, &ONE);
    (f, client)
}

#[test]
fn a_taxed_taker_token_is_priced_on_what_actually_arrived() {
    let (f, token) = skewed(100, 0);
    let mut order = f.rfq();
    order.taker_token = token.address.clone();
    order.min_received_amount = 1;

    let r = f.client.fill_rfq_order(
        &order,
        &f.sign(&f.client.hash_rfq_order(&order)),
        &None,
        &f.taker,
        &1_000_000,
    );
    assert_eq!(token.balance(&f.maker), 990_000);
    assert_eq!(r.maker_filled, 960_300);
    assert_eq!(f.usd_of(&f.taker), 960_300);
}

#[test]
fn a_taxed_taker_token_cannot_walk_under_the_takers_floor() {
    let (f, token) = skewed(100, 0);
    let mut order = f.rfq();
    order.taker_token = token.address.clone();
    order.min_received_amount = 970_000;

    let err = f
        .client
        .try_fill_rfq_order(
            &order,
            &f.sign(&f.client.hash_rfq_order(&order)),
            &None,
            &f.taker,
            &1_000_000,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::BelowMinReceived.into());
    assert_eq!(f.usd_of(&f.taker), 0);
}

#[test]
fn an_over_crediting_taker_token_cannot_inflate_the_makers_bill() {
    let (f, token) = skewed(0, 100);
    let mut order = f.rfq();
    order.taker_token = token.address.clone();
    order.min_received_amount = 1;

    let r = f.client.fill_rfq_order(
        &order,
        &f.sign(&f.client.hash_rfq_order(&order)),
        &None,
        &f.taker,
        &1_000_000,
    );
    assert_eq!(token.balance(&f.maker), 1_010_000);
    assert_eq!(r.maker_filled, 970_000);
}

#[test]
fn dutch_escrow_records_what_arrived_and_rescales_the_curve() {
    let (f, token) = skewed(100, 0);
    let mut order = f.dutch();
    order.taker_token = token.address.clone();

    let id = f.client.create_dutch_order(&f.taker, &order);
    let listing = f.client.get_listing(&id).unwrap();

    assert_eq!(listing.order.taker_amount, 990_000);
    assert_eq!(token.balance(&f.client.address), 990_000);
    assert_eq!(listing.order.start_maker_amount, 990_000);
    assert_eq!(listing.order.min_maker_amount, 792_000);
    assert_eq!(f.client.current_ask(&id), 990_000);
}

#[test]
fn anyone_can_submit_a_fill_the_taker_signed_for() {
    let f = setup();
    let relayer = Address::generate(&f.env);

    let mut order = f.rfq();
    order.taker = Some(f.taker.clone());
    let taker_sig = f.sign_as(&f.taker_key, &f.client.hash_request(&order.request()));
    let maker_sig = f.sign(&f.client.hash_rfq_order(&order));

    let r = f.client.fill_rfq_order(
        &order,
        &maker_sig,
        &Some(taker_sig.clone()),
        &relayer,
        &400_000,
    );
    assert_eq!(r.taker_filled, 400_000);
    assert_eq!(f.usd_of(&f.taker), 388_000);

    let request_hash = f.client.hash_request(&order.request());
    assert_eq!(f.client.request_filled_amount(&request_hash), 400_000);

    let err = f
        .client
        .try_fill_rfq_order(&order, &maker_sig, &None, &relayer, &1_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::SignerNotAuthorized.into());
}

#[test]
fn only_the_named_sender_may_submit() {
    let f = setup();
    let relayer = Address::generate(&f.env);

    let mut order = f.rfq();
    order.taker = Some(f.taker.clone());
    order.sender = Some(relayer.clone());
    let taker_sig = Some(f.sign_as(&f.taker_key, &f.client.hash_request(&order.request())));
    let maker_sig = f.sign(&f.client.hash_rfq_order(&order));

    let err = f
        .client
        .try_fill_rfq_order(&order, &maker_sig, &taker_sig, &f.taker, &1_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::WrongSender.into());

    assert_eq!(
        f.client
            .fill_rfq_order(&order, &maker_sig, &taker_sig, &relayer, &1_000)
            .taker_filled,
        1_000
    );
}

#[test]
fn a_fill_needs_the_takers_authorisation() {
    let f = setup();
    let order = f.rfq();
    let sig = f.sign(&f.client.hash_rfq_order(&order));

    f.env.set_auths(&[]);
    assert!(f
        .client
        .try_fill_rfq_order(&order, &sig, &None, &f.taker, &1_000)
        .is_err());
    assert_eq!(f.usd_of(&f.taker), 0);
    assert_eq!(f.client.filled_amount(&f.client.hash_rfq_order(&order)), 0);

    f.env.mock_all_auths();
    let r = f
        .client
        .fill_rfq_order(&order, &sig, &None, &f.taker, &1_000);
    assert_eq!(r.taker_filled, 1_000);
}
