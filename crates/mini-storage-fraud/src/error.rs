#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FraudError {
    /// A field exceeded a bound (bytes, count) enforced before allocation.
    LimitExceeded,
    /// Wire bytes ended before a value they promised was fully read.
    Truncated,
    /// Wire bytes remained after decoding a value fully.
    TrailingBytes,
    /// A DID string did not parse.
    InvalidDid,
    /// A [`did_mini::Kel`] passed to `verify` does not belong to the
    /// claimed provider root.
    ProviderMismatch,
    /// The claim carries no signatures, or a signature does not verify.
    BadProviderSignature,
    /// An unrecognized encoding version tag.
    UnsupportedVersion,
    /// Two claims passed to [`crate::verify_collision`] do not actually
    /// conflict (same root, or different committed roots).
    NotACollision,
}

impl core::fmt::Display for FraudError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FraudError::LimitExceeded => write!(f, "value exceeded a bound"),
            FraudError::Truncated => write!(f, "wire bytes ended early"),
            FraudError::TrailingBytes => write!(f, "trailing bytes after decode"),
            FraudError::InvalidDid => write!(f, "invalid DID"),
            FraudError::ProviderMismatch => {
                write!(f, "KEL does not belong to the claimed provider root")
            }
            FraudError::BadProviderSignature => write!(f, "bad or missing provider signature"),
            FraudError::UnsupportedVersion => write!(f, "unsupported encoding version"),
            FraudError::NotACollision => write!(f, "the two claims do not actually conflict"),
        }
    }
}

impl std::error::Error for FraudError {}

pub type Result<T> = core::result::Result<T, FraudError>;
