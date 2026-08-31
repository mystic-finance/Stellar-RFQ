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
    BelowMinOut = 6,
    TokenMismatch = 7,
    SourceNotRegistered = 8,
    LegTakerMismatch = 14,
    LegSignatureMismatch = 15,
    SourceUnderDelivered = 10,
    OverSpent = 11,
    InputShortfall = 9,
    Overflow = 13,
}
