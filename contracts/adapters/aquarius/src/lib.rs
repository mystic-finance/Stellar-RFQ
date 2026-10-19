#![no_std]
//! DexAdapter over Aquarius pools. One shared instance; callable only by the
//! DEX aggregator. A token pair can have several Aquarius pools (fee tiers,
//! constant-product or stableswap), so each hop names its pool: `data` is the
//! pool's address as XDR. The pool is called directly rather than through the
//! Aquarius router, keeping the token pull one level below this contract.

use common::interfaces::DexAdapter;
use soroban_sdk::auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation};
use soroban_sdk::xdr::FromXdr;
use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error, token,
    vec, Address, Bytes, Env, IntoVal, Symbol, Vec,
};

#[cfg(test)]
mod test;

/// The Aquarius pool functions this adapter uses, as deployed. Amounts are
/// unsigned on this interface.
#[contractclient(name = "AquaPoolClient")]
pub trait AquaPool {
    fn swap(
        e: Env,
        user: Address,
        in_idx: u32,
        out_idx: u32,
        in_amount: u128,
        out_min: u128,
    ) -> u128;
    fn estimate_swap(e: Env, in_idx: u32, out_idx: u32, in_amount: u128) -> u128;
    fn get_tokens(e: Env) -> Vec<Address>;
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    InvalidPool = 1,
    TokenNotInPool = 2,
    InvalidAmount = 3,
}

#[contracttype]
enum Key {
    Aggregator,
}

const CONTRACT_ADDRESS_XDR_LEN: u32 = 40;

#[contract]
pub struct AquariusAdapter;

#[contractimpl]
impl AquariusAdapter {
    pub fn __constructor(env: Env, aggregator: Address) {
        env.storage().instance().set(&Key::Aggregator, &aggregator);
    }

    fn pool(env: &Env, data: &Bytes) -> Address {
        if data.len() != CONTRACT_ADDRESS_XDR_LEN {
            panic_with_error!(env, Error::InvalidPool);
        }
        match Address::from_xdr(env, data) {
            Ok(a) => a,
            Err(_) => panic_with_error!(env, Error::InvalidPool),
        }
    }

    fn indices(
        env: &Env,
        pool: &AquaPoolClient,
        token_in: &Address,
        token_out: &Address,
    ) -> (u32, u32) {
        let tokens = pool.get_tokens();
        match (
            tokens.first_index_of(token_in),
            tokens.first_index_of(token_out),
        ) {
            (Some(i), Some(o)) => (i, o),
            _ => panic_with_error!(env, Error::TokenNotInPool),
        }
    }

    fn unsigned(env: &Env, v: i128) -> u128 {
        if v < 0 {
            panic_with_error!(env, Error::InvalidAmount);
        }
        v as u128
    }
}

#[contractimpl]
impl DexAdapter for AquariusAdapter {
    fn quote(
        env: Env,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        data: Bytes,
    ) -> i128 {
        let pool = AquaPoolClient::new(&env, &Self::pool(&env, &data));
        let (i, o) = Self::indices(&env, &pool, &token_in, &token_out);
        pool.estimate_swap(&i, &o, &Self::unsigned(&env, amount_in)) as i128
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
        let aggregator: Address = env.storage().instance().get(&Key::Aggregator).unwrap();
        aggregator.require_auth();
        let me = env.current_contract_address();
        let pool_addr = Self::pool(&env, &data);
        let pool = AquaPoolClient::new(&env, &pool_addr);
        let (i, o) = Self::indices(&env, &pool, &token_in, &token_out);

        // The pool pulls the input from `user` (us) inside its own call, so
        // that exact transfer is authorised up front. The entry covers only
        // the next call this contract makes.
        let out = token::Client::new(&env, &token_out);
        let before = out.balance(&me);
        env.authorize_as_current_contract(vec![
            &env,
            InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: token_in,
                    fn_name: Symbol::new(&env, "transfer"),
                    args: (me.clone(), pool_addr, amount_in).into_val(&env),
                },
                sub_invocations: Vec::new(&env),
            }),
        ]);
        pool.swap(
            &me,
            &i,
            &o,
            &Self::unsigned(&env, amount_in),
            &Self::unsigned(&env, min_out),
        );
        let received = out.balance(&me) - before;
        out.transfer(&me, &to, &received);
        received
    }
}
