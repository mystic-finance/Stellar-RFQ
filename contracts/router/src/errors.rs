use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotAuthorized = 2,
    Paused = 3,
    EmptyRoute = 4,
    InvalidAmount = 5,
    /// The route's realised output is below the taker's minimum.
    BelowMinOut = 6,
    /// A leg's tokens do not match the route's pair.
    TokenMismatch = 7,
    /// The leg names a source that governance has not whitelisted.
    SourceNotRegistered = 8,
    /// A signed leg is not quoted to this route's taker — settlement would pull
    /// from, and pay, somebody else.
    LegTakerMismatch = 14,
    /// The leg's taker signature does not match whether its order names a taker.
    LegSignatureMismatch = 15,
    /// A source delivered less than the quote it was held to.
    SourceUnderDelivered = 10,
    /// The route spent more taker tokens than its legs declared.
    OverSpent = 11,
    /// Less input arrived than the route declared — a transfer-taxed input token
    /// cannot be routed, since the legs' amounts are already fixed and signed.
    InputShortfall = 9,
    Overflow = 13,
}
