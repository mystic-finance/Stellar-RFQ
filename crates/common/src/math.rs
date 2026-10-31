use soroban_sdk::{Env, U256};

/// 1.0 in the 1e18 price scale.
pub const ONE: i128 = 1_000_000_000_000_000_000;
pub const BPS: i128 = 10_000;
/// Rates carry two decimal places: every `*_bps_per_day` counts hundredths of a
/// basis point, so 250 is 2.50 bps/day.
pub const RATE_SCALE: i128 = 100;
/// A rate is per day and a horizon is in seconds; a discount is a fraction of this.
pub const DENOM: i128 = BPS * RATE_SCALE * 86_400;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MathError {
    /// A negative operand, or a non-positive divisor.
    Domain,
    Overflow,
}

/// `a × b / d` in 256-bit, rounded down.
pub fn mul_div(env: &Env, a: i128, b: i128, d: i128) -> Result<i128, MathError> {
    mul_div_round(env, a, b, d, false)
}

/// `a × b / d` in 256-bit, rounded up.
pub fn mul_div_ceil(env: &Env, a: i128, b: i128, d: i128) -> Result<i128, MathError> {
    mul_div_round(env, a, b, d, true)
}

fn mul_div_round(env: &Env, a: i128, b: i128, d: i128, ceil: bool) -> Result<i128, MathError> {
    if a < 0 || b < 0 || d <= 0 {
        return Err(MathError::Domain);
    }
    let den = U256::from_u128(env, d as u128);
    let mut num = U256::from_u128(env, a as u128).mul(&U256::from_u128(env, b as u128));
    if ceil {
        num = num.add(&den).sub(&U256::from_u32(env, 1));
    }
    match num.div(&den).to_u128() {
        Some(v) if v <= i128::MAX as u128 => Ok(v as i128),
        _ => Err(MathError::Overflow),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn rounds_down_and_up() {
        let env = Env::default();
        assert_eq!(mul_div(&env, 10, 1, 3), Ok(3));
        assert_eq!(mul_div_ceil(&env, 10, 1, 3), Ok(4));
        // Exact division does not round up.
        assert_eq!(mul_div_ceil(&env, 9, 1, 3), Ok(3));
    }

    #[test]
    fn intermediate_products_beyond_i128_still_resolve() {
        let env = Env::default();
        // ONE × ONE × 1e3 overflows i128 as a product, but the quotient fits.
        assert_eq!(mul_div(&env, ONE * 1_000, ONE, ONE), Ok(ONE * 1_000));
    }

    #[test]
    fn rejects_bad_domain_and_overflow() {
        let env = Env::default();
        assert_eq!(mul_div(&env, -1, 1, 1), Err(MathError::Domain));
        assert_eq!(mul_div(&env, 1, 1, 0), Err(MathError::Domain));
        assert_eq!(mul_div(&env, i128::MAX, 2, 1), Err(MathError::Overflow));
    }
}
