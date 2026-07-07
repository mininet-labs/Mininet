//! Identity-root verification oracle: the forge never trusts an object's claimed
//! author — it re-derives provenance against a trusted directory of KELs at the
//! moment a decision is made.
//!
//! **Boundary (SPEC-01 vs SPEC-02):** this proves a `did:mini` **identity root**
//! signed, with valid device delegation. It does **not** prove unique
//! personhood — one person can create many roots until SPEC-02 exists. So every
//! quorum built on this oracle counts *distinct verified identity roots*, not
//! *humans*. When personhood lands, a `PersonhoodOracle` will wrap this one and
//! future `PersonhoodOracle` quorum APIs will require it; nothing here should be described as
//! human verification until then.
//!
//! Objects reach a store either through `mini-sync`'s verified ingest (already
//! provenance-checked) *or* by any other means. Governance and release
//! decisions are too important to assume the former, so every attestation,
//! approval, PR, chain entry, and release counted here must re-pass
//! `verify_provenance` against KELs the caller vouches for — exactly the KEL
//! carriers sync absorbed. An object whose author's KELs are unknown, or whose
//! signature/delegation no longer holds, simply does not count (the same
//! "unknown author is rejected, not quarantined" posture as sync).

use std::collections::BTreeMap;

use did_mini::{Did, Kel};

use crate::ForgeError;
use mini_objects::{verify_provenance, Object};

/// Supplies verified KELs by DID (identity roots and devices alike).
pub trait IdentityOracle {
    /// The verified KEL for `did`, if this oracle vouches for it.
    fn kel(&self, did: &Did) -> Option<&Kel>;
}

/// A simple in-memory directory of trusted KELs. Populate it from your own
/// identities plus the KEL carriers received over verified sync.
#[derive(Debug, Default)]
pub struct KelDirectory {
    kels: BTreeMap<String, Kel>,
}

impl KelDirectory {
    /// An empty directory.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a KEL, verifying it first (self-certifying: SCID re-derivation,
    /// hash chain, pre-rotation). A new log for a known scid may only *extend*
    /// the existing one — a conflicting fork is refused rather than silently
    /// replacing prior state (duplicity surfaces to witnesses later, SPEC-01).
    pub fn try_insert_verified(&mut self, kel: Kel) -> Result<(), ForgeError> {
        kel.verify().map_err(ForgeError::Identity)?;
        let scid = kel.scid().to_string();
        match self.kels.get(&scid) {
            None => {
                self.kels.insert(scid, kel);
                Ok(())
            }
            Some(existing) => {
                let old = existing.events();
                let new = kel.events();
                if new.len() >= old.len() && new[..old.len()] == *old {
                    self.kels.insert(scid, kel);
                    Ok(())
                } else {
                    Err(ForgeError::BadObject) // conflicting history
                }
            }
        }
    }

    /// Add a KEL the caller has ALREADY verified (e.g. a verified sync carrier
    /// or one's own identity). Prefer [`try_insert_verified`] for anything from
    /// an untrusted source.
    pub fn insert(&mut self, kel: Kel) {
        self.kels.insert(kel.scid().to_string(), kel);
    }

    /// Number of directory entries.
    pub fn len(&self) -> usize {
        self.kels.len()
    }

    /// Whether the directory is empty.
    pub fn is_empty(&self) -> bool {
        self.kels.is_empty()
    }
}

impl IdentityOracle for KelDirectory {
    fn kel(&self, did: &Did) -> Option<&Kel> {
        self.kels.get(did.scid())
    }
}

/// Whether `obj`'s author is vouched for by the oracle AND `obj` re-passes full
/// provenance (delegated, unrevoked, capability-scoped) right now.
pub(crate) fn author_verified(oracle: &dyn IdentityOracle, obj: &Object) -> bool {
    let root = match oracle.kel(&obj.author_human) {
        Some(k) => k,
        None => return false,
    };
    let device = match oracle.kel(&obj.author_device) {
        Some(k) => k,
        None => return false,
    };
    verify_provenance(obj, root, device).is_ok()
}
