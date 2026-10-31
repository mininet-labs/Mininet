//! Local identity persistence: a human root plus one delegated device,
//! reconstructed deterministically on every CLI invocation from a small
//! on-disk seed file — the CLI is a fresh process each run, so there is no
//! long-lived [`Controller`] to hold state in memory between commands.
//!
//! **Honest limit:** reconstruction replays exactly the inception +
//! device-delegation events, and nothing else. There is no key rotation
//! from the CLI yet — rotating would require persisting the full KEL, not
//! just the original seeds, and is deferred to a later batch (rotation
//! support is orthogonal to Batch 1's governed-merge exit condition). The
//! seed file is the actual secret: anyone who reads it controls this
//! identity. It is written with owner-only permissions on Unix; there is
//! no OS keychain integration yet.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

use did_mini::{Capabilities, Controller, Did};
use mini_crypto::SigningKey;

use crate::json::{CommandResult, JsonValue};
use crate::CliError;

const SEED_FILE_LEN: usize = 64; // current_seed(32) || next_seed(32)

/// The reconstructed local identity: a human root and its one delegated
/// device, ready to sign.
#[derive(Debug)]
pub struct Identity {
    pub human: Controller,
    pub device: Controller,
}

impl Identity {
    pub fn human_did(&self) -> Did {
        self.human.did()
    }

    pub fn device_did(&self) -> Did {
        self.device.did()
    }
}

fn human_seed_path(home: &Path) -> PathBuf {
    home.join("human.seed")
}

fn device_seed_path(home: &Path) -> PathBuf {
    home.join("device.seed")
}

fn write_seed_file(path: &Path, current: &[u8; 32], next: &[u8; 32]) -> Result<(), CliError> {
    let mut bytes = Zeroizing::new([0u8; SEED_FILE_LEN]);
    bytes[..32].copy_from_slice(current);
    bytes[32..].copy_from_slice(next);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // Exclusive creation with private mode: secret bytes are never written to
    // an existing file, symlink, or a temporarily world-readable file.
    let mut file = options
        .open(path)
        .map_err(|e| CliError::Io(e.to_string()))?;
    file.write_all(bytes.as_ref())
        .and_then(|_| file.sync_all())
        .map_err(|e| CliError::Io(e.to_string()))?;
    mini_durable::sync_parent(path).map_err(|e| CliError::Io(e.to_string()))
}

type SeedPair = (Zeroizing<[u8; 32]>, Zeroizing<[u8; 32]>);

fn read_seed_file(path: &Path) -> Result<SeedPair, CliError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CliError::NotInitialized
        } else {
            CliError::Io(error.to_string())
        }
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(CliError::CorruptSeedFile);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.permissions().mode() & 0o077 != 0 || metadata.nlink() != 1 {
            return Err(CliError::Identity(
                "seed file must be owner-only and not hard-linked".into(),
            ));
        }
    }
    let mut file = fs::File::open(path).map_err(|e| CliError::Io(e.to_string()))?;
    let opened = file.metadata().map_err(|e| CliError::Io(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if opened.dev() != metadata.dev() || opened.ino() != metadata.ino() {
            return Err(CliError::CorruptSeedFile);
        }
    }
    if opened.len() != SEED_FILE_LEN as u64 {
        return Err(CliError::CorruptSeedFile);
    }
    let mut bytes = Zeroizing::new([0u8; SEED_FILE_LEN]);
    file.read_exact(bytes.as_mut())
        .map_err(|_| CliError::CorruptSeedFile)?;
    let mut current = Zeroizing::new([0u8; 32]);
    let mut next = Zeroizing::new([0u8; 32]);
    current.copy_from_slice(&bytes[..32]);
    next.copy_from_slice(&bytes[32..]);
    Ok((current, next))
}

/// Create a fresh human root + delegated device identity under `home`.
/// Errors if one already exists — use [`load`] to reuse it.
pub fn init(home: &Path) -> Result<Identity, CliError> {
    if home.is_symlink() {
        return Err(CliError::Identity(
            "identity home must not be a symlink".into(),
        ));
    }
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(home)
        .map_err(|e| CliError::Io(e.to_string()))?;
    let _identity_lock = mini_durable::lock_exclusive(&home.join("identity.lock"))
        .map_err(|e| CliError::Io(e.to_string()))?;
    if human_seed_path(home).symlink_metadata().is_ok()
        || device_seed_path(home).symlink_metadata().is_ok()
    {
        return Err(CliError::AlreadyInitialized);
    }

    let (h_current, h_next) = fresh_seed_pair()?;
    let human = Controller::incept_single_from_seeds(&h_current, &h_next)
        .map_err(|e| CliError::Identity(e.to_string()))?;

    let (d_current, d_next) = fresh_seed_pair()?;
    let mut human = human;
    let device = Controller::incept_device_single_from_seeds(&human.did(), &d_current, &d_next)
        .map_err(|e| CliError::Identity(e.to_string()))?;

    human
        .delegate_device(&device.did(), Capabilities::primary())
        .map_err(|e| CliError::Identity(e.to_string()))?;

    // The human seed is the final publication marker. Interrupted initialization
    // fails closed on a retained device seed; it never invents a replacement.
    write_seed_file(&device_seed_path(home), &d_current, &d_next)?;
    write_seed_file(&human_seed_path(home), &h_current, &h_next)?;
    Ok(Identity { human, device })
}

/// Reconstruct the identity at `home` from its saved seeds, replaying the
/// same deterministic inception + delegation sequence [`init`] performed.
pub fn load(home: &Path) -> Result<Identity, CliError> {
    if !home.exists() {
        return Err(CliError::NotInitialized);
    }
    if home.is_symlink() {
        return Err(CliError::Identity(
            "identity home must not be a symlink".into(),
        ));
    }
    let _identity_lock = mini_durable::lock_exclusive(&home.join("identity.lock"))
        .map_err(|e| CliError::Io(e.to_string()))?;
    let (h_current, h_next) = read_seed_file(&human_seed_path(home))?;
    let mut human = Controller::incept_single_from_seeds(&h_current, &h_next)
        .map_err(|e| CliError::Identity(e.to_string()))?;

    let (d_current, d_next) = read_seed_file(&device_seed_path(home))?;
    let device = Controller::incept_device_single_from_seeds(&human.did(), &d_current, &d_next)
        .map_err(|e| CliError::Identity(e.to_string()))?;
    human
        .delegate_device(&device.did(), Capabilities::primary())
        .map_err(|e| CliError::Identity(e.to_string()))?;

    Ok(Identity { human, device })
}

/// The human root's current signing seed, for deriving unlinkable
/// purpose-specific identities (BETA accounts). The caller must not retain it
/// beyond one derivation and never print it.
pub(crate) fn human_root_seed(home: &Path) -> Result<Zeroizing<[u8; 32]>, CliError> {
    if !home.exists() {
        return Err(CliError::NotInitialized);
    }
    if home.is_symlink() {
        return Err(CliError::Identity(
            "identity home must not be a symlink".into(),
        ));
    }
    let _identity_lock = mini_durable::lock_exclusive(&home.join("identity.lock"))
        .map_err(|e| CliError::Io(e.to_string()))?;
    let (current, _next) = read_seed_file(&human_seed_path(home))?;
    Ok(current)
}

/// Load the identity at `home` if one exists, otherwise create one —
/// the convenience path most commands want.
pub fn load_or_init(home: &Path) -> Result<Identity, CliError> {
    if human_seed_path(home).exists() {
        load(home)
    } else {
        init(home)
    }
}

/// `mini identity init`
pub fn cmd_init(home: &Path) -> Result<String, CliError> {
    cmd_init_result(home).map(|result| result.human)
}

pub(crate) fn cmd_init_result(home: &Path) -> Result<CommandResult, CliError> {
    let identity = init(home)?;
    let human = format!(
        "identity created\n  human:  {}\n  device: {}",
        identity.human_did().as_str(),
        identity.device_did().as_str()
    );
    Ok(identity_result(&identity, human, "created"))
}

/// `mini identity show` loads an existing profile and never initializes one.
pub fn cmd_show(home: &Path) -> Result<String, CliError> {
    cmd_show_result(home).map(|result| result.human)
}

pub(crate) fn cmd_show_result(home: &Path) -> Result<CommandResult, CliError> {
    let identity = load(home)?;
    let human = format!(
        "human:  {}\ndevice: {}",
        identity.human_did().as_str(),
        identity.device_did().as_str()
    );
    Ok(identity_result(&identity, human, "initialized"))
}
fn identity_result(identity: &Identity, human: String, state: &str) -> CommandResult {
    CommandResult::new(human).field(
        "result",
        JsonValue::Object(vec![
            (
                "human_did".into(),
                JsonValue::str(identity.human_did().as_str()),
            ),
            (
                "device_did".into(),
                JsonValue::str(identity.device_did().as_str()),
            ),
            (state.into(), JsonValue::Bool(true)),
        ]),
    )
}

/// `mini kel export` — print this home's human **and** device KELs as
/// hex, for another `mini` home to `mini kel trust` (see
/// `crate::store::cmd_trust_kel`). Both are required: provenance verification
/// (`mini_objects::verify_provenance`) checks the signing device's
/// delegation against the root's KEL *and* needs the device's own KEL to
/// verify the device's signature itself -- trusting only the human root's
/// KEL is not enough to verify anything that root's device ever signed.
pub fn cmd_export_kel(home: &Path) -> Result<String, CliError> {
    cmd_export_kel_result(home).map(|result| result.human)
}
pub(crate) fn cmd_export_kel_result(home: &Path) -> Result<CommandResult, CliError> {
    let identity = load(home)?;
    let bytes = bundle_kels(&identity);
    let hex = hex_encode(&bytes);
    Ok(CommandResult::new(hex.clone()).field(
        "result",
        JsonValue::Object(vec![
            (
                "human_did".into(),
                JsonValue::str(identity.human_did().as_str()),
            ),
            (
                "device_did".into(),
                JsonValue::str(identity.device_did().as_str()),
            ),
            (
                "bundle_id".into(),
                JsonValue::str(blake3::hash(&bytes).to_hex().to_string()),
            ),
            ("hex".into(), JsonValue::str(hex)),
        ]),
    ))
}

fn bundle_kels(identity: &Identity) -> Vec<u8> {
    let mut out = Vec::new();
    for bytes in [
        identity.human.kel().to_bytes(),
        identity.device.kel().to_bytes(),
    ] {
        out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(&bytes);
    }
    out
}

/// Split a [`cmd_export_kel`] bundle back into its two KEL byte strings
/// (human, device).
pub(crate) fn unbundle_kels(bytes: &[u8]) -> Result<(Vec<u8>, Vec<u8>), CliError> {
    let mut off = 0usize;
    let mut take = |b: &[u8]| -> Result<Vec<u8>, CliError> {
        if off + 4 > b.len() {
            return Err(CliError::Usage("truncated KEL bundle".to_string()));
        }
        let len = u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]) as usize;
        off += 4;
        if off + len > b.len() {
            return Err(CliError::Usage("truncated KEL bundle".to_string()));
        }
        let out = b[off..off + len].to_vec();
        off += len;
        Ok(out)
    };
    let human = take(bytes)?;
    let device = take(bytes)?;
    if off != bytes.len() {
        return Err(CliError::Usage("trailing bytes in KEL bundle".to_string()));
    }
    Ok((human, device))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            use std::fmt::Write;
            let _ = write!(out, "{b:02x}");
            out
        })
}

pub(crate) fn hex_decode(s: &str) -> Result<Vec<u8>, CliError> {
    if s.len() % 2 != 0 {
        return Err(CliError::Usage("hex input has odd length".into()));
    }
    let digit = |b: u8| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    };
    s.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = digit(pair[0]).ok_or_else(|| CliError::Usage("invalid hex".into()))?;
            let low = digit(pair[1]).ok_or_else(|| CliError::Usage("invalid hex".into()))?;
            Ok(high * 16 + low)
        })
        .collect()
}

fn fresh_seed_pair() -> Result<SeedPair, CliError> {
    let current = SigningKey::generate().map_err(|e| CliError::Identity(e.to_string()))?;
    let next = SigningKey::generate().map_err(|e| CliError::Identity(e.to_string()))?;
    Ok((
        Zeroizing::new(current.to_seed_bytes()),
        Zeroizing::new(next.to_seed_bytes()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_then_load_reconstructs_the_identical_identity() {
        let dir = tempdir();
        let created = init(&dir).unwrap();
        let human_did = created.human_did();
        let device_did = created.device_did();

        let loaded = load(&dir).unwrap();
        assert_eq!(loaded.human_did(), human_did);
        assert_eq!(loaded.device_did(), device_did);
        // The reconstructed KELs must verify identically, not just share a DID.
        assert_eq!(
            loaded.human.kel().to_bytes(),
            created.human.kel().to_bytes()
        );
        assert_eq!(
            loaded.device.kel().to_bytes(),
            created.device.kel().to_bytes()
        );
    }

    #[test]
    fn init_twice_is_rejected() {
        let dir = tempdir();
        init(&dir).unwrap();
        assert!(matches!(init(&dir), Err(CliError::AlreadyInitialized)));
    }

    #[test]
    fn loading_before_init_fails() {
        let dir = tempdir();
        assert!(matches!(load(&dir), Err(CliError::NotInitialized)));
    }

    #[test]
    fn two_homes_get_different_identities() {
        let dir_a = tempdir();
        let dir_b = tempdir();
        let a = init(&dir_a).unwrap();
        let b = init(&dir_b).unwrap();
        assert_ne!(a.human_did(), b.human_did());
    }

    #[test]
    fn partial_initialization_never_overwrites_an_existing_device() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            device_seed_path(dir.path()),
            b"retained partial initialization",
        )
        .unwrap();
        assert!(matches!(
            init(dir.path()),
            Err(CliError::AlreadyInitialized)
        ));
        assert_eq!(
            fs::read(device_seed_path(dir.path())).unwrap(),
            b"retained partial initialization"
        );
        assert!(!human_seed_path(dir.path()).exists());
    }

    #[cfg(unix)]
    #[test]
    fn secret_files_are_private_and_links_or_widened_permissions_are_refused() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        init(dir.path()).unwrap();
        let path = human_seed_path(dir.path());
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(load(dir.path()), Err(CliError::Identity(_))));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let moved = dir.path().join("retained.seed");
        fs::rename(&path, &moved).unwrap();
        symlink(&moved, &path).unwrap();
        assert!(matches!(load(dir.path()), Err(CliError::CorruptSeedFile)));
        fs::remove_file(&path).unwrap();
        fs::hard_link(&moved, &path).unwrap();
        assert!(matches!(load(dir.path()), Err(CliError::Identity(_))));
    }

    #[test]
    fn concurrent_initializers_publish_only_one_identity() {
        let dir = tempfile::tempdir().unwrap();
        let mut threads = Vec::new();
        for _ in 0..4 {
            let path = dir.path().to_path_buf();
            threads.push(std::thread::spawn(move || init(&path)));
        }
        let mut winner = None;
        for thread in threads {
            match thread.join().unwrap() {
                Ok(identity) => {
                    assert!(winner.is_none());
                    winner = Some(identity.human_did());
                }
                Err(CliError::AlreadyInitialized) => {}
                Err(error) => panic!("unexpected initialization failure: {error}"),
            }
        }
        assert_eq!(winner.unwrap(), load(dir.path()).unwrap().human_did());
    }

    fn tempdir() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "mini-cli-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }
}
