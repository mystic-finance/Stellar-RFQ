use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    InvalidAmount = 1,
    InvalidConfig = 2,
    UnsupportedPair = 3,
    NotAccepted = 4,
    NoPrice = 5,
    CapExceeded = 6,
    InsufficientLiquidity = 7,
    BelowMinOut = 8,
    NotAllowlisted = 9,
    TooManyRwas = 10,
    StillHeld = 11,
    RedemptionOpen = 12,
    NoRedemption = 13,
    NotFullyRepaid = 14,
    Paused = 15,
}
