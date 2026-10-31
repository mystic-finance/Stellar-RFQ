pub use common::types::FacilityConfig;
use common::types::Schedule;
use soroban_sdk::{contracttype, Address};

/// How the facility trades one RWA. Rates are hundredths of a basis point per
/// day over the RWA's horizon: it buys at `bid_rate` and sells at `ask_rate`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Terms {
    pub accepted: bool,
    pub bid_rate: u32,
    pub ask_rate: u32,
    /// Ceiling on this RWA's oracle value as a share of NAV, in bps.
    pub max_bps: u32,
    /// Price of the RWA in the base asset (`get_price(rwa, base)`).
    pub oracle: Address,
    pub max_price_age: u64,
    pub schedule: Schedule,
    /// Where booked RWA goes: the issuer's redemption account or custody.
    pub redemption: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rwa {
    /// The vault holding this RWA; the address the issuer allowlists.
    pub vault: Address,
    pub terms: Terms,
}

/// RWA out with its issuer, carried at its oracle value when booked.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Redemption {
    pub amount: i128,
    pub carried: i128,
}

/// Changes that wait out the timelock.
#[contracttype]
#[derive(Clone)]
pub enum Change {
    CuratorFee(u32),
    Vest(u64),
    Queue(Address),
    /// Lists an RWA, or updates its terms. `vault` must hold `rwa`.
    Rwa(Address, Address, Terms),
}
