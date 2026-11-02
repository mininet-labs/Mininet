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

use did_mini::{assess_kel_assurance, Did, FreshnessPins, Kel, KelAssurance, WitnessEvidence};

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

/// An opt-in personhood/eligibility gate for any quorum or decision built on an
/// [`IdentityOracle`]: it vouches only for identity roots (and the devices
/// delegated by them) that `eligible` accepts, so an object authored by any
/// other root fails [`author_verified`] and simply does not count — the same
/// "unknown author is rejected" posture as above.
///
/// This adds **no weight**: it can only remove roots from the set that counts,
/// never count one twice or more. It is not used anywhere by default (existing
/// behaviour is unchanged until a caller wraps its oracle), and it does not
/// depend on any personhood or value crate — the caller supplies `eligible`
/// (e.g. built from `mini-uniqueness`' published policy and audit results), so
/// the voice/value wall is untouched. Counting still means identity roots, not
/// proven humans.
pub struct EligibleOnly<'a, F: Fn(&Did) -> bool> {
    inner: &'a dyn IdentityOracle,
    eligible: F,
}

impl<F: Fn(&Did) -> bool> std::fmt::Debug for EligibleOnly<'_, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EligibleOnly")
    }
}

impl<'a, F: Fn(&Did) -> bool> EligibleOnly<'a, F> {
    /// Wrap `inner`; `eligible` is asked about identity *roots*.
    pub fn new(inner: &'a dyn IdentityOracle, eligible: F) -> Self {
        EligibleOnly { inner, eligible }
    }
}

impl<F: Fn(&Did) -> bool> IdentityOracle for EligibleOnly<'_, F> {
    fn kel(&self, did: &Did) -> Option<&Kel> {
        let kel = self.inner.kel(did)?;
        // A device KEL is judged by the root that delegated it.
        let root = kel.delegator().unwrap_or_else(|| did.clone());
        (self.eligible)(&root).then_some(kel)
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

/// Classify how much assurance a caller has that `obj`'s author-root identity
/// is not silently missing a fresher, possibly conflicting branch (SPEC-01
/// §7, invariant M3; `did_mini::KelAssurance`), composed on top of the same
/// provenance re-check [`author_verified`] already performs.
///
/// **No governance decision in this crate calls this today.** `propose`/
/// `approve`/`merge`/`resolve_project` all still gate purely on
/// [`author_verified`]'s plain boolean — deliberately unchanged by this
/// function, since deciding *which* governance actions should require *which*
/// minimum assurance level is a founder-facing policy call (KEL witnessing
/// is still Phase 3-of-10 work per `docs/design/
/// kel-witness-receipts-and-duplicity-gossip.md`, gated behind external
/// review — D-0047 — before any high-value authority decision may depend on
/// it), not something this crate should decide unilaterally by wiring it
/// into every quorum check. This function exists so that decision, once
/// made, has real, tested, reusable machinery to call rather than starting
/// from nothing.
///
/// Returns `Err(ForgeError::BadObject)` if `author_verified` would already
/// reject `obj` (unknown author, broken provenance/delegation) — assurance is
/// only meaningful once ordinary authorization already holds. Otherwise
/// composes [`did_mini::assess_kel_assurance`] over the *root* identity's KEL
/// (the identity whose vote/authorship actually counts toward quorum, not
/// the signing device), propagating its own errors (an internally-invalid or
/// pin-stale root KEL) via [`ForgeError::Identity`].
pub fn author_assurance(
    oracle: &dyn IdentityOracle,
    obj: &Object,
    pins: &mut FreshnessPins,
    witnessing: Option<WitnessEvidence<'_>>,
    known_duplicity: bool,
) -> Result<KelAssurance, ForgeError> {
    if !author_verified(oracle, obj) {
        return Err(ForgeError::BadObject);
    }
    // `author_verified` already confirmed both KELs resolve, so this lookup
    // cannot fail in practice; still handled explicitly rather than assumed.
    let root = oracle.kel(&obj.author_human).ok_or(ForgeError::BadObject)?;
    assess_kel_assurance(root, pins, witnessing, known_duplicity).map_err(ForgeError::Identity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Capabilities;
    use did_mini::Controller;
    use mini_objects::{ObjectBuilder, ObjectType, Payload};

    fn human(seed: u8) -> (Controller, Controller) {
        let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed + 2; 32],
            &[seed + 3; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root, device)
    }

    fn an_object(human: &Did, device: &Controller) -> Object {
        ObjectBuilder::new(ObjectType::COMMIT)
            .payload(Payload::Public(b"anchor".to_vec()))
            .sign(human, device)
            .unwrap()
    }

    #[test]
    fn an_ineligible_root_and_its_devices_stop_counting_but_others_are_unaffected() {
        let (alice, alice_dev) = human(5);
        let (bob, bob_dev) = human(6);
        let mut oracle = KelDirectory::new();
        for k in [alice.kel(), alice_dev.kel(), bob.kel(), bob_dev.kel()] {
            oracle.insert(k);
        }
        let by_alice = an_object(&alice.did(), &alice_dev);
        let by_bob = an_object(&bob.did(), &bob_dev);
        assert!(author_verified(&oracle, &by_alice) && author_verified(&oracle, &by_bob));

        let alice_root = alice.did();
        let gated = EligibleOnly::new(&oracle, |root| *root != alice_root);
        assert!(
            !author_verified(&gated, &by_alice),
            "ineligible root is dropped"
        );
        assert!(
            author_verified(&gated, &by_bob),
            "eligible root still counts"
        );
        // The gate only ever removes: allowing everyone changes nothing.
        let open = EligibleOnly::new(&oracle, |_| true);
        assert!(author_verified(&open, &by_alice) && author_verified(&open, &by_bob));
    }

    #[test]
    fn a_verified_first_contact_author_is_direct() {
        let (root, device) = human(1);
        let mut oracle = KelDirectory::new();
        oracle.insert(root.kel());
        oracle.insert(device.kel());
        let obj = an_object(&root.did(), &device);

        let mut pins = FreshnessPins::new();
        let assurance = author_assurance(&oracle, &obj, &mut pins, None, false).unwrap();
        assert_eq!(assurance, KelAssurance::Direct);
    }

    #[test]
    fn a_second_check_of_the_same_root_is_pinned() {
        let (mut root, device) = human(2);
        let mut oracle = KelDirectory::new();
        oracle.insert(root.kel());
        oracle.insert(device.kel());
        let obj = an_object(&root.did(), &device);

        let mut pins = FreshnessPins::new();
        author_assurance(&oracle, &obj, &mut pins, None, false).unwrap();

        // The root rotates and re-signs; the oracle is refreshed with the new KEL.
        root.rotate().unwrap();
        oracle.insert(root.kel());
        let obj2 = an_object(&root.did(), &device);
        let assurance = author_assurance(&oracle, &obj2, &mut pins, None, false).unwrap();
        assert_eq!(assurance, KelAssurance::Pinned);
    }

    #[test]
    fn an_unknown_author_is_rejected_without_computing_assurance() {
        let (root, device) = human(3);
        let oracle = KelDirectory::new(); // empty: neither KEL is vouched for
        let obj = an_object(&root.did(), &device);

        let mut pins = FreshnessPins::new();
        assert_eq!(
            author_assurance(&oracle, &obj, &mut pins, None, false),
            Err(ForgeError::BadObject)
        );
    }

    #[test]
    fn known_duplicity_overrides_a_verified_author() {
        let (root, device) = human(4);
        let mut oracle = KelDirectory::new();
        oracle.insert(root.kel());
        oracle.insert(device.kel());
        let obj = an_object(&root.did(), &device);

        let mut pins = FreshnessPins::new();
        let assurance = author_assurance(&oracle, &obj, &mut pins, None, true).unwrap();
        assert_eq!(assurance, KelAssurance::DuplicityDetected);
    }
}
