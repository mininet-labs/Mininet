//! Windows install engine for the Mininet client: package format, digest
//! verification, per-user install, activation, rollback, and uninstall.
//!
//! ## What was missing, and what this is
//!
//! The repository already had a real installation layer --- `mini-installer`
//! (D-0071) --- but it activates by replacing a POSIX symlink and is
//! documented as Unix-only. Nothing could install the Windows client. There
//! was no package format, no way to check a downloaded build's bytes, no
//! Start Menu entry, no Apps & features registration, and no uninstall: the
//! only "install" was `cargo run -p mini-desktop`, which is a developer
//! workflow, not something a person can be asked to do.
//!
//! This crate is the Windows half of that layer. It is deliberately the
//! *engine* only: `mini-setup` is the wizard a user double-clicks,
//! `mini windows-setup` is the scriptable command line over it, and
//! `packaging/windows/` builds the artifacts. All the decisions live here so
//! all three agree, and so they can be tested without Windows.
//!
//! ## The five properties this engine is built around
//!
//! 1. **Nothing installs unverified.** Every file carries a length and two
//!    digests (BLAKE3 and SHA-256) in a canonical manifest. Bytes are
//!    checked on the way out of the container, and re-read and checked again
//!    from disk after writing, before anything is activated.
//! 2. **Nothing installs unapproved.** [`Setup::install`] requires an
//!    [`InstallApproval`] naming the exact manifest digest --- the
//!    typed-domain rule (`CLAUDE.md`): an authority-exercising call takes a
//!    specific named request, never a generic "go ahead". An approval for
//!    one build cannot install a different one.
//! 3. **No silent downgrade.** Activation runs
//!    `mini_forge::check_no_rollback`, the same check the update
//!    path uses, so moving to an older version requires an explicit
//!    rollback. Reinstalling a patched-away vulnerability should take a
//!    decision, not a mis-click.
//! 4. **No admin, no service, no network.** Per-user directories, `HKCU`
//!    only, and this crate has no networking dependency of any kind. It
//!    installs bytes it is handed; fetching them is somebody else's job, and
//!    keeping it that way is what makes an offline install from a USB stick
//!    the same code path as any other.
//! 5. **Uninstall never destroys an identity by accident.** Program files
//!    and user data are separate directories. The default
//!    [`UninstallApproval`] removes the former and reports the latter
//!    untouched; destroying identities needs a second constructor that names
//!    the exact path being destroyed.
//!
//! ## Honest limits
//!
//! * A per-user install directory is writable by anything else running as
//!   that user. This is not tamper-proof storage, and no arrangement of
//!   files could make it so; see [`layout`].
//! * Nothing here is code-signed. An Authenticode signature needs a
//!   certificate and a governed signing process that do not exist yet, so
//!   SmartScreen will warn on first run, and it should. The manifest's two
//!   digests are what a careful user can check instead --- with
//!   `Get-FileHash`, needing no Mininet binary --- and
//!   `mini-forge`'s release attestations are what makes a digest
//!   *authentic* rather than merely self-consistent.
//! * The manifest's `end` digest detects corruption and careless edits, not
//!   a determined forger: anyone who can rewrite the file can recompute it.
//! * No automatic update. Nothing in this crate polls, fetches, or
//!   self-invokes; installing is always something a person started
//!   (`docs/INVARIANTS.md` U1).

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]
#![warn(missing_docs)]

pub mod container;
pub mod error;
pub mod layout;
pub mod log;
pub mod manifest;
pub mod path;
pub mod report;
pub mod shell;

pub use container::Container;
pub use error::SetupError;
pub use layout::{InstallLayout, InstallRecord};
pub use log::{SetupEvent, SetupLog};
pub use manifest::{ManifestHeader, PackageFile, PackageManifest, PackageShortcut};
pub use report::Field;
pub use shell::{
    NoShell, RecordingShell, ShellAction, ShellIntegration, ShortcutRequest, UninstallRegistration,
    WindowsShell,
};

use mini_forge::check_no_rollback;
use std::path::{Path, PathBuf};

/// Registry key leaf and shortcut base name for the client.
pub const PRODUCT_KEY: &str = "Mininet";

/// Publisher string shown in Apps & features.
pub const PUBLISHER: &str = "The Mininet contributors";

/// The device owner's approval of one exact package.
///
/// Holds a manifest digest, not a version or a file path: two builds of
/// "0.1.0" are different packages, and only the digest distinguishes them.
/// This crate never constructs one for itself --- a caller must, from an
/// actual user decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallApproval {
    package_digest: String,
    approved_at_ms: u64,
}

impl InstallApproval {
    /// Approve the package described by `manifest`.
    pub fn new(manifest: &PackageManifest, approved_at_ms: u64) -> Self {
        Self {
            package_digest: manifest.digest_hex(),
            approved_at_ms,
        }
    }

    /// The approved package's manifest digest, hex.
    pub fn package_digest(&self) -> &str {
        &self.package_digest
    }

    /// When the owner approved it.
    pub fn approved_at_ms(&self) -> u64 {
        self.approved_at_ms
    }
}

/// The device owner's approval of one exact uninstall.
///
/// Names the install root so an approval cannot be replayed against a
/// different install, and carries the user-data path only when the owner
/// explicitly chose to destroy it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallApproval {
    install_root: PathBuf,
    destroy_user_data: Option<PathBuf>,
    approved_at_ms: u64,
}

impl UninstallApproval {
    /// Remove program files, keep identities, objects, and settings.
    ///
    /// The default, and what the wizard offers first: removing a client
    /// should not be a way to lose an identity that cannot be recreated.
    pub fn keeping_identities(install_root: impl Into<PathBuf>, approved_at_ms: u64) -> Self {
        Self {
            install_root: install_root.into(),
            destroy_user_data: None,
            approved_at_ms,
        }
    }

    /// Remove program files *and* irreversibly destroy the identity vault,
    /// object store, and settings at exactly `user_data_root`.
    ///
    /// Separate constructor, exact path required, and named for what it
    /// actually does. There is no recovery from this: the DPAPI-protected
    /// seed envelopes are the only copy of the device's signing keys, so
    /// every object signed by them becomes unreproducible.
    pub fn destroying_identities(
        install_root: impl Into<PathBuf>,
        user_data_root: impl Into<PathBuf>,
        approved_at_ms: u64,
    ) -> Self {
        Self {
            install_root: install_root.into(),
            destroy_user_data: Some(user_data_root.into()),
            approved_at_ms,
        }
    }

    /// The install root this approval authorizes.
    pub fn install_root(&self) -> &Path {
        &self.install_root
    }

    /// The user-data root to destroy, if the owner chose that.
    pub fn destroy_user_data(&self) -> Option<&Path> {
        self.destroy_user_data.as_deref()
    }

    /// When the owner approved it.
    pub fn approved_at_ms(&self) -> u64 {
        self.approved_at_ms
    }
}

/// What an install would do relative to what is already there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanKind {
    /// Nothing is installed yet.
    FirstInstall,
    /// A newer version is replacing an older active one.
    Upgrade,
    /// The same version is being written again.
    Reinstall,
    /// An older version would replace a newer active one. Requires
    /// [`InstallOptions::allow_downgrade`].
    Downgrade,
}

impl PlanKind {
    /// A short, stable machine name for `--json` output.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FirstInstall => "first_install",
            Self::Upgrade => "upgrade",
            Self::Reinstall => "reinstall",
            Self::Downgrade => "downgrade",
        }
    }
}

/// One file an install would write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    /// Relative package path.
    pub path: String,
    /// Absolute destination.
    pub destination: PathBuf,
    /// Length in bytes.
    pub length: u64,
}

/// Choices the caller makes about one install.
#[derive(Debug, Clone)]
pub struct InstallOptions {
    /// Add a Start Menu entry.
    pub start_menu_shortcut: bool,
    /// Add a Desktop shortcut. Off by default: an installer that litters
    /// the desktop without asking is the behaviour this project exists to
    /// not have.
    pub desktop_shortcut: bool,
    /// Register in Apps & features so the client can be removed the way
    /// every other Windows program is.
    pub register_uninstall: bool,
    /// Permit activating an older version than the active one.
    pub allow_downgrade: bool,
    /// Override the Start Menu directory (tests, portable installs).
    pub start_menu_dir: Option<PathBuf>,
    /// Override the Desktop directory.
    pub desktop_dir: Option<PathBuf>,
    /// Path recorded as the Apps & features "Uninstall" command. Defaults
    /// to `<install root>\mininet-setup.exe` when the package ships it.
    pub setup_exe: Option<PathBuf>,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            start_menu_shortcut: true,
            desktop_shortcut: false,
            register_uninstall: true,
            allow_downgrade: false,
            start_menu_dir: None,
            desktop_dir: None,
            setup_exe: None,
        }
    }
}

/// Everything an install would do, computed before anything is written.
#[derive(Debug, Clone)]
pub struct InstallPlan {
    /// Package identifier.
    pub package: String,
    /// Version being installed, as the manifest spelled it.
    pub version_text: String,
    /// Product display name.
    pub product: String,
    /// Manifest digest, hex: the value an [`InstallApproval`] names.
    pub package_digest: String,
    /// Install root.
    pub install_root: PathBuf,
    /// Directory this version's files go in.
    pub version_dir: PathBuf,
    /// Absolute path of the client executable once installed.
    pub launch_path: PathBuf,
    /// Files to write.
    pub files: Vec<PlannedFile>,
    /// Total bytes to write.
    pub total_bytes: u64,
    /// Shell changes to make.
    pub shell_actions: Vec<ShellAction>,
    /// What is active now, if anything.
    pub active: Option<InstallRecord>,
    /// Relationship to what is active now.
    pub kind: PlanKind,
    /// User-data directory this install will not touch.
    pub user_data_root: PathBuf,
}

/// What an install actually did.
#[derive(Debug, Clone)]
pub struct InstallReport {
    /// The record now active.
    pub active: InstallRecord,
    /// The record a rollback would return to, if any.
    pub previous: Option<InstallRecord>,
    /// Files written.
    pub files_written: usize,
    /// Bytes written.
    pub bytes_written: u64,
    /// Absolute path of the installed client executable.
    pub launch_path: PathBuf,
    /// Shell changes applied.
    pub shell_actions: Vec<ShellAction>,
}

/// A problem found while verifying an installed version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyProblem {
    /// A manifest file is not on disk.
    Missing {
        /// Relative package path.
        path: String,
    },
    /// A file is the wrong length.
    Length {
        /// Relative package path.
        path: String,
        /// Manifest length.
        expected: u64,
        /// Length on disk.
        found: u64,
    },
    /// A file's digests do not match the manifest.
    Digest {
        /// Relative package path.
        path: String,
    },
    /// A file is present that the manifest does not list.
    ///
    /// Reported, not deleted. An unexpected DLL beside an executable is how
    /// search-order hijacking works, so it is worth showing a user; guessing
    /// which stray file is safe to remove is not this crate's call.
    Unexpected {
        /// Relative path inside the version directory.
        path: String,
    },
}

/// The result of verifying an installed version against its manifest.
#[derive(Debug, Clone)]
pub struct VerifyReport {
    /// Which version was checked.
    pub version_text: String,
    /// How many manifest files were read and hashed.
    pub files_checked: usize,
    /// Total bytes read.
    pub bytes_checked: u64,
    /// Everything wrong, rather than only the first thing wrong.
    pub problems: Vec<VerifyProblem>,
}

impl VerifyReport {
    /// True when the installation matches its manifest exactly.
    pub fn is_intact(&self) -> bool {
        self.problems.is_empty()
    }
}

/// What is installed right now.
#[derive(Debug, Clone)]
pub struct SetupStatus {
    /// Install root inspected.
    pub install_root: PathBuf,
    /// Active version, if any.
    pub active: Option<InstallRecord>,
    /// Rollback target, if any.
    pub previous: Option<InstallRecord>,
    /// Every version directory present, oldest first.
    pub installed_versions: Vec<String>,
    /// Absolute path of the active client executable, if resolvable.
    pub launch_path: Option<PathBuf>,
    /// User-data directory, reported so a user can see where identities
    /// live without guessing.
    pub user_data_root: PathBuf,
    /// True when the user-data directory exists.
    pub user_data_present: bool,
}

/// What an uninstall did.
#[derive(Debug, Clone)]
pub struct UninstallReport {
    /// Install root removed.
    pub install_root: PathBuf,
    /// Versions that were present and are now gone.
    pub versions_removed: Vec<String>,
    /// Shell changes applied.
    pub shell_actions: Vec<ShellAction>,
    /// User-data directory left in place, when identities were kept.
    pub user_data_kept: Option<PathBuf>,
    /// User-data directory destroyed, when the owner explicitly asked.
    pub user_data_destroyed: Option<PathBuf>,
}

/// The install engine over one install root.
#[derive(Debug, Clone)]
pub struct Setup {
    layout: InstallLayout,
    user_data_root: PathBuf,
}

impl Setup {
    /// Operate on `root`, with the default user-data location.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            layout: InstallLayout::new(root),
            user_data_root: InstallLayout::default_user_data_root(),
        }
    }

    /// Operate on the default per-user root for this environment.
    pub fn for_current_user() -> Self {
        Self::new(InstallLayout::default_root())
    }

    /// Override the user-data location (tests, `MININET_HOME` profiles).
    pub fn with_user_data_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.user_data_root = root.into();
        self
    }

    /// The layout being operated on.
    pub fn layout(&self) -> &InstallLayout {
        &self.layout
    }

    /// The user-data directory this engine will never write to.
    pub fn user_data_root(&self) -> &Path {
        &self.user_data_root
    }

    /// Compute what installing `manifest` would do. Writes nothing.
    ///
    /// This is what `--dry-run` prints and what the wizard's confirmation
    /// page shows. A user should be able to read the complete list of
    /// changes --- every file, every shortcut, the registry key --- before
    /// agreeing to any of it.
    pub fn plan(
        &self,
        manifest: &PackageManifest,
        options: &InstallOptions,
    ) -> Result<InstallPlan, SetupError> {
        let active = self.layout.current()?;
        let kind = match &active {
            None => PlanKind::FirstInstall,
            Some(record) if record.version == manifest.version => PlanKind::Reinstall,
            Some(record) if manifest.version > record.version => PlanKind::Upgrade,
            Some(_) => PlanKind::Downgrade,
        };
        let version_dir = self.layout.version_dir(&manifest.version_text);
        let mut files = Vec::with_capacity(manifest.files.len());
        for file in &manifest.files {
            files.push(PlannedFile {
                path: file.path.clone(),
                destination: path::join(&version_dir, &file.path)?,
                length: file.length,
            });
        }
        let launch_path = path::join(&version_dir, &manifest.launch)?;
        let shell_actions = self.shell_plan(manifest, options, &launch_path, &version_dir)?;
        Ok(InstallPlan {
            package: manifest.package.clone(),
            version_text: manifest.version_text.clone(),
            product: manifest.product.clone(),
            package_digest: manifest.digest_hex(),
            install_root: self.layout.root().to_path_buf(),
            version_dir,
            launch_path,
            files,
            total_bytes: manifest.total_bytes(),
            shell_actions,
            active,
            kind,
            user_data_root: self.user_data_root.clone(),
        })
    }

    /// Install the package in `container`, then activate it.
    ///
    /// Order matters and is the reason a failed install cannot leave a
    /// broken client behind:
    ///
    /// 1. the approval is checked against the container's manifest digest;
    /// 2. a downgrade is refused unless the caller declared one;
    /// 3. files are written to a `.staging` directory --- never over the
    ///    running version, whose executable Windows has locked anyway;
    /// 4. every staged file is **re-read from disk** and re-hashed, so a
    ///    truncated write or a failing drive is caught before activation;
    /// 5. only then does the staging directory become the version directory
    ///    and the pointer file move.
    ///
    /// Up to step 5 the previously active version is untouched and still
    /// what the pointer names.
    pub fn install(
        &self,
        container: &Container<'_>,
        approval: &InstallApproval,
        options: &InstallOptions,
        shell: &mut dyn ShellIntegration,
        now_ms: u64,
    ) -> Result<InstallReport, SetupError> {
        let manifest = container.manifest();
        let offered = manifest.digest_hex();
        if approval.package_digest() != offered {
            return Err(SetupError::ApprovalMismatch {
                approved: approval.package_digest().to_string(),
                offered,
            });
        }
        let plan = self.plan(manifest, options)?;
        if plan.kind == PlanKind::Downgrade && !options.allow_downgrade {
            let active = plan
                .active
                .as_ref()
                .expect("a downgrade plan always has an active record");
            check_no_rollback(Some(&active.version), &manifest.version).map_err(|_| {
                SetupError::WouldDowngrade {
                    active: active.version_text.clone(),
                    candidate: manifest.version_text.clone(),
                }
            })?;
        }

        let log = SetupLog::new(self.layout.log_path());
        log.append(SetupEvent::InstallStarted {
            version: manifest.version_text.clone(),
            package_digest: offered.clone(),
            at_ms: now_ms,
        })?;

        let staging = self
            .layout
            .version_dir(&format!("{}.staging", manifest.version_text));
        if staging.exists() {
            std::fs::remove_dir_all(&staging).map_err(|error| SetupError::io(&staging, error))?;
        }
        std::fs::create_dir_all(&staging).map_err(|error| SetupError::io(&staging, error))?;

        let mut bytes_written = 0u64;
        for file in &manifest.files {
            let bytes = container.file(&file.path)?;
            let destination = path::join(&staging, &file.path)?;
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent).map_err(|error| SetupError::io(parent, error))?;
            }
            std::fs::write(&destination, bytes)
                .map_err(|error| SetupError::io(&destination, error))?;
            bytes_written += file.length;
        }
        // Step 4: read back what the filesystem actually stored. A write that
        // returned Ok and a file that contains the right bytes are different
        // claims, and only the second one is worth activating.
        for file in &manifest.files {
            let destination = path::join(&staging, &file.path)?;
            let stored = std::fs::read(&destination)
                .map_err(|error| SetupError::io(&destination, error))?;
            file.verify(&stored)?;
        }
        set_executable_bits(&staging, manifest)?;

        let final_dir = self.layout.version_dir(&manifest.version_text);
        if final_dir.exists() {
            std::fs::remove_dir_all(&final_dir)
                .map_err(|error| SetupError::io(&final_dir, error))?;
        }
        if let Some(parent) = final_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|error| SetupError::io(parent, error))?;
        }
        std::fs::rename(&staging, &final_dir)
            .map_err(|error| SetupError::io(&final_dir, error))?;

        let manifest_path = self.layout.manifest_path(&manifest.version_text);
        layout::write_atomic(&manifest_path, &manifest.to_bytes())?;

        let record = InstallRecord::for_manifest(manifest, now_ms);
        let previous = match plan.active.clone() {
            Some(active) if active.version != manifest.version => {
                self.layout.set_previous(&active)?;
                Some(active)
            }
            // Reinstalling the active version leaves the existing rollback
            // target alone: the version it names is still on disk and still
            // the right thing to fall back to.
            Some(_) => self.layout.previous()?,
            None => None,
        };
        self.layout.set_current(&record)?;

        shell.apply(&plan.shell_actions)?;
        log.append(SetupEvent::Activated {
            version: manifest.version_text.clone(),
            package_digest: offered,
            at_ms: now_ms,
        })?;

        Ok(InstallReport {
            active: record,
            previous,
            files_written: manifest.files.len(),
            bytes_written,
            launch_path: plan.launch_path,
            shell_actions: plan.shell_actions,
        })
    }

    /// Re-hash an installed version against its stored manifest.
    ///
    /// Collects every problem rather than stopping at the first: a user
    /// deciding whether they have a corrupted copy or a tampered one needs
    /// the whole picture, and "one file is wrong" versus "nine files are
    /// wrong" are different situations.
    pub fn verify_installed(&self, version_text: &str) -> Result<VerifyReport, SetupError> {
        let manifest = self.layout.stored_manifest(version_text)?;
        let version_dir = self.layout.version_dir(version_text);
        let mut problems = Vec::new();
        let mut files_checked = 0usize;
        let mut bytes_checked = 0u64;
        for file in &manifest.files {
            let destination = path::join(&version_dir, &file.path)?;
            match std::fs::read(&destination) {
                Ok(bytes) => {
                    files_checked += 1;
                    bytes_checked += bytes.len() as u64;
                    match file.verify(&bytes) {
                        Ok(()) => {}
                        Err(SetupError::LengthMismatch {
                            path,
                            expected,
                            found,
                        }) => problems.push(VerifyProblem::Length {
                            path,
                            expected,
                            found,
                        }),
                        Err(_) => problems.push(VerifyProblem::Digest {
                            path: file.path.clone(),
                        }),
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    problems.push(VerifyProblem::Missing {
                        path: file.path.clone(),
                    });
                }
                Err(error) => return Err(SetupError::io(&destination, error)),
            }
        }
        let expected: std::collections::BTreeSet<String> = manifest
            .files
            .iter()
            .map(|file| path::fold_case(&file.path))
            .collect();
        for found in walk_relative(&version_dir)? {
            if !expected.contains(&path::fold_case(&found)) {
                problems.push(VerifyProblem::Unexpected { path: found });
            }
        }
        Ok(VerifyReport {
            version_text: version_text.to_string(),
            files_checked,
            bytes_checked,
            problems,
        })
    }

    /// Read what is installed. Never fails on a missing install root; an
    /// empty root is a valid answer ("nothing installed"), not an error.
    pub fn status(&self) -> Result<SetupStatus, SetupError> {
        let active = self.layout.current()?;
        let launch_path = match &active {
            Some(record) => match self.layout.stored_manifest(&record.version_text) {
                Ok(manifest) => Some(path::join(
                    &self.layout.version_dir(&record.version_text),
                    &manifest.launch,
                )?),
                Err(_) => None,
            },
            None => None,
        };
        Ok(SetupStatus {
            install_root: self.layout.root().to_path_buf(),
            active,
            previous: self.layout.previous()?,
            installed_versions: self.layout.installed_versions()?,
            launch_path,
            user_data_root: self.user_data_root.clone(),
            user_data_present: self.user_data_root.exists(),
        })
    }

    /// Return to the previously active version.
    ///
    /// The rollback target is verified against its stored manifest *before*
    /// the pointer moves: rolling back onto a corrupted older install would
    /// turn one broken client into two. Shortcuts are rewritten to the older
    /// executable, since they point at a version directory.
    pub fn rollback(
        &self,
        options: &InstallOptions,
        shell: &mut dyn ShellIntegration,
        now_ms: u64,
    ) -> Result<InstallRecord, SetupError> {
        let previous = self.layout.previous()?.ok_or(SetupError::NoPreviousVersion)?;
        let version_dir = self.layout.version_dir(&previous.version_text);
        if !version_dir.is_dir() {
            return Err(SetupError::MissingVersionDirectory {
                version: previous.version_text.clone(),
            });
        }
        let report = self.verify_installed(&previous.version_text)?;
        if !report.is_intact() {
            return Err(SetupError::DigestMismatch {
                path: match report.problems.first() {
                    Some(VerifyProblem::Missing { path })
                    | Some(VerifyProblem::Digest { path })
                    | Some(VerifyProblem::Length { path, .. })
                    | Some(VerifyProblem::Unexpected { path }) => path.clone(),
                    None => previous.version_text.clone(),
                },
            });
        }
        let manifest = self.layout.stored_manifest(&previous.version_text)?;
        let launch_path = path::join(&version_dir, &manifest.launch)?;
        let actions = self.shell_plan(&manifest, options, &launch_path, &version_dir)?;
        self.layout.set_current(&previous)?;
        self.layout.clear_previous()?;
        shell.apply(&actions)?;
        SetupLog::new(self.layout.log_path()).append(SetupEvent::RolledBack {
            version: previous.version_text.clone(),
            at_ms: now_ms,
        })?;
        Ok(previous)
    }

    /// Remove the install.
    ///
    /// The approval must name this exact install root, so an approval
    /// collected for one install cannot delete another. Shell changes are
    /// reverted first: if file removal then fails partway, the user is not
    /// left with a Start Menu entry that launches a deleted executable.
    pub fn uninstall(
        &self,
        approval: &UninstallApproval,
        options: &InstallOptions,
        shell: &mut dyn ShellIntegration,
        now_ms: u64,
    ) -> Result<UninstallReport, SetupError> {
        if approval.install_root() != self.layout.root() {
            return Err(SetupError::ApprovalMismatch {
                approved: approval.install_root().display().to_string(),
                offered: self.layout.root().display().to_string(),
            });
        }
        let versions_removed = self.layout.installed_versions()?;
        let mut actions = Vec::new();
        if options.start_menu_shortcut {
            actions.push(ShellAction::RemoveShortcut {
                path: self.start_menu_link(options),
            });
        }
        if options.desktop_shortcut {
            actions.push(ShellAction::RemoveShortcut {
                path: self.desktop_link(options),
            });
        }
        if options.register_uninstall {
            actions.push(ShellAction::DeregisterUninstall {
                key_name: PRODUCT_KEY.to_string(),
            });
        }
        shell.apply(&actions)?;

        let log_bytes = std::fs::read(self.layout.log_path()).ok();
        let root = self.layout.root();
        if root.exists() {
            std::fs::remove_dir_all(root).map_err(|error| SetupError::io(root, error))?;
        }

        let mut user_data_kept = None;
        let mut user_data_destroyed = None;
        match approval.destroy_user_data() {
            Some(user_root) => {
                if user_root.exists() {
                    std::fs::remove_dir_all(user_root)
                        .map_err(|error| SetupError::io(user_root, error))?;
                }
                user_data_destroyed = Some(user_root.to_path_buf());
            }
            None => {
                if self.user_data_root.exists() {
                    user_data_kept = Some(self.user_data_root.clone());
                }
            }
        }

        // The log outlives the install it describes: "what happened to the
        // copy that used to be here?" is exactly the question asked after an
        // uninstall, and deleting the only record of it answers nothing.
        if let Some(bytes) = log_bytes {
            let preserved = root.with_extension("uninstalled.log.txt");
            layout::write_atomic(&preserved, &bytes)?;
            SetupLog::new(preserved).append(SetupEvent::Uninstalled {
                at_ms: now_ms,
                destroyed_user_data: user_data_destroyed.is_some(),
            })?;
        }

        Ok(UninstallReport {
            install_root: root.to_path_buf(),
            versions_removed,
            shell_actions: actions,
            user_data_kept,
            user_data_destroyed,
        })
    }

    fn start_menu_link(&self, options: &InstallOptions) -> PathBuf {
        options
            .start_menu_dir
            .clone()
            .unwrap_or_else(shell::start_menu_dir)
            .join(format!("{PRODUCT_KEY}.lnk"))
    }

    fn desktop_link(&self, options: &InstallOptions) -> PathBuf {
        options
            .desktop_dir
            .clone()
            .unwrap_or_else(shell::desktop_dir)
            .join(format!("{PRODUCT_KEY}.lnk"))
    }

    fn shell_plan(
        &self,
        manifest: &PackageManifest,
        options: &InstallOptions,
        launch_path: &Path,
        version_dir: &Path,
    ) -> Result<Vec<ShellAction>, SetupError> {
        let mut actions = Vec::new();
        let description = format!("{} client", manifest.product);
        manifest::check_display("shortcut description", &description)?;
        if options.start_menu_shortcut {
            actions.push(ShellAction::CreateShortcut(ShortcutRequest {
                link_path: self.start_menu_link(options),
                target: launch_path.to_path_buf(),
                working_dir: version_dir.to_path_buf(),
                description: description.clone(),
            }));
        }
        if options.desktop_shortcut {
            actions.push(ShellAction::CreateShortcut(ShortcutRequest {
                link_path: self.desktop_link(options),
                target: launch_path.to_path_buf(),
                working_dir: version_dir.to_path_buf(),
                description,
            }));
        }
        if options.register_uninstall {
            // Prefer a setup executable the package itself ships, so
            // "Uninstall" in Apps & features runs the same code that
            // installed, at the same version.
            let setup_exe = options.setup_exe.clone().unwrap_or_else(|| {
                manifest
                    .files
                    .iter()
                    .find(|file| {
                        path::fold_case(&file.path).ends_with("mininet-setup.exe")
                    })
                    .and_then(|file| path::join(version_dir, &file.path).ok())
                    .unwrap_or_else(|| version_dir.join("mininet-setup.exe"))
            });
            actions.push(ShellAction::RegisterUninstall(UninstallRegistration {
                key_name: PRODUCT_KEY.to_string(),
                display_name: manifest.product.clone(),
                display_version: manifest.version_text.clone(),
                publisher: PUBLISHER.to_string(),
                install_location: self.layout.root().to_path_buf(),
                uninstall_command: format!("\"{}\" --uninstall", setup_exe.display()),
                estimated_size_kb: manifest.total_bytes().div_ceil(1024).min(u32::MAX as u64)
                    as u32,
            }));
        }
        Ok(actions)
    }
}

/// Mark installed executables executable on Unix-like hosts.
///
/// A no-op on Windows, where the extension decides. This exists so the
/// engine can be exercised end to end on Linux --- including actually
/// launching what it installed in a test --- rather than only up to the
/// point where the bits matter.
#[cfg(unix)]
fn set_executable_bits(dir: &Path, manifest: &PackageManifest) -> Result<(), SetupError> {
    use std::os::unix::fs::PermissionsExt;
    for file in &manifest.files {
        if !path::fold_case(&file.path).ends_with(".exe") {
            continue;
        }
        let destination = path::join(dir, &file.path)?;
        let mut permissions = std::fs::metadata(&destination)
            .map_err(|error| SetupError::io(&destination, error))?
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&destination, permissions)
            .map_err(|error| SetupError::io(&destination, error))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_executable_bits(_dir: &Path, _manifest: &PackageManifest) -> Result<(), SetupError> {
    Ok(())
}

/// Every file under `dir`, as `/`-separated paths relative to it.
fn walk_relative(dir: &Path) -> Result<Vec<String>, SetupError> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let mut stack = vec![(dir.to_path_buf(), String::new())];
    while let Some((current, prefix)) = stack.pop() {
        for entry in std::fs::read_dir(&current).map_err(|error| SetupError::io(&current, error))? {
            let entry = entry.map_err(|error| SetupError::io(&current, error))?;
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            let relative = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if entry.path().is_dir() {
                stack.push((entry.path(), relative));
            } else {
                out.push(relative);
            }
        }
    }
    out.sort();
    Ok(out)
}
