use super::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};

const ONE: i128 = 1_000_000_000_000_000_000;

struct F {
    env: Env,
    o: PushOracleClient<'static>,
    admin: Address,
    keeper: Address,
    base: Address,
    quote: Address,
}

fn setup() -> F {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let [admin, keeper, base, quote] = [0; 4].map(|_| Address::generate(&env));
    let o = PushOracleClient::new(&env, &env.register(PushOracle, (admin.clone(), 200u32)));
    o.set_updater(&keeper, &true);
    F {
        env,
        o,
        admin,
        keeper,
        base,
        quote,
    }
}

impl F {
    fn push(&self, who: &Address, price: i128, at: u64) -> Result<(), Error> {
        match self.o.try_push(who, &self.base, &self.quote, &price, &at) {
            Ok(Ok(())) => Ok(()),
            Err(Ok(e)) => Err(e.try_into().unwrap()),
            _ => Err(Error::NoPrice),
        }
    }
}

#[test]
fn the_last_push_is_the_price_with_its_source_time() {
    let f = setup();
    assert!(
        f.o.try_get_price(&f.base, &f.quote).is_err(),
        "no price yet"
    );
    f.push(&f.keeper, ONE, 999_000).unwrap();
    assert_eq!(
        f.o.get_price(&f.base, &f.quote),
        PriceData {
            price: ONE,
            timestamp: 999_000
        }
    );
    assert!(
        f.o.try_get_price(&f.quote, &f.base).is_err(),
        "pairs are directional"
    );
}

#[test]
fn only_updaters_and_the_admin_push() {
    let f = setup();
    let stranger = Address::generate(&f.env);
    assert_eq!(f.push(&stranger, ONE, 999_000), Err(Error::NotUpdater));
    f.o.set_updater(&f.keeper, &false);
    assert!(!f.o.is_updater(&f.keeper));
    assert_eq!(f.push(&f.keeper, ONE, 999_000), Err(Error::NotUpdater));
    f.push(&f.admin, ONE, 999_000).unwrap();
    f.env.set_auths(&[]);
    assert!(f.o.try_set_updater(&stranger, &true).is_err());
    assert!(f.o.try_set_max_deviation(&0).is_err());
}

#[test]
fn pushes_move_forward_in_time_within_the_deviation_bound() {
    let f = setup();
    f.push(&f.keeper, ONE, 999_000).unwrap();
    assert_eq!(
        f.push(&f.keeper, ONE, 999_000),
        Err(Error::StaleOrFuture),
        "not newer"
    );
    assert_eq!(
        f.push(&f.keeper, ONE, 1_000_001),
        Err(Error::StaleOrFuture),
        "in the future"
    );
    assert_eq!(f.push(&f.keeper, 0, 999_500), Err(Error::InvalidPrice));
    // 2% bound: +2% passes, +2.01% does not.
    f.push(&f.keeper, ONE * 102 / 100, 999_500).unwrap();
    assert_eq!(
        f.push(&f.keeper, ONE * 102 / 100 * 10_201 / 10_000, 999_600),
        Err(Error::DeviationTooLarge)
    );
    // The admin can correct past it, and lift it.
    f.push(&f.admin, ONE / 2, 999_700).unwrap();
    f.o.set_max_deviation(&0);
    f.push(&f.keeper, ONE * 3, 999_800).unwrap();
    assert_eq!(f.o.get_price(&f.base, &f.quote).price, ONE * 3);
}
