use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    InvalidConfig = 1,
    NotRegistered = 2,
    TooManyActive = 3,
    AdapterNotAllowed = 4,
    FacilityNotActive = 5,
    InvalidData = 6,
    BelowMinOut = 7,
    Paused = 8,
}
