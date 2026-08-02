use mini_crypto::CryptoError;
use mini_intake::IntakeCoordError;
use mini_social::SocialError;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, IntakeSocialError>;

/// Why publishing an intake envelope as a post failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum IntakeSocialError {
    /// The envelope has not reached [`mini_intake_types::ReviewState::Accepted`]
    /// yet — publishing unreviewed external material as a Mininet object is
    /// exactly the authority-without-review shortcut Mininet Intake forbids.
    NotAccepted,
    /// The envelope's declared media type is not `TextPlain`/`Markdown` — the
    /// only two kinds Mininet Intake's Track B2 coordinator ever stores, and
    /// the only two this crate knows how to turn into post text.
    UnsupportedMediaType,
    /// The stored source bytes are not valid UTF-8, despite a `TextPlain`/
    /// `Markdown` media type. Defense in depth: `mini-intake`'s own
    /// coordinator already rejects non-UTF-8 bytes at intake time, so this
    /// should be unreachable in practice, not a silently ignored case.
    NotUtf8,
    /// Reading the immutable source bytes back from `mini-intake`'s backend
    /// failed.
    Intake(IntakeCoordError),
    /// Publishing (or decoding) the resulting `mini-social` post failed.
    Social(SocialError),
    /// Deriving the produced post's [`mini_intake_types::IntakeLink::Post`]
    /// target from its object id failed.
    Crypto(CryptoError),
}

impl core::fmt::Display for IntakeSocialError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            IntakeSocialError::NotAccepted => {
                write!(f, "envelope review state is not yet Accepted")
            }
            IntakeSocialError::UnsupportedMediaType => {
                write!(f, "envelope media type is not TextPlain/Markdown")
            }
            IntakeSocialError::NotUtf8 => write!(f, "source bytes are not valid UTF-8"),
            IntakeSocialError::Intake(e) => write!(f, "intake: {e}"),
            IntakeSocialError::Social(e) => write!(f, "social: {e}"),
            IntakeSocialError::Crypto(e) => write!(f, "crypto: {e}"),
        }
    }
}
impl std::error::Error for IntakeSocialError {}

impl From<IntakeCoordError> for IntakeSocialError {
    fn from(e: IntakeCoordError) -> Self {
        IntakeSocialError::Intake(e)
    }
}
impl From<SocialError> for IntakeSocialError {
    fn from(e: SocialError) -> Self {
        IntakeSocialError::Social(e)
    }
}
impl From<CryptoError> for IntakeSocialError {
    fn from(e: CryptoError) -> Self {
        IntakeSocialError::Crypto(e)
    }
}
