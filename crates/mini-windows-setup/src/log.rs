//! The append-only record of what setup did.
//!
//! An installer that fails on somebody else's machine is nearly impossible
//! to debug from a description, so every step writes one line of readable
//! text to `<install root>\setup-log.txt`. It is plain text rather than a
//! binary or hash-chained log on purpose: the person who needs it is a user
//! reading it in Notepad, or pasting it into a bug report.
//!
//! **This log is evidence, never permission.** Nothing in this crate reads
//! it to decide whether to install, activate, or roll back --- those
//! decisions come from the pointer files and a typed approval. The same
//! boundary `mini-installer`'s event log draws (D-0076), for the same
//! reason: a record that can authorize becomes a thing worth forging.
//!
//! Lines never contain a path from the user's profile, a display name, or
//! anything else that identifies a person: a version, a digest, and a
//! timestamp are enough to explain a failure, and a log a user is encouraged
//! to paste into a public issue should not carry their username.

use crate::error::SetupError;
use std::io::Write;
use std::path::PathBuf;

/// Something setup did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupEvent {
    /// An install began for one package.
    InstallStarted {
        /// Version being installed.
        version: String,
        /// Manifest digest, hex.
        package_digest: String,
        /// Milliseconds since the Unix epoch.
        at_ms: u64,
    },
    /// A version became the active one.
    Activated {
        /// Version activated.
        version: String,
        /// Manifest digest, hex.
        package_digest: String,
        /// Milliseconds since the Unix epoch.
        at_ms: u64,
    },
    /// The active version moved back to the previous one.
    RolledBack {
        /// Version rolled back to.
        version: String,
        /// Milliseconds since the Unix epoch.
        at_ms: u64,
    },
    /// The install was removed.
    Uninstalled {
        /// Milliseconds since the Unix epoch.
        at_ms: u64,
        /// Whether the owner also chose to destroy identities.
        destroyed_user_data: bool,
    },
}

impl SetupEvent {
    /// The single log line for this event, without its newline.
    pub fn line(&self) -> String {
        match self {
            Self::InstallStarted {
                version,
                package_digest,
                at_ms,
            } => format!("{at_ms} install-started version={version} package={package_digest}"),
            Self::Activated {
                version,
                package_digest,
                at_ms,
            } => format!("{at_ms} activated version={version} package={package_digest}"),
            Self::RolledBack { version, at_ms } => {
                format!("{at_ms} rolled-back version={version}")
            }
            Self::Uninstalled {
                at_ms,
                destroyed_user_data,
            } => format!("{at_ms} uninstalled identities-destroyed={destroyed_user_data}"),
        }
    }
}

/// An append-only text log at a fixed path.
#[derive(Debug, Clone)]
pub struct SetupLog {
    path: PathBuf,
}

impl SetupLog {
    /// Log to `path`, creating it and its parent on first append.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The log's path.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Append one event.
    ///
    /// Opened in append mode for each call rather than held open: setup runs
    /// as several short-lived processes (the wizard, then "Uninstall" from
    /// Apps & features later), and a handle held across those would be a
    /// handle held open across a directory rename.
    pub fn append(&self, event: SetupEvent) -> Result<(), SetupError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| SetupError::io(parent, error))?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| SetupError::io(&self.path, error))?;
        writeln!(file, "{}", event.line()).map_err(|error| SetupError::io(&self.path, error))?;
        Ok(())
    }

    /// Read the log back as lines, newest last. A missing log is empty.
    pub fn read(&self) -> Result<Vec<String>, SetupError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => Ok(text.lines().map(str::to_string).collect()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(SetupError::io(&self.path, error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_carry_a_version_and_digest_but_no_user_path() {
        let line = SetupEvent::Activated {
            version: "0.1.0".to_string(),
            package_digest: "ab".repeat(32),
            at_ms: 1_757_635_200_000,
        }
        .line();
        assert!(line.starts_with("1757635200000 activated version=0.1.0 package=abab"));
        assert!(!line.contains('\\'));
        assert!(!line.contains('/'));
    }

    #[test]
    fn appending_never_rewrites_an_earlier_line() {
        let dir = std::env::temp_dir().join(format!("mini-setup-log-{}", std::process::id()));
        let log = SetupLog::new(dir.join("setup-log.txt"));
        log.append(SetupEvent::RolledBack {
            version: "0.1.0".to_string(),
            at_ms: 1,
        })
        .unwrap();
        log.append(SetupEvent::Uninstalled {
            at_ms: 2,
            destroyed_user_data: false,
        })
        .unwrap();
        let lines = log.read().unwrap();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("rolled-back"));
        assert!(lines[1].contains("uninstalled"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
