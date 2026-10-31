//! The messages between this contract and its peers, byte-identical to the
//! EVM `CrossChainSettle`'s: 32-byte ABI words.
//!
//! - FILL: `(0, id, amount, Bid) ++ signature`, amount with 6 decimals
//! - FAIL: `(1, id)`
//! - PAID: `(2, id)`, riding with the payment
//! - PAYOUT: `(3, recipient, recipientIsContract)`, riding with the payment
//!
//! Accounts are 32-byte words: an EVM address left-padded, or a Stellar key
//! with a flag for `C…` over `G…`.

use soroban_sdk::{contracttype, Address, Bytes, BytesN, Env};

pub const FILL: u32 = 0;
pub const FAIL: u32 = 1;
pub const PAID: u32 = 2;
pub const PAYOUT: u32 = 3;
/// Where FILL's bid words start, and where its signature does.
pub const BID_AT: u32 = 96;
pub const SIG_AT: u32 = 416;

/// A maker's bid for a request on another chain, signed on the maker's chain.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bid {
    /// Pays on its chain, and receives REDEEM_PUSH proceeds and CROSS_DELIVER's RWA there.
    pub maker: BytesN<32>,
    pub maker_is_contract: bool,
    /// The token the maker pays in, on its chain.
    pub stable: BytesN<32>,
    /// The most the fill may pull from the maker, 6 decimals.
    pub max_amount: i128,
    pub maker_bps_per_day: u32,
    /// 0 DELIVER, 1 REDEEM_CT, 2 REDEEM_PUSH, 3 CROSS_DELIVER.
    pub delivery: u32,
    /// Taker's-chain account for DELIVER and REDEEM_CT.
    pub recipient: BytesN<32>,
    pub recipient_is_contract: bool,
    pub deadline: u64,
    pub salt: BytesN<32>,
}

pub fn bid_words(env: &Env, b: &Bid) -> Bytes {
    words(
        env,
        &[
            b.maker.clone(),
            uint(env, b.maker_is_contract as u128),
            b.stable.clone(),
            uint(env, b.max_amount as u128),
            uint(env, b.maker_bps_per_day as u128),
            uint(env, b.delivery as u128),
            b.recipient.clone(),
            uint(env, b.recipient_is_contract as u128),
            uint(env, b.deadline as u128),
            b.salt.clone(),
        ],
    )
}

pub fn fill(env: &Env, id: &BytesN<32>, amount: i128, bid: &Bid, signature: &Bytes) -> Bytes {
    let mut out = words(
        env,
        &[
            uint(env, FILL as u128),
            id.clone(),
            uint(env, amount as u128),
        ],
    );
    out.append(&bid_words(env, bid));
    out.append(signature);
    out
}

/// The bid in a FILL, if it is long enough to hold one.
pub fn parse_bid(m: &Bytes) -> Option<Bid> {
    let w = |i: u32| word_at(m, BID_AT + 32 * i);
    (m.len() >= SIG_AT).then(|| Bid {
        maker: w(0),
        maker_is_contract: tail(&w(1)) != 0,
        stable: w(2),
        max_amount: tail(&w(3)) as i128,
        maker_bps_per_day: tail(&w(4)) as u32,
        delivery: tail(&w(5)) as u32,
        recipient: w(6),
        recipient_is_contract: tail(&w(7)) != 0,
        deadline: tail(&w(8)) as u64,
        salt: w(9),
    })
}

/// FAIL or PAID.
pub fn about(env: &Env, kind: u32, id: &BytesN<32>) -> Bytes {
    words(env, &[uint(env, kind as u128), id.clone()])
}

pub fn payout(env: &Env, to: &BytesN<32>, to_is_contract: bool) -> Bytes {
    words(
        env,
        &[
            uint(env, PAYOUT as u128),
            to.clone(),
            uint(env, to_is_contract as u128),
        ],
    )
}

pub fn kind(m: &Bytes) -> u32 {
    tail(&word(m, 0)) as u32
}

pub fn word(m: &Bytes, i: u32) -> BytesN<32> {
    word_at(m, 32 * i)
}

pub fn word_at(m: &Bytes, at: u32) -> BytesN<32> {
    m.slice(at..at + 32).try_into().unwrap()
}

/// The low 16 bytes of a word, which hold every number in these messages.
pub fn tail(w: &BytesN<32>) -> u128 {
    let mut b = [0u8; 16];
    b.copy_from_slice(&w.to_array()[16..]);
    u128::from_be_bytes(b)
}

pub fn uint(env: &Env, v: u128) -> BytesN<32> {
    let mut w = [0u8; 32];
    w[16..].copy_from_slice(&v.to_be_bytes());
    BytesN::from_array(env, &w)
}

fn words(env: &Env, ws: &[BytesN<32>]) -> Bytes {
    let mut out = Bytes::new(env);
    for w in ws {
        out.extend_from_array(&w.to_array());
    }
    out
}

/// The 32 raw bytes behind a Stellar address: an account's ed25519 key or a
/// contract's id, the tail of its XDR either way.
pub fn key(env: &Env, a: &Address) -> BytesN<32> {
    use soroban_sdk::xdr::ToXdr;
    let xdr = a.clone().to_xdr(env);
    xdr.slice(xdr.len() - 32..).try_into().unwrap()
}

/// True for a contract (`C…`), false for an account (`G…`).
pub fn is_contract(env: &Env, a: &Address) -> bool {
    use soroban_sdk::xdr::ToXdr;
    a.clone().to_xdr(env).len() != 44
}

/// A Stellar address from its 32-byte key, rebuilt as its ScVal XDR.
pub fn address(env: &Env, key: &BytesN<32>, contract: bool) -> Address {
    use soroban_sdk::xdr::FromXdr;
    let head: &[u8] = if contract {
        &[0, 0, 0, 18, 0, 0, 0, 1]
    } else {
        &[0, 0, 0, 18, 0, 0, 0, 0, 0, 0, 0, 0]
    };
    let mut xdr = Bytes::from_slice(env, head);
    xdr.extend_from_array(&key.to_array());
    Address::from_xdr(env, &xdr).unwrap()
}
