//! `cctp_settle` against the real Stellar testnet Circle CCTP V2 contracts
//! (TokenMessengerMinter, MessageTransmitter (domain 27), USDC and its minter),
//! loaded from a ledger snapshot (`fixtures/testnet.json`, built by `snapshot.py`).
//!
//! Stand-ins: the RWA (a test SAC), our RFQ and verifier (our wasm), and a test
//! key enabled on the real transmitter in place of Circle's attesters.
//!
//! Our contract sits at the id the Sepolia fork tests (octarine-v2
//! `test/fork`) use for it, and its Sepolia peer at the contract they deploy,
//! so the bytes each side emits are fixtures the other replays.
#![allow(clippy::too_many_arguments)]

mod settle {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/release/cctp_settle.wasm");
}
mod common;

use common::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{contractclient, Address, Bytes, BytesN, Env};

const TOKEN_MESSENGER: &str = "CDNG7HXAPBWICI2E3AUBP3YZWZELJLYSB6F5CC7WLDTLTHVM74SLRTHP";
const TRANSMITTER: &str = "CBJ6MTCKKZG73PMDZCJMSFRD7DQEMI4FKDH7CGDSV4W6FHCRBCQAVVJY";
const STELLAR_DOMAIN: u32 = 27;
const SEPOLIA_DOMAIN: u32 = 0;
const SEPOLIA_MESSENGER: &str = "8fe6b999dc680ccfdd5bf7eb0974218be2542daa";
/// `CctpSettle` as the Sepolia fork tests deploy it.
const EVM_PEER: &str = "2e234dae75c793f67a35089c9d99245e1c58470b";
const TIP: i128 = 500_000; // 0.50 USDC on the wire
const ATTESTER: [u8; 32] = [0xA7; 32];

#[contractclient(name = "TransmitterClient")]
pub trait TransmitterApi {
    fn enable_attester(env: Env, attester: BytesN<20>);
    fn set_signature_threshold(env: Env, new_signature_threshold: u32);
    fn is_nonce_used(env: Env, nonce: BytesN<32>) -> bool;
    fn get_local_domain(env: Env) -> u32;
    fn receive_message(env: Env, caller: Address, message: Bytes, attestation: Bytes) -> bool;
}

#[contractclient(name = "MessengerClient")]
pub trait MessengerApi {
    fn get_remote_token_messenger(env: Env, domain: u32) -> Option<BytesN<32>>;
    fn get_local_token(env: Env, remote_domain: u32, remote_token: BytesN<32>) -> Option<Address>;
}

fn fork() -> Fork {
    let f = common::fork(SEPOLIA_DOMAIN, EVM_PEER, TIP, "", |e| {
        let bridge = settle::Bridge {
            transmitter: at(e, TRANSMITTER),
            messenger: at(e, TOKEN_MESSENGER),
        };
        (bridge, at(e, USDC), evm(e, SEPOLIA_USDC))
    });
    // A test key attests in place of Circle's: one more attester, one signature enough.
    let attester = attester();
    let t = transmitter(&f.env);
    t.enable_attester(&BytesN::from_array(&f.env, &eth_address(&f.env, &attester)));
    t.set_signature_threshold(&1);
    f
}

fn attester() -> k256::ecdsa::SigningKey {
    k256::ecdsa::SigningKey::from_slice(&ATTESTER).unwrap()
}

fn transmitter(env: &Env) -> TransmitterClient<'_> {
    TransmitterClient::new(env, &at(env, TRANSMITTER))
}

fn eth_address(env: &Env, key: &k256::ecdsa::SigningKey) -> [u8; 20] {
    let point = key.verifying_key().to_encoded_point(false);
    keccak(env, &point.as_bytes()[1..])[12..]
        .try_into()
        .unwrap()
}

/// Our test attester's signature: keccak256 of the message, `r ‖ s ‖ v`.
fn attest_with(env: &Env, key: &k256::ecdsa::SigningKey, message: &[u8]) -> Bytes {
    let digest = keccak(env, message);
    let (sig, v) = key.sign_prehash_recoverable(&digest).unwrap();
    let mut out = sig.to_bytes().to_vec();
    out.push(27 + v.to_byte());
    Bytes::from_slice(env, &out)
}

trait Cctp {
    fn message_from_sepolia(&self, nonce: u128, finality: u32, body: &[u8]) -> Vec<u8>;
    fn burn_from_sepolia(&self, nonce: u128, amount: u128, hook: &[u8]) -> Vec<u8>;
    fn attest(&self, message: &[u8]) -> Bytes;
    fn receive(&self, caller: &Address, message: &[u8]);
    fn complete(&self, caller: &Address, message: &[u8]);
}

impl Cctp for Fork {
    /// A message the peer sent us from Sepolia, as Iris attests it: nonce
    /// assigned, `finality` executed, for anyone to receive.
    fn message_from_sepolia(&self, nonce: u128, finality: u32, body: &[u8]) -> Vec<u8> {
        [
            &1u32.to_be_bytes()[..],
            &SEPOLIA_DOMAIN.to_be_bytes(),
            &STELLAR_DOMAIN.to_be_bytes(),
            &uint(nonce),
            &evm_word(EVM_PEER),
            &self.id(),
            &[0u8; 32], // destinationCaller: anyone
            &finality.to_be_bytes(),
            &finality.to_be_bytes(),
            body,
        ]
        .concat()
    }

    /// A Sepolia burn of `amount` (6 decimals) by our peer for us, with `hook`,
    /// as Iris attests it: nonce assigned and finalized (2000).
    fn burn_from_sepolia(&self, nonce: u128, amount: u128, hook: &[u8]) -> Vec<u8> {
        let me = self.id();
        [
            &1u32.to_be_bytes()[..],
            &SEPOLIA_DOMAIN.to_be_bytes(),
            &STELLAR_DOMAIN.to_be_bytes(),
            &uint(nonce),
            &evm_word(SEPOLIA_MESSENGER),
            &id(&at(&self.env, TOKEN_MESSENGER)),
            &me, // destinationCaller
            &2000u32.to_be_bytes(),
            &2000u32.to_be_bytes(),
            &1u32.to_be_bytes(),
            &evm_word(SEPOLIA_USDC),
            &me, // mintRecipient
            &uint(amount),
            &evm_word(EVM_PEER), // messageSender
            &[0u8; 96],          // maxFee, feeExecuted, expirationBlock
            hook,
        ]
        .concat()
    }

    fn attest(&self, message: &[u8]) -> Bytes {
        attest_with(&self.env, &attester(), message)
    }

    /// A relayer receives a message on the real transmitter, which calls us.
    fn receive(&self, caller: &Address, message: &[u8]) {
        let m = Bytes::from_slice(&self.env, message);
        transmitter(&self.env).receive_message(caller, &m, &self.attest(message));
    }

    fn complete(&self, caller: &Address, message: &[u8]) {
        let m = Bytes::from_slice(&self.env, message);
        self.settle.complete(caller, &m, &self.attest(message));
    }
}

/// The message Stellar's transmitter emitted for us to Sepolia, carrying `body`.
fn assert_sent(m: &[u8], body: &[u8]) {
    assert_fields(
        m,
        &[
            ("version", 0, &1u32.to_be_bytes()),
            ("sourceDomain", 4, &STELLAR_DOMAIN.to_be_bytes()),
            ("destinationDomain", 8, &SEPOLIA_DOMAIN.to_be_bytes()),
            ("nonce, which Iris assigns", 12, &[0; 32]),
            ("minFinalityThreshold", 140, &2000u32.to_be_bytes()),
            ("finalityThresholdExecuted, which Iris sets", 144, &[0; 4]),
            ("destinationCaller: anyone relays", 108, &[0; 32]),
            ("body", 148, body),
        ],
    );
    assert_eq!(m.len(), 148 + body.len());
}

// ------------------------------------------------------------------ the chain

#[test]
fn the_snapshot_is_testnet_wired_for_sepolia() {
    let f = fork();
    let e = &f.env;
    let messenger = MessengerClient::new(e, &at(e, TOKEN_MESSENGER));
    assert_eq!(
        messenger.get_local_token(&SEPOLIA_DOMAIN, &evm(e, SEPOLIA_USDC)),
        Some(f.stable.clone()),
        "CCTP mints this USDC for Sepolia's, so the route opened"
    );
    assert_eq!(
        messenger.get_remote_token_messenger(&SEPOLIA_DOMAIN),
        Some(evm(e, SEPOLIA_MESSENGER))
    );
    assert_eq!(
        soroban_sdk::token::TokenClient::new(e, &f.stable).decimals(),
        7
    );
    assert_eq!(transmitter(e).get_local_domain(), STELLAR_DOMAIN);
    let unlinked = settle::Change::Route(SEPOLIA_DOMAIN, f.xlm.clone(), Some(evm(e, "bad")));
    assert_eq!(
        f.settle.try_submit(&unlinked),
        Err(Ok(settle::Error::NoRoute.into())),
        "CCTP links no such pair"
    );
}

// ------------------------------------------------------------------ taker here

#[test]
fn fill_sends_the_bid_to_sepolia_through_the_real_transmitter() {
    let f = fork();
    let lp = Address::generate(&f.env);
    let bid = f.evm_bid(&lp);
    let id = f.request_id();
    let xlm = f.balance(&f.xlm, &f.taker);
    assert_eq!(
        f.settle.fill_fee(&SEPOLIA_DOMAIN),
        RETURN_FEE,
        "CCTP is free"
    );

    assert_eq!(f.fill(&bid), id);
    let m = f.emitted(TRANSMITTER, "message");
    assert_eq!(f.signers(), vec![f.taker.clone()], "only the taker signs");
    assert_eq!(xlm - f.balance(&f.xlm, &f.taker), RETURN_FEE);
    assert_eq!(f.balance(&f.rwa, &f.settle.address), UNITS);

    let fill = f.fill_to_sepolia(&id, &bid);
    assert_sent(&m, &fill);
    assert_fields(
        &m,
        &[
            ("sender: us", 44, &f.id()),
            ("recipient: the peer", 76, &evm_word(EVM_PEER)),
        ],
    );
    write_fixture(
        "stellar_cctp_fill",
        serde_json::json!({
            "about": "FILL from cctp_settle (Stellar testnet, CCTP domain 27) to the Sepolia peer, as the \
                      real MessageTransmitterV2 emitted it (MessageSent.message). Nonce and \
                      finalityThresholdExecuted are zero: Iris fills them before attesting.",
            "message": to_hex(&m),
            "body": to_hex(&fill),
            "amount": 992_500_000u64,
        }),
    );
}

#[test]
fn a_fail_from_sepolia_received_on_the_real_transmitter_returns_the_rwa() {
    let f = fork();
    let id = f.fill(&f.evm_bid(&Address::generate(&f.env)));
    let relayer = Address::generate(&f.env);
    let m = f.message_from_sepolia(1, 2000, &words(&[uint(FAIL), id.to_array()]));
    f.receive(&relayer, &m);
    assert_eq!(f.signers(), vec![relayer.clone()], "only the relayer signs");

    assert_eq!(f.balance(&f.rwa, &f.taker), 10 * UNITS);
    assert!(!f.settle.filled(&id));
    assert!(transmitter(&f.env).is_nonce_used(&BytesN::from_array(&f.env, &uint(1))));
}

#[test]
fn usdc_from_sepolia_mints_through_real_cctp_and_settles() {
    let f = fork();
    let lp = Address::generate(&f.env);
    let id = f.fill(&f.evm_bid(&lp));
    let relayer = Address::generate(&f.env);
    let m = f.burn_from_sepolia(1, 992_500_000, &words(&[uint(PAID), id.to_array()]));
    f.complete(&relayer, &m);
    assert_eq!(f.signers(), vec![relayer.clone()], "only the relayer signs");

    // The real minter scales 6 decimals to 7: 9,925 USDC here. A 9.925 USDC fee
    // and the 0.50 USDC tip come out of the taker's.
    assert_eq!(f.balance(&f.stable, &f.taker), 9_910_075_000);
    assert_eq!(f.balance(&f.stable, &f.fee_to), 9_925_000);
    assert_eq!(f.balance(&f.stable, &relayer), 5_000_000);
    assert_eq!(f.balance(&f.stable, &f.settle.address), 0);
    assert_eq!(f.balance(&f.rwa, &lp), UNITS);
    assert_eq!(f.settle.order(&id), None);
    assert!(transmitter(&f.env).is_nonce_used(&BytesN::from_array(&f.env, &uint(1))));
    let again = Bytes::from_slice(&f.env, &m);
    assert!(
        f.settle
            .try_complete(&relayer, &again, &f.attest(&m))
            .is_err(),
        "once"
    );
}

#[test]
fn unattested_from_anyone_but_the_peer_unfinalized_or_short_of_the_quote_does_not_settle() {
    let f = fork();
    let id = f.fill(&f.evm_bid(&f.taker));
    let paid = words(&[uint(PAID), id.to_array()]);
    let good = f.burn_from_sepolia(1, 992_500_000, &paid);
    let stranger = k256::ecdsa::SigningKey::from_slice(&[0xB0; 32]).unwrap();
    let m = Bytes::from_slice(&f.env, &good);
    assert!(
        f.settle
            .try_complete(&f.taker, &m, &attest_with(&f.env, &stranger, &good))
            .is_err(),
        "the real transmitter checks the attester"
    );
    let mut forged = f.burn_from_sepolia(1, 992_500_000, &paid);
    forged[248..280].copy_from_slice(&evm_word("bad"));
    let m = Bytes::from_slice(&f.env, &forged);
    assert_eq!(
        f.settle.try_complete(&f.taker, &m, &f.attest(&forged)),
        Err(Ok(settle::Error::UnknownSender.into()))
    );
    // Any contract can send a message for us to receive that reads like a burn
    // from the peer: only one the messenger receives is.
    let mut not_a_burn = f.burn_from_sepolia(1, 992_500_000, &paid);
    not_a_burn[76..108].copy_from_slice(&evm_word("a77ac4e7"));
    let m = Bytes::from_slice(&f.env, &not_a_burn);
    assert_eq!(
        f.settle.try_complete(&f.taker, &m, &f.attest(&not_a_burn)),
        Err(Ok(settle::Error::UnknownSender.into()))
    );
    let short = f.burn_from_sepolia(2, 992_499_999, &paid);
    let m = Bytes::from_slice(&f.env, &short);
    assert_eq!(
        f.settle.try_complete(&f.taker, &m, &f.attest(&short)),
        Err(Ok(settle::Error::Underpaid.into()))
    );

    let fail = words(&[uint(FAIL), id.to_array()]);
    let mut forged = f.message_from_sepolia(3, 2000, &fail);
    forged[44..76].copy_from_slice(&evm_word("bad"));
    let t = transmitter(&f.env);
    let attempt = |m: &[u8]| {
        t.try_receive_message(&f.taker, &Bytes::from_slice(&f.env, m), &f.attest(m))
            .is_err()
    };
    assert!(attempt(&forged), "a message from anyone but the peer");
    let unfinalized = f.message_from_sepolia(4, 1000, &fail);
    assert!(attempt(&unfinalized), "we take finalized messages only");
    assert!(f.settle.order(&id).is_some(), "the RWA stays escrowed");
}

// ------------------------------------------------------------------ maker here

#[test]
fn a_fill_from_sepolia_burns_the_makers_usdc_home_over_real_cctp() {
    let f = fork();
    let request = [0x1D; 32];
    let fill = f.fill_from_sepolia(&request, 992_500_000);
    let relayer = Address::generate(&f.env);
    f.receive(&relayer, &f.message_from_sepolia(1, 2000, &fill));
    assert_eq!(f.balance(&f.stable, &f.maker), 100 * UNITS, "waiting");
    f.settle.execute(&Bytes::from_slice(&f.env, &fill));
    let m = f.emitted(TRANSMITTER, "message");
    assert!(
        f.signers().is_empty(),
        "nobody signs: the maker approved before"
    );

    // 9,925 USDC (7 decimals) pulled and burned.
    assert_eq!(f.balance(&f.stable, &f.maker), 100 * UNITS - 9_925_000_000);
    assert_eq!(f.balance(&f.stable, &f.settle.address), 0, "burned");

    // The message Sepolia's transmitter receives, as Stellar's emitted it.
    let me = f.id();
    let hook = words(&[uint(PAID), request]);
    assert_fields(
        &m,
        &[
            ("version", 0, &1u32.to_be_bytes()),
            ("sourceDomain", 4, &STELLAR_DOMAIN.to_be_bytes()),
            ("destinationDomain", 8, &SEPOLIA_DOMAIN.to_be_bytes()),
            ("nonce, which Iris assigns", 12, &[0; 32]),
            ("sender", 44, &id(&at(&f.env, TOKEN_MESSENGER))),
            ("recipient", 76, &evm_word(SEPOLIA_MESSENGER)),
            ("destinationCaller: only the peer", 108, &evm_word(EVM_PEER)),
            ("minFinalityThreshold", 140, &2000u32.to_be_bytes()),
            ("burn message version", 148, &1u32.to_be_bytes()),
            ("burnToken", 152, &id(&f.stable)),
            ("mintRecipient", 184, &evm_word(EVM_PEER)),
            ("amount, at 6 decimals", 216, &uint(992_500_000)),
            ("messageSender: the EVM side's peer", 248, &me),
            ("maxFee", 280, &[0; 32]),
            ("hookData", 376, &hook),
        ],
    );
    assert_eq!(m.len(), 376 + hook.len());
    write_fixture(
        "stellar_cctp_paid_burn",
        serde_json::json!({
            "about": "PAID burn from cctp_settle (Stellar testnet, CCTP domain 27) to the Sepolia peer, \
                      as the real MessageTransmitterV2 emitted it (MessageSent.message). \
                      Nonce and finalityThresholdExecuted are zero: Iris fills them before attesting.",
            "message": to_hex(&m),
            "sourceDomain": STELLAR_DOMAIN,
            "messageSender": to_hex(&me),
            "amount": 992_500_000u64,
            "hookData": to_hex(&m[376..]),
            "request": to_hex(&request),
        }),
    );
    let again = f.settle.try_execute(&Bytes::from_slice(&f.env, &fill));
    assert_eq!(again, Err(Ok(settle::Error::NotFound.into())), "once");
}

#[test]
fn a_fill_from_sepolia_with_a_bad_signature_sends_fail_home_over_real_cctp() {
    let f = fork();
    let request = [0x1D; 32];
    let mut fill = f.fill_from_sepolia(&request, 992_500_000);
    let last = fill.len() - 1;
    fill[last] ^= 1;
    f.receive(
        &Address::generate(&f.env),
        &f.message_from_sepolia(1, 2000, &fill),
    );
    f.settle.execute(&Bytes::from_slice(&f.env, &fill));
    let m = f.emitted(TRANSMITTER, "message");

    let fail = words(&[uint(FAIL), request]);
    assert_sent(&m, &fail);
    assert_fields(&m, &[("recipient: the peer", 76, &evm_word(EVM_PEER))]);
    assert_eq!(f.balance(&f.stable, &f.maker), 100 * UNITS);
    write_fixture(
        "stellar_cctp_fail",
        serde_json::json!({
            "about": "FAIL from cctp_settle (Stellar testnet, CCTP domain 27) answering a FILL with a bad \
                      signature, as the real MessageTransmitterV2 emitted it (MessageSent.message)",
            "message": to_hex(&m),
            "body": to_hex(&fail),
            "request": to_hex(&request),
        }),
    );
}

#[test]
fn a_payout_over_real_cctp_reaches_the_maker_less_the_tip() {
    let f = fork();
    let payout = words(&[uint(3), id(&f.maker), uint(1)]);
    let relayer = Address::generate(&f.env);
    f.complete(&relayer, &f.burn_from_sepolia(1, 250_000_000, &payout));
    assert_eq!(
        f.balance(&f.stable, &f.maker),
        100 * UNITS + 2_500_000_000 - 5_000_000
    );
    assert_eq!(f.balance(&f.stable, &relayer), 5_000_000);
}

// --------------------------------------------------- the Sepolia fork's own bytes

/// A Sepolia message as Iris attests it: nonce assigned, finality executed.
fn as_attested(f: &Fork, fx: &serde_json::Value, field: &str) -> Vec<u8> {
    let mut m = hex(fx[field].as_str().unwrap());
    let nonce = keccak(&f.env, &m);
    m[12..44].copy_from_slice(&nonce);
    m[144..148].copy_from_slice(&2000u32.to_be_bytes());
    m
}

/// The Sepolia fork's FILL for a Stellar maker. Its signature is a
/// placeholder, so we send FAIL home.
#[test]
fn sepolias_fill_message_is_received_and_its_bad_signature_fails_home() {
    let Some(fx) = fixture("evm_cctp_fill_to_stellar") else {
        return eprintln!("skipped: no {EVM_FIXTURES}/evm_cctp_fill_to_stellar.json");
    };
    let f = fork();
    let m = as_attested(&f, &fx, "message");
    assert_eq!(&m[76..108], &f.id(), "sent to our id");
    assert_eq!(&m[44..76], &evm_word(EVM_PEER), "from the peer we trust");
    let deadline = u64::from_be_bytes(m[148 + 376..148 + 384].try_into().unwrap());
    if f.env.ledger().timestamp() > deadline {
        soroban_sdk::testutils::Ledger::set_timestamp(&f.env.ledger(), deadline - 600);
    }
    f.receive(&Address::generate(&f.env), &m);
    f.settle.execute(&Bytes::from_slice(&f.env, &m[148..]));

    let request: [u8; 32] = m[148 + 32..148 + 64].try_into().unwrap();
    assert_sent(
        &f.emitted(TRANSMITTER, "message"),
        &words(&[uint(FAIL), request]),
    );
}

/// The Sepolia fork's FAIL, answering a FILL from here.
#[test]
fn sepolias_fail_message_returns_the_rwa_here() {
    let Some(fx) = fixture("evm_cctp_fail_to_stellar") else {
        return eprintln!("skipped: no {EVM_FIXTURES}/evm_cctp_fail_to_stellar.json");
    };
    let f = fork();
    let m = as_attested(&f, &fx, "message");
    let request: [u8; 32] = m[148 + 32..148 + 64].try_into().unwrap();
    f.put_order(&request, &f.order_for(9_900_000_000, TIP * 10, &f.taker));
    f.receive(&Address::generate(&f.env), &m);
    assert_eq!(
        f.balance(&f.rwa, &f.taker),
        11 * UNITS,
        "its 1,000 back on top of the 10,000"
    );
    assert!(!f.settle.filled(&BytesN::from_array(&f.env, &request)));
}

/// The Sepolia fork's PAID burn.
#[test]
fn sepolias_paid_burn_mints_and_settles_here() {
    let Some(fx) = fixture("evm_cctp_paid_burn_to_stellar") else {
        return eprintln!("skipped: no {EVM_FIXTURES}/evm_cctp_paid_burn_to_stellar.json");
    };
    let f = fork();
    let m = as_attested(&f, &fx, "cctpMessage");
    assert_eq!(&m[108..140], &f.id(), "destinationCaller is us");
    assert_eq!(&m[184..216], &f.id(), "mintRecipient is us");
    let request: [u8; 32] = m[376 + 32..376 + 64].try_into().unwrap();
    let amount = u128::from_be_bytes(m[216 + 16..248].try_into().unwrap()) as i128 * 10;
    let lp = Address::generate(&f.env);
    f.put_order(&request, &f.order_for(amount, TIP * 10, &lp));
    let relayer = Address::generate(&f.env);
    f.complete(&relayer, &m);

    assert_eq!(
        f.balance(&f.stable, &f.taker),
        amount - amount / 1000 - TIP * 10
    );
    assert_eq!(f.balance(&f.stable, &f.fee_to), amount / 1000);
    assert_eq!(f.balance(&f.stable, &relayer), TIP * 10);
    assert_eq!(f.balance(&f.rwa, &lp), UNITS);
}
