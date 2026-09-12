//! `mini windows ...` --- build and inspect Windows client packages.
//!
//! ## Why packing lives here and installing does not
//!
//! `mini` is the developer's tool: it builds the package, inspects it, and
//! plans an install without performing one. `mininet-setup.exe` is the
//! *user's* tool and the one that actually installs, because it ships inside
//! the package and can therefore always uninstall exactly what it installed.
//!
//! Two tools that both install would be two code paths to keep honest, and
//! the Apps & features "Uninstall" command has to name a real executable
//! that will still be there later --- which `mini` on a developer's machine
//! is not. So the read and build verbs are here, and the write verbs stay
//! with the setup program. `mini windows plan` exists precisely so a
//! deployment can see every change first without a second implementation of
//! making them.
//!
//! ## Reproducibility
//!
//! `pack` takes `--built-at-ms` rather than reading the clock. Two runs over
//! the same input bytes then produce byte-identical containers, which is what
//! lets an independent builder confirm they got the same package (SPEC-11,
//! and the same reasoning as `mini-provenance`'s builder agreement). A tool
//! that silently stamped "now" would make every rebuild differ and quietly
//! remove that check.

use crate::error::{CliError, Result};
use crate::json::{CommandResult, JsonValue};
use mini_windows_setup::container::{self, Container};
use mini_windows_setup::manifest::{ManifestHeader, PackageManifest, PackageShortcut};
use mini_windows_setup::{report, Field, InstallOptions, PackageFile, Setup};
use std::path::{Path, PathBuf};

/// Default package identifier for the client.
pub const DEFAULT_PACKAGE: &str = "mininet-windows-client";

/// Default product name shown in the Start Menu and Apps & features.
pub const DEFAULT_PRODUCT: &str = "Mininet";

/// Default client executable inside the package.
pub const DEFAULT_LAUNCH: &str = "mininet-desktop.exe";

/// Default target triple, when the caller does not say.
pub const DEFAULT_TARGET: &str = "x86_64-pc-windows-msvc";

fn setup_error(error: mini_windows_setup::SetupError) -> CliError {
    CliError::WindowsSetup {
        code: error.code(),
        message: error.to_string(),
    }
}

/// Turn engine report fields into this crate's JSON values.
fn fields_into(result: CommandResult, fields: Vec<(&'static str, Field)>) -> CommandResult {
    let mut out = result;
    for (name, value) in fields {
        out = out.field(
            name,
            match value {
                Field::Text(text) => JsonValue::str(text),
                Field::MaybeText(text) => JsonValue::opt_str(text),
                Field::Number(number) => JsonValue::num(number),
                Field::Flag(flag) => JsonValue::Bool(flag),
                Field::List(items) => JsonValue::strs(items),
            },
        );
    }
    out
}

/// What `pack` was asked to build.
#[derive(Debug, Clone)]
pub struct PackRequest {
    /// Directory holding the built files.
    pub source: PathBuf,
    /// Where to write the container.
    pub out: PathBuf,
    /// Package version.
    pub version: String,
    /// Package identifier.
    pub package: String,
    /// Product display name.
    pub product: String,
    /// Client executable, relative to `source`.
    pub launch: String,
    /// Target triple.
    pub target: String,
    /// Build timestamp; required, so packing is reproducible.
    pub built_at_ms: u64,
    /// Explicit file list, relative to `source`. Empty means "walk it".
    pub include: Vec<String>,
    /// Start Menu entries, as `relative/path=Display Name`.
    pub shortcuts: Vec<String>,
}

/// Build a package container from a directory of built files.
pub fn pack(request: &PackRequest) -> Result<CommandResult> {
    let mut relative = if request.include.is_empty() {
        walk(&request.source)?
    } else {
        request.include.clone()
    };
    relative.sort();
    relative.dedup();
    if relative.is_empty() {
        return Err(CliError::Usage(format!(
            "no files found under {}",
            request.source.display()
        )));
    }
    let mut files = Vec::with_capacity(relative.len());
    let mut contents: Vec<(String, Vec<u8>)> = Vec::with_capacity(relative.len());
    for path in &relative {
        let absolute =
            mini_windows_setup::path::join(&request.source, path).map_err(setup_error)?;
        let bytes = std::fs::read(&absolute)
            .map_err(|error| CliError::Io(format!("{}: {error}", absolute.display())))?;
        files.push(PackageFile::describe(path, &bytes).map_err(setup_error)?);
        contents.push((path.clone(), bytes));
    }
    let mut shortcuts = Vec::new();
    for spec in &request.shortcuts {
        let (target, name) = spec.split_once('=').ok_or_else(|| {
            CliError::Usage(format!(
                "--shortcut needs `relative/path=Display Name`, got {spec:?}"
            ))
        })?;
        shortcuts.push(PackageShortcut {
            target: target.to_string(),
            name: name.to_string(),
        });
    }
    let manifest = PackageManifest::new(
        ManifestHeader {
            package: &request.package,
            version: &request.version,
            target: &request.target,
            product: &request.product,
            launch: &request.launch,
            built_at_ms: request.built_at_ms,
        },
        files,
        shortcuts,
    )
    .map_err(setup_error)?;
    let bytes = container::write(&manifest, |path| {
        contents
            .iter()
            .find(|(candidate, _)| candidate == path)
            .map(|(_, bytes)| bytes.clone())
            .ok_or(mini_windows_setup::SetupError::MissingFile {
                path: path.to_string(),
            })
    })
    .map_err(setup_error)?;
    if let Some(parent) = request.out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| CliError::Io(format!("{}: {error}", parent.display())))?;
    }
    std::fs::write(&request.out, &bytes)
        .map_err(|error| CliError::Io(format!("{}: {error}", request.out.display())))?;
    // The manifest is written beside the container too: it is the artifact a
    // reviewer reads and a user re-checks with Get-FileHash, and asking them
    // to extract it from a container first would make that harder than
    // trusting us, which defeats the point of shipping it.
    let manifest_path = request.out.with_extension("manifest.txt");
    std::fs::write(&manifest_path, manifest.to_bytes())
        .map_err(|error| CliError::Io(format!("{}: {error}", manifest_path.display())))?;

    let human = format!(
        "Packed {} {} for {}\n  container: {} ({} bytes)\n  manifest:  {}\n  digest:    {}\n  files:     {}\n",
        request.product,
        request.version,
        request.target,
        request.out.display(),
        bytes.len(),
        manifest_path.display(),
        manifest.digest_hex(),
        manifest.files.len()
    );
    Ok(CommandResult::new(human)
        .field("package", JsonValue::str(&manifest.package))
        .field("version", JsonValue::str(&manifest.version_text))
        .field("target", JsonValue::str(&manifest.target))
        .field("package_digest", JsonValue::str(manifest.digest_hex()))
        .field(
            "container",
            JsonValue::str(request.out.display().to_string()),
        )
        .field(
            "manifest_path",
            JsonValue::str(manifest_path.display().to_string()),
        )
        .field("container_bytes", JsonValue::num(bytes.len() as u64))
        .field("files", JsonValue::num(manifest.files.len() as u64))
        .field("total_bytes", JsonValue::num(manifest.total_bytes())))
}

/// Read a container or a manifest and describe what it contains.
pub fn inspect(path: &Path) -> Result<CommandResult> {
    let bytes = std::fs::read(path)
        .map_err(|error| CliError::Io(format!("{}: {error}", path.display())))?;
    // A container starts with its own tag; anything else is treated as a
    // bare manifest, so `inspect` works on the `.manifest.txt` a reviewer
    // was handed as readily as on the package itself.
    let (manifest, kind) = if bytes.starts_with(container::MAGIC) {
        let container = Container::open(&bytes).map_err(setup_error)?;
        container.verify_all().map_err(setup_error)?;
        (container.manifest().clone(), "container")
    } else {
        (
            PackageManifest::parse(&bytes).map_err(setup_error)?,
            "manifest",
        )
    };
    let mut human = format!(
        "{} {} ({} {})\n  target: {}\n  digest: {}\n  built:  {} ms since the epoch\n  launch: {}\n",
        manifest.product,
        manifest.version_text,
        manifest.package,
        kind,
        manifest.target,
        manifest.digest_hex(),
        manifest.built_at_ms,
        manifest.launch
    );
    for file in &manifest.files {
        human.push_str(&format!(
            "  {:>10}  blake3 {}  sha256 {}  {}\n",
            file.length,
            &mini_windows_setup::manifest::hex(&file.blake3)[..16],
            &mini_windows_setup::manifest::hex(&file.sha256)[..16],
            file.path
        ));
    }
    for shortcut in &manifest.shortcuts {
        human.push_str(&format!(
            "  shortcut {} -> {}\n",
            shortcut.name, shortcut.target
        ));
    }
    if kind == "container" {
        human.push_str("  every file matches its recorded length and both digests\n");
    }
    Ok(CommandResult::new(human)
        .field("kind", JsonValue::str(kind))
        .field("package", JsonValue::str(&manifest.package))
        .field("version", JsonValue::str(&manifest.version_text))
        .field("target", JsonValue::str(&manifest.target))
        .field("package_digest", JsonValue::str(manifest.digest_hex()))
        .field("launch", JsonValue::str(&manifest.launch))
        .field("built_at_ms", JsonValue::num(manifest.built_at_ms))
        .field("files", JsonValue::num(manifest.files.len() as u64))
        .field("total_bytes", JsonValue::num(manifest.total_bytes()))
        .field(
            "paths",
            JsonValue::strs(manifest.files.iter().map(|file| file.path.clone())),
        ))
}

/// Print what installing a container into `install_root` would change.
pub fn plan(
    package: &Path,
    install_root: Option<&Path>,
    user_data_root: Option<&Path>,
    options: &InstallOptions,
) -> Result<CommandResult> {
    let bytes = std::fs::read(package)
        .map_err(|error| CliError::Io(format!("{}: {error}", package.display())))?;
    let container = Container::open(&bytes).map_err(setup_error)?;
    let setup = engine(install_root, user_data_root);
    let plan = setup
        .plan(container.manifest(), options)
        .map_err(setup_error)?;
    let mut human = format!(
        "{} {} would be a {} into {}\n",
        plan.product,
        plan.version_text,
        plan.kind.as_str(),
        plan.version_dir.display()
    );
    for file in &plan.files {
        human.push_str(&format!("  write {}\n", file.destination.display()));
    }
    for action in &plan.shell_actions {
        human.push_str(&format!("  {}\n", report::shell_action_name(action)));
    }
    human.push_str(&format!("  untouched {}\n", plan.user_data_root.display()));
    human.push_str("Nothing was changed. Run mininet-setup to install.\n");
    Ok(fields_into(
        CommandResult::new(human),
        report::plan_fields(&plan),
    ))
}

/// Re-hash an installed version against its stored manifest.
pub fn verify(
    install_root: Option<&Path>,
    user_data_root: Option<&Path>,
    version: Option<&str>,
) -> Result<CommandResult> {
    let setup = engine(install_root, user_data_root);
    let status = setup.status().map_err(setup_error)?;
    let version = match version {
        Some(version) => version.to_string(),
        None => status
            .active
            .as_ref()
            .map(|record| record.version_text.clone())
            .ok_or_else(|| {
                CliError::Usage(format!(
                    "nothing is installed in {}; pass --version to check a specific one",
                    status.install_root.display()
                ))
            })?,
    };
    let verify = setup.verify_installed(&version).map_err(setup_error)?;
    let mut human = if verify.is_intact() {
        format!(
            "Installed {} is intact: {} file(s), {} bytes checked.\n",
            verify.version_text, verify.files_checked, verify.bytes_checked
        )
    } else {
        format!(
            "Installed {} does NOT match its manifest:\n",
            verify.version_text
        )
    };
    for problem in &verify.problems {
        human.push_str(&format!("  {}\n", report::describe_problem(problem)));
    }
    Ok(fields_into(
        CommandResult::new(human),
        report::verify_fields(&verify),
    ))
}

/// Report what is installed.
pub fn status(install_root: Option<&Path>, user_data_root: Option<&Path>) -> Result<CommandResult> {
    let setup = engine(install_root, user_data_root);
    let status = setup.status().map_err(setup_error)?;
    let mut human = match &status.active {
        Some(record) => format!(
            "Mininet {} installed in {}\n  digest {}\n",
            record.version_text,
            status.install_root.display(),
            record.package_digest
        ),
        None => format!("Nothing installed in {}\n", status.install_root.display()),
    };
    if let Some(previous) = &status.previous {
        human.push_str(&format!("  rollback to {}\n", previous.version_text));
    }
    if !status.installed_versions.is_empty() {
        human.push_str(&format!(
            "  versions on disk: {}\n",
            status.installed_versions.join(", ")
        ));
    }
    human.push_str(&format!(
        "  user data {} ({})\n",
        status.user_data_root.display(),
        if status.user_data_present {
            "present"
        } else {
            "absent"
        }
    ));
    Ok(fields_into(
        CommandResult::new(human),
        report::status_fields(&status),
    ))
}

fn engine(install_root: Option<&Path>, user_data_root: Option<&Path>) -> Setup {
    let setup = match install_root {
        Some(root) => Setup::new(root),
        None => Setup::for_current_user(),
    };
    match user_data_root {
        Some(root) => setup.with_user_data_root(root),
        None => setup,
    }
}

/// Every file under `dir`, as `/`-separated relative paths.
///
/// Directories that cannot be read are an error rather than a silent
/// omission: a package missing a DLL because a directory was unreadable is a
/// package that fails on the user's machine instead of on the build machine.
fn walk(dir: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), String::new())];
    while let Some((current, prefix)) = stack.pop() {
        let entries = std::fs::read_dir(&current)
            .map_err(|error| CliError::Io(format!("{}: {error}", current.display())))?;
        for entry in entries {
            let entry =
                entry.map_err(|error| CliError::Io(format!("{}: {error}", current.display())))?;
            let name = entry.file_name().to_string_lossy().to_string();
            let relative = if prefix.is_empty() {
                name
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
    Ok(out)
}
