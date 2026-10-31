use crate::types::{Schedule, ScheduleMode};

/// Ten years: the longest horizon any schedule may express.
pub const MAX_SECONDS: u64 = 3650 * 86_400;

pub fn is_valid(now: u64, s: &Schedule) -> bool {
    if s.max_bps_per_day == 0 {
        return false;
    }
    if s.mode == ScheduleMode::Rolling {
        return in_horizon(s.rolling_seconds);
    }
    in_horizon(s.cycle_seconds)
        && s.next_redemption_at > 0
        && s.next_redemption_at <= now + MAX_SECONDS
}

fn in_horizon(seconds: u32) -> bool {
    seconds >= 1 && seconds as u64 <= MAX_SECONDS
}

/// Seconds until the next redemption; a cyclical date that has passed rolls
/// forward whole cycles. `None` if the result exceeds `MAX_SECONDS`.
pub fn seconds_for(now: u64, s: &Schedule) -> Option<u32> {
    if s.mode == ScheduleMode::Rolling {
        return Some(s.rolling_seconds);
    }
    let cycle = s.cycle_seconds as u64;
    let mut anchor = s.next_redemption_at;
    if now >= anchor {
        anchor += ((now - anchor) / cycle + 1) * cycle;
    }
    let remaining = anchor - now;
    (remaining <= MAX_SECONDS).then_some(remaining as u32)
}
