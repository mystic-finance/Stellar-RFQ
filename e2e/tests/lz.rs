//! `lz_settle` against the real Stellar testnet LayerZero V2 contracts
//! (EndpointV2 (EID 40600), its default ULN302, executor, executor helper, DVN,
//! treasury, fee libs and price feeds), loaded from a ledger snapshot
//! (`fixtures/testnet.json`, built by `snapshot.py`).
//!
//! Stand-ins: the RWA and the OFT's token (test SACs: USDT0 has no testnet
//! deployment, so no payment crosses and the maker side is tested up to FAIL),
//! our RFQ and verifier (our wasm), and the auth of the DVN and the executor
//! (mocked, as their keys would sign).
//!
//! Our contract sits at the id the Sepolia fork tests (octarine-v2
//! `test/fork`) use for it, and its Sepolia peer at the `LzSettle` they deploy,
//! so the bytes each side emits are fixtures the other replays.
#![allow(clippy::too_many_arguments)]

mod settle {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/release/lz_settle.wasm");
}
mod common;

use common::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{contractclient, contracttype, Address, Bytes, BytesN, Env};

const ENDPOINT: &str = "CALTBA5S6GRJEHAXFP45LGGLKWWAF7HTZCPNUBUJF2HWWRRLQNV35AIV";
const ULN: &str = "CCMLPCAWCPIIMXOHJJKU3NZLOFTT2O6QTB2UUFPN6SEHLK35QRHVKKMB";
const EXECUTOR: &str = "CCAVZ7ESAV3PDJ6PISRCXVZCXFFH4NGI7K7MVMZZDR33LC5AD3UPZATT";
const EXECUTOR_HELPER: &str = "CANJCMHRRXEBM46675TSPJIFUVPW7PDOCBLMWS7QPO5TCBHBL7JC4CDL";
/// The ULN's default DVN for every EVM pathway: archived, and registered for an older ULN.
const DEFAULT_DVN: &str = "CBZ4QEWVC7EGNT5KDQAMPKP5PGIUEKONDEB767MOQSZDFPPLM6Q6LQQ3";
/// The LayerZero Labs DVN that OApps on testnet configure for ULN302.
const DVN: &str = "CB7EWGPRGD3R3CD2DIYIVXMOEFDINVBGBFFQLFMEMXFWJFJIMHA7ZLSA";
const STELLAR: u32 = 40600;
const SEPOLIA: u32 = 40161;
/// `LzSettle` as the Sepolia fork tests deploy it.
const EVM_PEER: &str = "f62849f9a0b5bf2913b396098f7c7019b51a820a";
/// Type 3 executor options, lzReceive with 1M gas: the ULN rejects empty ones,
/// and the Sepolia peer needs 500k left before it pays a FILL.
const OPTIONS: &str = "000301001101000000000000000000000000000f4240";

#[contractclient(name = "EndpointClient")]
pub trait EndpointApi {
    fn quote(env: Env, sender: Address, params: settle::MessagingParams) -> settle::MessagingFee;
    fn delegate(env: Env, oapp: Address) -> Option<Address>;
    fn set_config(
        env: Env,
        caller: Address,
        oapp: Address,
        lib: Address,
        params: soroban_sdk::Vec<SetConfigParam>,
    );
    fn outbound_nonce(env: Env, sender: Address, dst_eid: u32, receiver: BytesN<32>) -> u64;
    fn inbound_nonce(env: Env, receiver: Address, src_eid: u32, sender: BytesN<32>) -> u64;
    fn inbound_payload_hash(
        env: Env,
        receiver: Address,
        src_eid: u32,
        sender: BytesN<32>,
        nonce: u64,
    ) -> Option<BytesN<32>>;
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetConfigParam {
    pub eid: u32,
    pub config_type: u32,
    pub config: Bytes,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UlnConfig {
    pub confirmations: u64,
    pub required_dvns: soroban_sdk::Vec<Address>,
    pub optional_dvns: soroban_sdk::Vec<Address>,
    pub optional_dvn_threshold: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OAppUlnConfig {
    pub uln_config: UlnConfig,
    pub use_default_confirmations: bool,
    pub use_default_required_dvns: bool,
    pub use_default_optional_dvns: bool,
}

#[contractclient(name = "UlnClient")]
pub trait UlnApi {
    fn verify(
        env: Env,
        dvn: Address,
        packet_header: Bytes,
        payload_hash: BytesN<32>,
        confirmations: u64,
    );
    fn commit_verification(env: Env, packet_header: Bytes, payload_hash: BytesN<32>);
    fn default_send_uln_config(env: Env, dst_eid: u32) -> Option<UlnConfig>;
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionParams {
    pub receiver: Address,
    pub origin: settle::Origin,
    pub guid: BytesN<32>,
    pub message: Bytes,
    pub extra_data: Bytes,
    pub value: i128,
    pub gas_limit: i128,
}

/// How the executor delivers: it calls `lz_receive` from here.
#[contractclient(name = "ExecutorHelperClient")]
pub trait ExecutorHelperApi {
    fn execute(env: Env, executor: Address, params: ExecutionParams, value_payer: Address);
}

fn fork() -> Fork {
    let f = fork_with_default_dvn();
    use_dvn(&f, DVN);
    f
}

/// Deployed and governed, its LayerZero pathway still the ULN's default.
fn fork_with_default_dvn() -> Fork {
    common::fork(SEPOLIA, EVM_PEER, 0, OPTIONS, |e| {
        let token = e
            .register_stellar_asset_contract_v2(Address::generate(e))
            .address();
        let bridge = settle::Bridge {
            endpoint: at(e, ENDPOINT),
            ofts: soroban_sdk::map![e, (token.clone(), Address::generate(e))],
        };
        (bridge, token, evm(e, "07"))
    })
}

fn endpoint(env: &Env) -> EndpointClient<'_> {
    EndpointClient::new(env, &at(env, ENDPOINT))
}

/// The admin, as our delegate, points our send and receive pathways with
/// Sepolia at `dvn`: a deployment step, since the default DVN cannot serve.
fn use_dvn(f: &Fork, dvn: &str) {
    let e = &f.env;
    let config = OAppUlnConfig {
        uln_config: UlnConfig {
            confirmations: 0,
            required_dvns: soroban_sdk::vec![e, at(e, dvn)],
            optional_dvns: soroban_sdk::vec![e],
            optional_dvn_threshold: 0,
        },
        use_default_confirmations: true,
        use_default_required_dvns: false,
        use_default_optional_dvns: true,
    }
    .to_xdr(e);
    let params = [2u32, 3].map(|config_type| SetConfigParam {
        eid: SEPOLIA,
        config_type,
        config: config.clone(),
    });
    endpoint(e).set_config(
        &f.admin,
        &f.settle.address,
        &at(e, ULN),
        &soroban_sdk::Vec::from_array(e, params),
    );
}

/// Delivers a packet from Sepolia as LayerZero does: the DVN verifies it on the
/// real ULN302, anyone commits it to the endpoint, and the executor executes it
/// through its helper, which calls our `lz_receive`.
fn deliver(f: &Fork, nonce: u64, message: &[u8]) -> ExecutionParams {
    let header = [
        &[1u8][..],
        &nonce.to_be_bytes(),
        &SEPOLIA.to_be_bytes(),
        &evm_word(EVM_PEER),
        &STELLAR.to_be_bytes(),
        &f.id(),
    ]
    .concat();
    let guid = keccak(
        &f.env,
        &[&header[1..13], &header[13..45], &header[45..]].concat(),
    );
    deliver_packet(f, &[&header[..], &guid, message].concat())
}

fn deliver_packet(f: &Fork, packet: &[u8]) -> ExecutionParams {
    let e = &f.env;
    let (header, guid, message) = (&packet[..81], &packet[81..113], &packet[113..]);
    let hash = BytesN::from_array(e, &keccak(e, &packet[81..]));
    let uln = UlnClient::new(e, &at(e, ULN));
    uln.verify(&at(e, DVN), &Bytes::from_slice(e, header), &hash, &15);
    uln.commit_verification(&Bytes::from_slice(e, header), &hash);
    let origin = settle::Origin {
        src_eid: u32::from_be_bytes(header[9..13].try_into().unwrap()),
        sender: BytesN::from_array(e, header[13..45].try_into().unwrap()),
        nonce: u64::from_be_bytes(header[1..9].try_into().unwrap()),
    };
    let params = ExecutionParams {
        receiver: f.settle.address.clone(),
        origin,
        guid: BytesN::from_array(e, guid.try_into().unwrap()),
        message: Bytes::from_slice(e, message),
        extra_data: Bytes::new(e),
        value: 0,
        gas_limit: 0,
    };
    let executor = at(e, EXECUTOR);
    ExecutorHelperClient::new(e, &at(e, EXECUTOR_HELPER)).execute(&executor, &params, &executor);
    params
}

// ------------------------------------------------------------------ the chain

#[test]
fn the_constructor_made_the_admin_our_delegate() {
    let f = fork();
    assert_eq!(
        endpoint(&f.env).delegate(&f.settle.address),
        Some(f.admin.clone())
    );
}

/// FINDING (testnet infrastructure): ULN302's default send and receive config
/// for every EVM EID requires DVN CBZ4…LQQ3, which is archived and lists only
/// an older ULN (CAWC…JSH) as its message library. A send on the default
/// pathway fails in `assign_job` with WorkerError::UnsupportedMessageLib
/// (1213), as a live simulation of `endpoint.send` to 40161 also does. OApps on
/// testnet set their own DVN; our deployment must too (`use_dvn`).
#[test]
fn the_default_dvn_cannot_carry_a_fill() {
    let f = fork_with_default_dvn();
    let uln = UlnClient::new(&f.env, &at(&f.env, ULN));
    let default = uln.default_send_uln_config(&SEPOLIA).unwrap();
    assert_eq!(
        default.required_dvns,
        soroban_sdk::vec![&f.env, at(&f.env, DEFAULT_DVN)]
    );
    let r = f.settle.try_fill(
        &f.request(),
        &SEPOLIA,
        &f.evm_bid(&f.taker),
        &Bytes::from_array(&f.env, &[0xAA; 65]),
    );
    assert_eq!(
        r.err(),
        Some(Ok(soroban_sdk::Error::from_contract_error(1213))),
        "WorkerError::UnsupportedMessageLib from the default DVN"
    );
}

// ------------------------------------------------------------------ taker here

#[test]
fn fill_sends_the_bid_to_sepolia_through_the_real_endpoint() {
    let f = fork();
    let lp = Address::generate(&f.env);
    let bid = f.evm_bid(&lp);
    let id = f.request_id();
    let message = f.fill_to_sepolia(&id, &bid);
    let params = settle::MessagingParams {
        dst_eid: SEPOLIA,
        receiver: evm(&f.env, EVM_PEER),
        message: Bytes::from_slice(&f.env, &message),
        options: Bytes::from_slice(&f.env, &hex(OPTIONS)),
        pay_in_zro: false,
    };
    let quote = endpoint(&f.env).quote(&f.settle.address, &params);
    let fill_fee = f.settle.fill_fee(&SEPOLIA);
    let (xlm, endpoint_xlm) = (
        f.balance(&f.xlm, &f.taker),
        f.balance(&f.xlm, &at(&f.env, ENDPOINT)),
    );

    assert_eq!(f.fill(&bid), id);
    let packet = f.emitted(ENDPOINT, "encoded_packet");
    assert_eq!(f.signers(), vec![f.taker.clone()], "only the taker signs");

    assert!(quote.native_fee > 0);
    assert_eq!(
        xlm - f.balance(&f.xlm, &f.taker),
        quote.native_fee + RETURN_FEE,
        "the taker pays the real quote and the return leg"
    );
    assert!(
        fill_fee >= quote.native_fee + RETURN_FEE,
        "fill_fee quotes a 96-byte signature"
    );
    assert_eq!(
        f.balance(&f.xlm, &at(&f.env, ENDPOINT)),
        endpoint_xlm,
        "the workers took it all"
    );
    assert_eq!(f.balance(&f.rwa, &f.settle.address), UNITS);

    // The packet: version 1, nonce 1, from us on 40600 to the peer on 40161.
    let guid = keccak(&f.env, &packet[1..81]);
    assert_fields(
        &packet,
        &[
            ("version", 0, &[1]),
            ("nonce", 1, &1u64.to_be_bytes()),
            ("srcEid", 9, &STELLAR.to_be_bytes()),
            ("sender", 13, &f.id()),
            ("dstEid", 45, &SEPOLIA.to_be_bytes()),
            ("receiver", 49, &evm_word(EVM_PEER)),
            ("guid, as EVM recomputes it", 81, &guid),
            ("the FILL, byte for byte", 113, &message),
        ],
    );
    assert_eq!(packet.len(), 113 + message.len());
    assert_eq!(
        endpoint(&f.env).outbound_nonce(&f.settle.address, &SEPOLIA, &evm(&f.env, EVM_PEER)),
        1
    );
    write_fixture(
        "stellar_lz_fill",
        serde_json::json!({
            "about": "FILL from lz_settle (Stellar testnet, EID 40600) to LzSettle on Sepolia, \
                      as the real EndpointV2 emitted it (PacketSent.encoded_packet)",
            "packet": to_hex(&packet),
            "guid": to_hex(&packet[81..113]),
            "message": to_hex(&message),
            "amount": 992_500_000u64,
        }),
    );
}

#[test]
fn a_fail_from_sepolia_through_the_real_endpoint_returns_the_rwa() {
    let f = fork();
    let id = f.fill(&f.evm_bid(&Address::generate(&f.env)));
    deliver(&f, 1, &words(&[uint(FAIL), id.to_array()]));

    assert_eq!(f.balance(&f.rwa, &f.taker), 10 * UNITS);
    assert!(!f.settle.filled(&id));
    let peer = evm(&f.env, EVM_PEER);
    let e = endpoint(&f.env);
    assert_eq!(e.inbound_nonce(&f.settle.address, &SEPOLIA, &peer), 1);
    assert_eq!(
        e.inbound_payload_hash(&f.settle.address, &SEPOLIA, &peer, &1),
        None,
        "clear took the payload"
    );
}

// ------------------------------------------------------------------ maker here

#[test]
fn a_fill_from_sepolia_with_a_bad_signature_fails_home_once() {
    let f = fork();
    let request = [0x1D; 32];
    let mut fill = f.fill_from_sepolia(&request, 992_500_000);
    let last = fill.len() - 1;
    fill[last] ^= 1;
    let xlm = f.balance(&f.xlm, &f.settle.address);
    let params = deliver(&f, 1, &fill);
    assert_eq!(
        f.signers(),
        vec![at(&f.env, EXECUTOR)],
        "only the executor signs"
    );
    let back = f.emitted(ENDPOINT, "encoded_packet");
    assert_eq!(&back[49..81], &evm_word(EVM_PEER));
    assert_eq!(&back[113..], &words(&[uint(FAIL), request])[..]);
    assert!(
        f.balance(&f.xlm, &f.settle.address) < xlm,
        "paid from our balance"
    );
    assert_eq!(f.balance(&f.stable, &f.maker), 100 * UNITS);

    let executor = at(&f.env, EXECUTOR);
    let helper = ExecutorHelperClient::new(&f.env, &at(&f.env, EXECUTOR_HELPER));
    assert!(
        helper.try_execute(&executor, &params, &executor).is_err(),
        "the endpoint cleared it"
    );
}

// --------------------------------------------------- the Sepolia fork's own bytes

/// The Sepolia fork's FILL for a Stellar maker, its packet as the real endpoint
/// emitted it. Its signature is a placeholder, so we send FAIL home.
#[test]
fn sepolias_fill_packet_is_received_and_its_bad_signature_fails_home() {
    let Some(fx) = fixture("evm_lz_fill_to_stellar") else {
        return eprintln!("skipped: no {EVM_FIXTURES}/evm_lz_fill_to_stellar.json");
    };
    let f = fork();
    let packet = hex(fx["packet"].as_str().unwrap());
    assert_eq!(&packet[49..81], &f.id(), "sent to our id");
    assert_eq!(
        &packet[13..45],
        &evm_word(EVM_PEER),
        "from the peer we trust"
    );
    let deadline = u64::from_be_bytes(packet[113 + 376..113 + 384].try_into().unwrap());
    if f.env.ledger().timestamp() > deadline {
        soroban_sdk::testutils::Ledger::set_timestamp(&f.env.ledger(), deadline - 600);
    }
    deliver_packet(&f, &packet);

    let request: [u8; 32] = packet[113 + 32..113 + 64].try_into().unwrap();
    let back = f.emitted(ENDPOINT, "encoded_packet");
    assert_eq!(&back[49..81], &evm_word(EVM_PEER));
    assert_eq!(
        &back[113..],
        &words(&[uint(FAIL), request])[..],
        "FAIL for its request"
    );
    write_fixture(
        "stellar_lz_fail",
        serde_json::json!({
            "about": "FAIL from lz_settle (Stellar testnet) answering evm_lz_fill_to_stellar, as the \
                      real EndpointV2 emitted it",
            "packet": to_hex(&back),
            "message": to_hex(&back[113..]),
        }),
    );
}

/// The Sepolia fork's FAIL, its packet as the real endpoint emitted it.
#[test]
fn sepolias_fail_packet_returns_the_rwa_here() {
    let Some(fx) = fixture("evm_lz_fail_to_stellar") else {
        return eprintln!("skipped: no {EVM_FIXTURES}/evm_lz_fail_to_stellar.json");
    };
    let f = fork();
    let packet = hex(fx["packet"].as_str().unwrap());
    let request: [u8; 32] = packet[113 + 32..113 + 64].try_into().unwrap();
    f.put_order(&request, &f.order_for(9_900_000_000, 0, &f.taker));
    deliver_packet(&f, &packet);
    assert_eq!(
        f.balance(&f.rwa, &f.taker),
        11 * UNITS,
        "its 1,000 back on top of the 10,000"
    );
    assert!(!f.settle.filled(&BytesN::from_array(&f.env, &request)));
}
