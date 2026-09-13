//! Start Menu entries and the Apps & features registration --- the two
//! things that make an install feel like a Windows install rather than a
//! folder of files.
//!
//! ## Why this is a trait, and why the Windows side generates a script
//!
//! Everything else in this crate is pure file movement that a Linux CI
//! machine can execute and assert on. Shortcuts and the uninstall registry
//! key are not: a `.lnk` is a COM-serialized shell object and the
//! registration is `HKCU`. Two consequences shape this module.
//!
//! First, shell integration sits behind [`ShellIntegration`] so the install
//! engine never calls Windows directly. [`RecordingShell`] captures the
//! actions instead of performing them, which is what the test suite and
//! `--dry-run` use: the *decision* about what to create is verified on every
//! platform, and only the execution is Windows-only.
//!
//! Second, [`WindowsShell`] performs those actions by generating a
//! PowerShell script and running it once, rather than by driving COM through
//! `windows-sys` by hand. That is a deliberate trade. Hand-written COM here
//! would mean a few hundred lines of `unsafe` vtable calls that no
//! non-Windows machine can even type-check, in the one component a user runs
//! before they have any reason to trust us. `WScript.Shell.CreateShortcut`
//! and `New-ItemProperty` are the boring, universally available way to do
//! exactly this, present on every supported Windows install, needing no
//! administrator. The whole crate stays `forbid(unsafe_code)` as a result.
//!
//! The risk a generated script introduces is injection, so the generator is
//! a pure function ([`powershell_script`]) with its own tests, every
//! interpolated value goes through [`ps_literal`], and display text was
//! already stripped of script metacharacters at manifest-parse time
//! ([`crate::manifest`]). Two independent layers, because either one alone
//! is a single point of failure.

use crate::error::SetupError;
use std::path::{Path, PathBuf};

/// One shell-level change setup wants made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellAction {
    /// Create or replace a `.lnk` shortcut.
    CreateShortcut(ShortcutRequest),
    /// Delete a shortcut, ignoring one that is already gone.
    RemoveShortcut {
        /// Which known folder, or an exact directory.
        location: ShortcutLocation,
        /// File name of the `.lnk`.
        link_name: String,
    },
    /// Create or replace the Apps & features entry under `HKCU`.
    RegisterUninstall(UninstallRegistration),
    /// Delete the Apps & features entry.
    DeregisterUninstall {
        /// Registry key leaf name.
        key_name: String,
    },
}

/// Where a shortcut goes.
///
/// The Start Menu and Desktop are *known folders*, and on a machine with
/// OneDrive backup or enterprise folder redirection the Desktop is not
/// `%USERPROFILE%\\Desktop` at all. Building that path in Rust would create a
/// stray directory nobody sees. These variants are resolved on the machine by
/// `[Environment]::GetFolderPath`, which is what Windows itself uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutLocation {
    /// The user's Start Menu "Programs" folder.
    StartMenu,
    /// The user's Desktop, wherever it has been redirected to.
    Desktop,
    /// A caller-chosen directory. Used by tests and portable installs, which
    /// must not touch a real known folder.
    Exact(PathBuf),
}

impl ShortcutLocation {
    /// The PowerShell expression that yields this directory.
    fn expression(&self) -> Result<String, SetupError> {
        Ok(match self {
            Self::StartMenu => "[Environment]::GetFolderPath('Programs')".to_string(),
            Self::Desktop => "[Environment]::GetFolderPath('DesktopDirectory')".to_string(),
            Self::Exact(path) => ps_path("shortcut directory", path)?,
        })
    }

    /// A best-effort path for reporting and planning, before any script runs.
    pub fn planned_dir(&self) -> PathBuf {
        match self {
            Self::StartMenu => start_menu_dir(),
            Self::Desktop => desktop_dir(),
            Self::Exact(path) => path.clone(),
        }
    }
}

/// A shortcut to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutRequest {
    /// Which known folder, or an exact directory.
    pub location: ShortcutLocation,
    /// File name of the `.lnk`, including the extension.
    pub link_name: String,
    /// Full path of the executable it launches.
    pub target: PathBuf,
    /// Working directory the target starts in.
    pub working_dir: PathBuf,
    /// Tooltip text; already checked for script metacharacters.
    pub description: String,
}

/// The Apps & features entry.
///
/// Registered under `HKCU`, never `HKLM`: this is a per-user install, so it
/// appears in that user's Apps & features list and nowhere else, and writing
/// it needs no administrator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallRegistration {
    /// Registry key leaf name under
    /// `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall`.
    pub key_name: String,
    /// Name shown in Apps & features.
    pub display_name: String,
    /// Version shown beside it.
    pub display_version: String,
    /// Publisher column.
    pub publisher: String,
    /// The install root.
    pub install_location: PathBuf,
    /// Executable Windows runs for "Uninstall", and its arguments.
    pub uninstall_command: String,
    /// Rounded installed size, in kilobytes, for the size column.
    pub estimated_size_kb: u32,
}

/// Performs (or records) shell-level changes.
pub trait ShellIntegration: core::fmt::Debug {
    /// Apply every action, in order, stopping at the first failure.
    fn apply(&mut self, actions: &[ShellAction]) -> Result<(), SetupError>;
}

/// A [`ShellIntegration`] that records actions instead of performing them.
///
/// Used by the test suite on every platform and by `--dry-run`, so "what
/// would this installer touch outside its own directory?" is answerable
/// without touching anything.
#[derive(Debug, Default)]
pub struct RecordingShell {
    /// Every action handed to [`ShellIntegration::apply`], in order.
    pub actions: Vec<ShellAction>,
}

impl ShellIntegration for RecordingShell {
    fn apply(&mut self, actions: &[ShellAction]) -> Result<(), SetupError> {
        self.actions.extend_from_slice(actions);
        Ok(())
    }
}

/// A [`ShellIntegration`] that deliberately does nothing.
///
/// For a caller who wants the files placed and nothing registered --- a
/// portable install on a USB stick, or a build that will be packaged by
/// other tooling. Distinct from [`RecordingShell`] so "skip integration" is
/// an explicit choice in the code rather than an unused recording.
#[derive(Debug, Default)]
pub struct NoShell;

impl ShellIntegration for NoShell {
    fn apply(&mut self, _actions: &[ShellAction]) -> Result<(), SetupError> {
        Ok(())
    }
}

/// The real Windows implementation.
#[derive(Debug, Default)]
pub struct WindowsShell {
    /// Directory for the temporary generated script; defaults to the
    /// system temp directory.
    pub script_dir: Option<PathBuf>,
}

impl ShellIntegration for WindowsShell {
    #[cfg(windows)]
    fn apply(&mut self, actions: &[ShellAction]) -> Result<(), SetupError> {
        if actions.is_empty() {
            return Ok(());
        }
        let script = powershell_script(actions)?;
        let dir = self.script_dir.clone().unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&dir).map_err(|error| SetupError::io(&dir, error))?;
        let path = dir.join(format!("mininet-setup-{}.ps1", std::process::id()));
        std::fs::write(&path, script.as_bytes()).map_err(|error| SetupError::io(&path, error))?;
        let status = std::process::Command::new(windows_powershell()?)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&path)
            .status();
        let _ = std::fs::remove_file(&path);
        let status = status.map_err(|error| SetupError::ShellIntegrationFailed {
            step: "powershell".to_string(),
            detail: error.to_string(),
        })?;
        if !status.success() {
            return Err(SetupError::ShellIntegrationFailed {
                step: "powershell".to_string(),
                detail: format!("exit status {status}"),
            });
        }
        Ok(())
    }

    #[cfg(not(windows))]
    fn apply(&mut self, _actions: &[ShellAction]) -> Result<(), SetupError> {
        Err(SetupError::UnsupportedPlatform)
    }
}

/// The absolute path of Windows PowerShell.
///
/// Resolved from `%SystemRoot%` rather than spawned by bare name: a bare name
/// is resolved by the OS, and on Windows that search has historically
/// included the current directory, so an installer run from a directory an
/// attacker can write to would run their `powershell.exe` as the user.
#[cfg(windows)]
fn windows_powershell() -> Result<PathBuf, SetupError> {
    let root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"));
    let path = root
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    if !path.is_file() {
        return Err(SetupError::ShellIntegrationFailed {
            step: "powershell".to_string(),
            detail: format!(
                "{} is not present, so Start Menu and Apps & features integration cannot run",
                path.display()
            ),
        });
    }
    Ok(path)
}

/// The user's Start Menu "Programs" directory.
pub fn start_menu_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs");
    }
    fallback_dir("start-menu")
}

/// The user's Desktop directory.
pub fn desktop_dir() -> PathBuf {
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        return PathBuf::from(profile).join("Desktop");
    }
    fallback_dir("desktop")
}

/// Non-Windows stand-in so planning and tests have somewhere to point.
fn fallback_dir(leaf: &str) -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mininet-shell")
        .join(leaf)
}

/// Quote a value as a PowerShell single-quoted literal.
///
/// Single-quoted PowerShell strings expand nothing, so the only escape
/// needed is doubling an embedded `'`. Both are refused rather than escaped
/// where escaping would be ambiguous: a newline or NUL in a path is never
/// legitimate here, and silently dropping it would produce a script that
/// does something other than what the caller asked.
///
/// This must handle a genuinely awkward real case: a Windows user named
/// `O'Brien` has an apostrophe in every path in their profile.
pub fn ps_literal(field: &'static str, value: &str) -> Result<String, SetupError> {
    for ch in value.chars() {
        if ch == '\0' || ch == '\n' || ch == '\r' {
            return Err(SetupError::UnsafeDisplayText {
                field,
                reason: "control character in a path or value",
            });
        }
    }
    Ok(format!("'{}'", value.replace('\'', "''")))
}

fn ps_path(field: &'static str, path: &Path) -> Result<String, SetupError> {
    let text = path.to_str().ok_or(SetupError::UnsafeDisplayText {
        field,
        reason: "path is not valid UTF-16-compatible Unicode",
    })?;
    ps_literal(field, text)
}

/// Generate the PowerShell script that performs `actions`.
///
/// A pure function so the exact script text is unit-testable on any
/// platform. `$ErrorActionPreference = 'Stop'` plus the trailing `exit 0`
/// mean a partial failure exits non-zero rather than reporting success, and
/// `-ErrorAction SilentlyContinue` appears only on removals, where an
/// already-absent item is the desired end state, not a failure.
pub fn powershell_script(actions: &[ShellAction]) -> Result<String, SetupError> {
    let mut out = String::new();
    out.push_str("# Generated by mini-windows-setup. Safe to inspect before it runs.\n");
    out.push_str("$ErrorActionPreference = 'Stop'\n");
    out.push_str("$ProgressPreference = 'SilentlyContinue'\n");
    let uninstall_root = "HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
    for action in actions {
        match action {
            ShellAction::CreateShortcut(request) => {
                let directory = request.location.expression()?;
                let name = ps_literal("shortcut name", &request.link_name)?;
                let target = ps_path("shortcut target", &request.target)?;
                let working = ps_path("shortcut working directory", &request.working_dir)?;
                let description = ps_literal("shortcut description", &request.description)?;
                // The directory is resolved on the machine, then joined there
                // too: Rust's own path joining uses the *host's* separator
                // rules, so on a Linux CI machine it would not build a Windows
                // path correctly at all.
                out.push_str(&format!("$dir = {directory}\n"));
                out.push_str(&format!("$link = Join-Path $dir {name}\n"));
                out.push_str("New-Item -ItemType Directory -Force -Path $dir | Out-Null\n");
                out.push_str("$shell = New-Object -ComObject WScript.Shell\n");
                out.push_str("$link = $shell.CreateShortcut($link)\n");
                out.push_str(&format!("$link.TargetPath = {target}\n"));
                out.push_str(&format!("$link.WorkingDirectory = {working}\n"));
                out.push_str(&format!("$link.Description = {description}\n"));
                out.push_str("$link.Save()\n");
            }
            ShellAction::RemoveShortcut {
                location,
                link_name,
            } => {
                // Assigned to a variable first, never inlined as a command
                // argument. PowerShell parses command arguments in *argument
                // mode*, where `[Environment]::GetFolderPath('Programs')` is
                // a literal string rather than a call --- so inlining it made
                // `Join-Path` fail, and with `$ErrorActionPreference = 'Stop'`
                // that failed the whole uninstall. Assignment is expression
                // mode, where the call is evaluated.
                out.push_str(&format!("$dir = {}\n", location.expression()?));
                out.push_str(&format!(
                    "Remove-Item -LiteralPath (Join-Path $dir {}) -Force -ErrorAction SilentlyContinue\n",
                    ps_literal("shortcut name", link_name)?
                ));
            }
            ShellAction::RegisterUninstall(registration) => {
                let key = ps_literal(
                    "uninstall key",
                    &format!("{uninstall_root}\\{}", registration.key_name),
                )?;
                out.push_str(&format!("New-Item -Path {key} -Force | Out-Null\n"));
                let mut property = |name: &str, value: String, kind: &str| {
                    out.push_str(&format!(
                        "New-ItemProperty -Path {key} -Name '{name}' -Value {value} -PropertyType {kind} -Force | Out-Null\n"
                    ));
                };
                property(
                    "DisplayName",
                    ps_literal("display name", &registration.display_name)?,
                    "String",
                );
                property(
                    "DisplayVersion",
                    ps_literal("display version", &registration.display_version)?,
                    "String",
                );
                property(
                    "Publisher",
                    ps_literal("publisher", &registration.publisher)?,
                    "String",
                );
                property(
                    "InstallLocation",
                    ps_path("install location", &registration.install_location)?,
                    "String",
                );
                property(
                    "UninstallString",
                    ps_literal("uninstall command", &registration.uninstall_command)?,
                    "String",
                );
                property(
                    "EstimatedSize",
                    registration.estimated_size_kb.to_string(),
                    "DWord",
                );
                property("NoModify", "1".to_string(), "DWord");
                property("NoRepair", "1".to_string(), "DWord");
            }
            ShellAction::DeregisterUninstall { key_name } => {
                let key = ps_literal("uninstall key", &format!("{uninstall_root}\\{key_name}"))?;
                out.push_str(&format!(
                    "Remove-Item -Path {key} -Recurse -Force -ErrorAction SilentlyContinue\n"
                ));
            }
        }
    }
    out.push_str("exit 0\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_apostrophe_in_a_username_is_doubled_not_dropped() {
        let quoted = ps_literal("path", "C:\\Users\\O'Brien\\Programs").unwrap();
        assert_eq!(quoted, "'C:\\Users\\O''Brien\\Programs'");
    }

    #[test]
    fn newlines_and_nuls_are_refused_rather_than_escaped() {
        assert!(ps_literal("path", "C:\\a\nrm -rf").is_err());
        assert!(ps_literal("path", "C:\\a\r").is_err());
        assert!(ps_literal("path", "C:\\a\0b").is_err());
    }

    #[test]
    fn a_generated_script_stops_on_error_and_reports_its_exit_code() {
        let script = powershell_script(&[ShellAction::DeregisterUninstall {
            key_name: "Mininet".to_string(),
        }])
        .unwrap();
        assert!(script.contains("$ErrorActionPreference = 'Stop'"));
        assert!(script.ends_with("exit 0\n"));
    }

    #[test]
    fn shortcut_creation_writes_every_field_and_creates_its_directory() {
        let script = powershell_script(&[ShellAction::CreateShortcut(ShortcutRequest {
            location: ShortcutLocation::Exact(PathBuf::from("C:\\Menu")),
            link_name: "Mininet.lnk".to_string(),
            target: PathBuf::from("C:\\App\\mininet-desktop.exe"),
            working_dir: PathBuf::from("C:\\App"),
            description: "Mininet client".to_string(),
        })])
        .unwrap();
        assert!(script.contains("$dir = 'C:\\Menu'"));
        assert!(script.contains("$link = Join-Path $dir 'Mininet.lnk'"));
        assert!(script.contains("New-Item -ItemType Directory -Force -Path $dir"));
        assert!(script.contains("$link.TargetPath = 'C:\\App\\mininet-desktop.exe'"));
        assert!(script.contains("$link.WorkingDirectory = 'C:\\App'"));
        assert!(script.contains("$link.Description = 'Mininet client'"));
        assert!(script.contains("$link.Save()"));
    }

    #[test]
    fn an_injection_attempt_in_a_path_stays_inside_one_quoted_literal() {
        // A path a hostile build could try to smuggle a command through.
        let script = powershell_script(&[ShellAction::RemoveShortcut {
            location: ShortcutLocation::Exact(PathBuf::from(
                "C:\\a'; Remove-Item C:\\Windows -Recurse; '",
            )),
            link_name: "Mininet.lnk".to_string(),
        }])
        .unwrap();
        // The apostrophes are doubled, so the hostile text is one string
        // argument and no second statement exists. Exactly one line of the
        // script is a command; the injected text is inside its quotes.
        assert!(script.contains("$dir = 'C:\\a''; Remove-Item C:\\Windows -Recurse; '''"));
        let commands: Vec<&str> = script
            .lines()
            .filter(|line| line.starts_with("Remove-Item"))
            .collect();
        assert_eq!(commands.len(), 1);
        assert!(commands[0].starts_with("Remove-Item -LiteralPath (Join-Path $dir 'Mininet.lnk')"));
    }

    #[test]
    fn the_uninstall_entry_is_per_user_and_not_modifiable_from_apps_and_features() {
        let script = powershell_script(&[ShellAction::RegisterUninstall(UninstallRegistration {
            key_name: "Mininet".to_string(),
            display_name: "Mininet".to_string(),
            display_version: "0.1.0".to_string(),
            publisher: "The Mininet contributors".to_string(),
            install_location: PathBuf::from("C:\\App"),
            uninstall_command: "\"C:\\App\\mininet-setup.exe\" --uninstall".to_string(),
            estimated_size_kb: 4096,
        })])
        .unwrap();
        assert!(script
            .contains("HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Mininet"));
        assert!(!script.contains("HKLM"));
        assert!(script.contains("-Name 'NoModify' -Value 1 -PropertyType DWord"));
        assert!(script.contains("-Name 'EstimatedSize' -Value 4096 -PropertyType DWord"));
    }

    #[test]
    fn recording_shell_captures_decisions_without_touching_the_system() {
        let mut shell = RecordingShell::default();
        shell
            .apply(&[ShellAction::DeregisterUninstall {
                key_name: "Mininet".to_string(),
            }])
            .unwrap();
        assert_eq!(shell.actions.len(), 1);
    }

    #[test]
    fn a_known_folder_call_is_never_inlined_as_a_command_argument() {
        // PowerShell parses command arguments in argument mode, where a
        // method call is a literal string, not a call. Every generated script
        // must therefore assign the folder to a variable first. This is a
        // structural property because there is no PowerShell here to run:
        // the substring assertions below cannot tell a working script from a
        // syntactically valid one that does the wrong thing.
        for action in [
            ShellAction::CreateShortcut(ShortcutRequest {
                location: ShortcutLocation::StartMenu,
                link_name: "Mininet.lnk".to_string(),
                target: PathBuf::from("C:\\App\\mininet-desktop.exe"),
                working_dir: PathBuf::from("C:\\App"),
                description: "Mininet client".to_string(),
            }),
            ShellAction::RemoveShortcut {
                location: ShortcutLocation::Desktop,
                link_name: "Mininet.lnk".to_string(),
            },
        ] {
            let script = powershell_script(&[action]).unwrap();
            for line in script.lines() {
                if !line.contains("[Environment]::GetFolderPath") {
                    continue;
                }
                assert!(
                    line.starts_with("$dir = [Environment]::GetFolderPath"),
                    "a known-folder call must be assigned, not inlined: {line}"
                );
            }
        }
    }

    #[test]
    fn known_folders_are_resolved_on_the_machine_not_guessed_here() {
        // A redirected Desktop (OneDrive, enterprise folder redirection) is
        // not %USERPROFILE%\\Desktop, so the script must ask Windows.
        let script = powershell_script(&[ShellAction::CreateShortcut(ShortcutRequest {
            location: ShortcutLocation::Desktop,
            link_name: "Mininet.lnk".to_string(),
            target: PathBuf::from("C:\\App\\mininet-desktop.exe"),
            working_dir: PathBuf::from("C:\\App"),
            description: "Mininet client".to_string(),
        })])
        .unwrap();
        assert!(script.contains("[Environment]::GetFolderPath('DesktopDirectory')"));
        assert!(!script.contains("USERPROFILE"));

        let menu = powershell_script(&[ShellAction::RemoveShortcut {
            location: ShortcutLocation::StartMenu,
            link_name: "Mininet.lnk".to_string(),
        }])
        .unwrap();
        assert!(menu.contains("$dir = [Environment]::GetFolderPath('Programs')"));
        assert!(menu.contains("Remove-Item -LiteralPath (Join-Path $dir 'Mininet.lnk')"));
    }

    #[cfg(not(windows))]
    #[test]
    fn the_windows_shell_refuses_to_pretend_on_other_platforms() {
        let mut shell = WindowsShell::default();
        let error = shell
            .apply(&[ShellAction::DeregisterUninstall {
                key_name: "Mininet".to_string(),
            }])
            .unwrap_err();
        assert_eq!(error.code(), "unsupported_platform");
    }
}
