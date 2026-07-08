//! A single, canonical source of secure randomness for non-key material
//! (nonces, session salts) — the same OS CSPRNG [`crate::keys::SigningKey::generate`]
//! already uses for key generation.
//!
//! Every crate in this tree that needs a fresh nonce (`mini-presence`,
//! `mini-storage`, and future callers) should call [`random_32`] rather than
//! rolling its own randomness source. Centralizing it here means an audit of
//! this one function is an audit of every nonce in the workspace.

use crate::error::{CryptoError, Result};

/// 32 bytes of operating-system CSPRNG output. **Never hardcode this value's
/// output** — the entire point of a nonce is that it is unpredictable. Tests
/// throughout this workspace deliberately use fixed byte arrays instead of
/// this function, precisely so tests are deterministic and reproducible;
/// that convention must never leak into non-test code, where a predictable
/// nonce defeats the replay resistance it exists to provide.
pub fn random_32() -> Result<[u8; 32]> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|_| CryptoError::Entropy)?;
    Ok(bytes)
}
