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
    /// [`crate::ble::chunk_frame`]'s `mtu` argument left no room for even
    /// the chunk header.
    MtuTooSmall {
        /// The smallest `mtu` that would fit a header plus one payload byte.
        min: usize,
        /// The `mtu` that was passed.
        got: usize,
    },
    /// A frame would need more chunks than a `u16` chunk count can express
    /// at the given MTU.
    TooManyChunks {
        /// The largest representable chunk count ([`u16::MAX`]).
        max: u16,
        /// How many chunks the frame would actually need.
        needed: usize,
    },
    /// A chunk's header was structurally invalid (a zero `chunk_count`, or
    /// a `chunk_count` that disagrees with the reassembly already in
    /// progress).
    BadChunk,
    /// A chunk arrived with an index other than the one
    /// [`crate::ble::ChunkReassembler`] was expecting next.
    OutOfOrderChunk {
        /// The chunk index that was expected.
        expected: u16,
        /// The chunk index that actually arrived.
        got: u16,
    },
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
            BearerError::MtuTooSmall { min, got } => {
                write!(f, "mtu too small: need at least {min} bytes, got {got}")
            }
            BearerError::TooManyChunks { max, needed } => {
                write!(f, "frame needs {needed} chunks, more than the max {max}")
            }
            BearerError::BadChunk => write!(f, "malformed or inconsistent chunk header"),
            BearerError::OutOfOrderChunk { expected, got } => {
                write!(
                    f,
                    "out-of-order chunk: expected index {expected}, got {got}"
                )
            }
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
