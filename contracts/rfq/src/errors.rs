use soroban_sdk::contracterror;

/// Contract error codes. Mirrors the `LibNativeOrdersRichErrors` family.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    /// Order is not in the `Fillable` state.
    OrderNotFillable = 3,
    /// Caller is not the order's designated taker.
    OrderNotFillableByTaker = 4,
    /// Caller is not the order's designated sender.
    OrderNotFillableBySender = 5,
    /// Submitter is not the order's `tx_origin` (nor a whitelisted origin).
    OrderNotFillableByOrigin = 6,
    /// ed25519 signer is not registered to the maker.
    SignerNotAuthorized = 7,
    /// Fill-or-kill order could not be filled completely.
    FillOrKillFailed = 8,
    /// Fill amount must be positive.
    InvalidFillAmount = 10,
    /// Arithmetic overflow while computing fill amounts.
    Overflow = 11,
    /// Caller is not the admin.
    NotAdmin = 12,
}
