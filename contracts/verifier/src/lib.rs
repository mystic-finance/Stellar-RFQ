#![no_std]
//! `ed25519_verify` traps on a bad signature, and a contract cannot catch a
//! trap of its own: only a failed call into another contract comes back as an
//! error. `crosschain_settle` calls this to turn a bad maker signature into a
//! FAIL message instead of a stuck one.

use soroban_sdk::{contract, contractimpl, Bytes, BytesN, Env};

#[contract]
pub struct Verifier;

#[contractimpl]
impl Verifier {
    /// Returns if `signature` is `key`'s over `message`; traps otherwise.
    pub fn verify(env: Env, key: BytesN<32>, message: Bytes, signature: BytesN<64>) {
        env.crypto().ed25519_verify(&key, &message, &signature);
    }
}
