//! A local, in-memory registry of known duplicity proofs (audit #12 F4,
//! invariant M3) — the piece `docs/design/
//! kel-witness-receipts-and-duplicity-gossip.md`'s Phase 3 remainder
//! named as still missing: [`crate::assess_kel_assurance`]'s
//! `known_duplicity` flag was, until now, entirely caller-computed with
//! no shared place to record or look up a proof once assembled
//! ([`ControllerDuplicityProof`]/[`WitnessEquivocationProof`], both
//! Phase 2, D-0326).
//!
//! ## Scope
//!
//! Purely local and in-memory — not persisted, not networked, not shared
//! across processes, the same limitation [`crate::WitnessJournal`] already
//! carries and for the same reason: persistence and gossip are later
//! phases (Phase 6 and Phase 5 respectively), not silently assumed solved
//! here.
//!
//! Records two independent signals, matching [`crate::KelAssurance::
//! DuplicityDetected`]'s own doc ("a duplicity proof conflicting with
//! this identity **or a witness it relies on**"):
//! - a [`ControllerDuplicityProof`], recorded against the identity it
//!   directly names — the controller itself signed two branches, so that
//!   identity's KEL can never be trusted again without out-of-band
//!   recovery;
//! - a [`WitnessEquivocationProof`], recorded against the witness it
//!   names — that witness is dishonest or compromised, so any receipt it
//!   issues is suspect for every identity whose policy includes it, not
//!   just the identity named in the two conflicting receipts that proved
//!   the equivocation.
//!
//! [`DuplicityRegistry::has_known_duplicity`] combines both signals. This
//! module does not itself decide *when* a proof is trustworthy enough to
//! record — [`ControllerDuplicityProof::assemble`]/[`WitnessEquivocationProof::
//! assemble`] already did that structural validation before a caller ever
//! reaches this registry; recording is a caller decision this module
//! trusts, exactly as [`crate::WitnessJournal`] trusts its caller for
//! `event`'s own chain validity before Phase 3's `observe_verified`
//! existed.

use std::collections::HashMap;

use crate::witness::{WitnessId, WitnessPolicy};
use crate::witness_state::{ControllerDuplicityProof, WitnessEquivocationProof};
use crate::Did;

/// A local registry of duplicity proofs this verifier has independently
/// assembled or been shown. See the module doc for exactly what is and
/// is not covered.
#[derive(Debug, Default)]
pub struct DuplicityRegistry {
    controller: HashMap<Did, ControllerDuplicityProof>,
    witness: HashMap<WitnessId, WitnessEquivocationProof>,
}

impl DuplicityRegistry {
    pub fn new() -> Self {
        DuplicityRegistry {
            controller: HashMap::new(),
            witness: HashMap::new(),
        }
    }

    /// Record a controller-duplicity proof against the identity it names.
    /// A second proof for the same identity replaces the first — either
    /// is equally sufficient evidence that the identity's controller
    /// signed conflicting branches, so this registry keeps at most one.
    pub fn record_controller_duplicity(&mut self, proof: ControllerDuplicityProof) {
        self.controller.insert(proof.identity.clone(), proof);
    }

    /// Record a witness-equivocation proof against the witness it names.
    /// A second proof for the same witness replaces the first, for the
    /// same reason as [`Self::record_controller_duplicity`].
    pub fn record_witness_equivocation(&mut self, proof: WitnessEquivocationProof) {
        self.witness.insert(proof.witness_id.clone(), proof);
    }

    /// The recorded controller-duplicity proof for `identity`, if any.
    pub fn controller_duplicity_for(&self, identity: &Did) -> Option<&ControllerDuplicityProof> {
        self.controller.get(identity)
    }

    /// The recorded witness-equivocation proof for `witness_id`, if any.
    pub fn witness_equivocation_for(
        &self,
        witness_id: &WitnessId,
    ) -> Option<&WitnessEquivocationProof> {
        self.witness.get(witness_id)
    }

    /// Whether `identity` should be treated as having known duplicity:
    /// either a controller-duplicity proof was recorded directly against
    /// it, or (when `policy` is given) any witness in `policy`'s set has
    /// a recorded equivocation proof — that witness's receipts toward
    /// this identity, or any identity relying on the same policy, cannot
    /// be trusted. `policy` is `None` when the caller has no witnessing
    /// context to check (e.g. a plain [`crate::KelAssurance::Direct`]/
    /// [`crate::KelAssurance::Pinned`] check with no certificate
    /// involved); the witness-equivocation half is simply skipped in that
    /// case, never treated as an error.
    pub fn has_known_duplicity(&self, identity: &Did, policy: Option<&WitnessPolicy>) -> bool {
        if self.controller.contains_key(identity) {
            return true;
        }
        match policy {
            Some(policy) => self.witness.keys().any(|wid| policy.contains(wid)),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, EventKind};
    use crate::witness::{sign_witness_receipt, WitnessReceiptStatement, WitnessReceiptVersion};
    use crate::Controller;
    use mini_crypto::SigningKey;

    fn a_witness_id() -> WitnessId {
        WitnessId(Controller::incept_single().unwrap().did())
    }

    // Mirrors `witness_state::tests::an_event` -- a bare, unsigned
    // fixture event good enough for `ControllerDuplicityProof::assemble`'s
    // purely structural checks (identity/sequence/digest), which never
    // verify signatures.
    fn an_event(scid: &str, sn: u64, prior: Vec<u8>, distinguishing_byte: u8) -> Event {
        Event {
            suite: mini_crypto::SignatureSuite::Ed25519,
            scid: scid.to_string(),
            sn,
            prior,
            kind: EventKind::Interaction {
                anchors: vec![[distinguishing_byte; 32]],
            },
            signatures: vec![],
        }
    }

    #[test]
    fn an_empty_registry_has_no_known_duplicity() {
        let registry = DuplicityRegistry::new();
        let controller = Controller::incept_single().unwrap();
        assert!(!registry.has_known_duplicity(&controller.did(), None));
    }

    #[test]
    fn a_recorded_controller_duplicity_is_flagged_for_that_identity_only() {
        let alice = Controller::incept_single().unwrap();
        let bob = Controller::incept_single().unwrap();
        let scid = alice.did().scid().to_string();
        let e_a = an_event(&scid, 3, vec![], 1);
        let e_b = an_event(&scid, 3, vec![], 2);
        let proof = ControllerDuplicityProof::assemble(alice.did(), e_a, e_b).unwrap();

        let mut registry = DuplicityRegistry::new();
        registry.record_controller_duplicity(proof);

        assert!(registry.has_known_duplicity(&alice.did(), None));
        assert!(!registry.has_known_duplicity(&bob.did(), None));
        assert!(registry.controller_duplicity_for(&alice.did()).is_some());
    }

    #[test]
    fn a_recorded_witness_equivocation_is_flagged_only_for_a_policy_that_includes_the_witness() {
        let alice = Controller::incept_single().unwrap();
        let wid = a_witness_id();
        let key = SigningKey::generate().unwrap();
        let a = sign_witness_receipt(
            WitnessReceiptStatement {
                version: WitnessReceiptVersion::V1,
                identity: alice.did(),
                sequence: 1,
                event_digest: vec![0xAA; 34],
                prior_event_digest: None,
                event_kind: crate::witness::KeyEventKind::Inception,
                witness_policy_generation: 1,
                witness_id: wid.clone(),
                observed_epoch: 1,
            },
            &key,
        );
        let b = sign_witness_receipt(
            WitnessReceiptStatement {
                version: WitnessReceiptVersion::V1,
                identity: alice.did(),
                sequence: 1,
                event_digest: vec![0xBB; 34],
                prior_event_digest: None,
                event_kind: crate::witness::KeyEventKind::Inception,
                witness_policy_generation: 1,
                witness_id: wid.clone(),
                observed_epoch: 1,
            },
            &key,
        );
        let proof = WitnessEquivocationProof::assemble(a, b).unwrap();

        let mut registry = DuplicityRegistry::new();
        registry.record_witness_equivocation(proof);

        let policy_with_witness = WitnessPolicy::new(1, vec![wid.clone()], 1).unwrap();
        let other_witness = a_witness_id();
        let policy_without_witness = WitnessPolicy::new(1, vec![other_witness], 1).unwrap();

        assert!(registry.has_known_duplicity(&alice.did(), Some(&policy_with_witness)));
        assert!(!registry.has_known_duplicity(&alice.did(), Some(&policy_without_witness)));
        assert!(!registry.has_known_duplicity(&alice.did(), None));
        assert!(registry.witness_equivocation_for(&wid).is_some());
    }

    #[test]
    fn a_second_proof_for_the_same_key_replaces_the_first() {
        let alice = Controller::incept_single().unwrap();
        let scid = alice.did().scid().to_string();
        let e_a = an_event(&scid, 3, vec![], 1);
        let e_b = an_event(&scid, 3, vec![], 2);
        let e_c = an_event(&scid, 3, vec![], 3);
        let first = ControllerDuplicityProof::assemble(alice.did(), e_a, e_b.clone()).unwrap();
        let second = ControllerDuplicityProof::assemble(alice.did(), e_b, e_c).unwrap();

        let mut registry = DuplicityRegistry::new();
        registry.record_controller_duplicity(first);
        registry.record_controller_duplicity(second.clone());

        assert_eq!(
            registry.controller_duplicity_for(&alice.did()),
            Some(&second)
        );
    }
}
