extern crate std;

use super::*;
use call_verifier::{CallVerifierContract, CallVerifierContractClient, Rule};
use mock_oracle::{MockOracle, MockOracleClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::vec;

const CT: &[u8] = include_bytes!("../../../target/wasm32v1-none/release/claim_token.wasm");
const ACCOUNT: &[u8] =
    include_bytes!("../../../target/wasm32v1-none/release/redemption_account.wasm");

mod issuer {
    use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Env};

    /// Pulls RWA on `request` with the account's allowance; pays on `pay`.
    #[contract]
    pub struct Issuer;

    #[contractimpl]
    impl Issuer {
        pub fn __constructor(env: Env, rwa: Address, stable: Address) {
            env.storage().instance().set(&symbol_short!("rwa"), &rwa);
            env.storage()
                .instance()
                .set(&symbol_short!("stable"), &stable);
        }

        pub fn request(env: Env, account: Address, amount: i128) {
            account.require_auth();
            let me = env.current_contract_address();
            Self::token(&env, "rwa").transfer_from(&me, &account, &me, &amount);
        }

        pub fn pay(env: Env, account: Address, amount: i128) {
            let me = env.current_contract_address();
            Self::token(&env, "stable").transfer(&me, &account, &amount);
        }

        pub fn give_back(env: Env, account: Address, amount: i128) {
            let me = env.current_contract_address();
            Self::token(&env, "rwa").transfer(&me, &account, &amount);
        }

        fn token<'a>(env: &Env, k: &str) -> token::Client<'a> {
            let a: Address = env
                .storage()
                .instance()
                .get(&soroban_sdk::Symbol::new(env, k))
                .unwrap();
            token::Client::new(env, &a)
        }
    }
}

mod gate {
    use soroban_sdk::{contract, contractimpl, Address, Env};

    #[contract]
    pub struct Gate;

    #[contractimpl]
    impl Gate {
        pub fn allow(env: Env, account: Address) {
            env.storage().persistent().set(&account, &true);
        }

        pub fn can_hold(env: Env, _rwa: Address, account: Address) -> bool {
            env.storage().persistent().has(&account)
        }
    }
}

use gate::{Gate, GateClient};
use issuer::{Issuer, IssuerClient};

fn e(err: Error) -> soroban_sdk::Error {
    soroban_sdk::Error::from_contract_error(err as u32)
}

struct F {
    env: Env,
    v: RedemptionVaultClient<'static>,
    rwa: Address,
    stable: Address,
    issuer: IssuerClient<'static>,
    verifier: CallVerifierContractClient<'static>,
    oracle: MockOracleClient<'static>,
    roles: Roles,
    fees: Address,
    desk: Address,
    alice: Address,
    bob: Address,
}

fn settings(
    env: &Env,
    mode: Mode,
    verifier: &Address,
    oracle: &Address,
    fees: &Address,
    desk: &Address,
) -> Settings {
    Settings {
        mode,
        strike_at_send: false,
        verifier: Some(verifier.clone()),
        gate: None,
        oracle: Some(oracle.clone()),
        max_age: 3_600,
        fee_bps: 100,
        fee_recipient: Some(fees.clone()),
        floor_bps: 100,
        destinations: vec![env, desk.clone()],
    }
}

fn setup(mode: Mode, delay: u64) -> F {
    let env = Env::default();
    env.mock_all_auths();
    env.cost_estimate().budget().reset_unlimited();
    env.ledger().set_timestamp(1_000_000);
    let [admin, guardian, settler, operator, fees, desk, alice, bob] =
        [0; 8].map(|_| Address::generate(&env));
    let sac = |e: &Env| {
        e.register_stellar_asset_contract_v2(Address::generate(e))
            .address()
    };
    let (rwa, stable) = (sac(&env), sac(&env));
    let issuer = IssuerClient::new(&env, &env.register(Issuer, (rwa.clone(), stable.clone())));
    let verifier = CallVerifierContractClient::new(
        &env,
        &env.register(
            CallVerifierContract,
            (admin.clone(), guardian.clone(), 0u64),
        ),
    );
    let oracle = MockOracleClient::new(&env, &env.register(MockOracle, (admin.clone(), ONE)));
    let init = Init {
        rwa: rwa.clone(),
        stable: stable.clone(),
        ct_wasm: env.deployer().upload_contract_wasm(CT),
        account_wasm: env.deployer().upload_contract_wasm(ACCOUNT),
    };
    let roles = Roles {
        admin,
        guardian,
        settler,
        operator: operator.clone(),
    };
    let s = settings(&env, mode, &verifier.address, &oracle.address, &fees, &desk);
    let v = RedemptionVaultClient::new(
        &env,
        &env.register(RedemptionVault, (init, roles.clone(), s, delay)),
    );
    for (f, a) in [("request", 0b01), ("pay", 0b01), ("give_back", 0b01)] {
        verifier.add_rule(&Rule {
            caller: v.address.clone(),
            target: issuer.address.clone(),
            func: Symbol::new(&env, f),
            template: vec![&env, ().into_val(&env), 0i128.into_val(&env)],
            free: 0b10,
            account: a,
        });
    }
    for who in [&alice, &bob, &operator] {
        StellarAssetClient::new(&env, &rwa).mint(who, &1_000);
    }
    for who in [&issuer.address, &operator] {
        StellarAssetClient::new(&env, &stable).mint(who, &10_000);
    }
    F {
        env,
        v,
        rwa,
        stable,
        issuer,
        verifier,
        oracle,
        roles,
        fees,
        desk,
        alice,
        bob,
    }
}

impl F {
    fn bal(&self, token: &Address, who: &Address) -> i128 {
        TokenClient::new(&self.env, token).balance(who)
    }

    fn ct(&self, id: u64) -> TokenClient<'static> {
        TokenClient::new(&self.env, &self.v.batch(&id).unwrap().ct)
    }

    fn account(&self) -> Address {
        self.v.accounts().get(0).unwrap()
    }

    fn call(&self, func: &str, account: &Address, amount: i128) -> Call {
        Call {
            target: self.issuer.address.clone(),
            func: Symbol::new(&self.env, func),
            args: vec![
                &self.env,
                account.into_val(&self.env),
                amount.into_val(&self.env),
            ],
        }
    }

    fn calls(&self, func: &str, amount: i128) -> Vec<Call> {
        vec![&self.env, self.call(func, &self.account(), amount)]
    }

    /// Alice requests 300 and Bob 400 into one batch.
    fn requested(&self) -> u64 {
        let id = self.v.request_redeem(&self.alice, &300, &self.alice);
        assert_eq!(
            self.v.request_redeem(&self.bob, &400, &self.bob),
            id,
            "one open batch"
        );
        id
    }

    fn sent(&self) -> u64 {
        let id = self.requested();
        self.v.redeem_batch(
            &id,
            &Some(self.issuer.address.clone()),
            &self.calls("request", 700),
        );
        id
    }

    fn wait(&self, seconds: u64) {
        let t = self.env.ledger().timestamp();
        self.env.ledger().set_timestamp(t + seconds);
    }
}

#[test]
fn requests_mint_a_tradeable_claim_token_per_batch() {
    let f = setup(Mode::Auto, 0);
    let id = f.requested();
    let ct = f.ct(id);
    let sym = TokenClient::new(&f.env, &f.rwa).symbol();
    let mut b = [0u8; 24];
    sym.copy_into_slice(&mut b[..sym.len() as usize]);
    let want = std::format!(
        "ct{}-1",
        std::str::from_utf8(&b[..sym.len() as usize]).unwrap()
    );
    assert_eq!(ct.symbol(), String::from_str(&f.env, &want));
    assert_eq!(ct.decimals(), 7, "the RWA's decimals");
    assert_eq!((ct.balance(&f.alice), ct.balance(&f.bob)), (300, 400));
    ct.transfer(&f.alice, &f.bob, &100);
    assert_eq!(ct.balance(&f.bob), 500, "a CT trades like any token");
    assert_eq!(f.v.totals().pooled, 700);
    f.v.cancel(&f.alice, &id, &50, &f.alice);
    assert_eq!(
        f.bal(&f.rwa, &f.alice),
        750,
        "cancel returns the RWA while open"
    );
}

#[test]
fn an_auto_batch_leaves_from_a_redemption_account() {
    let f = setup(Mode::Auto, 0);
    let id = f.sent();
    let b = f.v.batch(&id).unwrap();
    assert_eq!((b.status, b.account), (Status::Sent, Some(f.account())));
    assert_eq!(f.bal(&f.rwa, &f.issuer.address), 700);
    assert_eq!(f.v.open_batch(), None, "the next request opens a new batch");
    assert_eq!(f.v.request_redeem(&f.alice, &10, &f.alice), id + 1);
}

#[test]
fn an_auto_batch_settles_with_what_arrives_and_pays_holders_pro_rata() {
    let f = setup(Mode::Auto, 0);
    let id = f.sent();
    f.v.settle(&id, &f.calls("pay", 701), &700);
    let b = f.v.batch(&id).unwrap();
    assert_eq!((b.proceeds, b.fee, b.remaining), (701, 7, 694));
    assert_eq!(f.bal(&f.stable, &f.fees), 7);
    assert_eq!(f.v.claim(&f.alice, &id, &300, &f.alice), 297);
    assert_eq!(
        f.v.claim(&f.bob, &id, &400, &f.bob),
        397,
        "the last holder takes the dust"
    );
    assert_eq!(f.ct(id).balance(&f.alice), 0);
    assert_eq!(f.v.totals().proceeds, 0);
}

#[test]
fn auto_calls_must_be_verified_and_take_the_whole_batch() {
    let f = setup(Mode::Auto, 0);
    let id = f.requested();
    let issuer = Some(f.issuer.address.clone());
    let bad = vec![
        &f.env,
        Call {
            target: f.stable.clone(),
            ..f.call("request", &f.account(), 700)
        },
    ];
    assert_eq!(
        f.v.try_redeem_batch(&id, &issuer, &bad),
        Err(Ok(e(Error::CallNotVerified)))
    );
    let other = Address::generate(&f.env);
    let elsewhere = vec![&f.env, f.call("request", &other, 700)];
    assert_eq!(
        f.v.try_redeem_batch(&id, &issuer, &elsewhere),
        Err(Ok(e(Error::CallNotVerified))),
        "the account argument must be the account itself"
    );
    assert_eq!(
        f.v.try_redeem_batch(&id, &issuer, &f.calls("request", 600)),
        Err(Ok(e(Error::IssuerTookWrongAmount)))
    );
    let id_rule = f
        .verifier
        .ids(
            &f.v.address,
            &f.issuer.address,
            &Symbol::new(&f.env, "request"),
        )
        .get(0)
        .unwrap();
    f.verifier.set_enabled(&f.roles.guardian, &id_rule, &false);
    assert_eq!(
        f.v.try_redeem_batch(&id, &issuer, &f.calls("request", 700)),
        Err(Ok(e(Error::CallNotVerified))),
        "the guardian revokes a rule at once"
    );
}

#[test]
fn auto_reopen_resend_and_return_in_kind() {
    let f = setup(Mode::Auto, 0);
    let id = f.sent();
    f.v.reopen(&id, &f.calls("give_back", 200), &200);
    assert_eq!(f.v.batch(&id).unwrap().unsent, 200);
    f.v.redeem_batch(
        &id,
        &Some(f.issuer.address.clone()),
        &f.calls("request", 200),
    );
    assert_eq!(f.v.batch(&id).unwrap().unsent, 0, "resent");

    f.v.reopen(&id, &f.calls("give_back", 100), &100);
    f.v.settle(&id, &f.calls("pay", 600), &0);
    assert_eq!(
        f.v.totals().rwa,
        100,
        "the unsent 100 is owed to holders in kind"
    );
    f.v.claim(&f.bob, &id, &400, &f.bob);
    assert_eq!(f.bal(&f.rwa, &f.bob), 600 + 400 * 100 / 700);

    let f = setup(Mode::Auto, 0);
    let id = f.sent();
    f.v.reopen(&id, &f.calls("give_back", 700), &700);
    let b = f.v.batch(&id).unwrap();
    assert_eq!(
        (b.status, b.account),
        (Status::Open, None),
        "all back: open again"
    );
    assert_eq!(
        f.v.totals(),
        Totals {
            pooled: 700,
            ..Totals::default()
        }
    );
    assert_eq!(f.bal(&f.rwa, &f.v.address), 700);
}

#[test]
fn every_account_busy_blocks_the_next_send() {
    let f = setup(Mode::Auto, 0);
    f.sent();
    let id = f.v.request_redeem(&f.alice, &10, &f.alice);
    let calls = vec![&f.env, f.call("request", &f.v.account_address(&1), 10)];
    assert_eq!(
        f.v.try_redeem_batch(&id, &Some(f.issuer.address.clone()), &calls),
        Err(Ok(e(Error::NoFreeAccount)))
    );
    f.v.add_accounts(&1);
    assert_eq!(
        f.v.accounts().get(1),
        Some(f.v.account_address(&1)),
        "deployed where predicted"
    );
    f.v.redeem_batch(&id, &Some(f.issuer.address.clone()), &calls);
}

#[test]
fn manual_batch_goes_to_an_allowed_desk_and_settles_above_the_floor() {
    let f = setup(Mode::Manual, 0);
    let id = f.requested();
    let operator = f.roles.operator.clone();
    let stranger = Address::generate(&f.env);
    assert_eq!(
        f.v.try_withdraw_batch(&id, &stranger),
        Err(Ok(e(Error::NotAllowed)))
    );
    assert_eq!(
        f.v.try_redeem_batch(&id, &None, &f.calls("request", 700)),
        Err(Ok(e(Error::WrongMode)))
    );
    f.v.withdraw_batch(&id, &f.desk);
    assert_eq!(f.bal(&f.rwa, &f.desk), 700);

    // Settles at the NAV then: 0.95, floor 1% under it.
    f.oracle.set_price(&(ONE * 95 / 100), &None);
    assert_eq!(
        f.v.try_settle_manual(&id, &658, &0),
        Err(Ok(e(Error::ProceedsTooLow)))
    );
    f.v.settle_manual(&id, &659, &0);
    assert_eq!(f.bal(&f.stable, &operator), 10_000 - 659);
    assert_eq!(
        f.v.claim(&f.alice, &id, &300, &f.alice),
        279,
        "300/700 of 659 less the fee of 6"
    );
    assert_eq!(
        f.v.try_settle(&id, &Vec::new(&f.env), &0),
        Err(Ok(e(Error::WrongStatus)))
    );
}

#[test]
fn a_struck_batch_is_held_to_its_strike_and_a_stale_nav_blocks_settlement() {
    let f = setup(Mode::Manual, 0);
    let mut s = f.v.settings();
    s.strike_at_send = true;
    f.v.submit(&Change::Settings(s));
    f.v.accept(&0);
    let id = f.requested();
    f.v.withdraw_batch(&id, &f.desk);
    assert_eq!(f.v.batch(&id).unwrap().strike, ONE);
    f.oracle.set_price(&(ONE / 2), &None);
    assert_eq!(
        f.v.try_settle_manual(&id, &593, &100),
        Err(Ok(e(Error::ProceedsTooLow))),
        "struck at 1.00 though the NAV fell: 600 units owe at least 594"
    );
    f.v.settle_manual(&id, &594, &100);
    assert_eq!(f.v.batch(&id).unwrap().unsent, 100);

    let f = setup(Mode::Manual, 0);
    let id = f.requested();
    f.v.withdraw_batch(&id, &f.desk);
    f.oracle.set_price(&ONE, &Some(1));
    assert_eq!(
        f.v.try_settle_manual(&id, &1_000, &0),
        Err(Ok(e(Error::NoPrice)))
    );
}

#[test]
fn governance_waits_out_the_timelock_and_mode_needs_an_empty_issuer() {
    let f = setup(Mode::Auto, 100);
    f.sent();
    let mut s = f.v.settings();
    s.mode = Mode::Manual;
    f.v.submit(&Change::Settings(s.clone()));
    let not_ready = Err(Ok(soroban_sdk::Error::from_contract_error(111)));
    assert_eq!(f.v.try_accept(&0), not_ready);
    f.wait(100);
    assert_eq!(f.v.try_accept(&0), Err(Ok(e(Error::BatchesInFlight))));

    let mut fee = f.v.settings();
    fee.fee_bps = 2_001;
    assert_eq!(
        f.v.try_submit(&Change::Settings(fee)),
        Err(Ok(e(Error::InvalidSettings)))
    );
    f.v.submit(&Change::Skim(f.stable.clone(), f.fees.clone()));
    f.v.revoke(&2, &f.roles.guardian);
    assert_eq!(f.v.pending(&2), None, "the guardian revokes");
}

#[test]
fn a_gate_binds_the_open_claim_token_and_pause_stops_requests() {
    let f = setup(Mode::Auto, 0);
    let id = f.requested();
    let g = GateClient::new(&f.env, &f.env.register(Gate, ()));
    g.allow(&f.bob);
    let mut s = f.v.settings();
    s.gate = Some(g.address.clone());
    f.v.submit(&Change::Settings(s));
    f.v.accept(&0);
    let carol = Address::generate(&f.env);
    assert!(f.ct(id).try_transfer(&f.alice, &carol, &1).is_err());
    f.ct(id).transfer(&f.alice, &f.bob, &1);

    f.v.set_paused(&f.roles.guardian, &true);
    assert_eq!(
        f.v.try_request_redeem(&f.alice, &1, &f.alice),
        Err(Ok(e(Error::Paused)))
    );
}

#[test]
fn the_vault_prices_its_own_claim_tokens() {
    let f = setup(Mode::Auto, 0);
    let id = f.sent();
    let ct = f.v.batch(&id).unwrap().ct;
    assert_eq!(
        f.v.get_price(&ct, &f.stable).price,
        ONE * 99 / 100,
        "NAV less the 1% fee"
    );
    assert_eq!(
        f.v.get_price(&f.rwa, &f.stable).price,
        0,
        "not one of its CTs"
    );
    f.v.settle(&id, &f.calls("pay", 1_400), &0);
    assert_eq!(
        f.v.get_price(&ct, &f.stable).price,
        ONE * 1_386 / 700,
        "settled: cash per CT"
    );
}
