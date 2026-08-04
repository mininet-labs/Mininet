//! Sealed-box owner-only encryption (D-0434): produces and consumes the
//! bytes that go inside [`mini_objects::Payload::Encrypted`].
//!
//! This is the NaCl/libsodium `crypto_box_seal` construction, composed
//! entirely from primitives `mini-crypto` already exposes for other
//! purposes: an ephemeral X25519 key agreed with the recipient's sealing
//! public key, HKDF-SHA256 to derive a ChaCha20-Poly1305 key, and the
//! ephemeral public key carried alongside the ciphertext so the recipient
//! can redo the same agreement. See
//! `docs/design/cold-storage-and-owner-only-encryption.md` for the full
//! rationale, including why the sealing keypair is independent of a
//! device's Ed25519 KEL signing key rather than derived from it.

use mini_crypto::{
    AeadNonce, AeadSuite, AgreementPublicKey, AgreementSecretKey, KdfSuite, KeyAgreementSuite,
};
use mini_objects::MAX_PAYLOAD_BYTES;

use crate::{Result, StoreError};

const SEAL_INFO: &[u8] = b"mini-store/owner-seal/v1";
const AEAD_SUITE: AeadSuite = AeadSuite::ChaCha20Poly1305;
const KDF_SUITE: KdfSuite = KdfSuite::HkdfSha256;

const PUBLIC_KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;
/// ChaCha20-Poly1305 appends a 16-byte authentication tag to every ciphertext.
const AEAD_TAG_LEN: usize = 16;

/// The smallest a sealed-box byte string can legally be: an ephemeral public
/// key, a nonce, and an empty plaintext's authentication tag.
const MIN_SEALED_LEN: usize = PUBLIC_KEY_LEN + NONCE_LEN + AEAD_TAG_LEN;

/// Largest plaintext that can still produce a legal `Payload::Encrypted`.
/// Framing and the AEAD tag must fit inside the object's payload ceiling too.
pub const MAX_OWNER_SEAL_PLAINTEXT_BYTES: usize =
    MAX_PAYLOAD_BYTES - PUBLIC_KEY_LEN - NONCE_LEN - AEAD_TAG_LEN;

/// A sealed box is itself the encrypted payload, so its complete framing must
/// fit inside the object format's payload ceiling.
const MAX_SEALED_LEN: usize = MAX_PAYLOAD_BYTES;

/// An owner's private sealing key: the secret half of an independent X25519
/// keypair used only for [`open_as_owner`]. Deliberately **not** a device's
/// Ed25519 signing/KEL key -- see the design doc's "why a separate
/// keypair" section. The caller is responsible for this key's own secure
/// storage, exactly as it already is for signing-key seeds.
#[derive(Clone)]
pub struct OwnerSealingKey(AgreementSecretKey);

/// The public half of an [`OwnerSealingKey`], shared with anyone who should
/// be able to seal content for its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OwnerSealingPublicKey(AgreementPublicKey);

impl OwnerSealingKey {
    /// Generate a fresh sealing keypair using operating-system entropy.
    pub fn generate() -> Result<Self> {
        Ok(OwnerSealingKey(
            AgreementSecretKey::generate().map_err(StoreError::Crypto)?,
        ))
    }

    /// Deterministically derive a sealing key from a 32-byte seed. The
    /// caller is responsible for that seed's own secure storage.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        OwnerSealingKey(AgreementSecretKey::from_seed(seed))
    }

    /// The public key to hand out to anyone sealing content for this owner.
    pub fn public_key(&self) -> OwnerSealingPublicKey {
        OwnerSealingPublicKey(self.0.public_key())
    }
}

impl core::fmt::Debug for OwnerSealingKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OwnerSealingKey")
            .field("public", &self.public_key())
            .finish()
    }
}

impl OwnerSealingPublicKey {
    /// Raw 32-byte X25519 public-key bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// Reconstruct from raw X25519 public-key bytes.
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self> {
        let key = AgreementPublicKey::from_suite_bytes(KeyAgreementSuite::X25519, bytes)
            .map_err(StoreError::Crypto)?;
        Ok(OwnerSealingPublicKey(key))
    }
}

/// Seal `plaintext` so only the holder of `recipient`'s matching
/// [`OwnerSealingKey`] can recover it. Returns the exact bytes to place in
/// [`mini_objects::Payload::Encrypted`].
///
/// A fresh ephemeral key and nonce are generated per call, so sealing the
/// same plaintext to the same recipient twice never produces the same
/// ciphertext bytes.
pub fn seal_for_owner(
    recipient: &OwnerSealingPublicKey,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>> {
    if plaintext.len() > MAX_OWNER_SEAL_PLAINTEXT_BYTES {
        return Err(StoreError::LimitExceeded);
    }
    let ephemeral = AgreementSecretKey::generate().map_err(StoreError::Crypto)?;
    let shared = ephemeral.agree(&recipient.0).map_err(StoreError::Crypto)?;
    let key = KDF_SUITE
        .derive_aead_key_from_shared(None, &shared, SEAL_INFO, AEAD_SUITE)
        .map_err(StoreError::Crypto)?;
    let nonce = AeadNonce::generate().map_err(StoreError::Crypto)?;
    let ciphertext = key
        .encrypt(&nonce, plaintext, aad)
        .map_err(StoreError::Crypto)?;

    let mut sealed = Vec::with_capacity(PUBLIC_KEY_LEN + NONCE_LEN + ciphertext.len());
    sealed.extend_from_slice(&ephemeral.public_key().to_bytes());
    sealed.extend_from_slice(nonce.as_bytes());
    sealed.extend_from_slice(&ciphertext);
    Ok(sealed)
}

/// Open bytes produced by [`seal_for_owner`] using the matching
/// [`OwnerSealingKey`]. Fails closed on truncated/oversized input, a
/// mismatched key, or any tampering with the ephemeral public key, nonce,
/// ciphertext, or `aad`.
pub fn open_as_owner(owner: &OwnerSealingKey, sealed: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    if sealed.len() < MIN_SEALED_LEN || sealed.len() > MAX_SEALED_LEN {
        return Err(StoreError::Corrupt);
    }
    let (ephemeral_public_bytes, rest) = sealed.split_at(PUBLIC_KEY_LEN);
    let (nonce_bytes, ciphertext) = rest.split_at(NONCE_LEN);

    let ephemeral_public =
        AgreementPublicKey::from_suite_bytes(KeyAgreementSuite::X25519, ephemeral_public_bytes)
            .map_err(StoreError::Crypto)?;
    let nonce = AeadNonce::from_bytes(nonce_bytes).map_err(StoreError::Crypto)?;

    let shared = owner
        .0
        .agree(&ephemeral_public)
        .map_err(StoreError::Crypto)?;
    let key = KDF_SUITE
        .derive_aead_key_from_shared(None, &shared, SEAL_INFO, AEAD_SUITE)
        .map_err(StoreError::Crypto)?;
    key.decrypt(&nonce, ciphertext, aad)
        .map_err(StoreError::Crypto)
}
