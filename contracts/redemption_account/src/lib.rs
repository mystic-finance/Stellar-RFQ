#![no_std]
//! An address that faces the issuer for one redemption batch at a time, so an
//! issuer that allows one redemption per address does not serialise the vault.
//! Its vault deploys a pool of them at predictable addresses, which can be
//! whitelisted with the issuer ahead of use, and recycles them. Only the vault
//! calls it, and the vault has already checked every call with its verifier.

use common::interfaces::RedemptionAccount as RedemptionAccountApi;
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, Symbol, Val, Vec};

#[contracttype]
enum Key {
    Vault,
}

#[contract]
pub struct RedemptionAccount;

#[contractimpl]
impl RedemptionAccount {
    pub fn __constructor(env: Env, vault: Address) {
        env.storage().instance().set(&Key::Vault, &vault);
    }

    pub fn vault(env: Env) -> Address {
        env.storage().instance().get(&Key::Vault).unwrap()
    }
}

#[contractimpl]
impl RedemptionAccountApi for RedemptionAccount {
    fn execute(env: Env, target: Address, func: Symbol, args: Vec<Val>) -> Val {
        Self::vault(env.clone()).require_auth();
        env.invoke_contract(&target, &func, args)
    }

    /// An allowance good for this ledger only, for an issuer that pulls.
    fn approve(env: Env, token: Address, spender: Address, amount: i128) {
        Self::vault(env.clone()).require_auth();
        let me = env.current_contract_address();
        token::Client::new(&env, &token).approve(&me, &spender, &amount, &env.ledger().sequence());
    }

    fn sweep(env: Env, token: Address, to: Address) -> i128 {
        Self::vault(env.clone()).require_auth();
        let t = token::Client::new(&env, &token);
        let me = env.current_contract_address();
        let amount = t.balance(&me);
        if amount != 0 {
            t.transfer(&me, &to, &amount);
        }
        amount
    }
}
