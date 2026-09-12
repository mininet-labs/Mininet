//! The actions, shared by the console modes and the wizard.
//!
//! Both front ends call exactly these functions, so `--silent` and the
//! window cannot drift apart in what they check, what they approve, or what
//! they report. Each returns an [`Outcome`] carrying both the human sentence
//! and the machine fields, so `--json` is a rendering choice rather than a
//! second code path.

use crate::args::Args;
use crate::payload;
use mini_windows_setup::container::Container;
use mini_windows_setup::{
    report, Field, InstallApproval, NoShell, Setup, ShellIntegration, UninstallApproval,
    WindowsShell,
};

/// The result of one action.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// Envelope kind for `--json`.
    pub kind: &'static str,
    /// What to print without `--json`.
    pub human: String,
    /// Structured fields, named by `mini_windows_setup::report`.
    pub fields: Vec<(&'static str, Field)>,
}

/// A failure with a machine-stable code, so `--json` callers can branch.
#[derive(Debug, Clone)]
pub struct Failure {
    /// Stable code, from [`mini_windows_setup::SetupError::code`] where the
    /// failure came from the engine.
    pub code: String,
    /// Human-readable explanation.
    pub message: String,
}

impl From<mini_windows_setup::SetupError> for Failure {
    fn from(error: mini_windows_setup::SetupError) -> Self {
        Self {
            code: error.code().to_string(),
            message: error.to_string(),
        }
    }
}

impl Failure {
    fn other(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

/// Result alias for the actions.
pub type Result<T> = core::result::Result<T, Failure>;

/// Build the engine for these arguments.
pub fn setup_for(args: &Args) -> Setup {
    let root = args
        .install_root
        .clone()
        .unwrap_or_else(mini_windows_setup::InstallLayout::default_root);
    let setup = Setup::new(root);
    match &args.user_data_root {
        Some(path) => setup.with_user_data_root(path),
        None => setup,
    }
}

/// Shell integration for this platform.
///
/// On Windows, the real thing. Elsewhere, [`NoShell`] plus a note in the
/// outcome: the file half of an install is genuinely portable, and being
/// able to run the whole flow on a Linux CI machine is worth more than
/// refusing to start. Nothing pretends a shortcut was created.
pub fn shell() -> (Box<dyn ShellIntegration>, Option<&'static str>) {
    #[cfg(windows)]
    {
        (Box::new(WindowsShell::default()), None)
    }
    #[cfg(not(windows))]
    {
        let _ = std::marker::PhantomData::<WindowsShell>;
        (
            Box::new(NoShell),
            Some("Shell integration skipped: Start Menu entries and Apps & features registration are Windows-only."),
        )
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

/// Report what is installed.
pub fn status(args: &Args) -> Result<Outcome> {
    let setup = setup_for(args);
    let status = setup.status()?;
    let mut human = String::new();
    match &status.active {
        Some(record) => {
            human.push_str(&format!(
                "Mininet {} is installed in {}.\n",
                record.version_text,
                status.install_root.display()
            ));
            human.push_str(&format!("Package digest: {}\n", record.package_digest));
        }
        None => human.push_str(&format!(
            "Nothing is installed in {}.\n",
            status.install_root.display()
        )),
    }
    if let Some(previous) = &status.previous {
        human.push_str(&format!(
            "Rollback available to {}.\n",
            previous.version_text
        ));
    }
    if !status.installed_versions.is_empty() {
        human.push_str(&format!(
            "Versions on disk: {}\n",
            status.installed_versions.join(", ")
        ));
    }
    human.push_str(&format!(
        "Identities and objects live in {} ({}).\n",
        status.user_data_root.display(),
        if status.user_data_present {
            "present"
        } else {
            "not created yet"
        }
    ));
    Ok(Outcome {
        kind: "setup.status",
        human,
        fields: report::status_fields(&status),
    })
}

/// Print what an install would do, without doing it.
pub fn plan(args: &Args) -> Result<Outcome> {
    let payload = payload::locate(args.payload.as_deref())
        .map_err(|error| Failure::other("no_payload", error))?;
    let container = Container::open(&payload.bytes)?;
    let setup = setup_for(args);
    let plan = setup.plan(container.manifest(), &args.options)?;
    let mut human = format!(
        "Package: {} {} ({})\nSource: {}\nDigest: {}\nPlan: {}\n",
        plan.product,
        plan.version_text,
        plan.package,
        payload.origin,
        plan.package_digest,
        plan.kind.as_str()
    );
    human.push_str(&format!("Install into: {}\n", plan.version_dir.display()));
    for file in &plan.files {
        human.push_str(&format!(
            "  write {} ({} bytes)\n",
            file.destination.display(),
            file.length
        ));
    }
    for action in &plan.shell_actions {
        human.push_str(&format!("  {}\n", report::shell_action_name(action)));
    }
    human.push_str(&format!(
        "Untouched: {}\n",
        plan.user_data_root.display()
    ));
    Ok(Outcome {
        kind: "setup.plan",
        human,
        fields: report::plan_fields(&plan),
    })
}

/// Verify the package, and the active installation when there is one.
pub fn verify(args: &Args) -> Result<Outcome> {
    let setup = setup_for(args);
    // An explicit payload means "check this file"; otherwise check what is
    // installed, which is what a person clicking Repair wants.
    if args.payload.is_some() || setup.status()?.active.is_none() {
        let payload = payload::locate(args.payload.as_deref())
            .map_err(|error| Failure::other("no_payload", error))?;
        let container = Container::open(&payload.bytes)?;
        container.verify_all()?;
        let manifest = container.manifest();
        return Ok(Outcome {
            kind: "setup.verify",
            human: format!(
                "Package {} {} is intact: {} file(s), {} bytes, digest {}.\nSource: {}\n",
                manifest.product,
                manifest.version_text,
                manifest.files.len(),
                manifest.total_bytes(),
                manifest.digest_hex(),
                payload.origin
            ),
            fields: vec![
                ("version", Field::Text(manifest.version_text.clone())),
                ("intact", Field::Flag(true)),
                ("files_checked", Field::Number(manifest.files.len() as u64)),
                ("bytes_checked", Field::Number(manifest.total_bytes())),
                ("problems", Field::List(Vec::new())),
                (
                    "package_digest",
                    Field::Text(manifest.digest_hex()),
                ),
            ],
        });
    }
    let status = setup.status()?;
    let active = status
        .active
        .as_ref()
        .expect("checked just above that a version is active");
    let verify = setup.verify_installed(&active.version_text)?;
    let human = if verify.is_intact() {
        format!(
            "Installed {} is intact: {} file(s), {} bytes checked.\n",
            verify.version_text, verify.files_checked, verify.bytes_checked
        )
    } else {
        let mut text = format!(
            "Installed {} does NOT match its manifest:\n",
            verify.version_text
        );
        for problem in &verify.problems {
            text.push_str(&format!("  {}\n", report::describe_problem(problem)));
        }
        text.push_str("Reinstall from a package you trust before running it.\n");
        text
    };
    Ok(Outcome {
        kind: "setup.verify",
        human,
        fields: report::verify_fields(&verify),
    })
}

/// Install the located package.
///
/// `--silent` *is* the user's decision, made when they typed it, so the
/// approval is constructed from the package actually on hand.
pub fn install(args: &Args) -> Result<Outcome> {
    let payload = payload::locate(args.payload.as_deref())
        .map_err(|error| Failure::other("no_payload", error))?;
    install_bytes(args, &payload.bytes, &payload.origin, None)
}

/// Install one exact set of container bytes.
///
/// `approved_digest` is how the wizard binds what the user saw to what gets
/// installed. The window shows a manifest digest and the user ticks a box
/// naming it; passing that digest here means a package whose bytes changed
/// between the review page and the Install click is refused rather than
/// silently installed under an approval given for something else. The
/// engine's own [`InstallApproval`] check would catch a mismatch between the
/// approval and the container, but only this check catches a mismatch
/// between the container and *what the human read*.
pub fn install_bytes(
    args: &Args,
    bytes: &[u8],
    origin: &str,
    approved_digest: Option<&str>,
) -> Result<Outcome> {
    let container = Container::open(bytes)?;
    let offered = container.manifest().digest_hex();
    if let Some(approved) = approved_digest {
        if approved != offered {
            return Err(Failure::other(
                "approval_mismatch",
                format!("the package changed since it was reviewed: approved {approved}, found {offered}"),
            ));
        }
    }
    let setup = setup_for(args);
    let now = now_ms();
    let approval = InstallApproval::new(container.manifest(), now);
    let (mut shell, note) = shell();
    let report_out = setup.install(
        &container,
        &approval,
        &args.options,
        shell.as_mut(),
        now,
    )?;
    let mut human = format!(
        "Installed {} {} from {}.\n",
        container.manifest().product,
        report_out.active.version_text,
        origin
    );
    human.push_str(&format!(
        "{} file(s), {} bytes written and re-verified.\n",
        report_out.files_written, report_out.bytes_written
    ));
    human.push_str(&format!("Run: {}\n", report_out.launch_path.display()));
    if let Some(previous) = &report_out.previous {
        human.push_str(&format!(
            "Rollback available to {}.\n",
            previous.version_text
        ));
    }
    if let Some(note) = note {
        human.push_str(note);
        human.push('\n');
    }
    Ok(Outcome {
        kind: "setup.install",
        human,
        fields: report::install_fields(&report_out),
    })
}

/// Return to the previous version.
pub fn rollback(args: &Args) -> Result<Outcome> {
    let setup = setup_for(args);
    let (mut shell, note) = shell();
    let record = setup.rollback(&args.options, shell.as_mut(), now_ms())?;
    let mut human = format!("Rolled back to {}.\n", record.version_text);
    if let Some(note) = note {
        human.push_str(note);
        human.push('\n');
    }
    let status = setup.status()?;
    Ok(Outcome {
        kind: "setup.rollback",
        human,
        fields: report::status_fields(&status),
    })
}

/// Remove the installation.
pub fn uninstall(args: &Args) -> Result<Outcome> {
    let setup = setup_for(args);
    let root = setup.layout().root().to_path_buf();
    let approval = if args.destroy_identities {
        UninstallApproval::destroying_identities(&root, setup.user_data_root(), now_ms())
    } else {
        UninstallApproval::keeping_identities(&root, now_ms())
    };
    let (mut shell, note) = shell();
    let report_out = setup.uninstall(&approval, &args.options, shell.as_mut(), now_ms())?;
    let mut human = format!("Removed {}.\n", report_out.install_root.display());
    if !report_out.versions_removed.is_empty() {
        human.push_str(&format!(
            "Versions removed: {}\n",
            report_out.versions_removed.join(", ")
        ));
    }
    match (&report_out.user_data_kept, &report_out.user_data_destroyed) {
        (Some(kept), _) => human.push_str(&format!(
            "Kept your identities, objects, and settings in {}.\n",
            kept.display()
        )),
        (None, Some(destroyed)) => human.push_str(&format!(
            "Destroyed identities, objects, and settings in {}. This cannot be undone.\n",
            destroyed.display()
        )),
        (None, None) => human.push_str("No user data was present.\n"),
    }
    if let Some(note) = note {
        human.push_str(note);
        human.push('\n');
    }
    Ok(Outcome {
        kind: "setup.uninstall",
        human,
        fields: report::uninstall_fields(&report_out),
    })
}
