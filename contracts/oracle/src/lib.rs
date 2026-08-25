#![no_std]
//! SEP-40 price adapter. Reads a Reflector-style feed and republishes it in the
//! 1e18 raw-unit convention the RFQ settlement contract expects. See README.md.

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
    /// SEP-40 feed to read.
    pub source: Address,
    pub base: Address,
    pub quote: Address,
    /// Keys into the feed. With `cross` unset the feed already quotes in `quote`
    /// and `quote_asset` is ignored; with it set the two legs are divided.
    pub base_asset: Asset,
    pub quote_asset: Asset,
    pub cross: bool,
    pub base_decimals: u32,
    pub quote_decimals: u32,
    pub max_age: u64,
    /// Set when the feed reports `quote/base` instead of `base/quote`.
    pub invert: bool,
}

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
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Cfg, &cfg);
    }

    pub fn set_config(env: Env, cfg: Config) {
        Self::admin(env.clone()).require_auth();
        env.storage().instance().set(&DataKey::Cfg, &cfg);
    }

    pub fn admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    pub fn config(env: Env) -> Config {
        env.storage().instance().get(&DataKey::Cfg).unwrap()
    }

    /// Price of `base` in `quote`, scaled so `quote_amount = base_amount * price / 1e18`
    /// in raw token units. Traps when the pair is unknown or the feed is stale.
    pub fn get_price(env: Env, base: Address, quote: Address) -> PriceData {
        let cfg = Self::config(env.clone());
        if base != cfg.base || quote != cfg.quote {
            panic_with_error!(&env, Error::PairMismatch);
        }
        let source = Sep40Client::new(&env, &cfg.source);
        let decimals = source.decimals();

        let head = Self::fresh(&env, cfg.max_age, source.lastprice(&cfg.base_asset));
        let mut raw = head.price;
        let mut timestamp = head.timestamp;

        if cfg.cross {
            let leg = Self::fresh(&env, cfg.max_age, source.lastprice(&cfg.quote_asset));
            raw = Self::mul_div(&env, raw, Self::pow10(&env, decimals), leg.price);
            timestamp = timestamp.min(leg.timestamp);
        }
        if cfg.invert {
            let unit = Self::pow10(&env, decimals);
            raw = Self::mul_div(&env, unit, unit, raw);
        }
        if raw <= 0 {
            panic_with_error!(&env, Error::NoPrice);
        }

        let exp = 18 + cfg.quote_decimals as i32 - cfg.base_decimals as i32 - decimals as i32;
        let price = if exp >= 0 {
            Self::checked(&env, raw.checked_mul(Self::pow10(&env, exp as u32)))
        } else {
            raw / Self::pow10(&env, (-exp) as u32)
        };
        if price <= 0 {
            panic_with_error!(&env, Error::NoPrice);
        }
        PriceData { price, timestamp }
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
