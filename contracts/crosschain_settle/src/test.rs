#![allow(clippy::too_many_arguments)]
//! The core's tests, run once per transport (`test_lz.rs`, `test_cctp.rs`
//! with feature `cctp`), each of which doubles its bridge.
extern crate std;

use super::*;
use ed25519_dalek::{Signer, SigningKey};
use mock_oracle::MockOracle;
use redemption_vault::{
    Init as VaultInitArgs, Mode, RedemptionVault, RedemptionVaultClient, Roles, Settings,
};
use rfq::{RfqContract, RfqContractClient};
use soroban_sdk::testutils::{Address as _, IssuerFlags, Ledger as _};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{contract, contractimpl, Vec};
use verifier::Verifier as VerifierContract;

#[cfg(feature = "cctp")]
#[path = "test_cctp.rs"]
mod cctp;
#[cfg(feature = "cctp")]
use cctp::*;
#[cfg(not(feature = "cctp"))]
#[path = "test_lz.rs"]
mod lz;
#[cfg(not(feature = "cctp"))]
use lz::*;

const DAY: u64 = 86_400;
const ONE: i128 = 1_000_000_000_000_000_000;
const UNITS: i128 = 10_000_000_000; // 1,000 RWA at 7 decimals
const RETURN_FEE: i128 = 1_000;
const XLM: i128 = 1_000_000;
const CT: &[u8] = include_bytes!("../../../target/wasm32v1-none/release/claim_token.wasm");
const ACCOUNT: &[u8] =
    include_bytes!("../../../target/wasm32v1-none/release/redemption_account.wasm");

fn word_u(env: &Env, v: u128) -> BytesN<32> {
    wire::uint(env, v)
}

fn hex(env: &Env, s: &str) -> Bytes {
    let v: std::vec::Vec<u8> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect();
    Bytes::from_slice(env, &v)
}

/// What a `try_` call into the contract returns.
type Try = Result<
    Result<(), soroban_sdk::ConversionError>,
    Result<soroban_sdk::Error, soroban_sdk::InvokeError>,
>;

struct F {
    env: Env,
    settle: CrossChainSettleClient<'static>,
    rfq: RfqContractClient<'static>,
    vault: RedemptionVaultClient<'static>,
    mocks: Mocks,
    /// The one stable routed to `CHAIN`, `REMOTE` there.
    stable: Address,
    xlm: Address,
    rwa: Address,
    taker: Address,
    operator: Address,
    fee_to: Address,
    peer: BytesN<32>,
    key: SigningKey,
    maker: Address,
    nonce: core::cell::Cell<u8>,
}

fn sac(env: &Env, admin: &Address) -> Address {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    sac.issuer().set_flag(IssuerFlags::RevocableFlag);
    sac.address()
}

fn setup() -> F {
    let env = Env::default();
    env.mock_all_auths();
    env.cost_estimate().budget().reset_unlimited();
    env.ledger().set_timestamp(1_000_000);
    let [admin, operator, taker, fee_to] = [0; 4].map(|_| Address::generate(&env));
    let xlm = sac(&env, &admin);
    let rwa = sac(&env, &Address::generate(&env));
    let (mocks, stable, bridge) = mocks(&env, &xlm);

    let rfq = RfqContractClient::new(&env, &env.register(RfqContract, ()));
    rfq.initialize(&admin, &stable);
    rfq.set_config(&rfq::Config {
        min_fee_bps: 0,
        max_fee_bps: 1_000,
        fallback_max_age: 3_600,
        max_deviation_bps: 5_000,
        min_push_interval: 300,
        max_shift_seconds: 0,
        decay_seconds: (7 * DAY) as u32,
    });
    let schedule = rfq::Schedule {
        mode: rfq::ScheduleMode::Rolling,
        rolling_seconds: (30 * DAY) as u32,
        next_redemption_at: 0,
        cycle_seconds: 0,
        max_bps_per_day: 1_000,
    };
    rfq.set_schedule(&admin, &rwa, &schedule);
    rfq.push_price(&admin, &rwa, &ONE);

    let vault = redemption_vault(&env, &admin, &operator, &rwa, &stable);
    let config = Config {
        rfq: rfq.address.clone(),
        verifier: env.register(VerifierContract, ()),
        xlm: xlm.clone(),
        tip: TIP,
        bridge,
    };
    let settle = CrossChainSettleClient::new(
        &env,
        &env.register(CrossChainSettle, (admin.clone(), admin, 0u64, config)),
    );
    let peer = word_u(&env, 0xEE5E77);
    let peer_cfg = Peer {
        addr: peer.clone(),
        return_fee: RETURN_FEE,
        options: Bytes::new(&env),
    };
    for change in [
        Change::Peer(CHAIN, peer_cfg),
        Change::Route(CHAIN, stable.clone(), Some(word_u(&env, REMOTE))),
        Change::Vault(rwa.clone(), Some(vault.address.clone())),
    ] {
        govern(&settle, &change);
    }

    let key = SigningKey::from_bytes(&[11u8; 32]);
    // A contract maker (a treasury) whose hot key signs: accounts need trustlines here.
    let maker = Address::generate(&env);
    rfq.register_order_signer(
        &maker,
        &BytesN::from_array(&env, &key.verifying_key().to_bytes()),
        &true,
    );
    for (token, who, amount) in [
        (&rwa, &taker, 10 * UNITS),
        (&xlm, &taker, XLM),
        (&xlm, &settle.address, XLM),
        (&stable, &maker, 100 * UNITS),
    ] {
        StellarAssetClient::new(&env, token).mint(who, &amount);
    }
    TokenClient::new(&env, &stable).approve(&maker, &settle.address, &(100 * UNITS), &1_000_000);

    F {
        env,
        settle,
        rfq,
        vault,
        mocks,
        stable,
        xlm,
        rwa,
        taker,
        operator,
        fee_to,
        peer,
        key,
        maker,
        nonce: core::cell::Cell::new(0),
    }
}

/// A MANUAL redemption vault for `rwa` paid in `stable`, no fee, the operator its desk.
fn redemption_vault(
    env: &Env,
    admin: &Address,
    operator: &Address,
    rwa: &Address,
    stable: &Address,
) -> RedemptionVaultClient<'static> {
    let init = VaultInitArgs {
        rwa: rwa.clone(),
        stable: stable.clone(),
        ct_wasm: env.deployer().upload_contract_wasm(CT),
        account_wasm: env.deployer().upload_contract_wasm(ACCOUNT),
    };
    let roles = Roles {
        admin: admin.clone(),
        guardian: admin.clone(),
        settler: admin.clone(),
        operator: operator.clone(),
    };
    let settings = Settings {
        mode: Mode::Manual,
        strike_at_send: false,
        verifier: None,
        gate: None,
        oracle: Some(env.register(MockOracle, (admin.clone(), ONE))),
        max_age: 3_600,
        fee_bps: 0,
        fee_recipient: None,
        floor_bps: 100,
        destinations: soroban_sdk::vec![env, operator.clone()],
    };
    let v = env.register(RedemptionVault, (init, roles, settings, 0u64));
    RedemptionVaultClient::new(env, &v)
}

/// Submits and, with no delay, accepts `change`.
fn govern(settle: &CrossChainSettleClient, change: &Change) {
    settle.submit(change);
    settle.accept(&settle.kind_of(change));
}

impl F {
    fn request(&self) -> Request {
        Request {
            maker_token: self.stable.clone(),
            taker_token: self.rwa.clone(),
            taker_amount: UNITS,
            min_received_amount: 9_000_000_000,
            fee_bps: 10,
            taker: Some(self.taker.clone()),
            sender: Some(self.settle.address.clone()),
            fee_recipient: self.fee_to.clone(),
            expiry: self.env.ledger().timestamp() + DAY,
            salt: 1,
            taker_max_bps_per_day: 1_000,
            order_type: OrderType::Rfq,
        }
    }

    /// A remote maker's bid, paying in the routed stable there.
    fn bid(&self, delivery: u32, recipient: &Address) -> Bid {
        Bid {
            maker: word_u(&self.env, 0xA11CE),
            maker_is_contract: false,
            stable: word_u(&self.env, REMOTE),
            max_amount: 1_000_000_000,
            maker_bps_per_day: 250,
            delivery,
            recipient: wire::key(&self.env, recipient),
            recipient_is_contract: wire::is_contract(&self.env, recipient),
            deadline: self.env.ledger().timestamp() + 3_600,
            salt: word_u(&self.env, 7),
        }
    }

    /// The taker fills `bid` (a remote maker's), which the peer checks.
    fn fill(&self, bid: &Bid) -> BytesN<32> {
        let evm_sig = Bytes::from_array(&self.env, &[0xAA; 65]);
        self.settle.fill(&self.request(), &CHAIN, bid, &evm_sig)
    }

    /// The payment for a fill arrives, `amount_6dp` on the wire; `relayer`
    /// delivers it.
    fn pay_in(&self, amount_6dp: u128, payload: &Bytes) -> Address {
        let relayer = Address::generate(&self.env);
        self.try_pay_in(amount_6dp, payload, &relayer)
            .unwrap()
            .unwrap();
        relayer
    }

    /// A FILL from a remote taker for a bid this maker signed with `key`.
    fn fill_message(&self, key: &SigningKey, id: &BytesN<32>, amount: i128) -> Bytes {
        let mut bid = self.bid(0, &self.taker);
        bid.stable = wire::key(&self.env, &self.stable);
        bid.recipient = word_u(&self.env, 0x1B);
        bid.maker = wire::key(&self.env, &self.maker);
        bid.maker_is_contract = true;
        let hash = self.settle.hash_bid(id, &CHAIN, &bid);
        let mut msg = std::vec::Vec::from(&b"Stellar Signed Message:\n"[..]);
        msg.extend_from_slice(&hash.to_array());
        let digest = self
            .env
            .crypto()
            .sha256(&Bytes::from_slice(&self.env, &msg))
            .to_array();
        let mut sig = Bytes::from_array(&self.env, &key.verifying_key().to_bytes());
        sig.extend_from_array(&key.sign(&digest).to_bytes());
        wire::fill(&self.env, id, amount, &bid, &sig)
    }

    fn balance(&self, token: &Address, who: &Address) -> i128 {
        TokenClient::new(&self.env, token).balance(who)
    }

    fn last_sent(&self) -> Bytes {
        self.sent().pop().unwrap()
    }
}

// ---------------------------------------------------------------- taker here

#[test]
fn fill_escrows_the_rwa_and_sends_the_bid() {
    let f = setup();
    let lp = Address::generate(&f.env);
    let bid = f.bid(DELIVER, &lp);
    let id = f.fill(&bid);

    assert_eq!(id, f.rfq.hash_request(&f.request()));
    assert!(f.settle.filled(&id));
    assert_eq!(f.balance(&f.rwa, &f.settle.address), UNITS);
    assert_eq!(
        f.balance(&f.xlm, &f.taker),
        XLM - MSG_FEE - RETURN_FEE,
        "the taker pays both legs"
    );
    assert_eq!(f.settle.fill_fee(&CHAIN), MSG_FEE + RETURN_FEE);
    // 1:1 at 2.50 bps/day over 30 days is 0.75% off: 992.5 USDC, 6 decimals on the wire.
    let sig = Bytes::from_array(&f.env, &[0xAA; 65]);
    assert_eq!(
        f.last_sent(),
        wire::fill(&f.env, &id, 992_500_000, &bid, &sig)
    );
}

#[test]
fn the_payment_settles_and_tips_whoever_delivers_it() {
    let f = setup();
    let lp = Address::generate(&f.env);
    let id = f.fill(&f.bid(DELIVER, &lp));
    let relayer = f.pay_in(992_500_000, &wire::about(&f.env, PAID, &id));

    // A 9.925 USDC fee, and the taker pays the relayer the flat tip.
    assert_eq!(f.balance(&f.stable, &f.taker), 9_915_075_000 - TIP * 10);
    assert_eq!(f.balance(&f.stable, &f.fee_to), 9_925_000);
    assert_eq!(f.balance(&f.stable, &relayer), TIP * 10);
    assert_eq!(f.balance(&f.rwa, &lp), UNITS);
    assert_eq!(f.balance(&f.stable, &f.settle.address), 0);
    assert_eq!(f.settle.order(&id), None);
}

#[test]
fn fail_returns_the_rwa_and_frees_the_request() {
    let f = setup();
    let bid = f.bid(DELIVER, &Address::generate(&f.env));
    let id = f.fill(&bid);
    f.deliver(&wire::about(&f.env, FAIL, &id));

    assert_eq!(f.balance(&f.rwa, &f.taker), 10 * UNITS);
    assert!(!f.settle.filled(&id));
    f.fill(&bid);
    assert!(f.settle.filled(&id), "and it can be filled again");
}

#[test]
fn fill_fails_early_here() {
    let f = setup();
    let lp = Address::generate(&f.env);
    let sig = Bytes::from_array(&f.env, &[0xAA; 65]);
    let r = f.request();
    let err = |bid: &Bid, chain: u32| {
        f.settle
            .try_fill(&r, &chain, bid, &sig)
            .unwrap_err()
            .unwrap()
    };

    let mut unrouted = f.bid(DELIVER, &lp);
    unrouted.stable = word_u(&f.env, 0xBAD);
    assert_eq!(err(&unrouted, CHAIN), Error::NoRoute.into());
    let bid = f.bid(DELIVER, &lp);
    assert_eq!(err(&bid, 7), Error::NoRoute.into());
    let mut small = bid.clone();
    small.max_amount = 500_000_000;
    assert_eq!(err(&small, CHAIN), Error::QuoteRejected.into());
    let mut expired = bid.clone();
    expired.deadline = 1;
    assert_eq!(err(&expired, CHAIN), Error::Expired.into());
    let mut unknown = bid.clone();
    unknown.delivery = 4;
    assert_eq!(err(&unknown, CHAIN), Error::InvalidBid.into());

    f.fill(&bid);
    assert_eq!(err(&bid, CHAIN), Error::AlreadyFilled.into());
}

#[test]
fn redeem_ct_hands_the_recipient_the_claim() {
    let f = setup();
    let lp = Address::generate(&f.env);
    let id = f.fill(&f.bid(REDEEM_CT, &lp));
    f.pay_in(992_500_000, &wire::about(&f.env, PAID, &id));
    let batch = f.vault.batch(&f.vault.open_batch().unwrap()).unwrap();
    assert_eq!(TokenClient::new(&f.env, &batch.ct).balance(&lp), UNITS);
}

mod mock_gate {
    use super::*;

    #[contract]
    pub struct MockGate;

    #[contractimpl]
    impl MockGate {
        pub fn allow(env: Env, account: Address) {
            env.storage().persistent().set(&account, &true);
        }

        pub fn can_hold(env: Env, _rwa: Address, account: Address) -> bool {
            env.storage().persistent().has(&account)
        }
    }
}

#[test]
fn a_claim_token_the_recipient_cannot_hold_yet_is_owed_not_blocking() {
    let f = setup();
    let gate = mock_gate::MockGateClient::new(&f.env, &f.env.register(mock_gate::MockGate, ()));
    gate.allow(&f.settle.address);
    let mut s = f.vault.settings();
    s.gate = Some(gate.address.clone());
    f.vault.submit(&redemption_vault::Change::Settings(s));
    f.vault.accept(&0);

    let lp = Address::generate(&f.env);
    let id = f.fill(&f.bid(REDEEM_CT, &lp));
    f.pay_in(992_500_000, &wire::about(&f.env, PAID, &id));
    assert_eq!(
        f.balance(&f.stable, &f.taker),
        9_915_075_000 - TIP * 10,
        "the taker is paid regardless"
    );
    let ct = f.vault.ct_of(&f.vault.open_batch().unwrap());
    assert_eq!(f.settle.owed(&lp, &ct), UNITS);

    gate.allow(&lp);
    f.settle.withdraw(&lp, &ct);
    assert_eq!(f.balance(&ct, &lp), UNITS);
}

#[test]
fn redeem_push_pays_the_proceeds_to_the_maker_home() {
    let f = setup();
    let id = f.fill(&f.bid(REDEEM_PUSH, &f.taker));
    f.pay_in(992_500_000, &wire::about(&f.env, PAID, &id));
    let p = f.settle.push_of(&id).unwrap();
    let ct = f.vault.batch(&p.batch).unwrap().ct;
    assert_eq!(
        TokenClient::new(&f.env, &ct).balance(&f.settle.address),
        UNITS
    );

    f.vault.withdraw_batch(&p.batch, &f.operator);
    StellarAssetClient::new(&f.env, &f.stable).mint(&f.operator, &10_010_000_003);
    f.vault.settle_manual(&p.batch, &10_010_000_003, &0);
    assert_eq!(
        f.settle.push(&f.taker, &id),
        10_010_000_000,
        "bridges move 6 decimals"
    );
    let payout = wire::payout(&f.env, &word_u(&f.env, 0xA11CE), false);
    assert_eq!(
        f.bridged().pop().unwrap(),
        (CHAIN, f.peer.clone(), 10_010_000_000, payout)
    );
    assert!(f.settle.try_push(&f.taker, &id).is_err());
}

#[test]
fn a_push_hands_rwa_returned_in_kind_to_the_bids_recipient() {
    let f = setup();
    let lp = Address::generate(&f.env);
    let id = f.fill(&f.bid(REDEEM_PUSH, &lp));
    f.pay_in(992_500_000, &wire::about(&f.env, PAID, &id));
    let p = f.settle.push_of(&id).unwrap();
    f.vault.withdraw_batch(&p.batch, &f.operator);
    StellarAssetClient::new(&f.env, &f.stable).mint(&f.operator, &(UNITS * 3 / 4));
    f.vault
        .settle_manual(&p.batch, &(UNITS * 3 / 4), &(UNITS / 4));
    assert_eq!(f.settle.push(&f.taker, &id), UNITS * 3 / 4);
    assert_eq!(
        f.balance(&f.rwa, &lp),
        UNITS / 4,
        "the issuer handed a quarter back"
    );
}

#[test]
fn cross_deliver_needs_a_route_for_the_rwa() {
    let f = setup();
    let sig = Bytes::from_array(&f.env, &[0xAA; 65]);
    let bid = f.bid(CROSS_DELIVER, &f.taker);
    assert_eq!(
        f.settle.try_fill(&f.request(), &CHAIN, &bid, &sig),
        Err(Ok(Error::NoRoute.into()))
    );
    let route = Change::Route(CHAIN, f.rwa.clone(), Some(word_u(&f.env, 0x8A)));
    assert_eq!(
        f.settle.try_submit(&route),
        Err(Ok(Error::NoRoute.into())),
        "the bridge does not carry the RWA"
    );
}

#[test]
fn a_payment_short_of_the_quote_does_not_settle() {
    let f = setup();
    let id = f.fill(&f.bid(DELIVER, &f.taker));
    let paid = wire::about(&f.env, PAID, &id);
    assert_eq!(
        f.try_pay_in(992_499_999, &paid, &f.taker),
        Err(Ok(Error::Underpaid.into()))
    );
    assert!(f.settle.order(&id).is_some(), "the RWA stays escrowed");
}

#[test]
fn redeem_modes_need_a_vault_and_push_needs_its_stable() {
    let f = setup();
    let sig = Bytes::from_array(&f.env, &[0xAA; 65]);
    let other = sac(&f.env, &f.operator);
    let elsewhere = redemption_vault(&f.env, &f.operator, &f.operator, &f.rwa, &other);
    govern(
        &f.settle,
        &Change::Vault(f.rwa.clone(), Some(elsewhere.address)),
    );
    let push = f.bid(REDEEM_PUSH, &f.taker);
    assert_eq!(
        f.settle.try_fill(&f.request(), &CHAIN, &push, &sig),
        Err(Ok(Error::NoVault.into())),
        "the vault pays another stable than the fill's"
    );
    let ct = f.bid(REDEEM_CT, &f.taker);
    f.settle.fill(&f.request(), &CHAIN, &ct, &sig);

    let mut r = f.request();
    r.salt = 2;
    let mut nobody = f.bid(DELIVER, &f.taker);
    nobody.recipient = BytesN::from_array(&f.env, &[0; 32]);
    assert_eq!(
        f.settle.try_fill(&r, &CHAIN, &nobody, &sig),
        Err(Ok(Error::InvalidBid.into()))
    );
    govern(&f.settle, &Change::Vault(f.rwa.clone(), None));
    assert_eq!(
        f.settle.try_fill(&r, &CHAIN, &ct, &sig),
        Err(Ok(Error::NoVault.into()))
    );
}

#[test]
fn governance_waits_and_keys_queue_apart() {
    let f = setup();
    govern(&f.settle, &Change::Delay(DAY));
    let close = Change::Route(CHAIN, f.stable.clone(), None);
    let other = Change::Vault(f.rwa.clone(), None);
    f.settle.submit(&close);
    f.settle.submit(&other);
    assert_ne!(f.settle.kind_of(&close), f.settle.kind_of(&other));
    let not_ready = soroban_sdk::Error::from_contract_error(111);
    assert_eq!(
        f.settle.try_accept(&f.settle.kind_of(&close)),
        Err(Ok(not_ready))
    );
    f.env
        .ledger()
        .set_timestamp(f.env.ledger().timestamp() + DAY);
    f.settle.accept(&f.settle.kind_of(&close));
    f.settle.accept(&f.settle.kind_of(&other));
    assert_eq!(f.settle.route(&CHAIN, &f.stable), None);
    let unbridged = Change::Route(CHAIN, f.xlm.clone(), Some(word_u(&f.env, REMOTE)));
    assert_eq!(
        f.settle.try_submit(&unbridged),
        Err(Ok(Error::NoRoute.into())),
        "the bridge does not carry XLM there"
    );
}

// ---------------------------------------------------------------- maker here

#[test]
fn a_fill_pulls_the_makers_stable_and_bridges_it_home_with_paid() {
    let f = setup();
    let id = word_u(&f.env, 0x1D);
    f.deliver(&f.fill_message(&f.key, &id, 992_500_000));

    assert_eq!(f.balance(&f.stable, &f.maker), 100 * UNITS - 9_925_000_000);
    assert_eq!(
        f.bridged().pop().unwrap(),
        (
            CHAIN,
            f.peer.clone(),
            9_925_000_000,
            wire::about(&f.env, PAID, &id)
        )
    );
    assert_eq!(f.balance(&f.stable, &f.settle.address), 0, "burned");
    assert_eq!(
        f.balance(&f.xlm, &f.settle.address),
        XLM - BRIDGE_FEE,
        "the return leg is ours to pay"
    );
}

#[test]
fn a_bad_signature_or_no_allowance_sends_fail_and_moves_nothing() {
    let f = setup();
    let id = word_u(&f.env, 0x1D);
    let stranger = SigningKey::from_bytes(&[12u8; 32]);
    f.deliver(&f.fill_message(&stranger, &id, 992_500_000));
    assert_eq!(f.last_sent(), wire::about(&f.env, FAIL, &id));

    let mut tampered = f.fill_message(&f.key, &id, 992_500_000);
    tampered.set(wire::SIG_AT + 40, 0);
    f.deliver(&tampered);
    assert_eq!(
        f.sent().len(),
        2,
        "a broken signature fails, it does not trap"
    );

    TokenClient::new(&f.env, &f.stable).approve(&f.maker, &f.settle.address, &0, &1_000_000);
    f.deliver(&f.fill_message(&f.key, &id, 992_500_000));
    assert_eq!(f.sent().len(), 3);
    assert_eq!(f.balance(&f.stable, &f.maker), 100 * UNITS);
    assert_eq!(f.balance(&f.xlm, &f.settle.address), XLM - 3 * MSG_FEE);
}

#[test]
fn over_cap_replayed_or_cancelled_bids_fail() {
    let f = setup();
    let id = word_u(&f.env, 0x1D);
    f.deliver(&f.fill_message(&f.key, &id, 1_000_000_001));
    assert_eq!(f.last_sent(), wire::about(&f.env, FAIL, &id));

    f.deliver(&f.fill_message(&f.key, &id, 992_500_000));
    f.deliver(&f.fill_message(&f.key, &id, 992_500_000));
    assert_eq!(f.sent().len(), 2, "a bid pays once");

    let other = word_u(&f.env, 0x2D);
    f.rfq.cancel_salt(&f.maker, &f.maker, &7);
    f.deliver(&f.fill_message(&f.key, &other, 992_500_000));
    assert_eq!(f.sent().len(), 3);
    assert_eq!(f.bridged().len(), 1);
}

#[test]
fn a_registered_hot_key_signs_for_the_maker() {
    let f = setup();
    let hot = SigningKey::from_bytes(&[13u8; 32]);
    let signer = BytesN::from_array(&f.env, &hot.verifying_key().to_bytes());
    f.rfq.register_order_signer(&f.maker, &signer, &true);
    f.deliver(&f.fill_message(&hot, &word_u(&f.env, 0x1D), 992_500_000));
    assert_eq!(f.bridged().len(), 1);
}

#[test]
fn a_payout_is_forwarded_to_the_maker() {
    let f = setup();
    let to = wire::key(&f.env, &f.maker);
    let relayer = f.pay_in(5_000_000, &wire::payout(&f.env, &to, true));
    assert_eq!(
        f.balance(&f.stable, &f.maker),
        100 * UNITS + 50_000_000 - TIP * 10
    );
    assert_eq!(
        f.balance(&f.stable, &relayer),
        TIP * 10,
        "the tip, out of the payout"
    );
}

// ---------------------------------------------------------------- wire format

/// FILL as `cast abi-encode` writes it for the EVM `CrossChainSettle`.
const FILL_HEX: &str = "00000000000000000000000000000000000000000000000000000000000000001111111111111111111111111111111111111111111111111111111111111111000000000000000000000000000000000000000000000000000000003b2859200000000000000000000000000000000000000000000000000000000000a11ce000000000000000000000000000000000000000000000000000000000000000002222222222222222222222222222222222222222222222222222222222222222000000000000000000000000000000000000000000000000000000003b9aca0000000000000000000000000000000000000000000000000000000000000000fa00000000000000000000000000000000000000000000000000000000000000023333333333333333333333333333333333333333333333333333333333333333000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000f50500000000000000000000000000000000000000000000000000000000000000007";
const PAYOUT_HEX: &str = "000000000000000000000000000000000000000000000000000000000000000344444444444444444444444444444444444444444444444444444444444444440000000000000000000000000000000000000000000000000000000000000001";

#[test]
fn messages_match_the_evm_abi_encoding() {
    let env = Env::default();
    let w = |b: u8| BytesN::from_array(&env, &[b; 32]);
    let bid = Bid {
        maker: word_u(&env, 0xA11CE0),
        maker_is_contract: false,
        stable: w(0x22),
        max_amount: 1_000_000_000,
        maker_bps_per_day: 250,
        delivery: REDEEM_PUSH,
        recipient: w(0x33),
        recipient_is_contract: true,
        deadline: 1_003_600,
        salt: word_u(&env, 7),
    };
    let fill = hex(&env, FILL_HEX);
    assert_eq!(
        wire::fill(&env, &w(0x11), 992_500_000, &bid, &Bytes::new(&env)),
        fill
    );
    assert_eq!(wire::parse_bid(&fill), Some(bid));
    assert_eq!(wire::payout(&env, &w(0x44), true), hex(&env, PAYOUT_HEX));
}
