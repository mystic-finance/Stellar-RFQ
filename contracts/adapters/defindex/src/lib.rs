#![no_std]
//! VenueAdapter over a single-asset DeFindex vault. DeFindex is not SEP-56:
//! amounts are per-asset vectors and exits are by shares, so this adapter
//! converts. One instance per owning vault; the position is this contract's
//! DeFindex shares. Interface verified against paltalabs/defindex 1.0.0.
//!
//! Exits are synchronous: DeFindex pays from idle and unwinds its strategies
//! in the same call, and reverts if a strategy cannot pay. `max_withdraw` is
//! therefore the position's value; a strategy short of liquidity surfaces as
//! a revert, which the owning vault treats as any venue failure.

use common::interfaces::VenueAdapter;
use common::math;
use soroban_sdk::auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation};
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, token, vec, Address, Env, IntoVal,
    Symbol, Val, Vec,
};

#[cfg(test)]
mod test;

/// The DeFindex vault functions this adapter decodes. `deposit` is invoked
/// untyped: its return carries allocation structs this adapter never reads.
#[contractclient(name = "DefindexClient")]
pub trait Defindex {
    fn withdraw(
        env: Env,
        withdraw_shares: i128,
        min_amounts_out: Vec<i128>,
        from: Address,
    ) -> Vec<i128>;
    fn get_asset_amounts_per_shares(env: Env, vault_shares: i128) -> Vec<i128>;
    fn total_supply(env: Env) -> i128;
    fn balance(env: Env, id: Address) -> i128;
}

#[contracttype]
enum Key {
    Owner,
    Asset,
    Vault,
}

#[contract]
pub struct DefindexAdapter;

#[contractimpl]
impl DefindexAdapter {
    pub fn __constructor(env: Env, owner: Address, asset: Address, venue: Address) {
        let s = env.storage().instance();
        s.set(&Key::Owner, &owner);
        s.set(&Key::Asset, &asset);
        s.set(&Key::Vault, &venue);
    }

    fn get(env: &Env, key: Key) -> Address {
        env.storage().instance().get(&key).unwrap()
    }

    fn vault(env: &Env) -> DefindexClient<'_> {
        DefindexClient::new(env, &Self::get(env, Key::Vault))
    }

    fn value_of(env: &Env, shares: i128) -> i128 {
        if shares <= 0 {
            return 0;
        }
        Self::vault(env)
            .get_asset_amounts_per_shares(&shares)
            .get(0)
            .unwrap_or(0)
    }
}

#[contractimpl]
impl VenueAdapter for DefindexAdapter {
    fn deposit(env: Env, amount: i128) -> i128 {
        Self::get(&env, Key::Owner).require_auth();
        let me = env.current_contract_address();
        let vault = Self::get(&env, Key::Vault);
        // DeFindex pulls from `from` (us) inside `deposit`.
        env.authorize_as_current_contract(vec![
            &env,
            InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: Self::get(&env, Key::Asset),
                    fn_name: Symbol::new(&env, "transfer"),
                    args: (me.clone(), vault.clone(), amount).into_val(&env),
                },
                sub_invocations: Vec::new(&env),
            }),
        ]);
        let args: Vec<Val> = vec![
            &env,
            vec![&env, amount].into_val(&env),
            vec![&env, amount].into_val(&env),
            me.into_val(&env),
            true.into_val(&env),
        ];
        env.invoke_contract::<Val>(&vault, &Symbol::new(&env, "deposit"), args);
        amount
    }

    /// Redeems the shares worth at least `amount`, or all of them, and
    /// forwards everything that arrived (rounding can add a unit or two).
    fn withdraw(env: Env, amount: i128, to: Address) -> i128 {
        Self::get(&env, Key::Owner).require_auth();
        let me = env.current_contract_address();
        let vault = Self::vault(&env);
        let held = vault.balance(&me);
        if amount <= 0 || held == 0 {
            return 0;
        }
        let supply = vault.total_supply();
        let managed = Self::value_of(&env, supply);
        let shares = match math::mul_div_ceil(&env, amount, supply, managed.max(1)) {
            Ok(s) => s.min(held),
            Err(_) => held,
        };
        let asset = token::Client::new(&env, &Self::get(&env, Key::Asset));
        let before = asset.balance(&me);
        vault.withdraw(&shares, &vec![&env, 0], &me);
        let received = asset.balance(&me) - before;
        asset.transfer(&me, &to, &received);
        received
    }

    fn total_assets(env: Env) -> i128 {
        Self::value_of(
            &env,
            Self::vault(&env).balance(&env.current_contract_address()),
        )
    }

    fn max_withdraw(env: Env) -> i128 {
        Self::total_assets(env)
    }
}
