#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EconomyError {
    Overflow,
    Underflow,
    EmptyEligibleSet,
    DuplicateBeneficiary,
    InvalidDuration,
    InvalidPolicy,
    ChannelExceeded,
    TotalExceeded,
    InvalidGenesis,
    InvalidSnapshot,
}

impl core::fmt::Display for EconomyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for EconomyError {}

pub type Result<T> = core::result::Result<T, EconomyError>;
