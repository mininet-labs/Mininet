//! Errors for the relay/rendezvous protocol.

use did_mini::IdentityError;
use mini_bearer::BearerError;
use mini_crypto::CryptoError;

use crate::role::RelayRole;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, RelayError>;

/// Why a relay/mailbox operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RelayError {
    /// Bytes ended before a declared length.
    Truncated,
    /// Bytes remained after a complete decode.
    TrailingBytes,
    /// A declared count or length exceeded a hard decode limit.
    LimitExceeded,
    /// An unrecognized [`RelayRole`] tag.
    BadRelayRole,
    /// An unrecognized payload-size-class tag.
    BadSizeClass,
    /// The mailbox grant's format version is not recognized.
    UnsupportedMailboxGrantVersion,
    /// The relay envelope's format version is not recognized.
    UnsupportedEnvelopeVersion,
    /// A mailbox check was made against a mailbox the grant was not
    /// issued for.
    MailboxMismatch,
    /// The presented token secret does not match the grant's commitment.
    MailboxTokenMismatch,
    /// The holder proof was not made by the grant's named grantee.
    MailboxGranteeMismatch,
    /// The signing device named by the grant does not match the KEL
    /// supplied for validation.
    MailboxIssuerMismatch,
    /// The grant's validity window has already ended.
    MailboxExpired,
    /// The grant's validity window has not started yet.
    MailboxNotYetValid,
    /// The same relay identity was assigned more than one role for a
    /// single delivery — violates the "no one provider owns every role"
    /// rule (research §5.2).
    SingleRelayMultipleRoles,
    /// A mandatory role (`Entry`/`Rendezvous`) is missing from a
    /// delivery's assignment.
    MissingRole(RelayRole),
    /// A role appears more than once in a delivery's assignment.
    DuplicateRole(RelayRole),
    /// An identity/delegation/signature failure.
    Identity(IdentityError),
    /// A cryptographic primitive failure.
    Crypto(CryptoError),
    /// A bearer/channel transport failure (including AEAD auth failure —
    /// tampered envelope fields break decryption).
    Bearer(BearerError),
}

impl core::fmt::Display for RelayError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RelayError::Truncated => write!(f, "relay bytes truncated"),
            RelayError::TrailingBytes => write!(f, "trailing bytes after relay structure"),
            RelayError::LimitExceeded => write!(f, "decode limit exceeded"),
            RelayError::BadRelayRole => write!(f, "unrecognized relay role tag"),
            RelayError::BadSizeClass => write!(f, "unrecognized payload size class tag"),
            RelayError::UnsupportedMailboxGrantVersion => {
                write!(f, "unsupported or unrecognized mailbox grant version")
            }
            RelayError::UnsupportedEnvelopeVersion => {
                write!(f, "unsupported or unrecognized relay envelope version")
            }
            RelayError::MailboxMismatch => {
                write!(f, "mailbox grant does not cover the requested mailbox")
            }
            RelayError::MailboxTokenMismatch => {
                write!(f, "mailbox token does not match the grant's commitment")
            }
            RelayError::MailboxGranteeMismatch => {
                write!(f, "holder proof was not made by the grant's named grantee")
            }
            RelayError::MailboxIssuerMismatch => {
                write!(f, "signing device does not match the issuer KEL")
            }
            RelayError::MailboxExpired => write!(f, "mailbox grant has expired"),
            RelayError::MailboxNotYetValid => write!(f, "mailbox grant is not valid yet"),
            RelayError::SingleRelayMultipleRoles => {
                write!(f, "one relay identity was assigned more than one role")
            }
            RelayError::MissingRole(role) => write!(f, "delivery is missing role {role:?}"),
            RelayError::DuplicateRole(role) => write!(f, "role {role:?} appears more than once"),
            RelayError::Identity(e) => write!(f, "identity error: {e}"),
            RelayError::Crypto(e) => write!(f, "crypto error: {e}"),
            RelayError::Bearer(e) => write!(f, "bearer error: {e}"),
        }
    }
}

impl std::error::Error for RelayError {}

impl From<IdentityError> for RelayError {
    fn from(e: IdentityError) -> Self {
        RelayError::Identity(e)
    }
}
impl From<CryptoError> for RelayError {
    fn from(e: CryptoError) -> Self {
        RelayError::Crypto(e)
    }
}
impl From<BearerError> for RelayError {
    fn from(e: BearerError) -> Self {
        RelayError::Bearer(e)
    }
}
