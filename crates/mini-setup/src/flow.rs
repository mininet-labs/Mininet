//! The wizard's state machine, separated from its rendering.
//!
//! Every consequential decision the window can reach --- whether the Install
//! button is live, whether identities may be destroyed, what the Back button
//! goes back to --- lives here as plain data, so it is unit-tested rather
//! than only clicked through by hand on a Windows machine nobody in CI has.
//! `gui.rs` is then a thin renderer over this: it draws pages and forwards
//! events, and holds no rule of its own.

use mini_windows_setup::{InstallOptions, PackageManifest, SetupStatus};
use std::path::PathBuf;

/// Which page the window is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// What this is, what it will install, and where the package came from.
    Welcome,
    /// Install location and the three integration choices.
    Options,
    /// Every change, listed, with the approval checkbox.
    Review,
    /// The result of an install, verify, rollback, or uninstall.
    Result,
    /// Repair / roll back / uninstall an existing installation.
    Maintenance,
    /// Confirming an irreversible identity deletion.
    ConfirmDestroy,
}

/// The word a user must type to confirm destroying identities.
///
/// A typed word rather than a second checkbox: the action deletes the only
/// copy of signing keys, and a checkbox is one mis-click away from being
/// ticked by someone clicking Next quickly.
pub const DESTROY_CONFIRMATION: &str = "DESTROY";

/// Everything the wizard knows.
#[derive(Debug, Clone)]
pub struct Wizard {
    /// Current page.
    pub page: Page,
    /// Install choices.
    pub options: InstallOptions,
    /// Install root, editable on the Options page.
    pub install_root: String,
    /// The approval checkbox on the Review page.
    pub approved: bool,
    /// Whether the user asked to destroy identities during uninstall.
    pub destroy_identities: bool,
    /// What the user typed into the destroy confirmation box.
    pub destroy_confirmation: String,
    /// The last result or error, shown on [`Page::Result`].
    pub message: String,
    /// True when the last action succeeded.
    pub succeeded: bool,
    /// Path of the installed client, once there is one to launch.
    pub launch_path: Option<PathBuf>,
}

impl Wizard {
    /// Start the wizard for a located package and the current install state.
    ///
    /// An existing installation opens on [`Page::Maintenance`]: someone who
    /// double-clicks setup with the client already installed almost always
    /// wants to repair, roll back, or remove it, and being dropped straight
    /// into a fresh-install flow invites reinstalling over a working copy.
    pub fn new(status: &SetupStatus, options: InstallOptions) -> Self {
        let page = if status.active.is_some() {
            Page::Maintenance
        } else {
            Page::Welcome
        };
        Self {
            page,
            options,
            install_root: status.install_root.display().to_string(),
            approved: false,
            destroy_identities: false,
            destroy_confirmation: String::new(),
            message: String::new(),
            succeeded: false,
            launch_path: status.launch_path.clone(),
        }
    }

    /// True when the Install button should be clickable.
    ///
    /// The approval checkbox is the gate, and it is reset by any change to
    /// what is being approved (see [`Self::set_options`] and
    /// [`Self::set_install_root`]): an approval collected for one set of
    /// changes must not survive an edit to those changes.
    pub fn can_install(&self) -> bool {
        self.page == Page::Review && self.approved && !self.install_root.trim().is_empty()
    }

    /// True when the irreversible identity deletion may proceed.
    pub fn can_destroy_identities(&self) -> bool {
        self.destroy_identities && self.destroy_confirmation.trim() == DESTROY_CONFIRMATION
    }

    /// Replace the install options, invalidating any approval.
    pub fn set_options(&mut self, options: InstallOptions) {
        self.options = options;
        self.approved = false;
    }

    /// Replace the install root, invalidating any approval.
    pub fn set_install_root(&mut self, root: impl Into<String>) {
        self.install_root = root.into();
        self.approved = false;
    }

    /// Move forward one page. Returns the new page.
    pub fn next(&mut self) -> Page {
        self.page = match self.page {
            Page::Welcome => Page::Options,
            Page::Options => Page::Review,
            // Review advances only by performing the install, which the
            // caller does; `next` from Review is not a way past the gate.
            Page::Review => Page::Review,
            Page::Result => Page::Result,
            Page::Maintenance => Page::Maintenance,
            Page::ConfirmDestroy => Page::ConfirmDestroy,
        };
        self.page
    }

    /// Move back one page. Returns the new page.
    pub fn back(&mut self) -> Page {
        self.page = match self.page {
            Page::Welcome => Page::Welcome,
            Page::Options => Page::Welcome,
            Page::Review => Page::Options,
            Page::Result => Page::Welcome,
            Page::Maintenance => Page::Maintenance,
            Page::ConfirmDestroy => Page::Maintenance,
        };
        // Leaving the destroy confirmation clears what was typed, so the
        // confirmation cannot be pre-armed and then triggered later.
        if self.page != Page::ConfirmDestroy {
            self.destroy_confirmation.clear();
            self.destroy_identities = false;
        }
        self.page
    }

    /// Record a successful action and show it.
    pub fn succeed(&mut self, message: impl Into<String>) {
        self.message = message.into();
        self.succeeded = true;
        self.page = Page::Result;
        self.approved = false;
        self.destroy_identities = false;
        self.destroy_confirmation.clear();
    }

    /// Record a failed action and show it.
    pub fn fail(&mut self, message: impl Into<String>) {
        self.message = message.into();
        self.succeeded = false;
        self.page = Page::Result;
        self.approved = false;
    }

    /// The human summary shown on the Review page.
    ///
    /// Written out in full rather than summarized: "3 files" tells a user
    /// nothing they can check, while a list of paths, the shortcut, and the
    /// registry key is something they can compare against what they expected.
    pub fn review_lines(&self, manifest: &PackageManifest, plan_kind: &str) -> Vec<String> {
        let mut lines = vec![
            format!("{} {}", manifest.product, manifest.version_text),
            format!("Package digest: {}", manifest.digest_hex()),
            format!("This is a(n) {plan_kind}."),
            format!("Install into: {}", self.install_root),
            format!(
                "{} file(s), {} KiB",
                manifest.files.len(),
                manifest.total_bytes().div_ceil(1024)
            ),
        ];
        for file in &manifest.files {
            lines.push(format!("  {}", file.path));
        }
        // A manifest declaring no shortcuts gets the one default entry to
        // the launch target (`Setup::requested_shortcuts`'s own fallback,
        // mirrored here); one declaring several gets every name and target
        // it names. Reducing that to a single "Start Menu entry: yes"
        // boolean, as this page used to, meant a manifest naming a second
        // shortcut at a different packaged executable created shell changes
        // the approval the user actually read never showed.
        let requested: Vec<(String, String)> = if manifest.shortcuts.is_empty() {
            vec![(
                format!("{}.lnk", mini_windows_setup::PRODUCT_KEY),
                manifest.launch.clone(),
            )]
        } else {
            manifest
                .shortcuts
                .iter()
                .map(|shortcut| (format!("{}.lnk", shortcut.name), shortcut.target.clone()))
                .collect()
        };
        if self.options.start_menu_shortcut {
            lines.push(format!("Start Menu entries ({}):", requested.len()));
            for (name, target) in &requested {
                lines.push(format!("  {name} -> {target}"));
            }
        } else {
            lines.push("Start Menu entry: no".to_string());
        }
        if self.options.desktop_shortcut {
            lines.push(format!("Desktop shortcuts ({}):", requested.len()));
            for (name, target) in &requested {
                lines.push(format!("  {name} -> {target}"));
            }
        } else {
            lines.push("Desktop shortcut: no".to_string());
        }
        lines.push(if self.options.register_uninstall {
            "Listed in Apps & features: yes".to_string()
        } else {
            "Listed in Apps & features: no".to_string()
        });
        lines.push("Administrator rights: not required".to_string());
        lines.push("Network access: none".to_string());
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_windows_setup::{InstallRecord, SetupStatus};

    fn empty_status() -> SetupStatus {
        SetupStatus {
            install_root: PathBuf::from("C:\\Users\\a\\AppData\\Local\\Programs\\Mininet"),
            active: None,
            previous: None,
            installed_versions: Vec::new(),
            launch_path: None,
            user_data_root: PathBuf::from("C:\\Users\\a\\AppData\\Local\\Mininet"),
            user_data_present: false,
        }
    }

    fn installed_status() -> SetupStatus {
        let mut status = empty_status();
        status.active = Some(InstallRecord {
            version_text: "0.1.0".to_string(),
            version: mini_windows_setup::manifest::PackageManifest::parse(
                &sample_manifest().to_bytes(),
            )
            .unwrap()
            .version,
            package_digest: "ab".repeat(32),
            installed_at_ms: 1,
            start_menu_shortcut: true,
            desktop_shortcut: false,
            register_uninstall: true,
        });
        status.installed_versions = vec!["0.1.0".to_string()];
        status
    }

    fn sample_manifest() -> PackageManifest {
        PackageManifest::new(
            mini_windows_setup::ManifestHeader {
                package: "mininet-windows-client",
                version: "0.1.0",
                target: "x86_64-pc-windows-msvc",
                product: "Mininet",
                launch: "mininet-desktop.exe",
                built_at_ms: 1,
            },
            vec![
                mini_windows_setup::PackageFile::describe("mininet-desktop.exe", b"a").unwrap(),
                mini_windows_setup::PackageFile::describe("mini.exe", b"bb").unwrap(),
            ],
            vec![],
        )
        .unwrap()
    }

    #[test]
    fn a_fresh_machine_starts_at_the_welcome_page() {
        let wizard = Wizard::new(&empty_status(), InstallOptions::default());
        assert_eq!(wizard.page, Page::Welcome);
    }

    #[test]
    fn an_existing_installation_opens_in_maintenance_rather_than_reinstalling() {
        let wizard = Wizard::new(&installed_status(), InstallOptions::default());
        assert_eq!(wizard.page, Page::Maintenance);
    }

    #[test]
    fn install_is_blocked_until_the_review_page_is_explicitly_approved() {
        let mut wizard = Wizard::new(&empty_status(), InstallOptions::default());
        assert!(!wizard.can_install());
        wizard.next();
        assert_eq!(wizard.page, Page::Options);
        assert!(!wizard.can_install());
        wizard.next();
        assert_eq!(wizard.page, Page::Review);
        assert!(!wizard.can_install());
        wizard.approved = true;
        assert!(wizard.can_install());
    }

    #[test]
    fn next_from_review_never_slips_past_the_approval_gate() {
        let mut wizard = Wizard::new(&empty_status(), InstallOptions::default());
        wizard.next();
        wizard.next();
        assert_eq!(wizard.next(), Page::Review);
        assert!(!wizard.can_install());
    }

    #[test]
    fn changing_an_option_or_the_location_withdraws_a_previous_approval() {
        let mut wizard = Wizard::new(&empty_status(), InstallOptions::default());
        wizard.next();
        wizard.next();
        wizard.approved = true;
        wizard.set_options(InstallOptions {
            desktop_shortcut: true,
            ..InstallOptions::default()
        });
        assert!(!wizard.can_install());
        wizard.approved = true;
        wizard.set_install_root("D:\\Elsewhere");
        assert!(!wizard.can_install());
    }

    #[test]
    fn an_empty_install_location_is_not_installable() {
        let mut wizard = Wizard::new(&empty_status(), InstallOptions::default());
        wizard.next();
        wizard.next();
        wizard.approved = true;
        wizard.set_install_root("   ");
        wizard.approved = true;
        assert!(!wizard.can_install());
    }

    #[test]
    fn destroying_identities_needs_the_word_typed_exactly() {
        let mut wizard = Wizard::new(&installed_status(), InstallOptions::default());
        wizard.destroy_identities = true;
        assert!(!wizard.can_destroy_identities());
        wizard.destroy_confirmation = "destroy".to_string();
        assert!(!wizard.can_destroy_identities());
        wizard.destroy_confirmation = "DESTROY ".to_string();
        assert!(wizard.can_destroy_identities());
        wizard.destroy_identities = false;
        assert!(!wizard.can_destroy_identities());
    }

    #[test]
    fn leaving_the_confirmation_page_disarms_it() {
        let mut wizard = Wizard::new(&installed_status(), InstallOptions::default());
        wizard.page = Page::ConfirmDestroy;
        wizard.destroy_identities = true;
        wizard.destroy_confirmation = DESTROY_CONFIRMATION.to_string();
        assert!(wizard.can_destroy_identities());
        assert_eq!(wizard.back(), Page::Maintenance);
        assert!(!wizard.can_destroy_identities());
        assert!(wizard.destroy_confirmation.is_empty());
    }

    #[test]
    fn a_result_clears_the_approval_so_a_second_click_cannot_repeat_it() {
        let mut wizard = Wizard::new(&empty_status(), InstallOptions::default());
        wizard.approved = true;
        wizard.succeed("installed 0.1.0");
        assert_eq!(wizard.page, Page::Result);
        assert!(wizard.succeeded);
        assert!(!wizard.approved);
        wizard.approved = true;
        wizard.fail("something went wrong");
        assert!(!wizard.succeeded);
        assert!(!wizard.approved);
    }

    #[test]
    fn the_review_page_lists_every_file_and_both_hard_guarantees() {
        let wizard = Wizard::new(&empty_status(), InstallOptions::default());
        let manifest = sample_manifest();
        let lines = wizard.review_lines(&manifest, "first install");
        assert!(lines
            .iter()
            .any(|line| line.contains(&manifest.digest_hex())));
        assert!(lines.iter().any(|line| line.trim() == "mini.exe"));
        assert!(lines
            .iter()
            .any(|line| line.trim() == "mininet-desktop.exe"));
        assert!(lines
            .iter()
            .any(|line| line == "Administrator rights: not required"));
        assert!(lines.iter().any(|line| line == "Network access: none"));
    }

    #[test]
    fn the_review_page_shows_every_declared_shortcut_not_a_yes_no_boolean() {
        // A manifest declaring a second shortcut at a packaged executable
        // other than the main launch target used to approve as a bare
        // "Start Menu entry: yes" -- shell changes the user never actually
        // saw before approving them.
        let manifest = PackageManifest::new(
            mini_windows_setup::ManifestHeader {
                package: "mininet-windows-client",
                version: "0.1.0",
                target: "x86_64-pc-windows-msvc",
                product: "Mininet",
                launch: "mininet-desktop.exe",
                built_at_ms: 1,
            },
            vec![
                mini_windows_setup::PackageFile::describe("mininet-desktop.exe", b"a").unwrap(),
                mini_windows_setup::PackageFile::describe("mini.exe", b"bb").unwrap(),
            ],
            vec![
                mini_windows_setup::manifest::PackageShortcut {
                    target: "mininet-desktop.exe".to_string(),
                    name: "Mininet".to_string(),
                },
                mini_windows_setup::manifest::PackageShortcut {
                    target: "mini.exe".to_string(),
                    name: "Mininet Console".to_string(),
                },
            ],
        )
        .unwrap();
        let wizard = Wizard::new(&empty_status(), InstallOptions::default());
        let lines = wizard.review_lines(&manifest, "first install");
        assert!(lines
            .iter()
            .any(|line| line == "  Mininet.lnk -> mininet-desktop.exe"));
        assert!(lines
            .iter()
            .any(|line| line == "  Mininet Console.lnk -> mini.exe"));
        assert!(!lines.iter().any(|line| line == "Start Menu entry: yes"));
    }

    #[test]
    fn back_from_the_first_page_stays_put_rather_than_closing() {
        let mut wizard = Wizard::new(&empty_status(), InstallOptions::default());
        assert_eq!(wizard.back(), Page::Welcome);
    }
}
