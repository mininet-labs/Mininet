//! `KelAssurance` — Phase 3 of `docs/design/
//! kel-witness-receipts-and-duplicity-gossip.md`'s committed phased plan
//! (audit #12 finding F4, invariant M3), first slice: an honest, gradable
//! classification of how much confidence a verifier has that a presented
//! KEL head is not silently missing a fresher, possibly conflicting branch
//! — replacing what would otherwise be one boolean "is this fresh" (the
//! research report's own §14-15 framing).
//!
//! ## Scope: Phase 3, first slice only
//!
//! [`assess_kel_assurance`] composes three already-shipped pieces — ordinary
//! [`Kel::verify`], the interim [`FreshnessPins`] rule (D-0088), and Phase
//! 1/2's [`crate::witness`]/[`crate::witness_state`] receipt/certificate
//! machinery — into one classification. It never replaces [`Kel::verify`]
//! with a boolean: an internally-invalid or pin-stale KEL is rejected
//! exactly as [`FreshnessPins::check_and_pin`] already rejects it, before
//! any witness evidence is even considered.
//!
//! **Deliberately not attempted here:**
//! - [`KelAssurance`] has no `WitnessedRecentAndGossiped` variant — the
//!   research report's own top tier, which requires counting independent
//!   gossip peers. No gossip protocol exists yet (Phase 5), so nothing in
//!   this crate could ever honestly produce that classification. Adding it
//!   is an additive, non-breaking follow-up once Phase 5 ships.
//! - `WitnessPolicy` is still not read from a real `Establishment` event —
//!   the caller supplies it directly. Wiring witness policy into real
//!   establishment events (a wire-format change to a core identity
//!   primitive) is separate, larger follow-up work, not bundled into this
//!   slice.
//! - No local duplicity-proof store exists yet; the caller tells this
//!   function whether it already knows of a conflicting proof for this
//!   identity (`known_duplicity`) rather than this function owning any
//!   proof storage or lookup.
//! - A stale-per-`FreshnessPins` KEL is rejected outright regardless of any
//!   witness certificate presented — the fail-closed, conservative choice;
//!   the research report does not specify that witnessing overrides a
//!   verifier's own local freshness violation, so this slice does not
//!   invent that override.

use mini_crypto::VerifyingKey;

use crate::error::{IdentityError, Result};
use crate::freshness::FreshnessPins;
use crate::kel::Kel;
use crate::witness::{WitnessId, WitnessPolicy, WitnessedEventCertificate};

/// How much assurance a verifier has that the presented KEL head is not
/// silently missing a fresher, possibly conflicting branch (SPEC-01 §7,
/// invariant M3's harder half). See the module doc for what this slice
/// does and does not compute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KelAssurance {
    /// The KEL verified and, per [`FreshnessPins`], is not a regression —
    /// but this is the first time this verifier has ever checked this
    /// SCID, so there is no prior pin to compare against, and no witness
    /// certificate was presented. Weakest level: a first-time verifier
    /// cannot detect that a fresher branch exists elsewhere.
    Direct,
    /// The KEL verified and matches or advances this verifier's own prior
    /// pin for this SCID ([`FreshnessPins`]) — closes the "already seen a
    /// fresher log" replay case, but not the "never seen a fresher log"
    /// case a first-contact verifier faces.
    Pinned,
    /// A [`WitnessedEventCertificate`] for the presented head event
    /// verified against the given [`WitnessPolicy`], meeting its
    /// threshold — but at least one receipt's `observed_epoch` is older
    /// than the caller's own `max_epoch_age` bound.
    Witnessed,
    /// Witnessed, and every receipt's `observed_epoch` is within the
    /// caller's `max_epoch_age` bound of `now_epoch`.
    WitnessedRecent,
    /// The caller already knows of a duplicity proof conflicting with
    /// this identity or a witness it relies on. This overrides every
    /// other signal — the verifier must not trust the presented head
    /// regardless of pinning or witnessing.
    DuplicityDetected,
}

/// Witness evidence for [`assess_kel_assurance`] to check against the
/// presented KEL's head event. Grouped into its own typed request rather
/// than loose parameters, per this crate's typed-domain discipline.
pub struct WitnessEvidence<'a> {
    pub certificate: &'a WitnessedEventCertificate,
    pub policy: &'a WitnessPolicy,
    pub resolve_witness_key: &'a dyn Fn(&WitnessId) -> Option<VerifyingKey>,
    /// The verifier's own current coarse epoch.
    pub now_epoch: u64,
    /// The maximum `now_epoch - observed_epoch` a receipt may show and
    /// still count as "recent."
    pub max_epoch_age: u64,
}

impl core::fmt::Debug for WitnessEvidence<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WitnessEvidence")
            .field("certificate", self.certificate)
            .field("policy", self.policy)
            .field("resolve_witness_key", &"<fn>")
            .field("now_epoch", &self.now_epoch)
            .field("max_epoch_age", &self.max_epoch_age)
            .finish()
    }
}

/// Classify a verifier's assurance for `kel`, per the module doc's scope.
///
/// `pins` is advanced exactly as [`FreshnessPins::check_and_pin`] already
/// does — call this instead of calling `check_and_pin` and `Kel::verify`
/// separately, not in addition to them; this is the composed replacement
/// for both. Returns the same [`IdentityError`] `check_and_pin` would
/// (internally invalid or pin-stale KEL) before any witness evidence is
/// considered.
///
/// `known_duplicity` overrides every other signal when `true`.
pub fn assess_kel_assurance(
    kel: &Kel,
    pins: &mut FreshnessPins,
    witnessing: Option<WitnessEvidence<'_>>,
    known_duplicity: bool,
) -> Result<KelAssurance> {
    let was_pinned = pins.pinned_sn(kel.scid()).is_some();
    pins.check_and_pin(kel)?;

    if known_duplicity {
        return Ok(KelAssurance::DuplicityDetected);
    }

    if let Some(evidence) = witnessing {
        let head = kel.events().last().ok_or(IdentityError::EmptyKel)?;
        evidence
            .certificate
            .verify(evidence.policy, evidence.resolve_witness_key)?;
        if evidence.certificate.identity != kel.did()
            || evidence.certificate.sequence != head.sn
            || evidence.certificate.event_digest != head.digest()
        {
            return Err(IdentityError::WitnessReceiptMismatch);
        }
        let recent = evidence.certificate.receipts.iter().all(|r| {
            evidence
                .now_epoch
                .saturating_sub(r.statement.observed_epoch)
                <= evidence.max_epoch_age
        });
        return Ok(if recent {
            KelAssurance::WitnessedRecent
        } else {
            KelAssurance::Witnessed
        });
    }

    Ok(if was_pinned {
        KelAssurance::Pinned
    } else {
        KelAssurance::Direct
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::witness::{sign_witness_receipt, WitnessReceiptStatement, WitnessReceiptVersion};
    use crate::Controller;
    use mini_crypto::SigningKey;

    fn a_witness() -> (WitnessId, SigningKey, VerifyingKey) {
        let did = Controller::incept_single().unwrap().did();
        let key = SigningKey::generate().unwrap();
        let vk = key.verifying_key();
        (WitnessId(did), key, vk)
    }

    fn certificate_for_head(
        kel: &Kel,
        witnesses: &[(WitnessId, SigningKey, VerifyingKey)],
        generation: u64,
        observed_epoch: u64,
    ) -> WitnessedEventCertificate {
        let head = kel.events().last().unwrap();
        let receipts = witnesses
            .iter()
            .map(|(wid, key, _)| {
                let statement = WitnessReceiptStatement {
                    version: WitnessReceiptVersion::V1,
                    identity: kel.did(),
                    sequence: head.sn,
                    event_digest: head.digest(),
                    prior_event_digest: if head.prior.is_empty() {
                        None
                    } else {
                        Some(head.prior.clone())
                    },
                    event_kind: (&head.kind).into(),
                    witness_policy_generation: generation,
                    witness_id: wid.clone(),
                    observed_epoch,
                };
                sign_witness_receipt(statement, key)
            })
            .collect();
        WitnessedEventCertificate::assemble(kel.did(), head.sn, head.digest(), generation, receipts)
            .unwrap()
    }

    #[test]
    fn first_check_with_no_witnessing_is_direct() {
        let alice = Controller::incept_single().unwrap();
        let mut pins = FreshnessPins::new();
        let assurance = assess_kel_assurance(&alice.kel(), &mut pins, None, false).unwrap();
        assert_eq!(assurance, KelAssurance::Direct);
    }

    #[test]
    fn a_second_check_of_the_same_scid_is_pinned() {
        let mut alice = Controller::incept_single().unwrap();
        let mut pins = FreshnessPins::new();
        assess_kel_assurance(&alice.kel(), &mut pins, None, false).unwrap();

        alice.rotate().unwrap();
        let assurance = assess_kel_assurance(&alice.kel(), &mut pins, None, false).unwrap();
        assert_eq!(assurance, KelAssurance::Pinned);
    }

    #[test]
    fn a_valid_recent_certificate_yields_witnessed_recent() {
        let alice = Controller::incept_single().unwrap();
        let witnesses: Vec<_> = (0..2).map(|_| a_witness()).collect();
        let policy = WitnessPolicy::new(
            1,
            witnesses.iter().map(|(id, _, _)| id.clone()).collect(),
            2,
        )
        .unwrap();
        let cert = certificate_for_head(&alice.kel(), &witnesses, 1, 100);
        let resolve = |id: &WitnessId| {
            witnesses
                .iter()
                .find(|(wid, _, _)| wid == id)
                .map(|(_, _, vk)| vk.clone())
        };
        let mut pins = FreshnessPins::new();
        let evidence = WitnessEvidence {
            certificate: &cert,
            policy: &policy,
            resolve_witness_key: &resolve,
            now_epoch: 105,
            max_epoch_age: 10,
        };
        let assurance =
            assess_kel_assurance(&alice.kel(), &mut pins, Some(evidence), false).unwrap();
        assert_eq!(assurance, KelAssurance::WitnessedRecent);
    }

    #[test]
    fn a_valid_but_stale_certificate_yields_witnessed_not_recent() {
        let alice = Controller::incept_single().unwrap();
        let witnesses: Vec<_> = (0..2).map(|_| a_witness()).collect();
        let policy = WitnessPolicy::new(
            1,
            witnesses.iter().map(|(id, _, _)| id.clone()).collect(),
            2,
        )
        .unwrap();
        let cert = certificate_for_head(&alice.kel(), &witnesses, 1, 100);
        let resolve = |id: &WitnessId| {
            witnesses
                .iter()
                .find(|(wid, _, _)| wid == id)
                .map(|(_, _, vk)| vk.clone())
        };
        let mut pins = FreshnessPins::new();
        let evidence = WitnessEvidence {
            certificate: &cert,
            policy: &policy,
            resolve_witness_key: &resolve,
            now_epoch: 1_000,
            max_epoch_age: 10,
        };
        let assurance =
            assess_kel_assurance(&alice.kel(), &mut pins, Some(evidence), false).unwrap();
        assert_eq!(assurance, KelAssurance::Witnessed);
    }

    #[test]
    fn known_duplicity_overrides_a_valid_certificate() {
        let alice = Controller::incept_single().unwrap();
        let witnesses: Vec<_> = (0..2).map(|_| a_witness()).collect();
        let policy = WitnessPolicy::new(
            1,
            witnesses.iter().map(|(id, _, _)| id.clone()).collect(),
            2,
        )
        .unwrap();
        let cert = certificate_for_head(&alice.kel(), &witnesses, 1, 100);
        let resolve = |id: &WitnessId| {
            witnesses
                .iter()
                .find(|(wid, _, _)| wid == id)
                .map(|(_, _, vk)| vk.clone())
        };
        let mut pins = FreshnessPins::new();
        let evidence = WitnessEvidence {
            certificate: &cert,
            policy: &policy,
            resolve_witness_key: &resolve,
            now_epoch: 105,
            max_epoch_age: 10,
        };
        let assurance =
            assess_kel_assurance(&alice.kel(), &mut pins, Some(evidence), true).unwrap();
        assert_eq!(assurance, KelAssurance::DuplicityDetected);
    }

    #[test]
    fn a_certificate_for_a_different_identity_is_rejected() {
        let alice = Controller::incept_single().unwrap();
        let bob = Controller::incept_single().unwrap();
        let witnesses: Vec<_> = (0..2).map(|_| a_witness()).collect();
        let policy = WitnessPolicy::new(
            1,
            witnesses.iter().map(|(id, _, _)| id.clone()).collect(),
            2,
        )
        .unwrap();
        // Certificate genuinely certifies bob's head, not alice's.
        let cert = certificate_for_head(&bob.kel(), &witnesses, 1, 100);
        let resolve = |id: &WitnessId| {
            witnesses
                .iter()
                .find(|(wid, _, _)| wid == id)
                .map(|(_, _, vk)| vk.clone())
        };
        let mut pins = FreshnessPins::new();
        let evidence = WitnessEvidence {
            certificate: &cert,
            policy: &policy,
            resolve_witness_key: &resolve,
            now_epoch: 105,
            max_epoch_age: 10,
        };
        assert_eq!(
            assess_kel_assurance(&alice.kel(), &mut pins, Some(evidence), false),
            Err(IdentityError::WitnessReceiptMismatch)
        );
    }

    #[test]
    fn an_internally_invalid_kel_is_rejected_before_any_assurance_is_computed() {
        let alice = Controller::incept_single().unwrap();
        let mut bytes = alice.kel().to_bytes();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        let mut pins = FreshnessPins::new();
        if let Ok(tampered) = Kel::from_bytes(&bytes) {
            assert!(assess_kel_assurance(&tampered, &mut pins, None, false).is_err());
        }
    }

    #[test]
    fn a_stale_kel_is_rejected_even_with_a_valid_certificate_for_its_old_head() {
        let mut alice = Controller::incept_single().unwrap();
        let mut pins = FreshnessPins::new();
        let stale_kel = alice.kel();
        let witnesses: Vec<_> = (0..2).map(|_| a_witness()).collect();
        let policy = WitnessPolicy::new(
            1,
            witnesses.iter().map(|(id, _, _)| id.clone()).collect(),
            2,
        )
        .unwrap();
        let cert = certificate_for_head(&stale_kel, &witnesses, 1, 100);

        // Pin advances past the stale snapshot via a real rotation.
        alice.rotate().unwrap();
        assess_kel_assurance(&alice.kel(), &mut pins, None, false).unwrap();

        let resolve = |id: &WitnessId| {
            witnesses
                .iter()
                .find(|(wid, _, _)| wid == id)
                .map(|(_, _, vk)| vk.clone())
        };
        let evidence = WitnessEvidence {
            certificate: &cert,
            policy: &policy,
            resolve_witness_key: &resolve,
            now_epoch: 105,
            max_epoch_age: 10,
        };
        assert!(matches!(
            assess_kel_assurance(&stale_kel, &mut pins, Some(evidence), false),
            Err(IdentityError::StaleKel { .. })
        ));
    }
}
