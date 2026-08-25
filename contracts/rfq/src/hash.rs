use soroban_sdk::{xdr::ToXdr, Bytes, BytesN, Env};

use crate::types::{FixedOrder, Request, RfqOrder};

const REQUEST_DOMAIN: &[u8] = b"OCTARINE_REQUEST_V1";
const RFQ_DOMAIN: &[u8] = b"OCTARINE_RFQ_ORDER_V1";
const FIXED_DOMAIN: &[u8] = b"OCTARINE_FIXED_ORDER_V1";
const SEP53_PREFIX: &[u8] = b"Stellar Signed Message:\n";

fn digest(env: &Env, domain: &[u8], body: Bytes) -> BytesN<32> {
    let mut buf = Bytes::from_slice(env, domain);
    buf.append(&env.current_contract_address().to_xdr(env));
    buf.append(&body);
    env.crypto().sha256(&buf).to_bytes()
}

pub fn request(env: &Env, r: &Request) -> BytesN<32> {
    digest(env, REQUEST_DOMAIN, r.clone().to_xdr(env))
}

pub fn rfq_order(env: &Env, order: &RfqOrder) -> BytesN<32> {
    digest(env, RFQ_DOMAIN, order.clone().to_xdr(env))
}

pub fn fixed_order(env: &Env, order: &FixedOrder) -> BytesN<32> {
    digest(env, FIXED_DOMAIN, order.clone().to_xdr(env))
}

/// `SHA256("Stellar Signed Message:\n" || order_hash)` — what a SEP-53 wallet signs.
pub fn sep53(env: &Env, order_hash: &BytesN<32>) -> BytesN<32> {
    let mut buf = Bytes::from_slice(env, SEP53_PREFIX);
    buf.append(&Bytes::from_array(env, &order_hash.to_array()));
    env.crypto().sha256(&buf).to_bytes()
}
