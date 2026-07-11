//! Error type for `mini-cli`.

use core::fmt;

/// Errors this crate can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// `identity init` was run twice against the same home.
    AlreadyInitialized,
    /// A command needing an identity was run before `identity init`.
    NotInitialized,
    /// A seed file existed but was not the expected length.
    CorruptSeedFile,
    /// Filesystem I/O failure, message from the underlying `io::Error`.
    Io(String),
    /// A `did-mini` operation failed.
    Identity(String),
    /// A `mini-forge` operation failed.
    Forge(String),
    /// A `mini-store` operation failed.
    Store(String),
    /// A `mini-objects` operation failed.
    Object(String),
    /// A `mini-bearer`/`mini-sync` network sync operation failed.
    Sync(String),
    /// A `mini-media` operation failed.
    Media(String),
    /// A `mini-provenance` operation failed.
    Provenance(String),
    /// A `mini-installer` operation failed.
    Installer(String),
    /// Spawning or speaking `mini-pipeline-protocol` to the real
    /// `mini-build-runner-wasmtime` binary failed.
    Build(String),
    /// The command line itself was malformed (missing/unknown flag, wrong
    /// argument count).
    Usage(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::AlreadyInitialized => {
                write!(
                    f,
                    "this home is already initialized (see `mini identity show`)"
                )
            }
            CliError::NotInitialized => {
                write!(f, "no identity here yet -- run `mini identity init` first")
            }
            CliError::CorruptSeedFile => write!(f, "seed file exists but is not valid"),
            CliError::Io(e) => write!(f, "I/O error: {e}"),
            CliError::Identity(e) => write!(f, "identity error: {e}"),
            CliError::Forge(e) => write!(f, "forge error: {e}"),
            CliError::Store(e) => write!(f, "store error: {e}"),
            CliError::Object(e) => write!(f, "object error: {e}"),
            CliError::Sync(e) => write!(f, "sync error: {e}"),
            CliError::Media(e) => write!(f, "media error: {e}"),
            CliError::Provenance(e) => write!(f, "provenance error: {e}"),
            CliError::Installer(e) => write!(f, "installer error: {e}"),
            CliError::Build(e) => write!(f, "build error: {e}"),
            CliError::Usage(e) => write!(f, "usage error: {e}"),
        }
    }
}

impl std::error::Error for CliError {}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, CliError>;
