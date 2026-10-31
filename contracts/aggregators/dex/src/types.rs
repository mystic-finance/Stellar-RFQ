use soroban_sdk::auth::InvokerContractAuthEntry;
use soroban_sdk::{contracttype, Address, Bytes, Symbol, Val, Vec};

/// One swap on one venue. `data` is the venue adapter's own payload.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hop {
    pub venue: Address,
    pub token_out: Address,
    pub data: Bytes,
}

/// A slice of the input, routed through `hops` in sequence.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Split {
    pub amount_in: i128,
    pub hops: Vec<Hop>,
}

/// An integrator taking a fee on swaps it sends through.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Partner {
    pub address: Address,
    pub fee_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeConfig {
    pub recipient: Address,
    pub protocol_fee_bps: u32,
    pub max_partner_fee_bps: u32,
    /// The protocol's cut of every partner fee.
    pub partner_share_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SwapResult {
    /// What the route delivered, before fees.
    pub gross_out: i128,
    pub protocol_fee: i128,
    pub partner_fee: i128,
    /// What the swapper received.
    pub amount_out: i128,
}

/// The terms of a direct swap: what goes in, what must come out, and where.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SwapDescription {
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in: i128,
    /// The least the swapper accepts, net of every fee.
    pub min_out: i128,
    pub to: Address,
}

/// One call of a route built off-chain (SimpleSwap-style). `target` must be an
/// allowlisted router; `auth` pre-authorises the `token_in` transfers it makes
/// from this contract, and nothing else.
#[contracttype]
#[derive(Clone)]
pub struct Call {
    pub target: Address,
    pub function: Symbol,
    pub args: Vec<Val>,
    pub auth: Vec<InvokerContractAuthEntry>,
}

/// A router fill's `data`: a route through adapters, or calldata.
#[contracttype]
#[derive(Clone)]
pub enum Route {
    Splits(Vec<Split>),
    Calls(Vec<Call>),
}
