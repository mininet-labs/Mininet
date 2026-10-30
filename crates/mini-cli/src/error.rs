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
    /// The author-sequence counter file existed but its contents were not
    /// a trustworthy `u64` (empty, non-numeric, out of range, or an
    /// unexpected read error) — never silently treated as "no counter
    /// yet" (F-04).
    CorruptSequenceFile,
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
    /// `mini selftest` ran and at least one check failed.
    ///
    /// Carried as an error, not an ordinary result, so the process exits
    /// non-zero: a smoke test that prints FAIL lines and then reports success
    /// to the shell is worse than no smoke test, because a script will trust
    /// it. The payload is the full report in human mode and a compact summary
    /// under `--json`, where the structured `clean` field is what a caller
    /// should read.
    SelfTest(String),
    /// A `mini-windows-setup` operation failed (`mini windows ...`).
    ///
    /// Carries the engine's own stable code rather than flattening every
    /// Windows-packaging failure to one string, so `mini windows --json`
    /// and `mininet-setup --json` report the same `error_code` for the same
    /// failure and a script can branch on it either way.
    WindowsSetup {
        /// `mini_windows_setup::SetupError::code`.
        code: &'static str,
        /// The engine's human-readable message.
        message: String,
    },
    /// Spawning or speaking `mini-pipeline-protocol` to the real
    /// `mini-build-runner-wasmtime` binary failed.
    Build(String),
    /// A `mini-keystone`/`mini-presence` operation failed (`mini keystone
    /// run`).
    Keystone(String),
    /// A `mini-intake`/`mini-intake-types`/`mini-intake-social` operation
    /// failed (`mini intake ...`).
    Intake(String),
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
            CliError::CorruptSequenceFile => write!(
                f,
                "sequence counter file exists but is not a trustworthy count -- refusing to reuse old sequence numbers"
            ),
            CliError::Io(e) => write!(f, "I/O error: {e}"),
            CliError::Identity(e) => write!(f, "identity error: {e}"),
            CliError::Forge(e) => write!(f, "forge error: {e}"),
            CliError::Store(e) => write!(f, "store error: {e}"),
            CliError::Object(e) => write!(f, "object error: {e}"),
            CliError::Sync(e) => write!(f, "sync error: {e}"),
            CliError::Media(e) => write!(f, "media error: {e}"),
            CliError::Provenance(e) => write!(f, "provenance error: {e}"),
            CliError::Installer(e) => write!(f, "installer error: {e}"),
            CliError::SelfTest(report) => write!(f, "{report}"),
            CliError::WindowsSetup { message, .. } => write!(f, "windows setup error: {message}"),
            CliError::Build(e) => write!(f, "build error: {e}"),
            CliError::Keystone(e) => write!(f, "keystone demo error: {e}"),
            CliError::Intake(e) => write!(f, "intake error: {e}"),
            CliError::Usage(e) => write!(f, "usage error: {e}"),
        }
    }
}

impl CliError {
    /// A stable, machine-readable snake_case identifier for `--json`
    /// error output (`crate::json::err_envelope`'s `error_code` field) --
    /// stable across releases (a scraper matches on this), unlike the
    /// free-text `Display` message which stays free to change wording.
    pub fn error_code(&self) -> &'static str {
        match self {
            CliError::AlreadyInitialized => "already_initialized",
            CliError::NotInitialized => "not_initialized",
            CliError::CorruptSeedFile => "corrupt_seed_file",
            CliError::CorruptSequenceFile => "corrupt_sequence_file",
            CliError::Io(_) => "io",
            CliError::Identity(_) => "identity",
            CliError::Forge(_) => "forge",
            CliError::Store(_) => "store",
            CliError::Object(_) => "object",
            CliError::Sync(_) => "sync",
            CliError::Media(_) => "media",
            CliError::Provenance(_) => "provenance",
            CliError::Installer(_) => "installer",
            CliError::SelfTest(_) => "selftest_failed",
            CliError::WindowsSetup { code, .. } => code,
            CliError::Build(_) => "build",
            CliError::Keystone(_) => "keystone",
            CliError::Intake(_) => "intake",
            CliError::Usage(_) => "usage",
        }
    }
}

impl std::error::Error for CliError {}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, CliError>;
