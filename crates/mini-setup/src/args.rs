//! Command-line parsing for `mininet-setup.exe`.
//!
//! Hand-rolled, matching this workspace's convention (`mini-cli`'s `cli.rs`
//! makes the same call): a setup program is the one binary a person runs
//! before they trust anything here, so its dependency list should be
//! readable in a sitting.
//!
//! Parsing is strict. An unrecognized flag is an error rather than being
//! ignored, because the ignored-flag failure mode for an installer is
//! "`--no-desktop-shortcut` was silently dropped and the user got a desktop
//! shortcut they explicitly declined".

use mini_windows_setup::InstallOptions;
use std::path::PathBuf;

/// What the process was asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Show the wizard (no mode flag given).
    Wizard,
    /// Install without a window, using the flags as given.
    Silent,
    /// Check a package or an installation and exit.
    Verify,
    /// Report what is installed.
    Status,
    /// Return to the previous version.
    Rollback,
    /// Remove the installation.
    Uninstall,
    /// Print the plan for the payload without changing anything.
    DryRun,
    /// Print usage.
    Help,
    /// Print the version.
    Version,
}

impl Mode {
    /// The `--json` envelope `kind` for this mode.
    pub fn kind(self) -> &'static str {
        match self {
            Self::Wizard => "setup.wizard",
            Self::Silent => "setup.install",
            Self::Verify => "setup.verify",
            Self::Status => "setup.status",
            Self::Rollback => "setup.rollback",
            Self::Uninstall => "setup.uninstall",
            Self::DryRun => "setup.plan",
            Self::Help => "setup.help",
            Self::Version => "setup.version",
        }
    }
}

/// A parsed command line.
#[derive(Debug, Clone)]
pub struct Args {
    /// What to do.
    pub mode: Mode,
    /// Package container to read, if given explicitly.
    pub payload: Option<PathBuf>,
    /// Install root override.
    pub install_root: Option<PathBuf>,
    /// User-data root override (mirrors the client's `MININET_HOME`).
    pub user_data_root: Option<PathBuf>,
    /// Install choices.
    pub options: InstallOptions,
    /// Destroy identities during uninstall. Requires [`Mode::Uninstall`].
    pub destroy_identities: bool,
    /// Emit one line of JSON instead of human text.
    pub json: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            mode: Mode::Wizard,
            payload: None,
            install_root: None,
            user_data_root: None,
            options: InstallOptions::default(),
            destroy_identities: false,
            json: false,
        }
    }
}

/// Usage text, also printed on a parse error so the mistake and the fix
/// arrive together.
pub const USAGE: &str = "\
mininet-setup - install the Mininet Windows client

USAGE:
    mininet-setup [OPTIONS]                 open the installer window
    mininet-setup --silent [OPTIONS]        install with no window
    mininet-setup --dry-run [OPTIONS]       print what an install would do
    mininet-setup --verify [OPTIONS]        re-hash the package or the install
    mininet-setup --status [OPTIONS]        report what is installed
    mininet-setup --rollback [OPTIONS]      return to the previous version
    mininet-setup --uninstall [OPTIONS]     remove the installation

OPTIONS:
    --payload <FILE>        package container to install from
    --install-root <DIR>    where to install (default: %LOCALAPPDATA%\\Programs\\Mininet)
    --user-data-root <DIR>  where identities and objects live (default: %LOCALAPPDATA%\\Mininet)
    --no-start-menu         do not create a Start Menu entry
    --desktop-shortcut      also create a Desktop shortcut
    --no-register           do not register in Apps & features
    --allow-downgrade       permit installing an older version than the active one
    --destroy-identities    with --uninstall: also delete identities, irreversibly
    --json                  emit one line of JSON instead of human text
    -h, --help              print this text
    -V, --version           print the setup version

Setup never contacts the network. It installs the package it is handed, into
the current user's own directories, with no administrator rights.
";

/// Parse arguments (without the program name).
pub fn parse<I, S>(raw: I) -> Result<Args, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = Args::default();
    let mut mode: Option<Mode> = None;
    let set_mode = |candidate: Mode, mode: &mut Option<Mode>| -> Result<(), String> {
        match mode {
            Some(existing) if *existing != candidate => Err(format!(
                "--{} and --{} cannot be combined",
                flag_for(*existing),
                flag_for(candidate)
            )),
            _ => {
                *mode = Some(candidate);
                Ok(())
            }
        }
    };
    let tokens: Vec<String> = raw
        .into_iter()
        .map(|token| token.as_ref().to_string())
        .collect();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index].as_str();
        let mut value = |name: &str| -> Result<PathBuf, String> {
            index += 1;
            tokens
                .get(index)
                .filter(|next| !next.starts_with('-'))
                .map(PathBuf::from)
                .ok_or_else(|| format!("{name} needs a path"))
        };
        match token {
            "--silent" | "/S" => set_mode(Mode::Silent, &mut mode)?,
            "--verify" => set_mode(Mode::Verify, &mut mode)?,
            "--status" => set_mode(Mode::Status, &mut mode)?,
            "--rollback" => set_mode(Mode::Rollback, &mut mode)?,
            "--uninstall" => set_mode(Mode::Uninstall, &mut mode)?,
            "--dry-run" => set_mode(Mode::DryRun, &mut mode)?,
            "-h" | "--help" | "/?" => set_mode(Mode::Help, &mut mode)?,
            "-V" | "--version" => set_mode(Mode::Version, &mut mode)?,
            "--payload" => args.payload = Some(value("--payload")?),
            "--install-root" => args.install_root = Some(value("--install-root")?),
            "--user-data-root" => args.user_data_root = Some(value("--user-data-root")?),
            "--no-start-menu" => args.options.start_menu_shortcut = false,
            "--desktop-shortcut" => args.options.desktop_shortcut = true,
            "--no-register" => args.options.register_uninstall = false,
            "--allow-downgrade" => args.options.allow_downgrade = true,
            "--destroy-identities" => args.destroy_identities = true,
            "--json" => args.json = true,
            other => return Err(format!("unrecognized argument: {other}")),
        }
        index += 1;
    }
    args.mode = mode.unwrap_or(Mode::Wizard);
    if args.destroy_identities && args.mode != Mode::Uninstall {
        return Err("--destroy-identities is only meaningful with --uninstall".to_string());
    }
    Ok(args)
}

fn flag_for(mode: Mode) -> &'static str {
    match mode {
        Mode::Wizard => "wizard",
        Mode::Silent => "silent",
        Mode::Verify => "verify",
        Mode::Status => "status",
        Mode::Rollback => "rollback",
        Mode::Uninstall => "uninstall",
        Mode::DryRun => "dry-run",
        Mode::Help => "help",
        Mode::Version => "version",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arguments_opens_the_wizard_with_conservative_defaults() {
        let args = parse(Vec::<String>::new()).unwrap();
        assert_eq!(args.mode, Mode::Wizard);
        assert!(args.options.start_menu_shortcut);
        assert!(!args.options.desktop_shortcut);
        assert!(args.options.register_uninstall);
        assert!(!args.options.allow_downgrade);
        assert!(!args.json);
    }

    #[test]
    fn each_mode_flag_selects_its_mode() {
        for (flag, expected) in [
            ("--silent", Mode::Silent),
            ("--verify", Mode::Verify),
            ("--status", Mode::Status),
            ("--rollback", Mode::Rollback),
            ("--uninstall", Mode::Uninstall),
            ("--dry-run", Mode::DryRun),
            ("--help", Mode::Help),
            ("--version", Mode::Version),
        ] {
            assert_eq!(parse([flag]).unwrap().mode, expected, "{flag}");
        }
    }

    #[test]
    fn the_windows_conventional_spellings_are_accepted() {
        assert_eq!(parse(["/S"]).unwrap().mode, Mode::Silent);
        assert_eq!(parse(["/?"]).unwrap().mode, Mode::Help);
    }

    #[test]
    fn two_different_modes_are_a_parse_error_not_a_silent_winner() {
        let error = parse(["--silent", "--uninstall"]).unwrap_err();
        assert!(error.contains("cannot be combined"));
    }

    #[test]
    fn an_unrecognized_flag_is_refused_rather_than_ignored() {
        let error = parse(["--no-desktop-shortcut"]).unwrap_err();
        assert!(error.contains("unrecognized"));
    }

    #[test]
    fn a_path_option_without_a_path_is_an_error() {
        assert!(parse(["--payload"]).is_err());
        assert!(parse(["--payload", "--json"]).is_err());
        assert!(parse(["--install-root"]).is_err());
    }

    #[test]
    fn paths_and_flags_parse_together_in_any_order() {
        let args = parse([
            "--json",
            "--silent",
            "--payload",
            "C:\\tmp\\client.mnpkg",
            "--install-root",
            "D:\\Mininet",
            "--desktop-shortcut",
            "--no-register",
            "--allow-downgrade",
        ])
        .unwrap();
        assert_eq!(args.mode, Mode::Silent);
        assert!(args.json);
        assert_eq!(args.payload.unwrap().to_str().unwrap(), "C:\\tmp\\client.mnpkg");
        assert_eq!(args.install_root.unwrap().to_str().unwrap(), "D:\\Mininet");
        assert!(args.options.desktop_shortcut);
        assert!(!args.options.register_uninstall);
        assert!(args.options.allow_downgrade);
    }

    #[test]
    fn destroying_identities_requires_an_uninstall() {
        // Guards against the worst possible mis-parse: an install that
        // deletes the user's keys because a flag landed in the wrong mode.
        assert!(parse(["--destroy-identities"]).is_err());
        assert!(parse(["--silent", "--destroy-identities"]).is_err());
        let args = parse(["--uninstall", "--destroy-identities"]).unwrap();
        assert!(args.destroy_identities);
    }

    #[test]
    fn repeating_the_same_mode_flag_is_harmless() {
        assert_eq!(parse(["--status", "--status"]).unwrap().mode, Mode::Status);
    }
}
