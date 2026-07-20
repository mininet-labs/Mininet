//! In-memory witness state machine (audit #12 finding F4, invariant M3) —
//! Phase 2 of `docs/design/kel-witness-receipts-and-duplicity-gossip.md`'s
//! committed phased plan, building on Phase 1's receipt/certificate
//! vocabulary ([`crate::witness`], D-0321).
//!
//! Phase 1 defined what a receipt *is*. This module defines what a witness
//! *does* when it observes an event: first-seen acceptance, direct-successor
//! verification, duplicate idempotence, stale rejection, and conflict
//! detection, per the research report §9's own case analysis. It also adds
//! the two duplicity-proof types the report's §11 taxonomy names for the
//! cases Phase 2 commits to: [`ControllerDuplicityProof`] (two different
//! controller-signed events at the same identity+sequence) and
//! [`WitnessEquivocationProof`] (one witness signing receipts for two
//! different event digests at the same identity+sequence+generation).
//!
//! ## Scope: Phase 2 only
//!
//! [`WitnessJournal`] retains, per identity, exactly the minimal state the
//! research report's §9 pseudocode names: the accepted sequence, its event
//! digest, the witness-policy generation last used, plus (beyond the
//! report's minimal sketch) the full accepted [`Event`] and the receipt
//! already issued for it — needed so an exact-duplicate observation returns
//! the *same* receipt rather than re-deriving a semantically different one,
//! and so a conflicting sibling event can be turned into a real,
//! independently-verifiable [`ControllerDuplicityProof`] built from actual
//! controller-signed bytes rather than from this witness's own paraphrased
//! receipt claims.
//!
//! **Deliberately not attempted here** (per the design doc's own phase
//! boundary): full KEL-chain verification (self-certifying inception,
//! signature/threshold/pre-rotation/recovery checks) — [`WitnessJournal::
//! observe`] trusts the caller to have already established that `event` is
//! a chain-valid candidate at this position; wiring the real `Kel`/
//! `KeyState` verification path in front of this state machine is Phase 3's
//! job (`KelAssurance`). Recovery events are not special-cased — this
//! module tracks sequence/digest/prior linkage only, agnostic to *why* a
//! rotation happened; recovery-aware handling is also Phase 3's job. The
//! harder "conflicting descendant" case (research report §9.3: an event
//! that builds on a branch inconsistent with accepted state, at a sequence
//! that isn't a same-slot conflict) is rejected outright rather than
//! answered with a constructed fork proof — that remains future work, not
//! silently promised here. No receipt collection protocol, gossip,
//! persistent witness service, witness rotation, or public transparency
//! logs (Phases 4-9) exist in this module.

use std::collections::HashMap;

use mini_crypto::{SigningKey, VerifyingKey};

use crate::codec::{Reader, Writer};
use crate::error::{IdentityError, Result};
use crate::event::{self, Event};
use crate::limits::MAX_DID_BYTES;
use crate::witness::{
    sign_witness_receipt, WitnessId, WitnessPolicy, WitnessReceipt, WitnessReceiptStatement,
};
use crate::Did;

/// An event's own encoded size is already bounded by [`crate::kel`]'s
/// verification path before it ever reaches this module; this is a
/// generous transport ceiling for one event inside a duplicity proof,
/// matching [`crate::kel`]'s own per-event bound.
const MAX_EVENT_BYTES: usize = 64 * 1024;

/// Per-identity state a witness retains across observations (research
/// report §9's `WitnessIdentityState`, extended with the full accepted
/// event and its issued receipt — see the module doc for why).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessIdentityState {
    pub identity: Did,
    pub accepted_sequence: u64,
    pub accepted_event_digest: Vec<u8>,
    pub witness_policy_generation: u64,
    accepted_event: Event,
    issued_receipt: WitnessReceipt,
}

impl WitnessIdentityState {
    /// The full event this witness most recently accepted for this
    /// identity — the material a [`ControllerDuplicityProof`] is built
    /// from when a conflicting sibling arrives.
    pub fn accepted_event(&self) -> &Event {
        &self.accepted_event
    }

    /// The receipt this witness issued for [`Self::accepted_event`] —
    /// returned verbatim (never re-derived) on an exact-duplicate
    /// observation.
    pub fn issued_receipt(&self) -> &WitnessReceipt {
        &self.issued_receipt
    }
}

/// What [`WitnessJournal::observe`] decided about one presented event,
/// per the research report §9.3's case analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WitnessObservation {
    /// First-seen acceptance, or a valid direct successor of the
    /// previously accepted event: state updated, a fresh receipt issued.
    Accepted(WitnessReceipt),
    /// Exact duplicate of the already-accepted event: the previously
    /// issued receipt, returned unchanged — never a new signature over a
    /// semantically identical statement.
    AlreadyAccepted(WitnessReceipt),
    /// Older than the accepted state for this identity: no receipt
    /// issued. Carries the accepted sequence as a head hint, deliberately
    /// nothing more — the research report's §9.3 warns against revealing
    /// excessive history to an unauthenticated requester.
    Stale { accepted_sequence: u64 },
    /// A different, controller-signed event at the same sequence as the
    /// already-accepted one: no receipt issued for either; the accepted
    /// state is left exactly as it was. Carries a real, independently-
    /// verifiable proof of the conflict. Boxed: two full [`Event`]s make
    /// this by far the largest variant, and it is also the rarest.
    ControllerDuplicity(Box<ControllerDuplicityProof>),
}

/// In-memory per-witness state, keyed by identity. Not persisted, not
/// networked, not shared across processes — a single witness's local view
/// (Phase 6's persistent/durable witness service is future work).
#[derive(Debug, Default)]
pub struct WitnessJournal {
    states: HashMap<Did, WitnessIdentityState>,
}

impl WitnessJournal {
    pub fn new() -> Self {
        WitnessJournal {
            states: HashMap::new(),
        }
    }

    /// This witness's retained state for `identity`, if it has observed
    /// anything for it yet.
    pub fn state_for(&self, identity: &Did) -> Option<&WitnessIdentityState> {
        self.states.get(identity)
    }

    /// Observe `event` as witness `witness_id`, signing with `witness_key`
    /// under `policy`, at coarse network time `observed_epoch`. `event`'s
    /// own chain validity (signatures, pre-rotation, recovery rules) is
    /// assumed already established by the caller — see the module doc for
    /// why that check does not live here.
    ///
    /// Returns [`IdentityError::WitnessNotInPolicy`] if `witness_id` is
    /// not actually a member of `policy` — an honest witness never signs
    /// under a policy it doesn't belong to.
    pub fn observe(
        &mut self,
        event: &Event,
        policy: &WitnessPolicy,
        witness_id: WitnessId,
        witness_key: &SigningKey,
        observed_epoch: u64,
    ) -> Result<WitnessObservation> {
        if !policy.contains(&witness_id) {
            return Err(IdentityError::WitnessNotInPolicy);
        }
        let identity = Did::from_scid(&event.scid)?;
        let event_digest = event.digest();

        // Borrow `self.states` only long enough to decide and extract
        // whatever owned data each outcome needs -- so the borrow ends
        // before the Accept arm below needs `&mut self.states`.
        let decision = match self.states.get(&identity) {
            None => Decision::Accept,
            Some(state) => {
                if event.sn == state.accepted_sequence
                    && event_digest == state.accepted_event_digest
                {
                    Decision::Duplicate(state.issued_receipt.clone())
                } else if event.sn == state.accepted_sequence + 1
                    && event.prior == state.accepted_event_digest
                {
                    Decision::Accept
                } else if event.sn < state.accepted_sequence {
                    Decision::Stale {
                        accepted_sequence: state.accepted_sequence,
                    }
                } else if event.sn == state.accepted_sequence
                    && event_digest != state.accepted_event_digest
                {
                    Decision::Conflict {
                        accepted_event: state.accepted_event.clone(),
                    }
                } else {
                    Decision::ConflictingDescendant
                }
            }
        };

        match decision {
            Decision::Duplicate(receipt) => Ok(WitnessObservation::AlreadyAccepted(receipt)),
            Decision::Stale { accepted_sequence } => {
                Ok(WitnessObservation::Stale { accepted_sequence })
            }
            Decision::Conflict { accepted_event } => {
                let proof =
                    ControllerDuplicityProof::assemble(identity, accepted_event, event.clone())?;
                Ok(WitnessObservation::ControllerDuplicity(Box::new(proof)))
            }
            Decision::ConflictingDescendant => {
                Err(IdentityError::WitnessConflictingDescendant { sequence: event.sn })
            }
            Decision::Accept => {
                let statement = WitnessReceiptStatement {
                    version: crate::witness::WitnessReceiptVersion::V1,
                    identity: identity.clone(),
                    sequence: event.sn,
                    event_digest: event_digest.clone(),
                    prior_event_digest: if event.prior.is_empty() {
                        None
                    } else {
                        Some(event.prior.clone())
                    },
                    event_kind: (&event.kind).into(),
                    witness_policy_generation: policy.generation,
                    witness_id,
                    observed_epoch,
                };
                let receipt = sign_witness_receipt(statement, witness_key);
                self.states.insert(
                    identity.clone(),
                    WitnessIdentityState {
                        identity,
                        accepted_sequence: event.sn,
                        accepted_event_digest: event_digest,
                        witness_policy_generation: policy.generation,
                        accepted_event: event.clone(),
                        issued_receipt: receipt.clone(),
                    },
                );
                Ok(WitnessObservation::Accepted(receipt))
            }
        }
    }
}

enum Decision {
    Accept,
    Duplicate(WitnessReceipt),
    Stale { accepted_sequence: u64 },
    Conflict { accepted_event: Event },
    ConflictingDescendant,
}

/// Two different controller-signed events at the same identity and
/// sequence (research report §11.1) — proof that either the controller
/// signed conflicting branches, or one of the two is forged/replayed.
///
/// Deliberately **not** verified against the controller's actual
/// authoritative key state here: confirming each event is genuinely
/// authorized at its claimed KEL position (pre-rotation chain, signing
/// threshold) is Phase 3's job (`KelAssurance`), which has the full
/// verified `Kel` this self-contained type does not carry. [`Self::
/// assemble`] only checks the *structural* claim: same identity, same
/// sequence, different digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerDuplicityProof {
    pub identity: Did,
    pub sequence: u64,
    pub event_a: Event,
    pub event_b: Event,
}

impl ControllerDuplicityProof {
    /// Assemble a proof from two events, rejecting the pair with
    /// [`IdentityError::ControllerDuplicityMismatch`] unless they name the
    /// same identity and sequence but differ in digest.
    pub fn assemble(identity: Did, event_a: Event, event_b: Event) -> Result<Self> {
        if event_a.scid != identity.scid()
            || event_b.scid != identity.scid()
            || event_a.sn != event_b.sn
            || event_a.digest() == event_b.digest()
        {
            return Err(IdentityError::ControllerDuplicityMismatch);
        }
        let sequence = event_a.sn;
        Ok(ControllerDuplicityProof {
            identity,
            sequence,
            event_a,
            event_b,
        })
    }

    /// Encode to the canonical wire form.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.bytes(self.identity.as_str().as_bytes());
        w.u64(self.sequence);
        w.bytes(&self.event_a.full_bytes());
        w.bytes(&self.event_b.full_bytes());
        w.into_bytes()
    }

    /// Decode from [`Self::encode`]'s wire form, re-validating exactly as
    /// [`Self::assemble`] does. Strict: rejects trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let identity_bytes = r.bytes_limited("identity", MAX_DID_BYTES)?;
        let identity_str =
            String::from_utf8(identity_bytes).map_err(|_| IdentityError::DidFormat)?;
        let identity = Did::parse(&identity_str)?;
        let sequence = r.u64()?;
        let event_a_bytes = r.bytes_limited("event_a", MAX_EVENT_BYTES)?;
        let mut ar = Reader::new(&event_a_bytes);
        let event_a = event::decode(&mut ar)?;
        if !ar.finished() {
            return Err(IdentityError::TrailingBytes);
        }
        let event_b_bytes = r.bytes_limited("event_b", MAX_EVENT_BYTES)?;
        let mut br = Reader::new(&event_b_bytes);
        let event_b = event::decode(&mut br)?;
        if !br.finished() {
            return Err(IdentityError::TrailingBytes);
        }
        if !r.finished() {
            return Err(IdentityError::TrailingBytes);
        }
        let proof = ControllerDuplicityProof::assemble(identity, event_a, event_b)?;
        if proof.sequence != sequence {
            return Err(IdentityError::ControllerDuplicityMismatch);
        }
        Ok(proof)
    }
}

/// One witness signing receipts for two different event digests at the
/// same identity, sequence, and witness-policy generation (research report
/// §11.3) — proof that the named witness is dishonest or compromised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessEquivocationProof {
    pub witness_id: WitnessId,
    pub receipt_a: WitnessReceipt,
    pub receipt_b: WitnessReceipt,
}

impl WitnessEquivocationProof {
    /// Assemble a proof from two receipts, rejecting the pair with
    /// [`IdentityError::WitnessEquivocationMismatch`] unless they share a
    /// witness, identity, sequence, and policy generation, but differ in
    /// claimed event digest.
    pub fn assemble(receipt_a: WitnessReceipt, receipt_b: WitnessReceipt) -> Result<Self> {
        let a = &receipt_a.statement;
        let b = &receipt_b.statement;
        if a.witness_id != b.witness_id
            || a.identity != b.identity
            || a.sequence != b.sequence
            || a.witness_policy_generation != b.witness_policy_generation
            || a.event_digest == b.event_digest
        {
            return Err(IdentityError::WitnessEquivocationMismatch);
        }
        let witness_id = a.witness_id.clone();
        Ok(WitnessEquivocationProof {
            witness_id,
            receipt_a,
            receipt_b,
        })
    }

    /// Verify both receipts' signatures against the named witness's key —
    /// confirming `witness_key` is actually the real, currently-valid key
    /// for [`Self::witness_id`] (e.g. via that witness's own KEL) is the
    /// caller's job, exactly as [`WitnessReceipt::verify`] already
    /// documents.
    pub fn verify(&self, witness_key: &VerifyingKey) -> Result<()> {
        self.receipt_a.verify(witness_key)?;
        self.receipt_b.verify(witness_key)?;
        Ok(())
    }

    /// Encode to the canonical wire form.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        let a = self.receipt_a.encode();
        w.bytes(&a);
        let b = self.receipt_b.encode();
        w.bytes(&b);
        w.into_bytes()
    }

    /// Decode from [`Self::encode`]'s wire form, re-validating exactly as
    /// [`Self::assemble`] does. Strict: rejects trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let a_bytes = r.bytes_limited("receipt_a", MAX_RECEIPT_BYTES)?;
        let receipt_a = WitnessReceipt::decode(&a_bytes)?;
        let b_bytes = r.bytes_limited("receipt_b", MAX_RECEIPT_BYTES)?;
        let receipt_b = WitnessReceipt::decode(&b_bytes)?;
        if !r.finished() {
            return Err(IdentityError::TrailingBytes);
        }
        WitnessEquivocationProof::assemble(receipt_a, receipt_b)
    }
}

const MAX_RECEIPT_BYTES: usize = 8192;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::witness::{KeyEventKind, WitnessReceiptVersion};
    use crate::Controller;
    use crate::EventKind;

    fn a_witness() -> (WitnessId, SigningKey, VerifyingKey) {
        let did = Controller::incept_single().unwrap().did();
        let key = SigningKey::generate().unwrap();
        let vk = key.verifying_key();
        (WitnessId(did), key, vk)
    }

    // `distinguishing_byte` varies the test fixture's trivial anchor body so
    // two otherwise-identical events produce different digests -- it is
    // plain fixture data, not cryptographic material (renamed from an
    // earlier `salt` parameter name after CodeQL's "hard-coded
    // cryptographic value" query flagged the name itself, not any actual
    // cryptographic use: this anchors arbitrary application data per
    // event.rs's own `EventKind::Interaction` doc, never a KDF/encryption
    // input).
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

    fn a_policy(witnesses: Vec<WitnessId>, threshold: u16) -> WitnessPolicy {
        WitnessPolicy::new(1, witnesses, threshold).unwrap()
    }

    #[test]
    fn first_seen_event_is_accepted_and_receipted() {
        let controller = Controller::incept_single().unwrap();
        let scid = controller.did().scid().to_string();
        let (wid, key, _vk) = a_witness();
        let policy = a_policy(vec![wid.clone()], 1);
        let mut journal = WitnessJournal::new();
        let event = an_event(&scid, 0, vec![], 1);

        let outcome = journal.observe(&event, &policy, wid, &key, 10).unwrap();
        match outcome {
            WitnessObservation::Accepted(receipt) => {
                assert_eq!(receipt.statement.sequence, 0);
                assert_eq!(receipt.statement.event_digest, event.digest());
                assert_eq!(receipt.statement.prior_event_digest, None);
                assert_eq!(receipt.statement.event_kind, KeyEventKind::Interaction);
            }
            other => panic!("expected Accepted, got {other:?}"),
        }
        let state = journal.state_for(&controller.did()).unwrap();
        assert_eq!(state.accepted_sequence, 0);
        assert_eq!(state.accepted_event_digest, event.digest());
    }

    #[test]
    fn a_valid_direct_successor_is_accepted_and_updates_state() {
        let controller = Controller::incept_single().unwrap();
        let scid = controller.did().scid().to_string();
        let (wid, key, _vk) = a_witness();
        let policy = a_policy(vec![wid.clone()], 1);
        let mut journal = WitnessJournal::new();
        let e0 = an_event(&scid, 0, vec![], 1);
        journal.observe(&e0, &policy, wid.clone(), &key, 1).unwrap();

        let e1 = an_event(&scid, 1, e0.digest(), 2);
        let outcome = journal.observe(&e1, &policy, wid, &key, 2).unwrap();
        assert!(matches!(outcome, WitnessObservation::Accepted(_)));
        let state = journal.state_for(&controller.did()).unwrap();
        assert_eq!(state.accepted_sequence, 1);
        assert_eq!(state.accepted_event_digest, e1.digest());
        assert_eq!(state.accepted_event(), &e1);
    }

    #[test]
    fn an_exact_duplicate_returns_the_same_receipt_without_resigning() {
        let controller = Controller::incept_single().unwrap();
        let scid = controller.did().scid().to_string();
        let (wid, key, _vk) = a_witness();
        let policy = a_policy(vec![wid.clone()], 1);
        let mut journal = WitnessJournal::new();
        let e0 = an_event(&scid, 0, vec![], 1);
        let first = journal.observe(&e0, &policy, wid.clone(), &key, 5).unwrap();
        let first_receipt = match first {
            WitnessObservation::Accepted(r) => r,
            _ => panic!("expected Accepted"),
        };

        // Observed again, even at a different epoch -- must be idempotent.
        let second = journal.observe(&e0, &policy, wid, &key, 999).unwrap();
        match second {
            WitnessObservation::AlreadyAccepted(r) => assert_eq!(r, first_receipt),
            other => panic!("expected AlreadyAccepted, got {other:?}"),
        }
    }

    #[test]
    fn a_stale_ancestor_is_rejected_without_a_receipt() {
        let controller = Controller::incept_single().unwrap();
        let scid = controller.did().scid().to_string();
        let (wid, key, _vk) = a_witness();
        let policy = a_policy(vec![wid.clone()], 1);
        let mut journal = WitnessJournal::new();
        let e0 = an_event(&scid, 0, vec![], 1);
        journal.observe(&e0, &policy, wid.clone(), &key, 1).unwrap();
        let e1 = an_event(&scid, 1, e0.digest(), 2);
        journal.observe(&e1, &policy, wid.clone(), &key, 2).unwrap();

        // Re-present the now-stale inception.
        let outcome = journal.observe(&e0, &policy, wid, &key, 3).unwrap();
        assert_eq!(
            outcome,
            WitnessObservation::Stale {
                accepted_sequence: 1
            }
        );
    }

    #[test]
    fn a_conflicting_same_sequence_event_yields_a_duplicity_proof() {
        let controller = Controller::incept_single().unwrap();
        let scid = controller.did().scid().to_string();
        let (wid, key, _vk) = a_witness();
        let policy = a_policy(vec![wid.clone()], 1);
        let mut journal = WitnessJournal::new();
        let e0a = an_event(&scid, 0, vec![], 1);
        journal
            .observe(&e0a, &policy, wid.clone(), &key, 1)
            .unwrap();

        let e0b = an_event(&scid, 0, vec![], 2);
        assert_ne!(e0a.digest(), e0b.digest());
        let outcome = journal.observe(&e0b, &policy, wid, &key, 2).unwrap();
        match outcome {
            WitnessObservation::ControllerDuplicity(proof) => {
                assert_eq!(proof.identity, controller.did());
                assert_eq!(proof.sequence, 0);
                assert_eq!(proof.event_a, e0a);
                assert_eq!(proof.event_b, e0b);
            }
            other => panic!("expected ControllerDuplicity, got {other:?}"),
        }
        // The accepted state must be left exactly as it was -- neither
        // conflicting event replaces it.
        let state = journal.state_for(&controller.did()).unwrap();
        assert_eq!(state.accepted_event(), &e0a);
    }

    #[test]
    fn a_conflicting_descendant_is_rejected() {
        let controller = Controller::incept_single().unwrap();
        let scid = controller.did().scid().to_string();
        let (wid, key, _vk) = a_witness();
        let policy = a_policy(vec![wid.clone()], 1);
        let mut journal = WitnessJournal::new();
        let e0 = an_event(&scid, 0, vec![], 1);
        journal.observe(&e0, &policy, wid.clone(), &key, 1).unwrap();

        // Claims sn 1 but does not extend the accepted digest.
        let bogus = an_event(&scid, 1, vec![0xEE; 34], 2);
        assert_eq!(
            journal.observe(&bogus, &policy, wid, &key, 2),
            Err(IdentityError::WitnessConflictingDescendant { sequence: 1 })
        );
    }

    #[test]
    fn observing_outside_the_policy_is_rejected() {
        let controller = Controller::incept_single().unwrap();
        let scid = controller.did().scid().to_string();
        let (member, _mkey, _mvk) = a_witness();
        let (outsider, okey, _ovk) = a_witness();
        let policy = a_policy(vec![member], 1);
        let mut journal = WitnessJournal::new();
        let e0 = an_event(&scid, 0, vec![], 1);
        assert_eq!(
            journal.observe(&e0, &policy, outsider, &okey, 1),
            Err(IdentityError::WitnessNotInPolicy)
        );
    }

    #[test]
    fn a_controller_duplicity_proof_round_trips() {
        let controller = Controller::incept_single().unwrap();
        let scid = controller.did().scid().to_string();
        let e_a = an_event(&scid, 3, vec![0xAA; 34], 1);
        let e_b = an_event(&scid, 3, vec![0xAA; 34], 2);
        let proof = ControllerDuplicityProof::assemble(controller.did(), e_a, e_b).unwrap();
        let decoded = ControllerDuplicityProof::decode(&proof.encode()).unwrap();
        assert_eq!(decoded, proof);
    }

    #[test]
    fn assembling_a_duplicity_proof_from_different_sequences_is_rejected() {
        let controller = Controller::incept_single().unwrap();
        let scid = controller.did().scid().to_string();
        let e_a = an_event(&scid, 3, vec![], 1);
        let e_b = an_event(&scid, 4, vec![], 2);
        assert_eq!(
            ControllerDuplicityProof::assemble(controller.did(), e_a, e_b),
            Err(IdentityError::ControllerDuplicityMismatch)
        );
    }

    #[test]
    fn assembling_a_duplicity_proof_from_identical_events_is_rejected() {
        let controller = Controller::incept_single().unwrap();
        let scid = controller.did().scid().to_string();
        let e_a = an_event(&scid, 3, vec![], 1);
        let e_b = e_a.clone();
        assert_eq!(
            ControllerDuplicityProof::assemble(controller.did(), e_a, e_b),
            Err(IdentityError::ControllerDuplicityMismatch)
        );
    }

    #[test]
    fn assembling_a_duplicity_proof_for_a_different_identity_is_rejected() {
        let controller = Controller::incept_single().unwrap();
        let other = Controller::incept_single().unwrap();
        let scid = controller.did().scid().to_string();
        let e_a = an_event(&scid, 3, vec![], 1);
        let e_b = an_event(&scid, 3, vec![], 2);
        assert_eq!(
            ControllerDuplicityProof::assemble(other.did(), e_a, e_b),
            Err(IdentityError::ControllerDuplicityMismatch)
        );
    }

    fn a_statement(
        witness_id: WitnessId,
        identity: Did,
        sequence: u64,
        digest: Vec<u8>,
        generation: u64,
    ) -> WitnessReceiptStatement {
        WitnessReceiptStatement {
            version: WitnessReceiptVersion::V1,
            identity,
            sequence,
            event_digest: digest,
            prior_event_digest: None,
            event_kind: KeyEventKind::Inception,
            witness_policy_generation: generation,
            witness_id,
            observed_epoch: 1,
        }
    }

    #[test]
    fn a_witness_equivocation_proof_round_trips_and_verifies() {
        let controller = Controller::incept_single().unwrap();
        let (wid, key, vk) = a_witness();
        let a = sign_witness_receipt(
            a_statement(wid.clone(), controller.did(), 1, vec![0xAA; 34], 1),
            &key,
        );
        let b = sign_witness_receipt(
            a_statement(wid, controller.did(), 1, vec![0xBB; 34], 1),
            &key,
        );
        let proof = WitnessEquivocationProof::assemble(a, b).unwrap();
        proof.verify(&vk).unwrap();
        let decoded = WitnessEquivocationProof::decode(&proof.encode()).unwrap();
        assert_eq!(decoded, proof);
    }

    #[test]
    fn assembling_an_equivocation_proof_from_the_same_digest_is_rejected() {
        let controller = Controller::incept_single().unwrap();
        let (wid, key, _vk) = a_witness();
        let a = sign_witness_receipt(
            a_statement(wid.clone(), controller.did(), 1, vec![0xAA; 34], 1),
            &key,
        );
        let b = sign_witness_receipt(
            a_statement(wid, controller.did(), 1, vec![0xAA; 34], 1),
            &key,
        );
        assert_eq!(
            WitnessEquivocationProof::assemble(a, b),
            Err(IdentityError::WitnessEquivocationMismatch)
        );
    }

    #[test]
    fn assembling_an_equivocation_proof_from_different_witnesses_is_rejected() {
        let controller = Controller::incept_single().unwrap();
        let (wid1, key1, _) = a_witness();
        let (wid2, key2, _) = a_witness();
        let a = sign_witness_receipt(
            a_statement(wid1, controller.did(), 1, vec![0xAA; 34], 1),
            &key1,
        );
        let b = sign_witness_receipt(
            a_statement(wid2, controller.did(), 1, vec![0xBB; 34], 1),
            &key2,
        );
        assert_eq!(
            WitnessEquivocationProof::assemble(a, b),
            Err(IdentityError::WitnessEquivocationMismatch)
        );
    }

    #[test]
    fn an_equivocation_proof_fails_verification_against_the_wrong_key() {
        let controller = Controller::incept_single().unwrap();
        let (wid, key, _vk) = a_witness();
        let a = sign_witness_receipt(
            a_statement(wid.clone(), controller.did(), 1, vec![0xAA; 34], 1),
            &key,
        );
        let b = sign_witness_receipt(
            a_statement(wid, controller.did(), 1, vec![0xBB; 34], 1),
            &key,
        );
        let proof = WitnessEquivocationProof::assemble(a, b).unwrap();
        let wrong_key = SigningKey::generate().unwrap().verifying_key();
        assert!(proof.verify(&wrong_key).is_err());
    }
}
