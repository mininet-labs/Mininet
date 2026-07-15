//! Suite-tagged signing/verifying keys and signatures.
//!
//! Keys are derived from a 32-byte seed, so generation is deterministic for tests
//! and reproducible across platforms; production callers use
//! [`SigningKey::generate`], which draws the seed from the operating system's
//! CSPRNG.
//!
//! Per SPEC-01 G1, secret key material is intended to live only on the user's
//! device. This type never serialises its secret half as part of any wire format;
//! the only export path is the explicit, loudly-named
//! [`SigningKey::to_seed_bytes`] for secure on-device storage, and the [`Debug`]
//! impl redacts the secret.
//!
//! ## Post-quantum note (D-0095, issue #15)
//!
//! [`VerifyingKey`] and [`Signature`] can now parse and verify
//! [`SignatureSuite::MlDsa65`] material (FIPS 204, composed via the external
//! `fips204` crate). [`SigningKey`] deliberately stays Ed25519-only — this is
//! Phase 1 of the migration research report's plan (verify-only, no
//! generation, no KEL activation); see `suite.rs`'s module docs.

use ed25519_dalek::{
    Signature as DalekSignature, Signer, SigningKey as DalekSigningKey, Verifier,
    VerifyingKey as DalekVerifyingKey,
};

use zeroize::Zeroize;

use crate::error::{CryptoError, Result};
use crate::suite::SignatureSuite;

/// Suite-specific key material backing a [`VerifyingKey`]. Kept as a private
/// enum rather than exposed directly, so adding a future suite never breaks
/// callers matching on [`SignatureSuite`] instead.
#[derive(Debug, Clone, PartialEq, Eq)]
enum KeyMaterial {
    Ed25519(DalekVerifyingKey),
    /// Raw canonical ML-DSA-65 public-key bytes. Parseability (not just
    /// length) is validated once, at [`VerifyingKey::from_suite_bytes`] —
    /// see that method's doc comment for why bytes are re-parsed at
    /// [`VerifyingKey::verify`] time rather than cached as `fips204`'s own
    /// (non-`Debug`, non-`PartialEq`) key struct.
    MlDsa65(Box<[u8; fips204::ml_dsa_65::PK_LEN]>),
}

/// A public verifying key, tagged with its suite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyingKey {
    suite: SignatureSuite,
    inner: KeyMaterial,
}

/// A secret signing key, tagged with its suite. Secret material stays on-device.
///
/// Ed25519-only today — see this module's doc comment.
#[derive(Clone)]
pub struct SigningKey {
    suite: SignatureSuite,
    inner: DalekSigningKey,
}

/// A signature, tagged with the suite that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    suite: SignatureSuite,
    bytes: Vec<u8>,
}

impl SigningKey {
    /// Deterministically derive a signing key from a 32-byte seed.
    ///
    /// The same seed yields the same key on every platform — used heavily in tests
    /// and anywhere reproducible key derivation is needed.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        SigningKey {
            suite: SignatureSuite::Ed25519,
            inner: DalekSigningKey::from_bytes(seed),
        }
    }

    /// Generate a fresh signing key using operating-system entropy.
    pub fn generate() -> Result<Self> {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed).map_err(|_| CryptoError::Entropy)?;
        let key = SigningKey::from_seed(&seed);
        // Best-effort scrub of the local seed copy.
        seed.zeroize();
        Ok(key)
    }

    /// The suite this key belongs to.
    pub fn suite(&self) -> SignatureSuite {
        self.suite
    }

    /// The corresponding public verifying key.
    pub fn verifying_key(&self) -> VerifyingKey {
        VerifyingKey {
            suite: self.suite,
            inner: KeyMaterial::Ed25519(self.inner.verifying_key()),
        }
    }

    /// Sign a message.
    pub fn sign(&self, message: &[u8]) -> Signature {
        let sig: DalekSignature = self.inner.sign(message);
        Signature {
            suite: self.suite,
            bytes: sig.to_bytes().to_vec(),
        }
    }

    /// Export the 32-byte seed for **secure on-device storage only**.
    ///
    /// This is the single place secret material leaves the type, and it is named
    /// loudly on purpose. It must never be placed in any network message
    /// (SPEC-01 G1: keys never leave the device).
    pub fn to_seed_bytes(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }
}

impl core::fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Never print secret material.
        f.debug_struct("SigningKey")
            .field("suite", &self.suite)
            .field("secret", &"<redacted>")
            .finish()
    }
}

impl VerifyingKey {
    /// The suite this key belongs to.
    pub fn suite(&self) -> SignatureSuite {
        self.suite
    }

    /// Raw public-key bytes (without the suite tag).
    pub fn to_bytes(&self) -> Vec<u8> {
        match &self.inner {
            KeyMaterial::Ed25519(inner) => inner.to_bytes().to_vec(),
            KeyMaterial::MlDsa65(bytes) => bytes.as_slice().to_vec(),
        }
    }

    /// Reconstruct a verifying key from a suite tag and raw bytes.
    ///
    /// For [`SignatureSuite::MlDsa65`], well-formedness is checked here (via
    /// `fips204::ml_dsa_65::PublicKey::try_from_bytes`) rather than only at
    /// [`VerifyingKey::verify`] time — the same fail-fast-at-decode
    /// discipline the Ed25519 arm already has. Note this is a weaker check
    /// than Ed25519's: an ML-DSA-65 public key is packed polynomial
    /// coefficients with no additional validity structure to reject (unlike
    /// a compressed curve point), so `try_from_bytes` only rejects the
    /// wrong *length* here, not a well-formed-but-meaningless key — such a
    /// key still simply never verifies a real signature.
    pub fn from_suite_bytes(suite: SignatureSuite, bytes: &[u8]) -> Result<Self> {
        match suite {
            SignatureSuite::Ed25519 => {
                let arr: [u8; 32] = bytes.try_into().map_err(|_| CryptoError::BadLength {
                    expected: 32,
                    got: bytes.len(),
                })?;
                let inner = DalekVerifyingKey::from_bytes(&arr)
                    .map_err(|_| CryptoError::InvalidPublicKey)?;
                Ok(VerifyingKey {
                    suite,
                    inner: KeyMaterial::Ed25519(inner),
                })
            }
            SignatureSuite::MlDsa65 => {
                use fips204::traits::SerDes;
                let arr: [u8; fips204::ml_dsa_65::PK_LEN] =
                    bytes.try_into().map_err(|_| CryptoError::BadLength {
                        expected: fips204::ml_dsa_65::PK_LEN,
                        got: bytes.len(),
                    })?;
                fips204::ml_dsa_65::PublicKey::try_from_bytes(arr)
                    .map_err(|_| CryptoError::InvalidPublicKey)?;
                Ok(VerifyingKey {
                    suite,
                    inner: KeyMaterial::MlDsa65(Box::new(arr)),
                })
            }
        }
    }

    /// Verify `signature` over `message`. Returns `Err(BadSignature)` on failure,
    /// including a suite mismatch.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<()> {
        if signature.suite != self.suite {
            return Err(CryptoError::BadSignature);
        }
        match &self.inner {
            KeyMaterial::Ed25519(inner) => {
                let sig_bytes: [u8; 64] = signature
                    .bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| CryptoError::BadSignature)?;
                let sig = DalekSignature::from_bytes(&sig_bytes);
                inner
                    .verify(message, &sig)
                    .map_err(|_| CryptoError::BadSignature)
            }
            KeyMaterial::MlDsa65(pk_bytes) => {
                use fips204::traits::{SerDes, Verifier as MlDsaVerifier};
                let pk = fips204::ml_dsa_65::PublicKey::try_from_bytes(**pk_bytes)
                    .map_err(|_| CryptoError::InvalidPublicKey)?;
                let sig_bytes: [u8; fips204::ml_dsa_65::SIG_LEN] = signature
                    .bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| CryptoError::BadSignature)?;
                if pk.verify(message, &sig_bytes, &[]) {
                    Ok(())
                } else {
                    Err(CryptoError::BadSignature)
                }
            }
        }
    }
}

impl Signature {
    /// The suite that produced this signature.
    pub fn suite(&self) -> SignatureSuite {
        self.suite
    }

    /// Raw signature bytes (without the suite tag).
    pub fn to_bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Reconstruct a signature from a suite tag and raw bytes. Validates
    /// only the exact length for `suite` — a signature's cryptographic
    /// validity can only be checked at [`VerifyingKey::verify`] time,
    /// against a real message and key, the same way the previous
    /// Ed25519-only implementation worked.
    pub fn from_suite_bytes(suite: SignatureSuite, bytes: &[u8]) -> Result<Self> {
        let expected = suite.signature_len();
        if bytes.len() != expected {
            return Err(CryptoError::BadLength {
                expected,
                got: bytes.len(),
            });
        }
        Ok(Signature {
            suite,
            bytes: bytes.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ed25519_signature_round_trips_through_sign_and_verify() {
        let signing_key = SigningKey::from_seed(&[1u8; 32]);
        let verifying_key = signing_key.verifying_key();
        let sig = signing_key.sign(b"hello");
        verifying_key.verify(b"hello", &sig).unwrap();
    }

    #[test]
    fn an_ed25519_signature_over_a_different_message_fails() {
        let signing_key = SigningKey::from_seed(&[1u8; 32]);
        let verifying_key = signing_key.verifying_key();
        let sig = signing_key.sign(b"hello");
        assert_eq!(
            verifying_key.verify(b"goodbye", &sig),
            Err(CryptoError::BadSignature)
        );
    }

    #[test]
    fn ml_dsa_65_tag_and_lengths_match_fips204() {
        assert_eq!(SignatureSuite::MlDsa65.tag(), 0x02);
        assert_eq!(
            SignatureSuite::MlDsa65.public_key_len(),
            fips204::ml_dsa_65::PK_LEN
        );
        assert_eq!(
            SignatureSuite::MlDsa65.signature_len(),
            fips204::ml_dsa_65::SIG_LEN
        );
        assert_eq!(
            SignatureSuite::from_tag(0x02).unwrap(),
            SignatureSuite::MlDsa65
        );
    }

    /// The one round-trip test proving `mini_crypto::VerifyingKey`/
    /// `Signature` actually verify a *real* ML-DSA-65 signature produced by
    /// `fips204` itself — not just that the byte-length bookkeeping is
    /// self-consistent. Uses `fips204`'s own `try_keygen_with_rng`/
    /// `try_sign_with_rng` (available only under this crate's
    /// `dev-dependencies`-only `default-rng` feature — see Cargo.toml) since
    /// `mini_crypto::SigningKey` does not expose ML-DSA generation (Phase 1
    /// is verify-only, per this module's doc comment).
    #[test]
    fn a_real_ml_dsa_65_signature_verifies_through_the_mini_crypto_wrapper() {
        use fips204::ml_dsa_65;
        use fips204::traits::{SerDes, Signer as MlDsaSigner};
        use rand_core::OsRng;

        let (pk, sk) = ml_dsa_65::try_keygen_with_rng(&mut OsRng).unwrap();
        let message = b"a real ML-DSA-65 message";
        let sig_bytes = sk.try_sign_with_rng(&mut OsRng, message, &[]).unwrap();

        let verifying_key =
            VerifyingKey::from_suite_bytes(SignatureSuite::MlDsa65, &pk.clone().into_bytes())
                .unwrap();
        let signature = Signature::from_suite_bytes(SignatureSuite::MlDsa65, &sig_bytes).unwrap();

        verifying_key.verify(message, &signature).unwrap();
    }

    #[test]
    fn a_tampered_ml_dsa_65_signature_is_rejected() {
        use fips204::ml_dsa_65;
        use fips204::traits::{SerDes, Signer as MlDsaSigner};
        use rand_core::OsRng;

        let (pk, sk) = ml_dsa_65::try_keygen_with_rng(&mut OsRng).unwrap();
        let message = b"a real ML-DSA-65 message";
        let mut sig_bytes = sk
            .try_sign_with_rng(&mut OsRng, message, &[])
            .unwrap()
            .to_vec();
        sig_bytes[0] ^= 0xFF;

        let verifying_key =
            VerifyingKey::from_suite_bytes(SignatureSuite::MlDsa65, &pk.into_bytes()).unwrap();
        let signature = Signature::from_suite_bytes(SignatureSuite::MlDsa65, &sig_bytes).unwrap();

        assert_eq!(
            verifying_key.verify(message, &signature),
            Err(CryptoError::BadSignature)
        );
    }

    #[test]
    fn an_ml_dsa_65_signature_verified_under_the_wrong_public_key_is_rejected() {
        use fips204::ml_dsa_65;
        use fips204::traits::{SerDes, Signer as MlDsaSigner};
        use rand_core::OsRng;

        let (_pk_a, sk_a) = ml_dsa_65::try_keygen_with_rng(&mut OsRng).unwrap();
        let (pk_b, _sk_b) = ml_dsa_65::try_keygen_with_rng(&mut OsRng).unwrap();
        let message = b"a real ML-DSA-65 message";
        let sig_bytes = sk_a.try_sign_with_rng(&mut OsRng, message, &[]).unwrap();

        let verifying_key_b =
            VerifyingKey::from_suite_bytes(SignatureSuite::MlDsa65, &pk_b.into_bytes()).unwrap();
        let signature = Signature::from_suite_bytes(SignatureSuite::MlDsa65, &sig_bytes).unwrap();

        assert_eq!(
            verifying_key_b.verify(message, &signature),
            Err(CryptoError::BadSignature)
        );
    }

    #[test]
    fn a_wrong_length_ml_dsa_65_public_key_is_rejected_before_any_verification() {
        let too_short = vec![0u8; fips204::ml_dsa_65::PK_LEN - 1];
        assert_eq!(
            VerifyingKey::from_suite_bytes(SignatureSuite::MlDsa65, &too_short),
            Err(CryptoError::BadLength {
                expected: fips204::ml_dsa_65::PK_LEN,
                got: too_short.len(),
            })
        );
    }

    #[test]
    fn an_all_zero_ml_dsa_65_key_parses_but_never_verifies_a_real_signature() {
        // Unlike Ed25519 (a compressed curve point, which can be a malformed
        // non-point), an ML-DSA-65 public key is just packed polynomial
        // coefficients with no extra range check beyond the fixed-width
        // encoding itself — `fips204` accepts any correctly-sized byte
        // string, including all-zero, as a *structurally* valid key. This
        // crate cannot add a stronger check without inventing its own
        // validity criterion outside FIPS 204, so the honest boundary is:
        // parsing succeeds, but the "key" corresponds to no real signer, so
        // it never verifies a real signature over any message.
        use fips204::ml_dsa_65;
        use fips204::traits::Signer as MlDsaSigner;
        use rand_core::OsRng;

        let all_zero = [0u8; fips204::ml_dsa_65::PK_LEN];
        let verifying_key =
            VerifyingKey::from_suite_bytes(SignatureSuite::MlDsa65, &all_zero).unwrap();

        let (_pk, sk) = ml_dsa_65::try_keygen_with_rng(&mut OsRng).unwrap();
        let message = b"a real ML-DSA-65 message";
        let sig_bytes = sk.try_sign_with_rng(&mut OsRng, message, &[]).unwrap();
        let signature = Signature::from_suite_bytes(SignatureSuite::MlDsa65, &sig_bytes).unwrap();

        assert_eq!(
            verifying_key.verify(message, &signature),
            Err(CryptoError::BadSignature)
        );
    }

    #[test]
    fn a_wrong_length_ml_dsa_65_signature_is_rejected_before_any_verification() {
        let too_short = vec![0u8; fips204::ml_dsa_65::SIG_LEN - 1];
        assert_eq!(
            Signature::from_suite_bytes(SignatureSuite::MlDsa65, &too_short),
            Err(CryptoError::BadLength {
                expected: fips204::ml_dsa_65::SIG_LEN,
                got: too_short.len(),
            })
        );
    }

    #[test]
    fn an_ed25519_key_cannot_be_parsed_as_ml_dsa_65_bytes() {
        let signing_key = SigningKey::from_seed(&[1u8; 32]);
        let ed25519_pub_bytes = signing_key.verifying_key().to_bytes();
        assert_eq!(
            VerifyingKey::from_suite_bytes(SignatureSuite::MlDsa65, &ed25519_pub_bytes),
            Err(CryptoError::BadLength {
                expected: fips204::ml_dsa_65::PK_LEN,
                got: ed25519_pub_bytes.len(),
            })
        );
    }

    #[test]
    fn a_signature_suite_mismatch_between_key_and_signature_is_rejected() {
        use fips204::ml_dsa_65;
        use fips204::traits::Signer as MlDsaSigner;
        use rand_core::OsRng;

        let ed_signing_key = SigningKey::from_seed(&[1u8; 32]);
        let ed_verifying_key = ed_signing_key.verifying_key();

        let (_pk, sk) = ml_dsa_65::try_keygen_with_rng(&mut OsRng).unwrap();
        let pq_sig_bytes = sk.try_sign_with_rng(&mut OsRng, b"hello", &[]).unwrap();
        let pq_signature =
            Signature::from_suite_bytes(SignatureSuite::MlDsa65, &pq_sig_bytes).unwrap();

        // An Ed25519 key must never accept a suite-mismatched signature,
        // regardless of byte content.
        assert_eq!(
            ed_verifying_key.verify(b"hello", &pq_signature),
            Err(CryptoError::BadSignature)
        );
    }
}
