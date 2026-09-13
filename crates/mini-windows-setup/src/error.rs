//! Failures this crate can report.
//!
//! Each variant names a *specific* refusal, because an installer that
//! reports "install failed" teaches the user to click through it. A person
//! who sees "file 3 of 11 did not match its recorded digest" can decide
//! whether they have a corrupted download or a tampered package.

/// A setup operation that did not happen.
#[derive(Debug)]
#[non_exhaustive]
pub enum SetupError {
    /// Filesystem failure, with the path it happened on.
    Io {
        /// What the engine was touching.
        path: String,
        /// The underlying OS error.
        source: std::io::Error,
    },
    /// A package path is not a safe relative Windows path.
    UnsafePath {
        /// The offending path, verbatim, so it can be reported to a human.
        path: String,
        /// Which rule it broke.
        reason: &'static str,
    },
    /// The manifest's bytes were not a well-formed manifest.
    MalformedManifest {
        /// 1-based line number, or 0 for whole-document problems.
        line: usize,
        /// Which rule it broke.
        reason: &'static str,
    },
    /// The manifest's trailing self-digest did not match its own body.
    ManifestDigestMismatch,
    /// Two manifest entries name one file on a case-insensitive filesystem.
    DuplicatePath {
        /// The path that collided with an earlier entry.
        path: String,
    },
    /// A version string was not dotted-numeric (`mini_forge::release`).
    BadVersion {
        /// The string that failed to parse.
        value: String,
    },
    /// A file's bytes did not match the digest the manifest recorded.
    DigestMismatch {
        /// Which package file.
        path: String,
    },
    /// A file's length did not match the length the manifest recorded.
    LengthMismatch {
        /// Which package file.
        path: String,
        /// What the manifest said.
        expected: u64,
        /// What was actually there.
        found: u64,
    },
    /// A file the manifest lists is missing from the payload or install.
    MissingFile {
        /// Which package file.
        path: String,
    },
    /// The payload container was not a well-formed container.
    MalformedContainer {
        /// Which rule it broke.
        reason: &'static str,
    },
    /// [`crate::Setup::install`] was called with an approval naming a
    /// different package than the one actually being installed.
    ///
    /// The typed-domain rule: approving an install approves one exact
    /// package, identified by its manifest digest, not "whatever is in the
    /// installer right now".
    ApprovalMismatch {
        /// Manifest digest the owner approved, hex.
        approved: String,
        /// Manifest digest of the package on hand, hex.
        offered: String,
    },
    /// Activation would move the client to an older version than the one
    /// already active, and the caller did not declare it a rollback.
    ///
    /// Mirrors `mini_forge::release::check_no_rollback`: a silent downgrade
    /// is how a fixed vulnerability gets reintroduced.
    WouldDowngrade {
        /// Version currently active, as the manifest spelled it.
        active: String,
        /// Version the caller asked to activate.
        candidate: String,
    },
    /// A rollback was requested with no previous version recorded.
    NoPreviousVersion,
    /// The install root contains, or is, the user-data root.
    ///
    /// Uninstall removes the install root recursively, so an overlap would
    /// destroy the identity vault under an approval that promised to keep it.
    OverlappingRoots {
        /// The install root.
        install_root: String,
        /// The user-data root it would swallow.
        user_data_root: String,
    },
    /// The install root's pointer file exists but is not well formed.
    CorruptPointer {
        /// Which rule it broke.
        reason: &'static str,
    },
    /// The version the pointer names is not present on disk.
    MissingVersionDirectory {
        /// The version that should have been installed.
        version: String,
    },
    /// Shell integration (Start Menu entry, uninstall registration) is only
    /// available on Windows.
    UnsupportedPlatform,
    /// Shell integration ran but reported failure.
    ShellIntegrationFailed {
        /// Which step.
        step: String,
        /// What it reported.
        detail: String,
    },
    /// A display string contained characters this crate refuses to put into
    /// a generated script or registry value.
    UnsafeDisplayText {
        /// Which field.
        field: &'static str,
        /// Which rule it broke.
        reason: &'static str,
    },
}

impl core::fmt::Display for SetupError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "i/o on {path}: {source}"),
            Self::UnsafePath { path, reason } => {
                write!(f, "unsafe package path {path:?}: {reason}")
            }
            Self::MalformedManifest { line, reason } if *line == 0 => {
                write!(f, "malformed package manifest: {reason}")
            }
            Self::MalformedManifest { line, reason } => {
                write!(f, "malformed package manifest, line {line}: {reason}")
            }
            Self::ManifestDigestMismatch => {
                write!(f, "package manifest does not match its own self-digest")
            }
            Self::DuplicatePath { path } => write!(
                f,
                "package lists {path:?} twice (Windows file names are case-insensitive)"
            ),
            Self::BadVersion { value } => write!(f, "not a dotted-numeric version: {value:?}"),
            Self::DigestMismatch { path } => {
                write!(f, "{path} does not match its recorded digest")
            }
            Self::LengthMismatch {
                path,
                expected,
                found,
            } => write!(f, "{path} is {found} bytes, manifest records {expected}"),
            Self::MissingFile { path } => write!(f, "package file {path} is missing"),
            Self::MalformedContainer { reason } => {
                write!(f, "malformed package container: {reason}")
            }
            Self::ApprovalMismatch { approved, offered } => write!(
                f,
                "owner approved package {approved} but the installer holds {offered}"
            ),
            Self::WouldDowngrade { active, candidate } => write!(
                f,
                "refusing to replace active version {active} with older {candidate}"
            ),
            Self::NoPreviousVersion => write!(f, "no previous version recorded to roll back to"),
            Self::OverlappingRoots {
                install_root,
                user_data_root,
            } => write!(
                f,
                "install location {install_root} contains your identities and posts in {user_data_root};                  removing the program would delete them, so this location is refused"
            ),
            Self::CorruptPointer { reason } => write!(f, "corrupt install pointer: {reason}"),
            Self::MissingVersionDirectory { version } => {
                write!(f, "installed version {version} is missing from disk")
            }
            Self::UnsupportedPlatform => {
                write!(
                    f,
                    "Windows shell integration is unavailable on this platform"
                )
            }
            Self::ShellIntegrationFailed { step, detail } => {
                write!(f, "shell integration step {step} failed: {detail}")
            }
            Self::UnsafeDisplayText { field, reason } => {
                write!(f, "unsafe text for {field}: {reason}")
            }
        }
    }
}

impl std::error::Error for SetupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl SetupError {
    /// Wrap an [`std::io::Error`] with the path it occurred on.
    pub(crate) fn io(path: impl AsRef<std::path::Path>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.as_ref().display().to_string(),
            source,
        }
    }

    /// A short, stable machine code for `--json` output.
    ///
    /// Stable in the same sense `mini-cli`'s envelope codes are stable:
    /// callers may match on these strings, so a rename is a breaking
    /// change, not a wording fix.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "io",
            Self::UnsafePath { .. } => "unsafe_path",
            Self::MalformedManifest { .. } => "malformed_manifest",
            Self::ManifestDigestMismatch => "manifest_digest_mismatch",
            Self::DuplicatePath { .. } => "duplicate_path",
            Self::BadVersion { .. } => "bad_version",
            Self::DigestMismatch { .. } => "digest_mismatch",
            Self::LengthMismatch { .. } => "length_mismatch",
            Self::MissingFile { .. } => "missing_file",
            Self::MalformedContainer { .. } => "malformed_container",
            Self::ApprovalMismatch { .. } => "approval_mismatch",
            Self::WouldDowngrade { .. } => "would_downgrade",
            Self::NoPreviousVersion => "no_previous_version",
            Self::OverlappingRoots { .. } => "overlapping_roots",
            Self::CorruptPointer { .. } => "corrupt_pointer",
            Self::MissingVersionDirectory { .. } => "missing_version_directory",
            Self::UnsupportedPlatform => "unsupported_platform",
            Self::ShellIntegrationFailed { .. } => "shell_integration_failed",
            Self::UnsafeDisplayText { .. } => "unsafe_display_text",
        }
    }
}
