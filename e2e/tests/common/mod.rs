//! What both suites share: the snapshot, our RFQ and verifier on it, a maker
//! and a taker, and the bytes. Each suite imports its own `settle` wasm.
#![allow(dead_code)]

use crate::settle;
use ed25519_dalek::Signer as _;
use soroban_sdk::address_payload::AddressPayload;
use soroban_sdk::testutils::{Address as _, EnvTestConfig, Events as _};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::xdr::{ContractEventBody, ScVal};
use soroban_sdk::{Address, Bytes, BytesN, Env, Symbol};

pub mod rfq {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/release/rfq.wasm");
}
const VERIFIER: &[u8] = include_bytes!("../../../target/wasm32v1-none/release/verifier.wasm");

const SNAPSHOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/testnet.json");
pub const EVM_FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../octarine-v2/test/fork/fixtures"
);
const OUR_FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../contracts/crosschain_settle/fixtures"
);

// Stellar testnet.
pub const USDC: &str = "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA";
pub const XLM: &str = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
/// The executor's fee deposit account, holding testnet XLM to fund with.
const FUNDER: &str = "GCRN5PMAG5FM5QLCH7BUZPRQ7UIW37LBZLF2BIDEOSG4ZQ6HYRC45ALA";

// Ethereum Sepolia.
pub const SEPOLIA_USDC: &str = "1c7d4b196cb0c7b01d743fbc6116a902379c7238";
/// Our contract's id: keccak256("stellar-rfq/crosschain_settle"), as the Sepolia side has it.
const ME: &str = "929881ea73aa7171898b6eff9d1740cc513d8f7352c0126e525d5f7f65847237";

pub const ONE: i128 = 1_000_000_000_000_000_000;
pub const DAY: u64 = 86_400;
pub const UNITS: i128 = 10_000_000_000; // 1,000 RWA at 7 decimals
pub const XLM_1: i128 = 10_000_000;
pub const RETURN_FEE: i128 = XLM_1;
pub const DELIVER: u32 = 0;
pub const FILL: u128 = 0;
pub const FAIL: u128 = 1;
pub const PAID: u128 = 2;

pub struct Fork {
    pub env: Env,
    pub admin: Address,
    pub settle: settle::Client<'static>,
    pub rfq: rfq::Client<'static>,
    /// The bridge's chain id for Sepolia, and the stable routed there, `remote` on Sepolia.
    pub chain: u32,
    pub stable: Address,
    pub remote: BytesN<32>,
    pub xlm: Address,
    pub rwa: Address,
    pub taker: Address,
    pub fee_to: Address,
    pub maker: Address,
    pub key: ed25519_dalek::SigningKey,
}

/// The snapshot with our contract deployed on `bridge`, at our id, with a tip
/// of `tip`, peered with `peer` on Sepolia (`chain`) and routing `stable` to `remote`.
pub fn fork(
    chain: u32,
    peer: &str,
    tip: i128,
    options: &str,
    bridge: impl FnOnce(&Env) -> (settle::Bridge, Address, BytesN<32>),
) -> Fork {
    let mut env = Env::from_ledger_snapshot_file(SNAPSHOT);
    env.set_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    env.cost_estimate().budget().reset_unlimited();
    let [admin, taker, fee_to, maker] = [0; 4].map(|_| Address::generate(&env));
    let (bridge, stable, remote) = bridge(&env);
    let xlm = at(&env, XLM);
    let rwa = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    let rfq = rfq::Client::new(&env, &env.register(rfq::WASM, ()));
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

    let config = settle::Config {
        rfq: rfq.address.clone(),
        verifier: env.register(VERIFIER, ()),
        xlm: xlm.clone(),
        tip,
        bridge,
    };
    let me = contract(&env, &hex(ME));
    env.register_at(
        &me,
        settle::WASM,
        (admin.clone(), admin.clone(), 0u64, config),
    );
    let settle = settle::Client::new(&env, &me);
    let peer = settle::Peer {
        addr: evm(&env, peer),
        return_fee: RETURN_FEE,
        options: Bytes::from_slice(&env, &hex(options)),
    };
    for change in [
        settle::Change::Peer(chain, peer),
        settle::Change::Route(chain, stable.clone(), Some(remote.clone())),
    ] {
        settle.submit(&change);
        settle.accept(&settle.kind_of(&change));
    }

    let key = ed25519_dalek::SigningKey::from_bytes(&[11u8; 32]);
    rfq.register_order_signer(
        &maker,
        &BytesN::from_array(&env, &key.verifying_key().to_bytes()),
        &true,
    );
    StellarAssetClient::new(&env, &rwa).mint(&taker, &(10 * UNITS));
    StellarAssetClient::new(&env, &stable).mint(&maker, &(100 * UNITS));
    TokenClient::new(&env, &stable).approve(
        &maker,
        &me,
        &(100 * UNITS),
        &(env.ledger().sequence() + 100_000),
    );
    let funder = at(&env, FUNDER);
    for who in [&taker, &me] {
        TokenClient::new(&env, &xlm).transfer(&funder, who, &(100 * XLM_1));
    }

    Fork {
        env,
        admin,
        settle,
        rfq,
        chain,
        stable,
        remote,
        xlm,
        rwa,
        taker,
        fee_to,
        maker,
        key,
    }
}

impl Fork {
    pub fn request(&self) -> settle::Request {
        settle::Request {
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
            order_type: settle::OrderType::Rfq,
        }
    }

    /// The request's id, as the RFQ contract hashes it.
    pub fn request_id(&self) -> BytesN<32> {
        let r = self.request();
        self.rfq.hash_request(&rfq::Request {
            maker_token: r.maker_token,
            taker_token: r.taker_token,
            taker_amount: r.taker_amount,
            min_received_amount: r.min_received_amount,
            fee_bps: r.fee_bps,
            taker: r.taker,
            sender: r.sender,
            fee_recipient: r.fee_recipient,
            expiry: r.expiry,
            salt: r.salt,
            taker_max_bps_per_day: r.taker_max_bps_per_day,
            order_type: rfq::OrderType::Rfq,
        })
    }

    /// A Sepolia maker's bid in the routed stable there, delivering to `recipient` here.
    pub fn evm_bid(&self, recipient: &Address) -> settle::Bid {
        settle::Bid {
            maker: evm(&self.env, "a11ce"),
            maker_is_contract: false,
            stable: self.remote.clone(),
            max_amount: 1_000_000_000,
            maker_bps_per_day: 250,
            delivery: DELIVER,
            recipient: BytesN::from_array(&self.env, &id(recipient)),
            recipient_is_contract: true,
            deadline: self.env.ledger().timestamp() + 3_600,
            salt: BytesN::from_array(&self.env, &uint(7)),
        }
    }

    pub fn fill(&self, bid: &settle::Bid) -> BytesN<32> {
        let evm_sig = Bytes::from_array(&self.env, &[0xAA; 65]);
        self.settle
            .fill(&self.request(), &self.chain, bid, &evm_sig)
    }

    /// FILL for `bid`, as the Sepolia peer sends it, with a placeholder signature.
    pub fn fill_to_sepolia(&self, id: &BytesN<32>, bid: &settle::Bid) -> Vec<u8> {
        let head = words(&[uint(FILL), id.to_array(), uint(992_500_000)]);
        [head, bid_words(bid), vec![0xAA; 65]].concat()
    }

    /// A FILL from a Sepolia taker for a bid this Stellar maker signed (SEP-53).
    pub fn fill_from_sepolia(&self, request_id: &[u8; 32], amount: u128) -> Vec<u8> {
        let e = &self.env;
        let bid = settle::Bid {
            maker: BytesN::from_array(e, &id(&self.maker)),
            maker_is_contract: true,
            stable: BytesN::from_array(e, &id(&self.stable)),
            max_amount: 1_000_000_000,
            maker_bps_per_day: 250,
            delivery: DELIVER,
            recipient: evm(e, "1b"),
            recipient_is_contract: false,
            deadline: e.ledger().timestamp() + 3_600,
            salt: BytesN::from_array(e, &uint(7)),
        };
        let hash = self
            .settle
            .hash_bid(&BytesN::from_array(e, request_id), &self.chain, &bid);
        let mut msg = b"Stellar Signed Message:\n".to_vec();
        msg.extend_from_slice(&hash.to_array());
        let digest = e.crypto().sha256(&Bytes::from_slice(e, &msg)).to_array();
        let mut sig = self.key.verifying_key().to_bytes().to_vec();
        sig.extend_from_slice(&self.key.sign(&digest).to_bytes());
        [
            words(&[uint(FILL), *request_id, uint(amount)]),
            bid_words(&bid),
            sig,
        ]
        .concat()
    }

    pub fn balance(&self, token: &Address, who: &Address) -> i128 {
        TokenClient::new(&self.env, token).balance(who)
    }

    /// The `field` bytes of the last event `contract` emitted in the last call.
    pub fn emitted(&self, contract: &str, field: &str) -> Vec<u8> {
        let events = self.env.events().all();
        let events = events.filter_by_contract(&at(&self.env, contract));
        let field = ScVal::Symbol(field.try_into().unwrap());
        events
            .events()
            .iter()
            .rev()
            .find_map(|e| {
                let ContractEventBody::V0(body) = &e.body;
                match &body.data {
                    ScVal::Map(Some(m)) => m.iter().find(|x| x.key == field).and_then(|x| match &x
                        .val
                    {
                        ScVal::Bytes(b) => Some(b.to_vec()),
                        _ => None,
                    }),
                    _ => None,
                }
            })
            .unwrap_or_else(|| panic!("no {field:?} from {contract}"))
    }

    /// Who signed the last call: real chains would ask each of them, and our
    /// contract must not be among them (it cannot sign).
    pub fn signers(&self) -> Vec<Address> {
        self.env.auths().into_iter().map(|(a, _)| a).collect()
    }

    /// The order state `fill` writes, for a request id another chain chose.
    pub fn put_order(&self, request_id: &[u8; 32], order: &settle::Order) {
        let e = &self.env;
        let rid = BytesN::from_array(e, request_id);
        e.as_contract(&self.settle.address, || {
            let s = e.storage().persistent();
            s.set(&(Symbol::new(e, "Filled"), rid.clone()), &true);
            s.set(&(Symbol::new(e, "Order"), rid.clone()), order);
        });
        StellarAssetClient::new(e, &self.rwa).mint(&self.settle.address, &order.units);
    }

    pub fn order_for(&self, amount: i128, tip: i128, recipient: &Address) -> settle::Order {
        settle::Order {
            taker: self.taker.clone(),
            chain: self.chain,
            rwa: self.rwa.clone(),
            units: UNITS,
            stable: self.stable.clone(),
            amount,
            fee: amount / 1000,
            tip,
            fee_recipient: self.fee_to.clone(),
            delivery: DELIVER,
            recipient: recipient.clone(),
            maker: evm(&self.env, "a11ce"),
            maker_is_contract: false,
        }
    }

    pub fn id(&self) -> [u8; 32] {
        id(&self.settle.address)
    }
}

// --------------------------------------------------------------------- bytes

pub fn hex(s: &str) -> Vec<u8> {
    let s = s.trim_start_matches("0x");
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

pub fn to_hex(b: &[u8]) -> String {
    let mut s = String::from("0x");
    for x in b {
        s += &format!("{x:02x}");
    }
    s
}

pub fn uint(v: u128) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[16..].copy_from_slice(&v.to_be_bytes());
    w
}

/// A hex EVM address (or any short value) as a left-padded word.
pub fn evm_word(h: &str) -> [u8; 32] {
    let mut b = hex(&format!("{h:0>64}"));
    b.truncate(32);
    b.try_into().unwrap()
}

pub fn evm(env: &Env, h: &str) -> BytesN<32> {
    BytesN::from_array(env, &evm_word(h))
}

pub fn words(ws: &[[u8; 32]]) -> Vec<u8> {
    ws.concat()
}

pub fn bid_words(b: &settle::Bid) -> Vec<u8> {
    words(&[
        b.maker.to_array(),
        uint(b.maker_is_contract as u128),
        b.stable.to_array(),
        uint(b.max_amount as u128),
        uint(b.maker_bps_per_day as u128),
        uint(b.delivery as u128),
        b.recipient.to_array(),
        uint(b.recipient_is_contract as u128),
        uint(b.deadline as u128),
        b.salt.to_array(),
    ])
}

pub fn keccak(env: &Env, b: &[u8]) -> [u8; 32] {
    env.crypto()
        .keccak256(&Bytes::from_slice(env, b))
        .to_array()
}

pub fn at(env: &Env, strkey: &str) -> Address {
    Address::from_str(env, strkey)
}

fn contract(env: &Env, id: &[u8]) -> Address {
    Address::from_payload(
        env,
        AddressPayload::ContractIdHash(BytesN::from_array(env, id.try_into().unwrap())),
    )
}

/// The 32 bytes behind an address: a contract's id or an account's key.
pub fn id(a: &Address) -> [u8; 32] {
    match a.to_payload().unwrap() {
        AddressPayload::ContractIdHash(h) => h.to_array(),
        AddressPayload::AccountIdPublicKeyEd25519(k) => k.to_array(),
    }
}

/// Checks each `(name, offset, bytes)` in `m`.
pub fn assert_fields(m: &[u8], fields: &[(&str, usize, &[u8])]) {
    for (name, at, want) in fields {
        assert_eq!(&m[*at..*at + want.len()], *want, "{name} at {at}");
    }
}

pub fn fixture(name: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(format!("{EVM_FIXTURES}/{name}.json")).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

pub fn write_fixture(name: &str, value: serde_json::Value) {
    std::fs::create_dir_all(OUR_FIXTURES).unwrap();
    let text = serde_json::to_string_pretty(&value).unwrap() + "\n";
    std::fs::write(format!("{OUR_FIXTURES}/{name}.json"), text).unwrap();
}
