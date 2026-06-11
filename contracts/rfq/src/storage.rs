//! Persistent storage layout and typed accessors.
//!
//! Order fill state and registries live in **persistent** storage (they must
//! outlive a single transaction and survive ledger archival via TTL bumps).
//! Configuration (admin, protocol fee) lives in **instance** storage so it is
//! loaded alongside the contract.

use soroban_sdk::{contracttype, Address, BytesN, Env};

/// Discriminates the two order families. 0x keeps separate min-salt mappings for
/// limit vs RFQ pair cancellation; this mirrors that.
#[contracttype]
#[derive(Clone, Copy)]
pub enum OrderKind {
    Rfq,
    Limit,
}

// Roughly 30 days of ledgers at ~5s close time, used to keep fill state and
// registry entries alive. Real deployments tune these to their needs.
const BUMP_THRESHOLD: u32 = 518_400;
const BUMP_EXTEND: u32 = 535_680;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Admin address (instance) — upgrade authority.
    Admin,
    /// order_hash -> taker-token amount already filled (persistent).
    Filled(BytesN<32>),
    /// order_hash -> explicitly cancelled flag (persistent).
    Cancelled(BytesN<32>),
    /// (kind, maker, maker_token, taker_token) -> minimum valid salt
    /// (persistent). Orders for this pair with `salt < min_salt` are cancelled.
    /// Kept separate for RFQ vs limit, matching 0x.
    MinSalt(OrderKind, Address, Address, Address),
    /// (maker, signer_pubkey) -> allowed. Which ed25519 keys may sign for a
    /// maker (persistent). Equivalent to 0x `orderSignerRegistry`.
    OrderSigner(Address, BytesN<32>),
    /// (origin_owner, submitter) -> allowed. Equivalent to 0x `originRegistry`.
    RfqOrigin(Address, Address),
}

fn bump(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, BUMP_THRESHOLD, BUMP_EXTEND);
}

// --- admin / config (instance) ---

pub fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn get_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

// --- fill state (persistent) ---

pub fn get_filled(env: &Env, order_hash: &BytesN<32>) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Filled(order_hash.clone()))
        .unwrap_or(0)
}

pub fn set_filled(env: &Env, order_hash: &BytesN<32>, amount: i128) {
    let key = DataKey::Filled(order_hash.clone());
    env.storage().persistent().set(&key, &amount);
    bump(env, &key);
}

pub fn is_cancelled(env: &Env, order_hash: &BytesN<32>) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::Cancelled(order_hash.clone()))
        .unwrap_or(false)
}

pub fn set_cancelled(env: &Env, order_hash: &BytesN<32>) {
    let key = DataKey::Cancelled(order_hash.clone());
    env.storage().persistent().set(&key, &true);
    bump(env, &key);
}

pub fn get_min_salt(
    env: &Env,
    kind: OrderKind,
    maker: &Address,
    maker_token: &Address,
    taker_token: &Address,
) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::MinSalt(
            kind,
            maker.clone(),
            maker_token.clone(),
            taker_token.clone(),
        ))
        .unwrap_or(0)
}

pub fn set_min_salt(
    env: &Env,
    kind: OrderKind,
    maker: &Address,
    maker_token: &Address,
    taker_token: &Address,
    min_salt: u64,
) {
    let key = DataKey::MinSalt(kind, maker.clone(), maker_token.clone(), taker_token.clone());
    env.storage().persistent().set(&key, &min_salt);
    bump(env, &key);
}

pub fn is_order_signer(env: &Env, maker: &Address, signer: &BytesN<32>) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::OrderSigner(maker.clone(), signer.clone()))
        .unwrap_or(false)
}

pub fn set_order_signer(env: &Env, maker: &Address, signer: &BytesN<32>, allowed: bool) {
    let key = DataKey::OrderSigner(maker.clone(), signer.clone());
    env.storage().persistent().set(&key, &allowed);
    bump(env, &key);
}

pub fn is_allowed_origin(env: &Env, origin_owner: &Address, submitter: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::RfqOrigin(origin_owner.clone(), submitter.clone()))
        .unwrap_or(false)
}

pub fn set_allowed_origin(env: &Env, origin_owner: &Address, submitter: &Address, allowed: bool) {
    let key = DataKey::RfqOrigin(origin_owner.clone(), submitter.clone());
    env.storage().persistent().set(&key, &allowed);
    bump(env, &key);
}

pub fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(BUMP_THRESHOLD, BUMP_EXTEND);
}
