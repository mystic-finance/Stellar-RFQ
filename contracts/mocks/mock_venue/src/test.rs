use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::{StellarAssetClient, TokenClient};

struct Fixture {
    env: Env,
    venue: MockVenueClient<'static>,
    owner: Address,
    token: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let operator = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let id = env.register(MockVenue, (owner.clone(), token.clone(), operator));
    Fixture {
        venue: MockVenueClient::new(&env, &id),
        env,
        owner,
        token,
    }
}

impl Fixture {
    fn fund(&self, amount: i128) {
        StellarAssetClient::new(&self.env, &self.token).mint(&self.venue.address, &amount);
    }
}

#[test]
fn the_position_is_the_balance_and_yield_accrues_to_it() {
    let f = setup();
    f.fund(1_000);
    assert_eq!(f.venue.deposit(&1_000), 1_000);
    assert_eq!(f.venue.total_assets(), 1_000);

    // Yield arrives as tokens sent to the venue.
    f.fund(50);
    assert_eq!(f.venue.total_assets(), 1_050);
    assert_eq!(f.venue.max_withdraw(), 1_050);
}

#[test]
fn a_liquidity_cap_limits_withdrawals_but_not_value() {
    let f = setup();
    f.fund(1_000);
    f.venue.set_liquidity(&Some(300));

    // Worth 1_000, only 300 pullable right now.
    assert_eq!(f.venue.total_assets(), 1_000);
    assert_eq!(f.venue.max_withdraw(), 300);

    let to = Address::generate(&f.env);
    assert_eq!(f.venue.withdraw(&500, &to), 300);
    assert_eq!(TokenClient::new(&f.env, &f.token).balance(&to), 300);

    f.venue.set_liquidity(&None);
    assert_eq!(f.venue.max_withdraw(), 700);
}

#[test]
fn only_the_owner_can_move_funds() {
    let f = setup();
    f.fund(1_000);
    f.env.set_auths(&[]);
    assert!(f
        .venue
        .try_withdraw(&100, &Address::generate(&f.env))
        .is_err());
    let _ = &f.owner;
}
