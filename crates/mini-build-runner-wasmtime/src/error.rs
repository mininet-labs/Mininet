//! Error type for `mini-build-runner-wasmtime`.

use core::fmt;

/// Errors this crate's own plumbing can produce (setup/IPC/content-store
/// failures). A failure *inside the guest* is never represented here --
/// that is an [`mini_pipeline_protocol::ExitStatus`], reported inside a
/// normal [`mini_pipeline_protocol::ExecutionResult`], because a hostile
/// or buggy component failing is an expected outcome the protocol must
/// carry, not a runner-process error.
#[derive(Debug)]
pub enum RunnerError {
    Protocol(mini_pipeline_protocol::ProtocolError),
    Io(std::io::Error),
    /// The bytes read from the content store under a claimed digest do not
    /// hash to that digest -- the store is corrupt, lying, or the
    /// coordinator sent a digest for content it never actually stored.
    DigestMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },
    /// No request was ever sent (clean EOF before a single frame).
    NoRequest,
    Wasmtime(String),
}

impl fmt::Display for RunnerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunnerError::Protocol(e) => write!(f, "protocol error: {e}"),
            RunnerError::Io(e) => write!(f, "I/O error: {e}"),
            RunnerError::DigestMismatch { expected, actual } => write!(
                f,
                "content-store digest mismatch: expected {}, got {}",
                hex(expected),
                hex(actual)
            ),
            RunnerError::NoRequest => write!(f, "no execution request received"),
            RunnerError::Wasmtime(e) => write!(f, "wasmtime setup error: {e}"),
        }
    }
}

impl std::error::Error for RunnerError {}

impl From<mini_pipeline_protocol::ProtocolError> for RunnerError {
    fn from(e: mini_pipeline_protocol::ProtocolError) -> Self {
        RunnerError::Protocol(e)
    }
}

impl From<std::io::Error> for RunnerError {
    fn from(e: std::io::Error) -> Self {
        RunnerError::Io(e)
    }
}

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub type Result<T> = core::result::Result<T, RunnerError>;
