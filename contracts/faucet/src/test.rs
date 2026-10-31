use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::IntoVal;

struct Fixture {
    env: Env,
    faucet: FaucetClient<'static>,
    admin: Address,
    a: Address,
    b: Address,
}

fn token(env: &Env) -> Address {
    env.register_stellar_asset_contract_v2(Address::generate(env))
        .address()
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(Faucet, (admin.clone(), 3_600u64));
    let faucet = FaucetClient::new(&env, &id);
    let (a, b) = (token(&env), token(&env));
    faucet.set_drip(&a, &1_000);
    faucet.set_drip(&b, &50);
    Fixture {
        env,
        faucet,
        admin,
        a,
        b,
    }
}

impl Fixture {
    fn stock(&self, t: &Address, amount: i128) {
        StellarAssetClient::new(&self.env, t).mint(&self.faucet.address, &amount);
    }

    fn bal(&self, t: &Address, who: &Address) -> i128 {
        TokenClient::new(&self.env, t).balance(who)
    }

    fn warp(&self, secs: u64) {
        self.env
            .ledger()
            .with_mut(|l| l.timestamp = l.timestamp.saturating_add(secs));
    }
}

#[test]
fn claim_sends_one_drip_of_each_token() {
    let f = setup();
    f.stock(&f.a, 10_000);
    f.stock(&f.b, 10_000);
    let user = Address::generate(&f.env);

    let sent = f.faucet.claim(&user);
    assert_eq!(sent, vec![&f.env, (f.a.clone(), 1_000), (f.b.clone(), 50)]);
    assert_eq!(f.bal(&f.a, &user), 1_000);
    assert_eq!(f.bal(&f.b, &user), 50);
    assert_eq!(f.bal(&f.a, &f.faucet.address), 9_000);
    assert_eq!(f.faucet.time_until_next_claim(&user), 3_600);
}

#[test]
fn claim_needs_the_claimants_auth() {
    let f = setup();
    f.stock(&f.a, 10_000);
    let user = Address::generate(&f.env);
    f.faucet.claim(&user);
    let auths = f.env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, user);
}

#[test]
fn a_second_claim_waits_for_the_cooldown() {
    let f = setup();
    f.stock(&f.a, 10_000);
    let user = Address::generate(&f.env);
    f.faucet.claim(&user);

    f.warp(3_599);
    assert_eq!(f.faucet.time_until_next_claim(&user), 1);
    assert_eq!(f.faucet.try_claim(&user), Err(Ok(Error::Cooldown.into())));

    // Other addresses are not affected.
    let other = Address::generate(&f.env);
    assert_eq!(f.faucet.time_until_next_claim(&other), 0);
    f.faucet.claim(&other);

    f.warp(1);
    assert_eq!(f.faucet.time_until_next_claim(&user), 0);
    f.faucet.claim(&user);
    assert_eq!(f.bal(&f.a, &user), 2_000);
}

#[test]
fn a_token_short_of_its_drip_is_skipped() {
    let f = setup();
    f.stock(&f.a, 10_000);
    f.stock(&f.b, 49);
    let user = Address::generate(&f.env);

    assert_eq!(f.faucet.claim(&user), vec![&f.env, (f.a.clone(), 1_000)]);
    assert_eq!(f.bal(&f.b, &user), 0);
    assert_eq!(f.bal(&f.b, &f.faucet.address), 49);

    // Nothing in stock: the claim still succeeds and starts the cooldown.
    let empty = setup();
    let user = Address::generate(&empty.env);
    assert_eq!(empty.faucet.claim(&user), Vec::new(&empty.env));
    assert_eq!(empty.faucet.time_until_next_claim(&user), 3_600);
}

#[test]
fn admin_setters_need_the_admins_auth() {
    let f = setup();
    let c = token(&f.env);
    f.faucet.set_drip(&c, &7);
    assert_eq!(
        f.env.auths()[0].1.function,
        soroban_sdk::testutils::AuthorizedFunction::Contract((
            f.faucet.address.clone(),
            soroban_sdk::Symbol::new(&f.env, "set_drip"),
            (c.clone(), 7i128).into_val(&f.env),
        ))
    );
    assert_eq!(f.env.auths()[0].0, f.admin);

    f.faucet.set_cooldown(&60);
    assert_eq!(f.env.auths()[0].0, f.admin);
    assert_eq!(f.faucet.cooldown(), 60);
    assert_eq!(f.faucet.admin(), f.admin);

    // Without the admin's signature both setters fail.
    f.env.set_auths(&[]);
    assert!(f.faucet.try_set_drip(&c, &8).is_err());
    assert!(f.faucet.try_set_cooldown(&1).is_err());
}

#[test]
fn set_drip_updates_in_place_and_zero_removes() {
    let f = setup();
    let c = token(&f.env);
    f.faucet.set_drip(&c, &7);
    f.faucet.set_drip(&f.a, &2_000);
    assert_eq!(
        f.faucet.drips(),
        vec![
            &f.env,
            (f.a.clone(), 2_000),
            (f.b.clone(), 50),
            (c.clone(), 7)
        ]
    );

    f.faucet.set_drip(&f.b, &0);
    assert_eq!(
        f.faucet.drips(),
        vec![&f.env, (f.a.clone(), 2_000), (c.clone(), 7)]
    );

    // Removing an unlisted token is a no-op; negative drips are rejected.
    f.faucet.set_drip(&f.b, &0);
    assert_eq!(f.faucet.drips().len(), 2);
    assert_eq!(
        f.faucet.try_set_drip(&c, &-1),
        Err(Ok(Error::InvalidAmount.into()))
    );

    // A removed token is no longer handed out.
    f.stock(&f.b, 10_000);
    let user = Address::generate(&f.env);
    f.faucet.claim(&user);
    assert_eq!(f.bal(&f.b, &user), 0);
}
