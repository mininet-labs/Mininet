//! Narrow Windows DPAPI boundary for identity seed envelopes.
//!
//! The vault protects only the current and pre-rotation seed bytes. It does
//! not attempt to make a compromised Windows account safe: DPAPI normally
//! protects data for the current user profile, not against malware running as
//! that user. The rest of Mininet never receives a plaintext file path or
//! implements its own key wrapping.

#![cfg_attr(not(windows), forbid(unsafe_code))]
#![warn(missing_debug_implementations)]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const VERSION: u8 = 1;
const SEED_BYTES: usize = 64;

/// A current/pre-rotation seed pair suitable for `did-mini` incept-from-seeds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedPair {
    /// Current signing seed.
    pub current: [u8; 32],
    /// Pre-rotation next signing seed.
    pub next: [u8; 32],
}

/// Vault operation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultError {
    /// The platform does not provide this protection boundary.
    UnsupportedPlatform,
    /// Filesystem failure.
    Io(String),
    /// DPAPI rejected or failed to protect the envelope.
    ProtectionFailed,
    /// The protected payload had an invalid version or length.
    InvalidEnvelope,
    /// OS randomness failed.
    Entropy,
}

impl core::fmt::Display for VaultError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedPlatform => write!(f, "Windows DPAPI is unavailable on this platform"),
            Self::Io(error) => write!(f, "vault i/o: {error}"),
            Self::ProtectionFailed => write!(f, "Windows user protection failed"),
            Self::InvalidEnvelope => write!(f, "invalid protected identity envelope"),
            Self::Entropy => write!(f, "OS entropy unavailable"),
        }
    }
}

impl std::error::Error for VaultError {}

/// Load an existing protected seed pair, or create and protect a new one.
pub fn load_or_create(path: &Path) -> Result<SeedPair, VaultError> {
    if path.exists() {
        return load_existing(path);
    }
    let pair = SeedPair {
        current: mini_crypto::random_32().map_err(|_| VaultError::Entropy)?,
        next: mini_crypto::random_32().map_err(|_| VaultError::Entropy)?,
    };
    let protected = protect(&encode(&pair)?)?;
    atomic_write(path, &protected)?;
    Ok(pair)
}

/// Load an existing protected seed pair without creating a new root.
pub fn load_existing(path: &Path) -> Result<SeedPair, VaultError> {
    let protected = fs::read(path).map_err(|error| VaultError::Io(error.to_string()))?;
    decode(&unprotect(&protected)?)
}

/// Load an arbitrary small user setting protected by the same DPAPI boundary.
/// Missing files are reported as ordinary I/O errors so callers can choose a
/// documented default without confusing absence with successful decryption.
pub fn load_user_data(path: &Path) -> Result<Vec<u8>, VaultError> {
    let protected = fs::read(path).map_err(|error| VaultError::Io(error.to_string()))?;
    unprotect(&protected)
}

/// Atomically write arbitrary small user settings protected for the current
/// Windows user. Plaintext is never written to `path`.
pub fn save_user_data(path: &Path, plaintext: &[u8]) -> Result<(), VaultError> {
    atomic_write(path, &protect(plaintext)?)
}

fn encode(pair: &SeedPair) -> Result<Vec<u8>, VaultError> {
    let mut bytes = Vec::with_capacity(1 + SEED_BYTES);
    bytes.push(VERSION);
    bytes.extend_from_slice(&pair.current);
    bytes.extend_from_slice(&pair.next);
    Ok(bytes)
}

fn decode(bytes: &[u8]) -> Result<SeedPair, VaultError> {
    if bytes.len() != 1 + SEED_BYTES || bytes[0] != VERSION {
        return Err(VaultError::InvalidEnvelope);
    }
    let mut current = [0u8; 32];
    let mut next = [0u8; 32];
    current.copy_from_slice(&bytes[1..33]);
    next.copy_from_slice(&bytes[33..65]);
    Ok(SeedPair { current, next })
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), VaultError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| VaultError::Io(error.to_string()))?;
    }
    let mut temporary = PathBuf::from(path);
    temporary.set_extension("tmp");
    let result = (|| {
        let mut file =
            fs::File::create(&temporary).map_err(|error| VaultError::Io(error.to_string()))?;
        file.write_all(bytes)
            .map_err(|error| VaultError::Io(error.to_string()))?;
        file.sync_all()
            .map_err(|error| VaultError::Io(error.to_string()))?;
        fs::rename(&temporary, path).map_err(|error| VaultError::Io(error.to_string()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
fn protect(plaintext: &[u8]) -> Result<Vec<u8>, VaultError> {
    use std::ptr::null;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: plaintext.len() as u32,
        pbData: plaintext.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &input,
            null(),
            null(),
            null(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 || output.pbData.is_null() {
        return Err(VaultError::ProtectionFailed);
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        windows_sys::Win32::Foundation::LocalFree(output.pbData as *mut core::ffi::c_void);
    }
    Ok(bytes)
}

#[cfg(not(windows))]
fn protect(_plaintext: &[u8]) -> Result<Vec<u8>, VaultError> {
    Err(VaultError::UnsupportedPlatform)
}

#[cfg(windows)]
fn unprotect(ciphertext: &[u8]) -> Result<Vec<u8>, VaultError> {
    use std::ptr::null;
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: ciphertext.len() as u32,
        pbData: ciphertext.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            null(),
            null(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 || output.pbData.is_null() {
        return Err(VaultError::ProtectionFailed);
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        windows_sys::Win32::Foundation::LocalFree(output.pbData as *mut core::ffi::c_void);
    }
    Ok(bytes)
}

#[cfg(not(windows))]
fn unprotect(_ciphertext: &[u8]) -> Result<Vec<u8>, VaultError> {
    Err(VaultError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::{decode, encode, SeedPair};

    #[test]
    fn envelope_is_versioned_and_exactly_sized() {
        let pair = SeedPair {
            current: [1; 32],
            next: [2; 32],
        };
        assert_eq!(decode(&encode(&pair).unwrap()).unwrap(), pair);
        assert!(decode(&[1; 64]).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn windows_dpapi_round_trip_reloads_the_same_seed_pair() {
        let root = std::env::temp_dir().join(format!(
            "mininet-vault-test-{}-{}",
            std::process::id(),
            super::VERSION
        ));
        let path = root.join("identity.dpapi");
        let first = super::load_or_create(&path).unwrap();
        let second = super::load_or_create(&path).unwrap();
        assert_eq!(first, second);
        let _ = std::fs::remove_dir_all(root);
    }
}
