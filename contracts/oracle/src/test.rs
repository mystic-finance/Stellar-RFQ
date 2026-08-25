use super::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{contract, contractimpl, Env};

/// Minimal SEP-40 feed: 14 decimals, prices keyed by asset.
#[contract]
pub struct MockFeed;

#[contractimpl]
impl MockFeed {
    pub fn set(env: Env, asset: Asset, price: i128, timestamp: u64) {
        env.storage().persistent().set(&asset, &(price, timestamp));
    }

    pub fn decimals(_env: Env) -> u32 {
        14
    }

    pub fn lastprice(env: Env, asset: Asset) -> Option<PriceData> {
        env.storage()
            .persistent()
            .get::<Asset, (i128, u64)>(&asset)
            .map(|(price, timestamp)| PriceData { price, timestamp })
    }
}

const E14: i128 = 100_000_000_000_000;
const ONE: i128 = 1_000_000_000_000_000_000;

struct Fixture {
    env: Env,
    feed: MockFeedClient<'static>,
    base: Address,
    quote: Address,
    admin: Address,
    source: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let source = env.register(MockFeed, ());
    Fixture {
        feed: MockFeedClient::new(&env, &source),
        base: Address::generate(&env),
        quote: Address::generate(&env),
        admin: Address::generate(&env),
        source,
        env,
    }
}

impl Fixture {
    fn deploy(&self, cross: Option<Asset>, invert: bool) -> OctarineOracleClient<'static> {
        let id = self.env.register(OctarineOracle, ());
        let client = OctarineOracleClient::new(&self.env, &id);
        client.initialize(
            &self.admin,
            &Config {
                source: self.source.clone(),
                base: self.base.clone(),
                quote: self.quote.clone(),
                base_asset: Asset::Stellar(self.base.clone()),
                quote_asset: cross.clone().unwrap_or(Asset::Stellar(self.base.clone())),
                cross: cross.is_some(),
                base_decimals: 7,
                quote_decimals: 7,
                max_age: 3_600,
                invert,
            },
        );
        client
    }
}

#[test]
fn direct_feed_normalises_to_1e18() {
    let f = setup();
    let now = f.env.ledger().timestamp();
    // 1 base = 1.5 quote, same token decimals.
    f.feed
        .set(&Asset::Stellar(f.base.clone()), &(E14 * 3 / 2), &now);

    let p = f.deploy(None, false).get_price(&f.base, &f.quote);
    assert_eq!(p.price, ONE * 3 / 2);
    assert_eq!(p.timestamp, now);
}

#[test]
fn differing_token_decimals_shift_the_scale() {
    let f = setup();
    let now = f.env.ledger().timestamp();
    f.feed.set(&Asset::Stellar(f.base.clone()), &E14, &now);

    let id = f.env.register(OctarineOracle, ());
    let client = OctarineOracleClient::new(&f.env, &id);
    client.initialize(
        &f.admin,
        &Config {
            source: f.source.clone(),
            base: f.base.clone(),
            quote: f.quote.clone(),
            base_asset: Asset::Stellar(f.base.clone()),
            quote_asset: Asset::Stellar(f.base.clone()),
            cross: false,
            base_decimals: 7,
            quote_decimals: 6,
            max_age: 3_600,
            invert: false,
        },
    );
    // One raw base unit buys a tenth of a raw quote unit.
    assert_eq!(client.get_price(&f.base, &f.quote).price, ONE / 10);
}

#[test]
fn cross_rate_divides_two_legs_and_takes_the_older_stamp() {
    let f = setup();
    let now = f.env.ledger().timestamp();
    f.feed.set(&Asset::Stellar(f.base.clone()), &(E14 * 4), &now);
    f.feed
        .set(&Asset::Stellar(f.quote.clone()), &(E14 * 2), &(now - 100));

    let p = f
        .deploy(Some(Asset::Stellar(f.quote.clone())), false)
        .get_price(&f.base, &f.quote);
    assert_eq!(p.price, ONE * 2);
    assert_eq!(p.timestamp, now - 100);
}

#[test]
fn invert_flips_a_reversed_feed() {
    let f = setup();
    let now = f.env.ledger().timestamp();
    f.feed.set(&Asset::Stellar(f.base.clone()), &(E14 * 4), &now);
    assert_eq!(
        f.deploy(None, true).get_price(&f.base, &f.quote).price,
        ONE / 4
    );
}

#[test]
fn stale_or_unknown_prices_and_wrong_pairs_trap() {
    let f = setup();
    let now = f.env.ledger().timestamp();
    let client = f.deploy(None, false);

    assert!(client.try_get_price(&f.base, &f.quote).is_err());

    f.feed
        .set(&Asset::Stellar(f.base.clone()), &E14, &(now - 3_601));
    assert!(client.try_get_price(&f.base, &f.quote).is_err());

    f.feed.set(&Asset::Stellar(f.base.clone()), &E14, &now);
    assert!(client.get_price(&f.base, &f.quote).price > 0);
    assert!(client
        .try_get_price(&f.base, &Address::generate(&f.env))
        .is_err());
}
