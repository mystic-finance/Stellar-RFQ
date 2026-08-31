use soroban_sdk::{contractclient, contracttype, Address, Bytes, Env, Vec};

pub use orders::{FillResult, FixedOrder, RfqOrder, Signature};

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

#[contractclient(name = "AggregatorClient")]
/// The router transfers `amount_in` to the aggregator before calling `fill`, so
/// the aggregator never holds an allowance on anyone. `data` is an opaque
/// aggregator-specific payload carried through untouched.
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

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignedOrder {
    Rfq(RfqOrder),
    Fixed(FixedOrder),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RfqLeg {
    pub order: SignedOrder,
    pub maker_signature: Signature,
    /// Exactly one entry when the order names a taker, empty when it does not.
    /// A bare `Option` cannot nest inside a `contracttype` struct in this SDK.
    pub taker_signature: Vec<Signature>,
    pub taker_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AggregatorLeg {
    pub aggregator: Address,
    pub taker_amount: i128,
    pub min_maker_amount: i128,
    pub data: Bytes,
}

// Boxing the signed-bid variant would change the XDR the backend encodes.
#[allow(clippy::large_enum_variant)]
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Leg {
    Rfq(RfqLeg),
    Dex(AggregatorLeg),
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
    pub taker_spent: i128,
    pub amount_out: i128,
}
