use soroban_sdk::{contractclient, contracttype, Address, Bytes, Env, Vec};

pub use orders::{FillResult, FixedOrder, RfqOrder, Signature};

/// The settlement entry points the router routes signed bids through.
#[contractclient(name = "SettlementClient")]
pub trait Settlement {
    fn fill_rfq_order(
        env: Env,
        order: RfqOrder,
        maker_signature: Signature,
        taker_signature: Option<Signature>,
        sender: Address,
        taker_amount_in: i128,
    ) -> FillResult;

    fn fill_fixed_order(
        env: Env,
        order: FixedOrder,
        maker_signature: Signature,
        taker_signature: Option<Signature>,
        sender: Address,
        taker_amount_in: i128,
    ) -> FillResult;
}

/// What an aggregator must implement. The router **transfers `amount_in` of
/// `token_in` to the aggregator before calling**, so the aggregator never holds
/// an allowance on anyone; it must send at least `min_amount_out` of `token_out`
/// to `recipient` and return what it actually sent. `data` is an opaque
/// aggregator-specific payload (a DEX path, a facility id) that the router
/// carries through untouched.
#[contractclient(name = "AggregatorClient")]
pub trait Aggregator {
    fn quote(env: Env, token_in: Address, token_out: Address, amount_in: i128) -> i128;

    fn fill(
        env: Env,
        recipient: Address,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        min_amount_out: i128,
        data: Bytes,
    ) -> i128;
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceKind {
    Dex,
    Facility,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceEntry {
    pub address: Address,
    pub kind: SourceKind,
}

/// A maker-signed bid. The order carries its own pricing type.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignedOrder {
    /// Duration-priced: the maker quoted a rate, settlement derives the amount.
    Rfq(RfqOrder),
    /// The maker stated the amount outright.
    Fixed(FixedOrder),
}

/// A maker-signed bid. How it settles depends on whether the order names a taker,
/// and the two cases are what decide which contract the taker must have approved:
///
/// - **Named taker** — the bid was quoted to this specific taker. Settlement
///   moves the input from their own allowance and pays them directly; the router
///   never touches the leg's tokens. Keeps the bid bound to the counterparty it
///   was quoted to, and keeps a permissioned RWA moving only between them.
/// - **No named taker** — open liquidity anyone may hit. The router pulls the
///   input, becomes the taker of record, and forwards the proceeds on.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RfqLeg {
    pub order: SignedOrder,
    pub maker_signature: Signature,
    /// The taker's consent to their own request: exactly one entry when the
    /// order names a taker, empty when it does not. (A bare `Option` cannot nest
    /// inside a `contracttype` struct in this SDK.)
    pub taker_signature: Vec<Signature>,
    pub taker_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AggregatorLeg {
    pub aggregator: Address,
    pub taker_amount: i128,
    /// What the backend's ranking said this leg would pay; the aggregator is held to it.
    pub min_maker_amount: i128,
    /// Aggregator-specific routing payload, opaque to the router.
    pub data: Bytes,
}

/// One hop of the route the taker chose, one variant per bid channel.
///
/// The two shapes settle differently on purpose: an `Rfq` leg passes straight
/// through to the counterparties, while `Dex`/`Facility` legs are paid by the
/// router. So the taker approves the settlement contract for signed bids and the
/// router for aggregator quotes — an approval set the frontend can derive from
/// the route before anything is signed.
// Variants differ in size because a signed bid carries a whole order; boxing
// would change the XDR the backend encodes, and only the XDR matters here.
#[allow(clippy::large_enum_variant)]
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Leg {
    /// An MM's signed bid, settled through the settlement contract.
    Rfq(RfqLeg),
    /// A DEX aggregator quote.
    Dex(AggregatorLeg),
    /// A facility aggregator quote.
    Facility(AggregatorLeg),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceQuote {
    pub source: Address,
    pub kind: SourceKind,
    pub maker_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteResult {
    /// Taker tokens actually spent across every leg, net of any refund.
    pub taker_spent: i128,
    /// Maker tokens the route delivered to the taker. Fees are taken by each
    /// venue where it settles, never here, so this is what `min_out` guards.
    pub amount_out: i128,
}
