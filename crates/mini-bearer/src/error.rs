//! Errors for the bearer transport and encrypted channel.

use mini_crypto::CryptoError;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, BearerError>;

/// A bearer or channel failure.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BearerError {
    /// The bearer's peer endpoint is gone; no more frames can be sent or received.
    Closed,
    /// A frame exceeded the wire size limit.
    FrameTooLarge {
        /// The maximum permitted frame length in bytes.
        max: usize,
        /// The length that was attempted.
        got: usize,
    },
    /// A framed stream ended in the middle of a frame.
    Truncated,
    /// A handshake message was malformed (wrong length or structure).
    BadHandshake,
    /// A handshake advertised an unsupported protocol version.
    UnsupportedVersion(u8),
    /// The per-direction message counter is exhausted; the channel must be torn
    /// down and re-established (a nonce must never repeat under one key).
    CounterExhausted,
    /// A wrapped cryptographic error (bad key agreement, AEAD auth failure, KDF).
    Crypto(CryptoError),
    /// A wrapped OS I/O error from a real socket-based bearer (e.g.
    /// [`crate::tcp::TcpBearer`]). Carries the error's message rather than
    /// the error itself so `BearerError` can stay `Clone`/`PartialEq`/`Eq`.
    Io(String),
}

impl core::fmt::Display for BearerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BearerError::Closed => write!(f, "bearer endpoint is closed"),
            BearerError::FrameTooLarge { max, got } => {
                write!(f, "frame too large: max {max} bytes, got {got}")
            }
            BearerError::Truncated => write!(f, "framed stream ended mid-frame"),
            BearerError::BadHandshake => write!(f, "malformed handshake message"),
            BearerError::UnsupportedVersion(v) => {
                write!(f, "unsupported channel protocol version {v}")
            }
            BearerError::CounterExhausted => {
                write!(
                    f,
                    "channel message counter exhausted; re-establish the channel"
                )
            }
            BearerError::Crypto(e) => write!(f, "crypto error: {e}"),
            BearerError::Io(msg) => write!(f, "I/O error: {msg}"),
        }
    }
}

impl std::error::Error for BearerError {}

impl From<CryptoError> for BearerError {
    fn from(e: CryptoError) -> Self {
        BearerError::Crypto(e)
    }
}

impl From<std::io::Error> for BearerError {
    fn from(e: std::io::Error) -> Self {
        BearerError::Io(e.to_string())
    }
}
