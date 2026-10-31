#![no_std]
//! Test/testnet swap venue: pays `rate_bps` of the input in the output token
//! from its own inventory, less an optional `shortfall` to model a venue that
//! returns less than it reports. Never deploy to mainnet.

use common::interfaces::DexAdapter;
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Bytes, Env};

#[contracttype]
enum Key {
    Rate,
    Shortfall,
}

#[contract]
pub struct MockDex;

#[contractimpl]
impl MockDex {
    pub fn __constructor(env: Env, rate_bps: i128, shortfall: i128) {
        env.storage().instance().set(&Key::Rate, &rate_bps);
        env.storage().instance().set(&Key::Shortfall, &shortfall);
    }
}

#[contractimpl]
impl DexAdapter for MockDex {
    fn quote(
        env: Env,
        _token_in: Address,
        _token_out: Address,
        amount_in: i128,
        _data: Bytes,
    ) -> i128 {
        let rate: i128 = env.storage().instance().get(&Key::Rate).unwrap();
        amount_in * rate / 10_000
    }

    fn swap(
        env: Env,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        min_out: i128,
        data: Bytes,
        to: Address,
    ) -> i128 {
        let short: i128 = env.storage().instance().get(&Key::Shortfall).unwrap();
        let out = Self::quote(env.clone(), token_in, token_out.clone(), amount_in, data) - short;
        assert!(out >= min_out, "below min");
        token::Client::new(&env, &token_out).transfer(&env.current_contract_address(), &to, &out);
        out
    }
}
