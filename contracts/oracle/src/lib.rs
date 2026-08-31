#![no_std]
//! SEP-40 price adapter: reads a Reflector-style feed and republishes it in the
//! 1e18 raw-unit convention the settlement contract expects. See README.md.

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error, Address,
    Env, Symbol,
};

#[cfg(test)]
mod test;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Asset {
    Stellar(Address),
    Other(Symbol),
}

#[contractclient(name = "Sep40Client")]
pub trait Sep40 {
    fn decimals(env: Env) -> u32;
    fn lastprice(env: Env, asset: Asset) -> Option<PriceData>;
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    PairMismatch = 2,
    NoPrice = 3,
    Overflow = 4,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub source: Address,
    pub base: Address,
    pub quote: Address,
    pub base_asset: Asset,
    pub quote_asset: Asset,
    pub cross: bool,
    pub base_decimals: u32,
    pub quote_decimals: u32,
    pub max_age: u64,
    pub invert: bool,
}

const THRESHOLD: u32 = 518_400;
const EXTEND: u32 = 535_680;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Cfg,
}

#[contract]
pub struct OctarineOracle;

#[contractimpl]
impl OctarineOracle {
    pub fn initialize(env: Env, admin: Address, cfg: Config) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        // Deploy and initialize are not necessarily one transaction.
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Cfg, &cfg);
        Self::touch(&env);
    }

    pub fn set_config(env: Env, cfg: Config) {
        Self::admin(env.clone()).require_auth();
        env.storage().instance().set(&DataKey::Cfg, &cfg);
        Self::touch(&env);
    }

    pub fn admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    pub fn config(env: Env) -> Config {
        env.storage().instance().get(&DataKey::Cfg).unwrap()
    }

    pub fn get_price(env: Env, base: Address, quote: Address) -> PriceData {
        Self::touch(&env);
        let cfg = Self::config(env.clone());
        if base != cfg.base || quote != cfg.quote {
            panic_with_error!(&env, Error::PairMismatch);
        }
        let source = Sep40Client::new(&env, &cfg.source);
        let decimals = source.decimals();
        let (raw, timestamp) = Self::feed_rate(&env, &cfg, &source, decimals);
        PriceData {
            price: Self::scale_to_1e18(&env, raw, &cfg, decimals),
            timestamp,
        }
    }

    fn feed_rate(env: &Env, cfg: &Config, source: &Sep40Client, decimals: u32) -> (i128, u64) {
        let head = Self::fresh(env, cfg.max_age, source.lastprice(&cfg.base_asset));
        let mut raw = head.price;
        let mut timestamp = head.timestamp;
        if cfg.cross {
            let leg = Self::fresh(env, cfg.max_age, source.lastprice(&cfg.quote_asset));
            raw = Self::mul_div(env, raw, Self::pow10(env, decimals), leg.price);
            timestamp = timestamp.min(leg.timestamp);
        }
        if cfg.invert {
            let unit = Self::pow10(env, decimals);
            raw = Self::mul_div(env, unit, unit, raw);
        }
        if raw <= 0 {
            panic_with_error!(env, Error::NoPrice);
        }
        (raw, timestamp)
    }

    fn scale_to_1e18(env: &Env, raw: i128, cfg: &Config, decimals: u32) -> i128 {
        let exp = 18 + cfg.quote_decimals as i32 - cfg.base_decimals as i32 - decimals as i32;
        let price = if exp >= 0 {
            Self::checked(env, raw.checked_mul(Self::pow10(env, exp as u32)))
        } else {
            raw / Self::pow10(env, (-exp) as u32)
        };
        if price <= 0 {
            panic_with_error!(env, Error::NoPrice);
        }
        price
    }

    /// This contract has no upgrade path and serves almost nothing but reads, so
    /// reads refresh the TTL. Otherwise recovery means a restore or a redeploy.
    fn touch(env: &Env) {
        env.storage().instance().extend_ttl(THRESHOLD, EXTEND);
    }

    fn fresh(env: &Env, max_age: u64, data: Option<PriceData>) -> PriceData {
        match data {
            Some(p)
                if p.price > 0
                    && p.timestamp > 0
                    && env.ledger().timestamp() <= p.timestamp + max_age =>
            {
                p
            }
            _ => panic_with_error!(env, Error::NoPrice),
        }
    }

    fn mul_div(env: &Env, a: i128, b: i128, d: i128) -> i128 {
        if d <= 0 {
            panic_with_error!(env, Error::NoPrice);
        }
        Self::checked(env, a.checked_mul(b)) / d
    }

    fn pow10(env: &Env, exp: u32) -> i128 {
        if exp > 38 {
            panic_with_error!(env, Error::Overflow);
        }
        10i128.pow(exp)
    }

    fn checked(env: &Env, v: Option<i128>) -> i128 {
        match v {
            Some(v) => v,
            None => panic_with_error!(env, Error::Overflow),
        }
    }
}
