#![no_std]
//! Order types shared by the router and settlement contracts.
//!
//! The leading fields of `RfqOrder` and `FixedOrder` repeat `Request` in the
//! same order. One taker signature over `Request` pairs with whichever bid
//! wins, so reordering or inserting a field in one and not the others silently
//! breaks signature matching.

use soroban_sdk::{contracttype, Address, BytesN};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderType {
    Rfq,
    Fixed,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
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
    /// Ceiling on the maker's rate, in hundredths of a basis point per day:
    /// 250 is 2.50 bps/day.
    pub taker_max_bps_per_day: u32,
    pub order_type: OrderType,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RfqOrder {
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
    /// Ceiling on the maker's rate, in hundredths of a basis point per day.
    pub taker_max_bps_per_day: u32,
    /// The bid, in hundredths of a basis point per day: 250 is 2.50 bps/day.
    pub maker_bps_per_day: u32,
    pub max_maker_amount: i128,
    pub maker: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedOrder {
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
