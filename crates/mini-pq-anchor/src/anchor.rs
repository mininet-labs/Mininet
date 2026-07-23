//! A pre-provisioned, dormant PQ anchor -- a wallet-held ML-DSA-65 keypair
//! that exists only so it is available *before* a live classical-signature
//! break, not something that repairs a break after the fact.
//!
//! PR #220's research proposal §4.2 (Frontier Trust 10, roadmap issue
//! #231): pre-provisioning is the one PQ migration piece genuinely
//! buildable today without waiting on a live-break scenario. Everything
//! this module produces stays a [`PqAnchorRecord`] the wallet holds
//! locally -- **never** committed into a `did-mini` KEL, never attested,
//! never relied upon by any other crate. KEL activation is Phase 3 of
//! `docs/design/post-quantum-identity-migration.md`, `did-mini`'s work,
//! explicitly not started, and gated on external cryptographic review
//! before any production identity use. This crate must never be read as
//! claiming otherwise: an identity with **no** pre-break anchor on record
//! anywhere still has no path back after a live break (PQ recovery Class
//! C, PR #220 §4) -- pre-provisioning only helps identities that actually
//! do this *before* the break.

use did_mini::Did;
use mini_crypto::{SignatureSuite, SigningKey, VerifyingKey};

use crate::error::{PqAnchorError, Result};

/// Hard limit on [`PqAnchorRecord::label`], the same defensive-decoding
/// discipline every untrusted/UI-facing string in this workspace applies.
pub const MAX_LABEL_BYTES: usize = 256;

/// A dormant PQ anchor a wallet holds in reserve. This crate can only ever
/// produce the [`AnchorStatus::Provisioned`] variant -- there is
/// deliberately no "committed"/"active" status, because committing an
/// anchor into an identity's KEL is separate, gated, unbuilt work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AnchorStatus {
    /// Generated and held by the wallet. Not in any KEL, not attested by
    /// anything, not relied upon by any other crate. The only status this
    /// crate's own code can ever construct.
    Provisioned,
}

/// One pre-provisioned PQ anchor: its public key, who it was provisioned
/// for, when, and under what human-chosen label. The matching
/// [`mini_crypto::SigningKey`] secret is returned once, at
/// [`provision_anchor`] time, and never stored by this crate -- per
/// SPEC-01 G1 (secret key material lives on-device only) and per
/// `mini-crypto`'s own Phase 2 boundary: an `MlDsa65` `SigningKey` has no
/// storage export/import path yet (`docs/design/
/// post-quantum-identity-migration.md`, "Honest limit"), so *how* a
/// caller actually persists this secret across restarts is real,
/// separate, unbuilt work this crate does not paper over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PqAnchorRecord {
    /// The identity root this anchor was provisioned for. Carries no
    /// authority by itself -- ownership here is bookkeeping, not a KEL
    /// commitment.
    pub owner: Did,
    /// Human-chosen label for wallet UI (e.g. "Primary PQ anchor",
    /// "Backup anchor -- safe deposit box"). No protocol meaning.
    pub label: String,
    /// The ML-DSA-65 public key. Never a secret.
    pub public_key: VerifyingKey,
    /// Device-clock time this anchor was generated.
    pub generated_at_ms: u64,
    pub status: AnchorStatus,
}

impl PqAnchorRecord {
    /// Structural well-formedness only -- never a judgment about whether
    /// this anchor is trustworthy, backed up, or actually recoverable.
    pub fn check_wellformed(&self) -> Result<()> {
        if self.label.len() > MAX_LABEL_BYTES {
            return Err(PqAnchorError::LabelTooLong);
        }
        if self.public_key.suite() != SignatureSuite::MlDsa65 {
            return Err(PqAnchorError::NotMlDsa65);
        }
        Ok(())
    }

    /// A short, display-friendly fingerprint of the public key (first 8
    /// bytes of its BLAKE3-256 digest, hex-encoded) -- for wallet UI to
    /// show alongside `label` without printing the full 1952-byte public
    /// key. Not a content address, not used by any other crate; purely a
    /// UI convenience.
    pub fn short_fingerprint(&self) -> String {
        let digest = mini_crypto::hash::blake3_256(&self.public_key.to_bytes());
        digest[..8].iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// Generate a fresh, dormant ML-DSA-65 keypair and wrap its public half in
/// a [`PqAnchorRecord`]. Returns the [`SigningKey`] alongside the record so
/// the caller can hand the secret to whatever real secure on-device
/// storage exists (this crate never stores or serializes it itself).
///
/// This is provisioning only: the returned record is never committed to
/// any KEL, never gossiped, never attested. It exists purely so that, if a
/// PQ break ever happens, an identity that called this *beforehand* has an
/// unbroken anchor on record to build a real migration on top of --
/// something the emergency migration procedure itself (roadmap issue
/// #230, not this one) would still need to define end to end.
pub fn provision_anchor(
    owner: Did,
    label: impl Into<String>,
    generated_at_ms: u64,
) -> Result<(SigningKey, PqAnchorRecord)> {
    let label = label.into();
    if label.len() > MAX_LABEL_BYTES {
        return Err(PqAnchorError::LabelTooLong);
    }

    let signing_key = SigningKey::generate_ml_dsa_65().map_err(|_| PqAnchorError::NotMlDsa65)?;
    let public_key = signing_key.verifying_key();

    let record = PqAnchorRecord {
        owner,
        label,
        public_key,
        generated_at_ms,
        status: AnchorStatus::Provisioned,
    };
    Ok((signing_key, record))
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;

    fn owner() -> Did {
        Controller::incept_single().unwrap().did()
    }

    #[test]
    fn provisioning_produces_a_well_formed_mldsa65_record() {
        let (signing_key, record) = provision_anchor(owner(), "Primary anchor", 1_000).unwrap();
        assert_eq!(signing_key.suite(), SignatureSuite::MlDsa65);
        assert_eq!(record.public_key.suite(), SignatureSuite::MlDsa65);
        assert_eq!(record.status, AnchorStatus::Provisioned);
        assert_eq!(record.generated_at_ms, 1_000);
        record.check_wellformed().unwrap();
    }

    #[test]
    fn the_returned_signing_key_actually_matches_the_records_public_key() {
        let (signing_key, record) = provision_anchor(owner(), "Primary anchor", 1_000).unwrap();
        let message = b"a real message this key should be able to sign";
        let sig = signing_key.sign_ml_dsa_65(message).unwrap();
        assert!(record.public_key.verify(message, &sig).is_ok());
    }

    #[test]
    fn an_oversized_label_is_rejected_before_any_key_generation() {
        let too_long = "x".repeat(MAX_LABEL_BYTES + 1);
        assert_eq!(
            provision_anchor(owner(), too_long, 0).unwrap_err(),
            PqAnchorError::LabelTooLong
        );
    }

    #[test]
    fn two_provisioned_anchors_for_the_same_owner_have_different_keys() {
        let owner = owner();
        let (_, a) = provision_anchor(owner.clone(), "a", 0).unwrap();
        let (_, b) = provision_anchor(owner, "b", 0).unwrap();
        assert_ne!(a.public_key, b.public_key);
        assert_ne!(a.short_fingerprint(), b.short_fingerprint());
    }

    #[test]
    fn short_fingerprint_is_stable_for_the_same_key() {
        let (_, record) = provision_anchor(owner(), "a", 0).unwrap();
        assert_eq!(record.short_fingerprint(), record.short_fingerprint());
        assert_eq!(record.short_fingerprint().len(), 16);
    }
}
