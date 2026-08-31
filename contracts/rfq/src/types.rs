use soroban_sdk::{contracttype, Address};

pub use orders::{FillResult, FixedOrder, OrderType, Request, RfqOrder, Signature};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleMode {
    Rolling,
    Cyclical,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Schedule {
    pub mode: ScheduleMode,
    pub rolling_seconds: u32,
    pub next_redemption_at: u64,
    pub cycle_seconds: u32,
    pub max_bps_per_day: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DutchOrder {
    pub maker_token: Address,
    pub taker_token: Address,
    pub taker_amount: i128,
    pub start_maker_amount: i128,
    pub min_maker_amount: i128,
    pub fee_bps: u32,
    pub fee_recipient: Address,
    pub expiry: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Listing {
    pub order: DutchOrder,
    pub seller: Address,
    pub created_at: u64,
    pub decay_seconds: u32,
    pub active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Quote {
    pub maker_amount: i128,
    pub fee: i128,
    pub horizon_seconds: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PushedPrice {
    pub price: i128,
    pub updated_at: u64,
    pub epoch: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleCfg {
    pub oracle: Address,
    pub max_age: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reference {
    pub asset: Address,
    pub epoch: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub min_fee_bps: u32,
    pub max_fee_bps: u32,
    pub fallback_max_age: u64,
    pub max_deviation_bps: u32,
    /// Seconds a keeper must wait between backstop pushes. Floors at one ledger.
    pub min_push_interval: u64,
    pub max_shift_seconds: u32,
    pub decay_seconds: u32,
}
