use soroban_sdk::{panic_with_error, symbol_short, vec, Address, Env, IntoVal, U256};

use crate::errors::Error;
use crate::storage;
use crate::types::{PriceData, Schedule, ScheduleMode};

pub const ONE: i128 = 1_000_000_000_000_000_000;
pub const BPS: i128 = 10_000;
/// Rates are per day, horizons are in seconds.
pub const DENOM: i128 = BPS * 86_400;
pub const MAX_SECONDS: u64 = 3650 * 86_400;

/// `a * b / d` in 256-bit. All inputs must be non-negative.
fn mul_div_round(env: &Env, a: i128, b: i128, d: i128, ceil: bool) -> i128 {
    if a < 0 || b < 0 || d <= 0 {
        panic_with_error!(env, Error::InvalidAmount);
    }
    let den = U256::from_u128(env, d as u128);
    let mut num = U256::from_u128(env, a as u128).mul(&U256::from_u128(env, b as u128));
    if ceil {
        num = num.add(&den).sub(&U256::from_u32(env, 1));
    }
    match num.div(&den).to_u128() {
        Some(v) if v <= i128::MAX as u128 => v as i128,
        _ => panic_with_error!(env, Error::Overflow),
    }
}

pub fn mul_div(env: &Env, a: i128, b: i128, d: i128) -> i128 {
    mul_div_round(env, a, b, d, false)
}

pub fn mul_div_ceil(env: &Env, a: i128, b: i128, d: i128) -> i128 {
    mul_div_round(env, a, b, d, true)
}

pub fn validate_schedule(env: &Env, s: &Schedule) {
    let now = env.ledger().timestamp();
    let ok = if s.max_bps_per_day == 0 {
        false
    } else if s.mode == ScheduleMode::Rolling {
        s.rolling_seconds >= 1 && s.rolling_seconds as u64 <= MAX_SECONDS
    } else {
        s.cycle_seconds >= 1
            && s.cycle_seconds as u64 <= MAX_SECONDS
            && s.next_redemption_at > 0
            && s.next_redemption_at <= now + MAX_SECONDS
    };
    if !ok {
        panic_with_error!(env, Error::InvalidSchedule);
    }
}

/// Seconds until the asset's next redemption, exact to the second.
pub fn seconds_for(env: &Env, s: &Schedule) -> u32 {
    if s.mode == ScheduleMode::Rolling {
        return s.rolling_seconds;
    }
    let now = env.ledger().timestamp();
    let cycle = s.cycle_seconds as u64;
    let mut anchor = s.next_redemption_at;
    if now >= anchor {
        anchor += ((now - anchor) / cycle + 1) * cycle;
    }
    let remaining = anchor - now;
    if remaining > MAX_SECONDS {
        panic_with_error!(env, Error::InvalidSchedule);
    }
    remaining as u32
}

pub fn seconds_to_redemption(env: &Env, asset: &Address) -> u32 {
    match storage::schedule(env, asset) {
        Some(s) => seconds_for(env, &s),
        None => panic_with_error!(env, Error::NoSchedule),
    }
}

/// Net horizon a fill is charged over, plus the taker leg's rate ceiling.
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

/// Price of `base` in `quote`, scaled by 1e18 over raw token units.
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
    let cfg = storage::config(env);
    if let Some(fp) = storage::fallback(env, asset) {
        if fp.price > 0
            && fp.epoch == r.epoch
            && cfg.fallback_max_age > 0
            && env.ledger().timestamp() <= fp.updated_at + cfg.fallback_max_age
        {
            return fp.price;
        }
    }
    panic_with_error!(env, Error::NoPrice)
}

/// A registered feed's price, or `None` if absent, trapping, zero or stale.
fn try_oracle(env: &Env, base: &Address, quote: &Address) -> Option<i128> {
    let cfg = storage::oracle(env, base, quote)?;
    let args = vec![env, base.into_val(env), quote.into_val(env)];
    let r: Result<Result<PriceData, _>, Result<soroban_sdk::Error, _>> =
        env.try_invoke_contract(&cfg.oracle, &symbol_short!("get_price"), args);
    match r {
        Ok(Ok(p))
            if p.price > 0 && env.ledger().timestamp() <= p.timestamp + cfg.max_age =>
        {
            Some(p.price)
        }
        _ => None,
    }
}
