use common::math::MathError;
use soroban_sdk::{panic_with_error, symbol_short, vec, Address, Env, IntoVal};

use crate::errors::Error;
use crate::storage;
use crate::types::{PriceData, PushedPrice, Schedule};

pub use common::math::{BPS, DENOM, ONE};

pub fn mul_div(env: &Env, a: i128, b: i128, d: i128) -> i128 {
    checked(env, common::math::mul_div(env, a, b, d))
}

pub fn mul_div_ceil(env: &Env, a: i128, b: i128, d: i128) -> i128 {
    checked(env, common::math::mul_div_ceil(env, a, b, d))
}

fn checked(env: &Env, r: Result<i128, MathError>) -> i128 {
    match r {
        Ok(v) => v,
        Err(MathError::Domain) => panic_with_error!(env, Error::InvalidAmount),
        Err(MathError::Overflow) => panic_with_error!(env, Error::Overflow),
    }
}

pub fn validate_schedule(env: &Env, s: &Schedule) {
    if !is_valid_schedule(env, s) {
        panic_with_error!(env, Error::InvalidSchedule);
    }
}

fn is_valid_schedule(env: &Env, s: &Schedule) -> bool {
    common::schedule::is_valid(env.ledger().timestamp(), s)
}

pub fn seconds_for(env: &Env, s: &Schedule) -> u32 {
    match common::schedule::seconds_for(env.ledger().timestamp(), s) {
        Some(v) => v,
        None => panic_with_error!(env, Error::InvalidSchedule),
    }
}

pub fn seconds_to_redemption(env: &Env, asset: &Address) -> u32 {
    match storage::schedule(env, asset) {
        Some(s) => seconds_for(env, &s),
        None => panic_with_error!(env, Error::NoSchedule),
    }
}

pub fn horizon(env: &Env, taker_token: &Address, maker_token: &Address) -> (u32, u32) {
    let s = match storage::schedule(env, taker_token) {
        Some(s) => s,
        None => panic_with_error!(env, Error::NoSchedule),
    };
    let t = seconds_for(env, &s);
    let m = storage::schedule(env, maker_token)
        .map(|x| seconds_for(env, &x))
        .unwrap_or(0);
    (if t > m { t - m } else { 1 }, s.max_bps_per_day)
}

pub fn price_of(env: &Env, base: &Address, quote: &Address) -> i128 {
    if base == quote {
        return ONE;
    }
    if let Some(p) = try_oracle(env, base, quote) {
        return p;
    }
    mul_div(env, unit_price(env, base), ONE, unit_price(env, quote))
}

fn unit_price(env: &Env, asset: &Address) -> i128 {
    let r = storage::reference(env);
    if asset == &r.asset {
        return ONE;
    }
    if let Some(p) = try_oracle(env, asset, &r.asset) {
        return p;
    }
    match storage::fallback(env, asset) {
        Some(fp) if is_live_fallback(env, &fp, r.epoch) => fp.price,
        _ => panic_with_error!(env, Error::NoPrice),
    }
}

fn is_live_fallback(env: &Env, fp: &PushedPrice, epoch: u32) -> bool {
    let max_age = storage::config(env).fallback_max_age;
    fp.price > 0
        && fp.epoch == epoch
        && max_age > 0
        && env.ledger().timestamp() <= fp.updated_at + max_age
}

fn try_oracle(env: &Env, base: &Address, quote: &Address) -> Option<i128> {
    let cfg = storage::oracle(env, base, quote)?;
    let args = vec![env, base.into_val(env), quote.into_val(env)];
    let r: Result<Result<PriceData, _>, Result<soroban_sdk::Error, _>> =
        env.try_invoke_contract(&cfg.oracle, &symbol_short!("get_price"), args);
    match r {
        Ok(Ok(p)) if p.price > 0 && env.ledger().timestamp() <= p.timestamp + cfg.max_age => {
            Some(p.price)
        }
        _ => None,
    }
}
