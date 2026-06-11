//! Order, signature and status types.
//!
//! These mirror 0x's `LibNativeOrder` structs. An RFQ order is a quote a maker
//! signs off-chain and a taker fills on-chain; a limit order is the more general
//! resting order with an explicit taker fee. Optional counterparties (taker,
//! sender) are modelled with `Option<Address>` because Soroban has no concept of
//! the EVM "zero address" sentinel.

use soroban_sdk::{contracttype, Address, BytesN};

/// An RFQ (request-for-quote) order. Equivalent to `LibNativeOrder.RfqOrder`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RfqOrder {
    /// Token the maker gives to the taker.
    pub maker_token: Address,
    /// Token the maker receives from the taker.
    pub taker_token: Address,
    /// Total maker token amount offered.
    pub maker_amount: i128,
    /// Total taker token amount requested.
    pub taker_amount: i128,
    /// The maker (offline signer / liquidity provider).
    pub maker: Address,
    /// Allowed taker. `None` => fillable by anyone.
    pub taker: Option<Address>,
    /// The address authorised to *submit* the fill (0x `txOrigin`). Other
    /// submitters may be whitelisted by the origin owner via
    /// [`register_allowed_rfq_origin`](crate::RfqContract::register_allowed_rfq_origin).
    pub tx_origin: Address,
    /// Liquidity pool / fee-routing tag (opaque 32 bytes, like 0x's `pool`).
    pub pool: BytesN<32>,
    /// Unix expiry timestamp (seconds). Order is unfillable once reached.
    pub expiry: u64,
    /// Uniqueness / cancellation salt (ordered for pair cancellation).
    pub salt: u64,
}

/// A limit order. Equivalent to `LibNativeOrder.LimitOrder`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LimitOrder {
    pub maker_token: Address,
    pub taker_token: Address,
    pub maker_amount: i128,
    pub taker_amount: i128,
    /// Total fee (at full fill) denominated in the **maker token**, skimmed from
    /// the maker output and paid to `fee_recipient` before the remainder goes to
    /// the taker (taker receives `maker_filled - fee`). Proportional to fill.
    /// Same role as 0x's `takerTokenFeeAmount`, but taken from the maker side.
    pub token_fee_amount: i128,
    pub maker: Address,
    /// Allowed taker. `None` => anyone.
    pub taker: Option<Address>,
    /// Allowed sender (0x `sender`). `None` => anyone.
    pub sender: Option<Address>,
    /// Recipient of the maker-token fee.
    pub fee_recipient: Address,
    pub pool: BytesN<32>,
    pub expiry: u64,
    pub salt: u64,
}

/// An ed25519 order signature. The maker signs the canonical order hash
/// off-chain; the taker carries the signature on-chain.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Signature {
    /// ed25519 public key of the signer.
    pub signer: BytesN<32>,
    /// 64-byte ed25519 signature over the order hash.
    pub signature: BytesN<64>,
}

/// Lifecycle status of an order. Equivalent to `LibNativeOrder.OrderStatus`.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum OrderStatus {
    Invalid = 0,
    Fillable = 1,
    Filled = 2,
    Cancelled = 3,
    Expired = 4,
}

/// Resolved view of an order. Equivalent to `LibNativeOrder.OrderInfo`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderInfo {
    pub order_hash: BytesN<32>,
    pub status: OrderStatus,
    pub taker_token_filled_amount: i128,
}

/// Result of a fill. Equivalent to 0x's `FillNativeOrderResults` (minus the
/// ETH `ethProtocolFeePaid`, which has no Stellar analogue — see README).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FillResult {
    pub taker_token_filled_amount: i128,
    /// Gross maker token filled. The taker receives this **minus**
    /// `fee_filled_amount`; the fee recipient receives `fee_filled_amount`.
    pub maker_token_filled_amount: i128,
    /// Maker-token fee paid to the fee recipient (limit orders only; 0 for RFQ).
    pub fee_filled_amount: i128,
}
