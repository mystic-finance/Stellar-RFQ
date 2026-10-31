#![no_std]
//! VenueAdapter over any SEP-56 vault, Octarine's provisioning vaults
//! included. One instance per owning vault; the position is this contract's
//! share balance in the target vault.

use common::interfaces::VenueAdapter;
use soroban_sdk::auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation};
use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error, vec,
    Address, Env, IntoVal, Symbol, Vec,
};

#[cfg(test)]
mod test;

/// The SEP-56 functions this adapter uses.
#[contractclient(name = "Sep56Client")]
pub trait Sep56 {
    fn query_asset(env: Env) -> Address;
    fn deposit(env: Env, assets: i128, receiver: Address, from: Address, operator: Address)
        -> i128;
    fn withdraw(
        env: Env,
        assets: i128,
        receiver: Address,
        owner: Address,
        operator: Address,
    ) -> i128;
    fn convert_to_assets(env: Env, shares: i128) -> i128;
    fn max_withdraw(env: Env, owner: Address) -> i128;
    fn balance(env: Env, id: Address) -> i128;
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AssetMismatch = 1,
}

#[contracttype]
enum Key {
    Owner,
    Asset,
    Venue,
}

#[contract]
pub struct Sep56Adapter;

#[contractimpl]
impl Sep56Adapter {
    pub fn __constructor(env: Env, owner: Address, asset: Address, venue: Address) {
        if Sep56Client::new(&env, &venue).query_asset() != asset {
            panic_with_error!(&env, Error::AssetMismatch);
        }
        let s = env.storage().instance();
        s.set(&Key::Owner, &owner);
        s.set(&Key::Asset, &asset);
        s.set(&Key::Venue, &venue);
    }

    fn get(env: &Env, key: Key) -> Address {
        env.storage().instance().get(&key).unwrap()
    }

    fn venue(env: &Env) -> Sep56Client<'_> {
        Sep56Client::new(env, &Self::get(env, Key::Venue))
    }
}

#[contractimpl]
impl VenueAdapter for Sep56Adapter {
    fn deposit(env: Env, amount: i128) -> i128 {
        Self::get(&env, Key::Owner).require_auth();
        let me = env.current_contract_address();
        let venue = Self::venue(&env);
        // The target vault pulls from us inside its own call.
        env.authorize_as_current_contract(vec![
            &env,
            InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: Self::get(&env, Key::Asset),
                    fn_name: Symbol::new(&env, "transfer"),
                    args: (me.clone(), venue.address.clone(), amount).into_val(&env),
                },
                sub_invocations: Vec::new(&env),
            }),
        ]);
        venue.deposit(&amount, &me, &me, &me);
        amount
    }

    fn withdraw(env: Env, amount: i128, to: Address) -> i128 {
        Self::get(&env, Key::Owner).require_auth();
        let me = env.current_contract_address();
        let venue = Self::venue(&env);
        let sent = amount.min(venue.max_withdraw(&me));
        if sent <= 0 {
            return 0;
        }
        venue.withdraw(&sent, &to, &me, &me);
        sent
    }

    fn total_assets(env: Env) -> i128 {
        let venue = Self::venue(&env);
        venue.convert_to_assets(&venue.balance(&env.current_contract_address()))
    }

    fn max_withdraw(env: Env) -> i128 {
        Self::venue(&env).max_withdraw(&env.current_contract_address())
    }
}
