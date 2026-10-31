#![no_std]
//! VenueAdapter over a Blend v2 pool: supplies the vault's base asset as a
//! plain (non-collateral) supply position and withdraws it on demand. One
//! instance per owning vault; the position is this contract's b-tokens.
//! Interface verified against blend-contracts-v2 v2.0.0.

use common::interfaces::VenueAdapter;
use common::math;
use soroban_sdk::auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation};
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, vec, Address, Env, IntoVal, Map, Symbol,
    Vec,
};

#[cfg(test)]
mod test;

const SCALAR_12: i128 = 1_000_000_000_000;
const SCALAR_7: i128 = 10_000_000;
const SUPPLY: u32 = 0;
const WITHDRAW: u32 = 1;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    pub request_type: u32,
    pub address: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Positions {
    pub liabilities: Map<u32, i128>,
    pub collateral: Map<u32, i128>,
    pub supply: Map<u32, i128>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReserveConfig {
    pub index: u32,
    pub decimals: u32,
    pub c_factor: u32,
    pub l_factor: u32,
    pub util: u32,
    pub max_util: u32,
    pub r_base: u32,
    pub r_one: u32,
    pub r_two: u32,
    pub r_three: u32,
    pub reactivity: u32,
    pub supply_cap: i128,
    pub enabled: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReserveData {
    pub d_rate: i128,
    pub b_rate: i128,
    pub ir_mod: i128,
    pub b_supply: i128,
    pub d_supply: i128,
    pub backstop_credit: i128,
    pub last_time: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reserve {
    pub asset: Address,
    pub config: ReserveConfig,
    pub data: ReserveData,
    pub scalar: i128,
}

/// The Blend v2 pool functions this adapter uses, as deployed.
#[contractclient(name = "BlendPoolClient")]
pub trait BlendPool {
    fn submit(
        env: Env,
        from: Address,
        spender: Address,
        to: Address,
        requests: Vec<Request>,
    ) -> Positions;
    fn get_positions(env: Env, address: Address) -> Positions;
    /// Accrues interest to now before returning, so `b_rate` is current.
    fn get_reserve(env: Env, asset: Address) -> Reserve;
}

#[contracttype]
enum Key {
    Owner,
    Asset,
    Pool,
}

#[contract]
pub struct BlendAdapter;

#[contractimpl]
impl BlendAdapter {
    /// Traps if the pool has no reserve for `asset`.
    pub fn __constructor(env: Env, owner: Address, asset: Address, venue: Address) {
        BlendPoolClient::new(&env, &venue).get_reserve(&asset);
        let s = env.storage().instance();
        s.set(&Key::Owner, &owner);
        s.set(&Key::Asset, &asset);
        s.set(&Key::Pool, &venue);
    }

    fn get(env: &Env, key: Key) -> Address {
        env.storage().instance().get(&key).unwrap()
    }

    fn pool(env: &Env) -> BlendPoolClient<'_> {
        BlendPoolClient::new(env, &Self::get(env, Key::Pool))
    }

    fn request(env: &Env, request_type: u32, amount: i128) -> Vec<Request> {
        vec![
            env,
            Request {
                request_type,
                address: Self::get(env, Key::Asset),
                amount,
            },
        ]
    }

    fn floor(env: &Env, a: i128, rate: i128) -> i128 {
        math::mul_div(env, a, rate, SCALAR_12).unwrap_or(0)
    }

    /// Underlying the reserve can pay out while utilization stays under 100%,
    /// less one unit for the pool rounding the burn up.
    fn available(env: &Env, r: &ReserveData) -> i128 {
        let supply = Self::floor(env, r.b_supply, r.b_rate);
        let owed = math::mul_div_ceil(env, r.d_supply, r.d_rate, SCALAR_12).unwrap_or(supply);
        if owed == 0 {
            return supply;
        }
        let floor = math::mul_div_ceil(env, owed, SCALAR_7, SCALAR_7 - 1).unwrap_or(supply);
        (supply - floor - 1).max(0)
    }
}

#[contractimpl]
impl VenueAdapter for BlendAdapter {
    fn deposit(env: Env, amount: i128) -> i128 {
        Self::get(&env, Key::Owner).require_auth();
        let me = env.current_contract_address();
        let pool = Self::pool(&env);
        // The pool pulls from us, the spender, inside `submit`.
        env.authorize_as_current_contract(vec![
            &env,
            InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: Self::get(&env, Key::Asset),
                    fn_name: Symbol::new(&env, "transfer"),
                    args: (me.clone(), pool.address.clone(), amount).into_val(&env),
                },
                sub_invocations: Vec::new(&env),
            }),
        ]);
        pool.submit(&me, &me, &me, &Self::request(&env, SUPPLY, amount));
        amount
    }

    /// The pool pays `to` directly.
    fn withdraw(env: Env, amount: i128, to: Address) -> i128 {
        Self::get(&env, Key::Owner).require_auth();
        let sent = amount.min(Self::max_withdraw(env.clone()));
        if sent <= 0 {
            return 0;
        }
        let me = env.current_contract_address();
        Self::pool(&env).submit(&me, &me, &to, &Self::request(&env, WITHDRAW, sent));
        sent
    }

    fn total_assets(env: Env) -> i128 {
        let pool = Self::pool(&env);
        let r = pool.get_reserve(&Self::get(&env, Key::Asset));
        let held = pool
            .get_positions(&env.current_contract_address())
            .supply
            .get(r.config.index)
            .unwrap_or(0);
        Self::floor(&env, held, r.data.b_rate)
    }

    fn max_withdraw(env: Env) -> i128 {
        let r = Self::pool(&env).get_reserve(&Self::get(&env, Key::Asset));
        Self::total_assets(env.clone()).min(Self::available(&env, &r.data))
    }
}
