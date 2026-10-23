#![no_std]
//! A redemption batch's claim token (CT). The vault that deployed it mints one
//! per RWA unit a batch takes in and burns it when the holder cancels or
//! claims. It trades like any SEP-41 token; with a gate set, every holder but
//! the vault must pass it. See docs/REDEMPTION_VAULT.md.

use common::interfaces::{ClaimToken as ClaimTokenApi, GateClient};
use soroban_sdk::token::{self, TokenInterface};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env, String,
};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    NotEligible = 1,
}

#[contracttype]
enum Key {
    Minter,
    Rwa,
    Batch,
    Gate,
}

#[contract]
pub struct ClaimToken;

#[contractimpl]
impl ClaimToken {
    pub fn __constructor(
        env: Env,
        minter: Address,
        rwa: Address,
        batch: u64,
        gate: Option<Address>,
        name: String,
        symbol: String,
    ) {
        let decimals = token::Client::new(&env, &rwa).decimals();
        shares::init_meta(&env, decimals, name, symbol);
        let s = env.storage().instance();
        s.set(&Key::Minter, &minter);
        s.set(&Key::Rwa, &rwa);
        s.set(&Key::Batch, &batch);
        s.set(&Key::Gate, &gate);
    }

    pub fn minter(env: Env) -> Address {
        env.storage().instance().get(&Key::Minter).unwrap()
    }

    pub fn rwa(env: Env) -> Address {
        env.storage().instance().get(&Key::Rwa).unwrap()
    }

    pub fn batch(env: Env) -> u64 {
        env.storage().instance().get(&Key::Batch).unwrap()
    }

    pub fn gate(env: Env) -> Option<Address> {
        env.storage().instance().get(&Key::Gate).unwrap()
    }

    fn check_holder(env: &Env, to: &Address) {
        let Some(gate) = Self::gate(env.clone()) else {
            return;
        };
        if to != &Self::minter(env.clone())
            && !GateClient::new(env, &gate).can_hold(&Self::rwa(env.clone()), to)
        {
            panic_with_error!(env, Error::NotEligible);
        }
    }
}

#[contractimpl]
impl ClaimTokenApi for ClaimToken {
    fn mint(env: Env, to: Address, amount: i128) {
        Self::minter(env.clone()).require_auth();
        Self::check_holder(&env, &to);
        shares::mint(&env, &to, amount);
    }

    fn burn_claim(env: Env, from: Address, amount: i128) {
        Self::minter(env.clone()).require_auth();
        shares::burn(&env, &from, amount);
    }

    fn set_gate(env: Env, gate: Option<Address>) {
        Self::minter(env.clone()).require_auth();
        env.storage().instance().set(&Key::Gate, &gate);
        env.events().publish((symbol_short!("gate"),), gate);
    }

    fn total_supply(env: Env) -> i128 {
        shares::supply(&env)
    }
}

#[contractimpl]
impl TokenInterface for ClaimToken {
    fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        shares::allowance(&env, &from, &spender)
    }

    fn approve(env: Env, from: Address, spender: Address, amount: i128, expiration_ledger: u32) {
        from.require_auth();
        shares::approve(&env, &from, &spender, amount, expiration_ledger);
    }

    fn balance(env: Env, id: Address) -> i128 {
        shares::balance(&env, &id)
    }

    fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        Self::check_holder(&env, &to);
        shares::transfer(&env, &from, &to, amount);
    }

    fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        spender.require_auth();
        Self::check_holder(&env, &to);
        shares::spend_allowance(&env, &from, &spender, amount);
        shares::transfer(&env, &from, &to, amount);
    }

    fn burn(env: Env, from: Address, amount: i128) {
        from.require_auth();
        shares::burn(&env, &from, amount);
    }

    fn burn_from(env: Env, spender: Address, from: Address, amount: i128) {
        spender.require_auth();
        shares::spend_allowance(&env, &from, &spender, amount);
        shares::burn(&env, &from, amount);
    }

    fn decimals(env: Env) -> u32 {
        shares::meta(&env).decimals
    }

    fn name(env: Env) -> String {
        shares::meta(&env).name
    }

    fn symbol(env: Env) -> String {
        shares::meta(&env).symbol
    }
}
