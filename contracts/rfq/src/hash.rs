//! Canonical, domain-separated order hashing.
//!
//! 0x uses EIP-712 typed-data hashing. The Soroban equivalent here is:
//!
//! ```text
//! order_hash = sha256( DOMAIN_TAG || current_contract_address_xdr || order_xdr )
//! ```
//!
//! Using the canonical XDR encoding of the `#[contracttype]` struct means the
//! hash is fully reproducible: an off-chain signer never has to re-implement the
//! byte layout — it calls [`crate::RfqContract::get_rfq_order_hash`] /
//! `get_limit_order_hash` (read-only) to obtain the exact 32-byte digest, then
//! signs it with ed25519. The contract address in the preimage binds a signature
//! to this specific deployment (domain separation).

use soroban_sdk::{xdr::ToXdr, Bytes, BytesN, Env};

use crate::types::{LimitOrder, RfqOrder};

const RFQ_DOMAIN: &[u8] = b"STELLAR_RFQ_ORDER_V1";
const LIMIT_DOMAIN: &[u8] = b"STELLAR_LIMIT_ORDER_V1";

fn hash_with_domain(env: &Env, domain: &[u8], body: Bytes) -> BytesN<32> {
    let mut buf = Bytes::from_slice(env, domain);
    buf.append(&env.current_contract_address().to_xdr(env));
    buf.append(&body);
    env.crypto().sha256(&buf).to_bytes()
}

pub fn rfq_order_hash(env: &Env, order: &RfqOrder) -> BytesN<32> {
    hash_with_domain(env, RFQ_DOMAIN, order.clone().to_xdr(env))
}

pub fn limit_order_hash(env: &Env, order: &LimitOrder) -> BytesN<32> {
    hash_with_domain(env, LIMIT_DOMAIN, order.clone().to_xdr(env))
}
