#![no_std]
//! Order types crossing the router/settlement boundary. Defined once so the two
//! contracts cannot drift on XDR encoding.
//!
//! The leading fields of every order are the **request** — the terms the taker
//! creates and signs before any maker has bid — repeated in the same order, so
//! one taker signature over [`Request`] pairs with whichever bid wins and the
//! maker signs the whole thing on top.

use soroban_sdk::{contracttype, Address, BytesN};

/// Which settlement path a taker's request authorises. Part of the request
/// digest, so a signature for one path cannot be replayed on the other.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderType {
    Rfq,
    Fixed,
}

/// The taker's own terms, signed at step 1 before any maker has bid.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    pub maker_token: Address,
    pub taker_token: Address,
    pub taker_amount: i128,
    pub min_received_amount: i128,
    pub fee_bps: u32,
    /// Whose assets move. `None` => the sender.
    pub taker: Option<Address>,
    /// Who may submit the fill. `None` => anyone.
    pub sender: Option<Address>,
    pub fee_recipient: Address,
    pub expiry: u64,
    pub salt: u64,
    pub taker_max_bps_per_day: u32,
    pub order_type: OrderType,
}

/// Duration-priced order: the maker signs a **rate**, not an amount. The
/// absolute amount is derived at settlement from the live redemption horizon and
/// the oracle price, which is what lets one signature stay correct as the clock
/// moves. `max_maker_amount` is the maker's signed ceiling — without it a live
/// order would be a free option on the taker asset.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RfqOrder {
    // ---- the taker's request, in Request order ----
    pub maker_token: Address,
    pub taker_token: Address,
    pub taker_amount: i128,
    pub min_received_amount: i128,
    pub fee_bps: u32,
    pub taker: Option<Address>,
    pub sender: Option<Address>,
    pub fee_recipient: Address,
    pub expiry: u64,
    pub salt: u64,
    pub taker_max_bps_per_day: u32,
    // ---- the maker's bid ----
    pub maker_bps_per_day: u32,
    pub max_maker_amount: i128,
    pub maker: Address,
}

/// Off-model order: the maker states the amount outright. No schedule, no oracle.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedOrder {
    // ---- the taker's request, in Request order ----
    pub maker_token: Address,
    pub taker_token: Address,
    pub taker_amount: i128,
    pub min_received_amount: i128,
    pub fee_bps: u32,
    pub taker: Option<Address>,
    pub sender: Option<Address>,
    pub fee_recipient: Address,
    pub expiry: u64,
    pub salt: u64,
    // ---- the maker's bid ----
    pub maker_amount: i128,
    pub maker: Address,
}

impl RfqOrder {
    pub fn request(&self) -> Request {
        Request {
            maker_token: self.maker_token.clone(),
            taker_token: self.taker_token.clone(),
            taker_amount: self.taker_amount,
            min_received_amount: self.min_received_amount,
            fee_bps: self.fee_bps,
            taker: self.taker.clone(),
            sender: self.sender.clone(),
            fee_recipient: self.fee_recipient.clone(),
            expiry: self.expiry,
            salt: self.salt,
            taker_max_bps_per_day: self.taker_max_bps_per_day,
            order_type: OrderType::Rfq,
        }
    }
}

impl FixedOrder {
    pub fn request(&self) -> Request {
        Request {
            maker_token: self.maker_token.clone(),
            taker_token: self.taker_token.clone(),
            taker_amount: self.taker_amount,
            min_received_amount: self.min_received_amount,
            fee_bps: self.fee_bps,
            taker: self.taker.clone(),
            sender: self.sender.clone(),
            fee_recipient: self.fee_recipient.clone(),
            expiry: self.expiry,
            salt: self.salt,
            taker_max_bps_per_day: 0,
            order_type: OrderType::Fixed,
        }
    }
}

/// An ed25519 signature over a SEP-53 digest.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Signature {
    pub signer: BytesN<32>,
    pub signature: BytesN<64>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FillResult {
    pub taker_filled: i128,
    pub maker_filled: i128,
    pub fee: i128,
}
