#![no_std]
//! Push oracle. Privileged updaters (e.g. a keeper reading an issuer's NAV)
//! push prices for any pair; `get_price` serves the last one in the oracle
//! convention: quote per base, 1e18-scaled over raw units. Freshness is the
//! consumer's call (a facility checks each RWA's `max_price_age`). An updater
//! may move a price at most `max_deviation_bps`; the admin can correct beyond.

#[cfg(test)]
mod test;

use common::interfaces::Oracle;
use common::math::BPS;
use common::types::PriceData;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env,
};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    NotUpdater = 1,
    InvalidPrice = 2,
    StaleOrFuture = 3,
    DeviationTooLarge = 4,
    NoPrice = 5,
}

#[contracttype]
enum Key {
    Admin,
    MaxDeviation,
    Updater(Address),
    Price(Address, Address),
}

#[contract]
pub struct PushOracle;

#[contractimpl]
impl PushOracle {
    /// `max_deviation_bps` 0 disables the bound.
    pub fn __constructor(env: Env, admin: Address, max_deviation_bps: u32) {
        env.storage().instance().set(&Key::Admin, &admin);
        env.storage()
            .instance()
            .set(&Key::MaxDeviation, &max_deviation_bps);
    }

    pub fn admin(env: Env) -> Address {
        env.storage().instance().get(&Key::Admin).unwrap()
    }

    pub fn max_deviation_bps(env: Env) -> u32 {
        env.storage().instance().get(&Key::MaxDeviation).unwrap()
    }

    pub fn is_updater(env: Env, who: Address) -> bool {
        env.storage()
            .persistent()
            .get(&Key::Updater(who))
            .unwrap_or(false)
    }

    pub fn set_admin(env: Env, admin: Address) {
        Self::admin(env.clone()).require_auth();
        env.storage().instance().set(&Key::Admin, &admin);
    }

    pub fn set_max_deviation(env: Env, bps: u32) {
        Self::admin(env.clone()).require_auth();
        env.storage().instance().set(&Key::MaxDeviation, &bps);
    }

    pub fn set_updater(env: Env, updater: Address, allowed: bool) {
        Self::admin(env.clone()).require_auth();
        Self::put(&env, Key::Updater(updater.clone()), &allowed);
        env.events()
            .publish((symbol_short!("updater"), updater), allowed);
    }

    /// Records `price` as of `timestamp` (the source's time, not the push time).
    pub fn push(
        env: Env,
        caller: Address,
        base: Address,
        quote: Address,
        price: i128,
        timestamp: u64,
    ) {
        caller.require_auth();
        let is_admin = caller == Self::admin(env.clone());
        if !is_admin && !Self::is_updater(env.clone(), caller.clone()) {
            panic_with_error!(&env, Error::NotUpdater);
        }
        if price <= 0 {
            panic_with_error!(&env, Error::InvalidPrice);
        }
        let last: Option<PriceData> = env
            .storage()
            .persistent()
            .get(&Key::Price(base.clone(), quote.clone()));
        if timestamp > env.ledger().timestamp()
            || last.as_ref().is_some_and(|l| timestamp <= l.timestamp)
        {
            panic_with_error!(&env, Error::StaleOrFuture);
        }
        let max = Self::max_deviation_bps(env.clone()) as i128;
        if last
            .is_some_and(|l| !is_admin && max > 0 && (price - l.price).abs() * BPS > l.price * max)
        {
            panic_with_error!(&env, Error::DeviationTooLarge);
        }
        Self::put(
            &env,
            Key::Price(base.clone(), quote.clone()),
            &PriceData { price, timestamp },
        );
        env.events()
            .publish((symbol_short!("price"), base, quote), (price, timestamp));
    }

    fn put<T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(env: &Env, k: Key, v: &T) {
        env.storage().persistent().set(&k, v);
        env.storage().persistent().extend_ttl(&k, 518_400, 535_680);
        env.storage().instance().extend_ttl(518_400, 535_680);
    }
}

#[contractimpl]
impl Oracle for PushOracle {
    fn get_price(env: Env, base: Address, quote: Address) -> PriceData {
        env.storage()
            .persistent()
            .get(&Key::Price(base, quote))
            .unwrap_or_else(|| panic_with_error!(&env, Error::NoPrice))
    }
}
