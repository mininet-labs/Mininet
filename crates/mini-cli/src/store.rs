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

/// Open (creating if needed) the `FsBackend` store at `store_path`.
pub fn open_store(store_path: &Path) -> Result<Store<FsBackend>> {
    let backend = FsBackend::open(store_path).map_err(|e| CliError::Store(e.to_string()))?;
    Ok(Store::new(backend))
}

fn trusted_kels_dir(home: &Path) -> PathBuf {
    home.join("trusted_kels")
}

/// Save `kel` under `home`'s trust directory, keyed by its SCID, after
/// verifying it self-certifies. Idempotent.
pub fn trust_kel(home: &Path, kel: Kel) -> Result<()> {
    kel.verify()
        .map_err(|e| CliError::Identity(e.to_string()))?;
    let dir = trusted_kels_dir(home);
    fs::create_dir_all(&dir).map_err(|e| CliError::Io(e.to_string()))?;
    let path = dir.join(format!("{}.kel", kel.scid()));
    fs::write(path, kel.to_bytes()).map_err(|e| CliError::Io(e.to_string()))?;
    Ok(())
}

/// `mini kel trust <hex>` — decode, verify, and locally trust the human
/// **and** device KELs exported by another `mini` home (see
/// `identity::cmd_export_kel`). Both are required for that home's objects
/// to ever verify here — see `identity::cmd_export_kel`'s module docs.
pub fn cmd_trust_kel(home: &Path, hex: &str) -> Result<String> {
    let bytes = crate::identity::hex_decode(hex)?;
    let (human_bytes, device_bytes) = crate::identity::unbundle_kels(&bytes)?;
    let human = Kel::from_bytes(&human_bytes).map_err(|e| CliError::Identity(e.to_string()))?;
    let device = Kel::from_bytes(&device_bytes).map_err(|e| CliError::Identity(e.to_string()))?;
    let human_scid = human.scid().to_string();
    let device_scid = device.scid().to_string();
    trust_kel(home, human)?;
    trust_kel(home, device)?;
    Ok(format!(
        "now trusting KELs for human {human_scid} and device {device_scid}"
    ))
}

/// Build the [`KelDirectory`] oracle governance decisions are checked
/// against: this identity's own human + device KELs, plus every KEL this
/// home has explicitly trusted (see [`trust_kel`]).
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
