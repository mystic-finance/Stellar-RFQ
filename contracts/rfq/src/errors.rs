use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotAuthorized = 2,
    Paused = 3,
    OrderNotFillable = 4,
    NotFillableByTaker = 5,
    SignerNotAuthorized = 6,
    InvalidAmount = 7,
    Overflow = 8,
    NoSchedule = 9,
    InvalidSchedule = 10,
    NoPrice = 11,
    BpsPerDayTooHigh = 12,
    DiscountTooLarge = 13,
    FeeOutOfBounds = 14,
    BelowMinReceived = 15,
    MakerAmountTooHigh = 16,
    SameToken = 17,
    ListingNotActive = 18,
    AskAboveMax = 19,
    PriceDeviation = 20,
    ScheduleShiftTooLarge = 21,
    InvalidConfig = 22,
    WrongSender = 23,
    SaltIsCancelled = 24,
    RequestOverfilled = 25,
    PriceUpdateTooSoon = 26,
}
