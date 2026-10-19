use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    InvalidConfig = 1,
    InvalidRoute = 2,
    VenueNotAllowed = 3,
    BelowMinOut = 4,
    InputShortfall = 5,
    PartnerFeeTooHigh = 6,
    Paused = 7,
    TooManyVenues = 8,
    Overflow = 9,
}
