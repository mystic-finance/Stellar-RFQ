use soroban_sdk::{contracttype, Address, String};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleMode {
    /// Always T+N: `rolling_seconds` regardless of the date.
    Rolling,
    /// Redemption lands on a calendar date and rolls forward once it passes.
    Cyclical,
}

/// An asset's redemption schedule. The settlement contract keeps one per asset
/// for market makers; a facility keeps its own per RWA it buys.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Schedule {
    pub mode: ScheduleMode,
    pub rolling_seconds: u32,
    pub next_redemption_at: u64,
    pub cycle_seconds: u32,
    /// Rate ceiling for this asset, in hundredths of a basis point per day:
    /// 1000 is 10.00 bps/day.
    pub max_bps_per_day: u32,
}

/// An oracle reading: `price` is quote per base, 1e18-scaled over raw units.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

/// A vault's settings at creation. `asset` never changes.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultConfig {
    pub asset: Address,
    pub curator: Address,
    pub manager: Address,
    /// May revoke any pending timelocked change.
    pub guardian: Address,
    pub name: String,
    pub symbol: String,
    /// NAV ceiling for deposits; 0 means uncapped.
    pub deposit_cap: i128,
    /// Performance fee on share-price gains above the high-water mark.
    pub curator_fee_bps: u32,
    /// Seconds a governed change waits before it can be accepted.
    pub timelock: u64,
}

/// A Facility leg's `data`: which facility, and who pays the input. The
/// facility requires the payer's own signature, so naming anyone else fails.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FacilityRoute {
    pub facility: Address,
    pub payer: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FacilityConfig {
    /// The vault holding the facility's base asset (its stablecoin).
    pub base_vault: Address,
    pub curator: Address,
    pub manager: Address,
    pub guardian: Address,
    pub name: String,
    pub symbol: String,
    /// Performance fee on oLP share-price gains above the high-water mark.
    pub curator_fee_bps: u32,
    /// Seconds over which a trade's gain against the oracle mark unlocks.
    pub vest_seconds: u64,
    pub timelock: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueueConfig {
    pub facility: Address,
    /// Who deployed it, through the factory.
    pub curator: Address,
}
