#![no_std]
//! A minimal fungible token implementing the Soroban token interface entry
//! points used by the RFQ contract and demo: `mint`, `balance`, `approve`,
//! `allowance`, `transfer`, `transfer_from`, plus metadata.
//!
//! Unlike a wrapped classic asset (Stellar Asset Contract), this needs no
//! trustlines — `mint` simply credits a balance — which keeps the end-to-end
//! demo self-contained. It is **not** meant for production use.

use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, contracterror, Address, Env, String,
};

#[contracterror]
#[derive(Clone, Copy)]
#[repr(u32)]
pub enum TokenError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    InsufficientBalance = 3,
    InsufficientAllowance = 4,
    NegativeAmount = 5,
}

#[contracttype]
#[derive(Clone)]
pub struct AllowanceValue {
    pub amount: i128,
    pub expiration_ledger: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct Meta {
    pub decimal: u32,
    pub name: String,
    pub symbol: String,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Meta,
    Balance(Address),
    Allowance(Address, Address), // (from, spender)
}

fn read_balance(env: &Env, id: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Balance(id.clone()))
        .unwrap_or(0)
}

fn write_balance(env: &Env, id: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::Balance(id.clone()), &amount);
}

fn read_allowance(env: &Env, from: &Address, spender: &Address) -> AllowanceValue {
    env.storage()
        .persistent()
        .get(&DataKey::Allowance(from.clone(), spender.clone()))
        .unwrap_or(AllowanceValue { amount: 0, expiration_ledger: 0 })
}

fn check_nonneg(env: &Env, amount: i128) {
    if amount < 0 {
        panic_with_error!(env, TokenError::NegativeAmount);
    }
}

#[contract]
pub struct TestToken;

#[contractimpl]
impl TestToken {
    pub fn initialize(env: Env, admin: Address, decimal: u32, name: String, symbol: String) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, TokenError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Meta, &Meta { decimal, name, symbol });
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        check_nonneg(&env, amount);
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        let balance = read_balance(&env, &to) + amount;
        write_balance(&env, &to, balance);
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        read_balance(&env, &id)
    }

    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        let a = read_allowance(&env, &from, &spender);
        if a.expiration_ledger < env.ledger().sequence() {
            0
        } else {
            a.amount
        }
    }

    pub fn approve(env: Env, from: Address, spender: Address, amount: i128, expiration_ledger: u32) {
        check_nonneg(&env, amount);
        from.require_auth();
        env.storage().persistent().set(
            &DataKey::Allowance(from.clone(), spender.clone()),
            &AllowanceValue { amount, expiration_ledger },
        );
        env.events()
            .publish((soroban_sdk::symbol_short!("approve"), from, spender), (amount, expiration_ledger));
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        check_nonneg(&env, amount);
        from.require_auth();
        Self::do_transfer(&env, &from, &to, amount);
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        check_nonneg(&env, amount);
        spender.require_auth();
        Self::spend_allowance(&env, &from, &spender, amount);
        Self::do_transfer(&env, &from, &to, amount);
    }

    pub fn decimals(env: Env) -> u32 {
        let m: Meta = env.storage().instance().get(&DataKey::Meta).unwrap();
        m.decimal
    }

    pub fn name(env: Env) -> String {
        let m: Meta = env.storage().instance().get(&DataKey::Meta).unwrap();
        m.name
    }

    pub fn symbol(env: Env) -> String {
        let m: Meta = env.storage().instance().get(&DataKey::Meta).unwrap();
        m.symbol
    }

    // --- internals ---

    fn do_transfer(env: &Env, from: &Address, to: &Address, amount: i128) {
        let from_balance = read_balance(env, from);
        if from_balance < amount {
            panic_with_error!(env, TokenError::InsufficientBalance);
        }
        write_balance(env, from, from_balance - amount);
        write_balance(env, to, read_balance(env, to) + amount);
        env.events()
            .publish((soroban_sdk::symbol_short!("transfer"), from.clone(), to.clone()), amount);
    }

    fn spend_allowance(env: &Env, from: &Address, spender: &Address, amount: i128) {
        let a = read_allowance(env, from, spender);
        let live = a.expiration_ledger >= env.ledger().sequence();
        if !live || a.amount < amount {
            panic_with_error!(env, TokenError::InsufficientAllowance);
        }
        env.storage().persistent().set(
            &DataKey::Allowance(from.clone(), spender.clone()),
            &AllowanceValue { amount: a.amount - amount, expiration_ledger: a.expiration_ledger },
        );
    }
}
