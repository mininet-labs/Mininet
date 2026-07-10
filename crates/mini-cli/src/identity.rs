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

use std::fs;
use std::path::{Path, PathBuf};

use did_mini::{Capabilities, Controller, Did};
use mini_crypto::SigningKey;

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

fn write_seed_file(path: &Path, current: [u8; 32], next: [u8; 32]) -> Result<(), CliError> {
    let mut bytes = Vec::with_capacity(SEED_FILE_LEN);
    bytes.extend_from_slice(&current);
    bytes.extend_from_slice(&next);
    fs::write(path, &bytes).map_err(|e| CliError::Io(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(path, perms).map_err(|e| CliError::Io(e.to_string()))?;
    }
    Ok(())
}

fn read_seed_file(path: &Path) -> Result<([u8; 32], [u8; 32]), CliError> {
    let bytes = fs::read(path).map_err(|_| CliError::NotInitialized)?;
    if bytes.len() != SEED_FILE_LEN {
        return Err(CliError::CorruptSeedFile);
    }
    let mut current = [0u8; 32];
    let mut next = [0u8; 32];
    current.copy_from_slice(&bytes[..32]);
    next.copy_from_slice(&bytes[32..]);
    Ok((current, next))
}

/// Create a fresh human root + delegated device identity under `home`.
/// Errors if one already exists — use [`load`] to reuse it.
pub fn init(home: &Path) -> Result<Identity, CliError> {
    fs::create_dir_all(home).map_err(|e| CliError::Io(e.to_string()))?;
    if human_seed_path(home).exists() {
        return Err(CliError::AlreadyInitialized);
    }

    let (h_current, h_next) = fresh_seed_pair()?;
    let human = Controller::incept_single_from_seeds(&h_current, &h_next)
        .map_err(|e| CliError::Identity(e.to_string()))?;
    write_seed_file(&human_seed_path(home), h_current, h_next)?;

    let (d_current, d_next) = fresh_seed_pair()?;
    let mut human = human;
    let device = Controller::incept_device_single_from_seeds(&human.did(), &d_current, &d_next)
        .map_err(|e| CliError::Identity(e.to_string()))?;
    write_seed_file(&device_seed_path(home), d_current, d_next)?;
    human
        .delegate_device(&device.did(), Capabilities::primary())
        .map_err(|e| CliError::Identity(e.to_string()))?;

    Ok(Identity { human, device })
}

/// Reconstruct the identity at `home` from its saved seeds, replaying the
/// same deterministic inception + delegation sequence [`init`] performed.
pub fn load(home: &Path) -> Result<Identity, CliError> {
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
    let identity = init(home)?;
    Ok(format!(
        "identity created\n  human:  {}\n  device: {}",
        identity.human_did().as_str(),
        identity.device_did().as_str()
    ))
}

/// `mini identity show`
pub fn cmd_show(home: &Path) -> Result<String, CliError> {
    let identity = load(home)?;
    Ok(format!(
        "human:  {}\ndevice: {}",
        identity.human_did().as_str(),
        identity.device_did().as_str()
    ))
}

/// `mini kel export` — print this home's human **and** device KELs as
/// hex, for another `mini` home to `mini kel trust` (see
/// `crate::store::trust_kel`). Both are required: provenance verification
/// (`mini_objects::verify_provenance`) checks the signing device's
/// delegation against the root's KEL *and* needs the device's own KEL to
/// verify the device's signature itself -- trusting only the human root's
/// KEL is not enough to verify anything that root's device ever signed.
pub fn cmd_export_kel(home: &Path) -> Result<String, CliError> {
    let identity = load(home)?;
    Ok(hex_encode(&bundle_kels(&identity)))
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
        return Err(CliError::Usage("hex input has odd length".to_string()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|_| CliError::Usage("invalid hex".to_string()))
        })
        .collect()
}

fn fresh_seed_pair() -> Result<([u8; 32], [u8; 32]), CliError> {
    let current = SigningKey::generate().map_err(|e| CliError::Identity(e.to_string()))?;
    let next = SigningKey::generate().map_err(|e| CliError::Identity(e.to_string()))?;
    Ok((current.to_seed_bytes(), next.to_seed_bytes()))
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
