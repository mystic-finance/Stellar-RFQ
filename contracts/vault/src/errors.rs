use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    InvalidAmount = 1,
    InvalidConfig = 2,
    DepositCapExceeded = 3,
    NotAllowlisted = 4,
    InsufficientLiquidity = 5,
    VenueNotFound = 6,
    VenueNotEmpty = 7,
    TooManyVenues = 8,
}
