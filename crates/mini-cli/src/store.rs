//! Opening the shared object store and building the identity-verification
//! oracle a governance decision needs.
//!
//! **On "shared":** two `mini` homes pointed at the same `--store` path
//! (a synced folder, a USB stick, anything that copies files) can exchange
//! signed objects with no networking code at all — content-addressed
//! signed objects are safe to share via any medium. Live network sync
//! (`mini sync`, `crate::sync`, reusing `mini_bearer`/`mini_sync` the way
//! `mini-bootstrap`'s live demo already proved, D-0062) is the fast-follow
//! this module's docs used to name as deferred — now real, see `crate::sync`.

use std::fs;
use std::path::{Path, PathBuf};

use did_mini::Kel;
use mini_forge::KelDirectory;
use mini_store::{FsBackend, Store};
use mini_sync::KelCache;

use crate::error::{CliError, Result};
use crate::identity::Identity;
use crate::json::{CommandResult, JsonValue};

/// Open (creating if needed) the `FsBackend` store at `store_path`.
pub fn open_store(store_path: &Path) -> Result<Store<FsBackend>> {
    let backend = FsBackend::open(store_path).map_err(|e| CliError::Store(e.to_string()))?;
    Ok(Store::new(backend))
}

fn trusted_kels_dir(home: &Path) -> PathBuf {
    home.join("trusted_kels")
}

struct InspectedBundle {
    human: Kel,
    device: Kel,
    id: String,
}
fn inspect_bundle(hex: &str) -> Result<InspectedBundle> {
    if hex.len() > 128 * 1024 {
        return Err(CliError::Usage(
            "KEL bundle exceeds 128 KiB hex limit".into(),
        ));
    }
    let bytes = crate::identity::hex_decode(hex)?;
    let (human_bytes, device_bytes) = crate::identity::unbundle_kels(&bytes)?;
    let human = Kel::from_bytes(&human_bytes).map_err(|e| CliError::Identity(e.to_string()))?;
    let device = Kel::from_bytes(&device_bytes).map_err(|e| CliError::Identity(e.to_string()))?;
    // Verify BOTH logs and their mutual delegation before creating trust state.
    did_mini::verify_delegation(&human, &device).map_err(|e| CliError::Identity(e.to_string()))?;
    Ok(InspectedBundle {
        human,
        device,
        id: blake3::hash(&bytes).to_hex().to_string(),
    })
}
fn bundle_result(bundle: &InspectedBundle, human: String, trusted: bool) -> CommandResult {
    CommandResult::new(human).field(
        "result",
        JsonValue::Object(vec![
            (
                "human_did".into(),
                JsonValue::str(bundle.human.did().as_str()),
            ),
            (
                "device_did".into(),
                JsonValue::str(bundle.device.did().as_str()),
            ),
            ("bundle_id".into(), JsonValue::str(&bundle.id)),
            ("trusted".into(), JsonValue::Bool(trusted)),
        ]),
    )
}
/// Inspect public identity material without creating a trust directory or
/// accepting it. The fingerprint binds the exact decoded bundle bytes.
pub fn cmd_inspect_kel(hex: &str) -> Result<CommandResult> {
    let bundle = inspect_bundle(hex)?;
    let human = format!(
        "human: {}\ndevice: {}\nbundle: {}\ntrusted: false",
        bundle.human.did().as_str(),
        bundle.device.did().as_str(),
        bundle.id
    );
    Ok(bundle_result(&bundle, human, false))
}
fn prepare_trust(home: &Path) -> Result<std::fs::File> {
    if home.is_symlink() || trusted_kels_dir(home).is_symlink() {
        return Err(CliError::Identity(
            "KEL trust directory must not be a symlink".into(),
        ));
    }
    mini_durable::create_dir_all(home).map_err(|e| CliError::Io(e.to_string()))?;
    mini_durable::lock_exclusive(&home.join("trusted-kels.lock"))
        .map_err(|e| CliError::Io(e.to_string()))
}
fn check_existing_trust(home: &Path, kel: &Kel) -> Result<()> {
    let path = trusted_kels_dir(home).join(format!("{}.kel", kel.scid()));
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_symlink() || !meta.is_file() => {
            return Err(CliError::Identity(
                "trusted KEL must be a regular file".into(),
            ))
        }
        Ok(_) => {
            let existing =
                Kel::from_bytes(&fs::read(path).map_err(|e| CliError::Io(e.to_string()))?)
                    .map_err(|e| CliError::Identity(e.to_string()))?;
            let mut directory = KelDirectory::new();
            directory
                .try_insert_verified(existing)
                .map_err(|e| CliError::Identity(e.to_string()))?;
            directory
                .try_insert_verified(kel.clone())
                .map_err(|e| CliError::Identity(e.to_string()))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(CliError::Io(e.to_string())),
    }
    Ok(())
}
fn save_pinned_kel(home: &Path, kel: &Kel) -> Result<()> {
    let dir = trusted_kels_dir(home);
    mini_durable::create_dir_all(&dir).map_err(|e| CliError::Io(e.to_string()))?;
    mini_durable::atomic_replace(&dir.join(format!("{}.kel", kel.scid())), &kel.to_bytes())
        .map_err(|e| CliError::Io(e.to_string()))
}
/// `mini kel trust <hex> [--confirm <bundle-id>]`. Legacy callers may omit
/// confirmation. Desktop onboarding binds the exact inspected public bundle.
/// Both logs and all existing history checks precede either durable replacement.
/// Individual files are atomic; this is not a cross-file transaction on I/O failure.
pub fn cmd_trust_kel_result(
    home: &Path,
    hex: &str,
    confirm: Option<&str>,
) -> Result<CommandResult> {
    let bundle = inspect_bundle(hex)?;
    if confirm.is_some_and(|id| id != bundle.id) {
        return Err(CliError::Usage(
            "KEL confirmation differs from exact inspected bundle ID".into(),
        ));
    }
    let _lock = prepare_trust(home)?;
    check_existing_trust(home, &bundle.human)?;
    check_existing_trust(home, &bundle.device)?;
    save_pinned_kel(home, &bundle.human)?;
    save_pinned_kel(home, &bundle.device)?;
    let human = format!(
        "now trusting KELs for human {} and device {}",
        bundle.human.scid(),
        bundle.device.scid()
    );
    Ok(bundle_result(&bundle, human, true))
}

/// Compatibility entrypoint for independently supplied, self-certifying KELs.
/// Existing history must be an exact prefix; rollback and forks are refused.
pub fn trust_kel(home: &Path, kel: Kel) -> Result<()> {
    kel.verify()
        .map_err(|e| CliError::Identity(e.to_string()))?;
    let _lock = prepare_trust(home)?;
    check_existing_trust(home, &kel)?;
    save_pinned_kel(home, &kel)
}
/// Human-output compatibility wrapper for existing Rust callers.
pub fn cmd_trust_kel(home: &Path, hex: &str) -> Result<String> {
    cmd_trust_kel_result(home, hex, None).map(|result| result.human)
}

/// Build the [`KelDirectory`] oracle governance decisions are checked
/// against: this identity's own human + device KELs, plus every KEL this
/// home has explicitly trusted (see [`cmd_trust_kel`]).
pub fn build_oracle(home: &Path, identity: &Identity) -> Result<KelDirectory> {
    let mut dir = KelDirectory::new();
    dir.insert(identity.human.kel());
    dir.insert(identity.device.kel());

    let trusted_dir = trusted_kels_dir(home);
    if trusted_dir.exists() {
        for entry in fs::read_dir(&trusted_dir).map_err(|e| CliError::Io(e.to_string()))? {
            let entry = entry.map_err(|e| CliError::Io(e.to_string()))?;
            let bytes = fs::read(entry.path()).map_err(|e| CliError::Io(e.to_string()))?;
            let kel = Kel::from_bytes(&bytes).map_err(|e| CliError::Identity(e.to_string()))?;
            // A locally-trusted file that no longer verifies is skipped,
            // not fatal to the whole command -- one bad file should not
            // block every other operation.
            let _ = dir.try_insert_verified(kel);
        }
    }
    Ok(dir)
}

/// Build the [`KelCache`] `mini_sync`'s ingest pipeline checks incoming
/// objects' authors against — the same trust set as [`build_oracle`]
/// (this identity's own human + device KELs, plus every KEL this home has
/// explicitly trusted), just in `mini-sync`'s own cache type rather than
/// `mini-forge`'s `KelDirectory`. Two separate oracle types because the
/// two crates each define their own minimal trait/struct rather than
/// sharing one across a dependency edge neither strictly needs.
pub fn build_kel_cache(home: &Path, identity: &Identity) -> Result<KelCache> {
    let mut cache = KelCache::new();
    cache.insert_verified(identity.human.kel());
    cache.insert_verified(identity.device.kel());

    let trusted_dir = trusted_kels_dir(home);
    if trusted_dir.exists() {
        for entry in fs::read_dir(&trusted_dir).map_err(|e| CliError::Io(e.to_string()))? {
            let entry = entry.map_err(|e| CliError::Io(e.to_string()))?;
            let bytes = fs::read(entry.path()).map_err(|e| CliError::Io(e.to_string()))?;
            let kel = Kel::from_bytes(&bytes).map_err(|e| CliError::Identity(e.to_string()))?;
            if kel.verify().is_ok() {
                cache.insert_verified(kel);
            }
        }
    }
    Ok(cache)
}
