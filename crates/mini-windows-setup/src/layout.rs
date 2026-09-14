//! Where a per-user install lives on disk, and what records which version
//! is active.
//!
//! ## Per-user by default, and why that is not laziness
//!
//! The default install root is `%LOCALAPPDATA%\Programs\Mininet`, not
//! `C:\Program Files`. That means no UAC prompt, no administrator, and no
//! service: a person can install, run, and remove the client without asking
//! anyone for permission, which is the whole point of the project. It also
//! narrows the blast radius --- nothing here can write outside the user's own
//! profile, so a bug in this crate cannot damage the machine.
//!
//! The cost is honest and worth stating: a per-user install is writable by
//! anything else running as that user, so it offers no protection against
//! malware already running in the session. `Program Files` would, at the
//! price of requiring an administrator for every update. A managed
//! deployment that wants that trade needs a per-machine install, which does
//! not exist yet; the MSI in `packaging/windows/` is a managed-deployment
//! wrapper around this same per-user path, not an elevation of it. This crate
//! does not pretend a per-user directory is tamper-proof.
//!
//! ## Versioned directories and an atomic pointer
//!
//! ```text
//! <root>\versions\<version>\...     program files, one directory per version
//! <root>\manifests\<version>.txt    the manifest that produced it
//! <root>\current.txt                which version is active
//! <root>\previous.txt               which version to roll back to
//! <root>\setup-log.txt              append-only record of what setup did
//! ```
//!
//! Installing a new version never touches the running one: it writes a new
//! directory, then swaps a small pointer file. Activation is therefore a
//! single `rename` over one file rather than a partial overwrite of an
//! executable that may be in use --- which on Windows would fail outright,
//! since a running image is locked.
//!
//! **User data is not here.** Identities, the object store, and settings
//! live in `%LOCALAPPDATA%\Mininet`, a sibling this crate never reads or
//! writes. Uninstalling program files therefore cannot destroy an identity,
//! and that separation is what makes [`crate::UninstallApproval`]'s
//! keep-data default safe to trust.

use crate::error::SetupError;
use crate::manifest::{unhex, PackageManifest};
use crate::InstallOptions;
use mini_forge::Version;
use std::path::{Path, PathBuf};

/// Format tag for the pointer files.
const POINTER_MAGIC: &str = "MNWINCUR1";

/// Directory name holding one subdirectory per installed version.
pub const VERSIONS_DIR: &str = "versions";

/// Directory name holding one manifest per installed version.
pub const MANIFESTS_DIR: &str = "manifests";

/// Pointer file naming the active version.
pub const CURRENT_FILE: &str = "current.txt";

/// Pointer file naming the version a rollback would return to.
pub const PREVIOUS_FILE: &str = "previous.txt";

/// Append-only log of setup actions.
pub const LOG_FILE: &str = "setup-log.txt";

/// Exclusive lock held across every mutating operation on an install root.
///
/// Written as a **sibling** of the root, as `<root name>` plus this suffix.
/// A lock file inside the root would be an open handle inside a directory
/// that uninstall removes, and Windows refuses to remove such a directory.
pub const LOCK_SUFFIX: &str = ".setup-lock";

/// Which version is installed, and the exact package it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRecord {
    /// Version as the manifest spelled it.
    pub version_text: String,
    /// The same version, parsed.
    pub version: Version,
    /// BLAKE3 of the manifest that produced this install, hex.
    pub package_digest: String,
    /// When setup activated it, milliseconds since the Unix epoch.
    pub installed_at_ms: u64,
    /// Whether this install created a Start Menu entry.
    ///
    /// Recorded so a later rollback or reinstall can recover the owner's
    /// actual shell-integration choice instead of reconstructing
    /// [`InstallOptions::default`], which would silently add or drop
    /// shortcuts the owner never asked to change.
    pub start_menu_shortcut: bool,
    /// Whether this install created a Desktop shortcut.
    pub desktop_shortcut: bool,
    /// Whether this install registered in Apps & features.
    pub register_uninstall: bool,
}

impl InstallRecord {
    /// Build a record for a manifest activated at `now_ms`, remembering the
    /// shell-integration choices `options` actually applied.
    pub fn for_manifest(manifest: &PackageManifest, options: &InstallOptions, now_ms: u64) -> Self {
        Self {
            version_text: manifest.version_text.clone(),
            version: manifest.version.clone(),
            package_digest: manifest.digest_hex(),
            installed_at_ms: now_ms,
            start_menu_shortcut: options.start_menu_shortcut,
            desktop_shortcut: options.desktop_shortcut,
            register_uninstall: options.register_uninstall,
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        format!(
            "{POINTER_MAGIC}\nversion {}\ndigest {}\ninstalled {}\nstart_menu {}\ndesktop {}\nregister_uninstall {}\n",
            self.version_text,
            self.package_digest,
            self.installed_at_ms,
            self.start_menu_shortcut as u8,
            self.desktop_shortcut as u8,
            self.register_uninstall as u8,
        )
        .into_bytes()
    }

    fn parse(bytes: &[u8]) -> Result<Self, SetupError> {
        let corrupt = |reason: &'static str| SetupError::CorruptPointer { reason };
        let text = core::str::from_utf8(bytes).map_err(|_| corrupt("pointer is not UTF-8"))?;
        let mut lines = text.lines();
        if lines.next() != Some(POINTER_MAGIC) {
            return Err(corrupt("pointer does not start with the MNWINCUR1 tag"));
        }
        let version_text = lines
            .next()
            .and_then(|line| line.strip_prefix("version "))
            .ok_or_else(|| corrupt("pointer has no version line"))?;
        let digest = lines
            .next()
            .and_then(|line| line.strip_prefix("digest "))
            .ok_or_else(|| corrupt("pointer has no digest line"))?;
        let installed = lines
            .next()
            .and_then(|line| line.strip_prefix("installed "))
            .ok_or_else(|| corrupt("pointer has no installed line"))?;
        // The shell-integration lines were added after this format shipped.
        // A pointer file written by an older binary simply lacks them, and
        // that is not corruption: fall back to `InstallOptions::default`'s
        // choices, the same choices such an install would have made.
        let parse_flag = |value: &str| -> Result<bool, SetupError> {
            match value {
                "0" => Ok(false),
                "1" => Ok(true),
                _ => Err(corrupt("pointer flag is not 0 or 1")),
            }
        };
        let mut start_menu_shortcut = true;
        let mut desktop_shortcut = false;
        let mut register_uninstall = true;
        for line in lines {
            if let Some(value) = line.strip_prefix("start_menu ") {
                start_menu_shortcut = parse_flag(value)?;
            } else if let Some(value) = line.strip_prefix("desktop ") {
                desktop_shortcut = parse_flag(value)?;
            } else if let Some(value) = line.strip_prefix("register_uninstall ") {
                register_uninstall = parse_flag(value)?;
            } else {
                return Err(corrupt("pointer has trailing content"));
            }
        }
        if unhex(digest).is_none() {
            return Err(corrupt("pointer digest is not 32 hex bytes"));
        }
        let version =
            Version::parse(version_text).map_err(|_| corrupt("pointer version is not numeric"))?;
        let installed_at_ms: u64 = installed
            .parse()
            .map_err(|_| corrupt("pointer timestamp is not a u64"))?;
        Ok(Self {
            version_text: version_text.to_string(),
            version,
            package_digest: digest.to_string(),
            installed_at_ms,
            start_menu_shortcut,
            desktop_shortcut,
            register_uninstall,
        })
    }
}

/// The directory layout of one per-user install root.
#[derive(Debug, Clone)]
pub struct InstallLayout {
    root: PathBuf,
}

impl InstallLayout {
    /// Treat `root` as an install root. Nothing is created or read yet.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The default per-user root, derived from the environment.
    ///
    /// `%LOCALAPPDATA%\Programs\Mininet` on Windows. On other platforms this
    /// falls back to `$XDG_DATA_HOME` or `~/.local/share` so the engine's
    /// tests and the `mini` CLI's planning commands work anywhere, while the
    /// shell integration that genuinely requires Windows stays gated.
    pub fn default_root() -> PathBuf {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local).join("Programs").join("Mininet");
        }
        if let Some(data) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(data).join("mininet-programs");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("mininet-programs");
        }
        PathBuf::from("mininet-programs")
    }

    /// Where the client keeps identities, objects, and settings.
    ///
    /// Separate from the install root on purpose; see this module's docs.
    /// `mini-desktop` derives the same path from `MININET_HOME` or
    /// `%LOCALAPPDATA%\Mininet`, and this function mirrors that so an
    /// uninstall can *report* what it is deliberately not deleting.
    pub fn default_user_data_root() -> PathBuf {
        if let Some(home) = std::env::var_os("MININET_HOME") {
            return PathBuf::from(home);
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local).join("Mininet");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(".mininet");
        }
        PathBuf::from(".mininet")
    }

    /// The install root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Directory for one version's program files.
    pub fn version_dir(&self, version_text: &str) -> PathBuf {
        self.root.join(VERSIONS_DIR).join(version_text)
    }

    /// Stored manifest for one version.
    pub fn manifest_path(&self, version_text: &str) -> PathBuf {
        self.root
            .join(MANIFESTS_DIR)
            .join(format!("{version_text}.txt"))
    }

    /// The pointer naming the active version.
    pub fn current_path(&self) -> PathBuf {
        self.root.join(CURRENT_FILE)
    }

    /// The pointer naming the rollback target.
    pub fn previous_path(&self) -> PathBuf {
        self.root.join(PREVIOUS_FILE)
    }

    /// The append-only setup log.
    pub fn log_path(&self) -> PathBuf {
        self.root.join(LOG_FILE)
    }

    /// The exclusive lock guarding this install root.
    ///
    /// Beside the root rather than in it: see [`LOCK_SUFFIX`].
    pub fn lock_path(&self) -> PathBuf {
        let name = self
            .root
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "mininet".to_string());
        match self.root.parent() {
            Some(parent) => parent.join(format!("{name}{LOCK_SUFFIX}")),
            None => PathBuf::from(format!("{name}{LOCK_SUFFIX}")),
        }
    }

    /// Read the active install record, if there is one.
    pub fn current(&self) -> Result<Option<InstallRecord>, SetupError> {
        read_pointer(&self.current_path())
    }

    /// Read the rollback target record, if there is one.
    pub fn previous(&self) -> Result<Option<InstallRecord>, SetupError> {
        read_pointer(&self.previous_path())
    }

    /// Read the stored manifest for one version.
    pub fn stored_manifest(&self, version_text: &str) -> Result<PackageManifest, SetupError> {
        let path = self.manifest_path(version_text);
        let bytes = std::fs::read(&path).map_err(|error| SetupError::io(&path, error))?;
        PackageManifest::parse(&bytes)
    }

    /// Every version directory present on disk, sorted oldest-first.
    ///
    /// Entries whose names are not dotted-numeric versions are skipped
    /// rather than reported: a stray directory someone dropped in is not a
    /// reason to refuse to list the real installs.
    pub fn installed_versions(&self) -> Result<Vec<String>, SetupError> {
        let dir = self.root.join(VERSIONS_DIR);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut found: Vec<(Version, String)> = Vec::new();
        for entry in std::fs::read_dir(&dir).map_err(|error| SetupError::io(&dir, error))? {
            let entry = entry.map_err(|error| SetupError::io(&dir, error))?;
            if !entry.path().is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if let Ok(version) = Version::parse(&name) {
                found.push((version, name));
            }
        }
        found.sort();
        Ok(found.into_iter().map(|(_, name)| name).collect())
    }

    /// Write the active pointer atomically.
    pub fn set_current(&self, record: &InstallRecord) -> Result<(), SetupError> {
        write_atomic(&self.current_path(), &record.to_bytes())
    }

    /// Write the rollback pointer atomically.
    pub fn set_previous(&self, record: &InstallRecord) -> Result<(), SetupError> {
        write_atomic(&self.previous_path(), &record.to_bytes())
    }

    /// Remove the rollback pointer, after a rollback consumes it.
    pub fn clear_previous(&self) -> Result<(), SetupError> {
        let path = self.previous_path();
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(SetupError::io(&path, error)),
        }
    }
}

fn read_pointer(path: &Path) -> Result<Option<InstallRecord>, SetupError> {
    match std::fs::read(path) {
        Ok(bytes) => InstallRecord::parse(&bytes).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(SetupError::io(path, error)),
    }
}

/// Write `bytes` to `path` by writing a sibling temporary file and renaming
/// it over the destination.
///
/// The rename is the reason activation cannot leave a half-written pointer:
/// a reader either sees the whole old pointer or the whole new one. On
/// Windows `fs::rename` fails if the destination exists, so the old file is
/// removed first --- a narrow window where neither exists, which
/// [`crate::Setup::status`] reports as "not installed" rather than
/// misreading as corrupt, and which the next activation repairs.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), SetupError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| SetupError::io(parent, error))?;
    }
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, bytes).map_err(|error| SetupError::io(&temp, error))?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|error| SetupError::io(path, error))?;
    }
    std::fs::rename(&temp, path).map_err(|error| SetupError::io(path, error))?;
    Ok(())
}
