//! Turning a signed claim into a [`SettlementState`] — the two moments
//! that matter: a recipient deciding whether to act *before* finality
//! (risk, never truth), and canonical resolution *after* reconnection
//! (truth, never risk).
//!
//! This is invariant **M3** made concrete: *"Canonical ordering alone
//! decides conflicting spends (double-spends). No local committee,
//! hotspot, relay, or cache may finalize ownership."* Every function here
//! that can answer `Finalized` does so by reading a [`CanonicalLedgerView`]
//! — nothing in this module ever marks a claim final on its own authority.

use crate::claim::{claim_digest, verify_claim_signature, PaymentClaim};
use crate::error::{Result, SettlementError};
use crate::ledger::CanonicalLedgerView;
use crate::state::SettlementState;
use crate::watcher::ClaimWatcher;

/// A recipient's local risk policy for acting on a claim *before*
/// canonical finality (Directive 5's "signed promise" — a recipient may
/// choose to trust it up to a point, entirely at their own risk).
#[derive(Debug, Clone, Copy)]
pub struct LocalAcceptancePolicy {
    /// Claims at or below this amount may be locally accepted without
    /// waiting for canonical finality. Above it, the recipient is expected
    /// to wait — the whole point of a threshold is that a recipient who
    /// sets it to `0` never accepts anything early, and one of
    /// `u64::MAX` accepts everything early, both deliberate, explicit
    /// choices this type makes a policy decision, not a default someone
    /// forgot to set.
    pub max_amount_micro_without_finality: u64,
}

impl LocalAcceptancePolicy {
    /// Never accept anything before canonical finality — the conservative
    /// default matching M2's letter most literally.
    pub const fn never_accept_early() -> Self {
        LocalAcceptancePolicy {
            max_amount_micro_without_finality: 0,
        }
    }
}

/// Evaluate a freshly-received claim for local acceptance: verify it,
/// check it against everything this recipient has already seen from the
/// same payer, and decide — as a **risk decision, never a truth claim** —
/// whether to treat it as [`SettlementState::AcceptedLocal`] or to require
/// waiting for canonical resolution ([`SettlementState::PendingCanonical`]).
///
/// Returns `Err` only for structural problems (bad signature, or an
/// outright conflict with a *different* claim already seen at this exact
/// `(payer, nonce)` — the cheapest double-spend attempt, catchable
/// entirely offline). A `Result::Ok` here is never a claim of finality;
/// only [`reconcile`] can produce [`SettlementState::Finalized`].
pub fn evaluate_local_acceptance(
    claim: &PaymentClaim,
    policy: &LocalAcceptancePolicy,
    now_ms: u64,
    watcher: &mut impl ClaimWatcher,
) -> Result<SettlementState> {
    verify_claim_signature(claim)?;

    if now_ms >= claim.valid_until_ms {
        return Ok(SettlementState::Expired);
    }

    let digest = claim_digest(claim);
    if !watcher.observe(&claim.payer, claim.nonce, digest) {
        return Err(SettlementError::ConflictsWithKnownClaim);
    }

    if claim.amount_micro <= policy.max_amount_micro_without_finality {
        Ok(SettlementState::AcceptedLocal)
    } else {
        Ok(SettlementState::PendingCanonical)
    }
}

/// Resolve a claim against the canonical ledger — the only function in
/// this crate that can return [`SettlementState::Finalized`]. Reads
/// `ledger`; never writes anything and never combines this claim with any
/// other (M1: no merge path exists here or anywhere in this crate).
pub fn reconcile(
    claim: &PaymentClaim,
    ledger: &impl CanonicalLedgerView,
    now_ms: u64,
) -> Result<SettlementState> {
    verify_claim_signature(claim)?;

    let digest = claim_digest(claim);
    let outcome = match ledger.finalized_nonce(&claim.payer) {
        None => SettlementState::PendingCanonical,
        Some(finalized_nonce) if finalized_nonce < claim.nonce => SettlementState::PendingCanonical,
        Some(finalized_nonce) if finalized_nonce == claim.nonce => {
            match ledger.finalized_claim_digest(&claim.payer, claim.nonce) {
                Some(finalized_digest) if finalized_digest == digest => SettlementState::Finalized,
                // Either a different claim finalized at this slot, or the
                // ledger claims a finalized nonce with no matching digest
                // on record — either way, this exact claim did not win,
                // and it is rejected outright, never merged with whatever
                // did (M1).
                _ => SettlementState::RejectedConflict,
            }
        }
        // The canonical ledger has already moved past this nonce without
        // ever finalizing this claim -- superseded, not merely pending.
        Some(_) => SettlementState::RejectedConflict,
    };

    // A claim that already lost to a conflicting one, or already won,
    // reports that truth regardless of its validity window -- expiry only
    // matters for a claim still awaiting resolution.
    if matches!(outcome, SettlementState::PendingCanonical) && now_ms >= claim.valid_until_ms {
        return Ok(SettlementState::Expired);
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claim::sign_claim;
    use crate::ledger::InMemoryLedgerView;
    use crate::watcher::InMemoryClaimWatcher;
    use mini_crypto::SigningKey;

    fn payer() -> SigningKey {
        SigningKey::from_seed(&[0x22; 32])
    }

    #[test]
    fn a_claim_with_nothing_finalized_yet_is_pending() {
        let claim = sign_claim(&payer(), b"payee", 1_000, 0, 10_000, b"chain-1", 0).unwrap();
        let ledger = InMemoryLedgerView::new();
        assert_eq!(
            reconcile(&claim, &ledger, 100).unwrap(),
            SettlementState::PendingCanonical
        );
    }

    #[test]
    fn a_claim_that_matches_what_the_ledger_finalized_is_finalized() {
        let claim = sign_claim(&payer(), b"payee", 1_000, 0, 10_000, b"chain-1", 0).unwrap();
        let mut ledger = InMemoryLedgerView::new();
        ledger.finalize(&claim.payer, 0, crate::claim_digest(&claim));
        assert_eq!(
            reconcile(&claim, &ledger, 100).unwrap(),
            SettlementState::Finalized
        );
    }

    /// THE core double-spend case (M3): two claims, same payer, same
    /// nonce, different payees/amounts. The canonical ledger finalizes
    /// exactly one. The other is rejected outright -- never merged,
    /// never partially honored, never averaged.
    #[test]
    fn conflicting_claims_at_the_same_nonce_never_both_finalize() {
        let claim_a = sign_claim(&payer(), b"merchant-a", 5_000, 0, 10_000, b"chain-1", 0).unwrap();
        let claim_b = sign_claim(&payer(), b"merchant-b", 5_000, 0, 10_000, b"chain-1", 0).unwrap();
        assert_ne!(crate::claim_digest(&claim_a), crate::claim_digest(&claim_b));

        let mut ledger = InMemoryLedgerView::new();
        ledger.finalize(&claim_a.payer, 0, crate::claim_digest(&claim_a));

        assert_eq!(
            reconcile(&claim_a, &ledger, 100).unwrap(),
            SettlementState::Finalized
        );
        assert_eq!(
            reconcile(&claim_b, &ledger, 100).unwrap(),
            SettlementState::RejectedConflict
        );

        // Never both finalized -- the actual invariant M1/M3 exist to
        // guarantee, checked directly rather than just implied above.
        let a_final = reconcile(&claim_a, &ledger, 100).unwrap().is_final();
        let b_final = reconcile(&claim_b, &ledger, 100).unwrap().is_final();
        assert!(
            a_final ^ b_final,
            "exactly one of the two must be final, never both, never neither"
        );
    }

    #[test]
    fn a_claim_superseded_by_a_later_finalized_nonce_is_rejected_not_pending() {
        let claim = sign_claim(&payer(), b"payee", 1_000, 0, 10_000, b"chain-1", 0).unwrap();
        let later = sign_claim(&payer(), b"payee-2", 2_000, 1, 10_000, b"chain-1", 0).unwrap();
        let mut ledger = InMemoryLedgerView::new();
        // Nonce 1 finalized; nonce 0 was apparently never included as this
        // exact claim (e.g. a different nonce-0 claim finalized earlier,
        // or nonce 0 was consumed by something else entirely) -- either
        // way, an observer must not keep reporting PendingCanonical
        // forever for a slot the canonical ledger has moved past.
        ledger.finalize(&claim.payer, 1, crate::claim_digest(&later));
        assert_eq!(
            reconcile(&claim, &ledger, 100).unwrap(),
            SettlementState::RejectedConflict
        );
    }

    #[test]
    fn an_unresolved_claim_past_its_validity_window_expires() {
        let claim = sign_claim(&payer(), b"payee", 1_000, 0, 500, b"chain-1", 0).unwrap();
        let ledger = InMemoryLedgerView::new();
        assert_eq!(
            reconcile(&claim, &ledger, 600).unwrap(),
            SettlementState::Expired
        );
    }

    #[test]
    fn a_finalized_claim_past_its_validity_window_still_reports_finalized() {
        // Expiry is about claims still waiting -- a claim that already won
        // finality does not retroactively un-finalize because a clock
        // moved past valid_until_ms afterward.
        let claim = sign_claim(&payer(), b"payee", 1_000, 0, 500, b"chain-1", 0).unwrap();
        let mut ledger = InMemoryLedgerView::new();
        ledger.finalize(&claim.payer, 0, crate::claim_digest(&claim));
        assert_eq!(
            reconcile(&claim, &ledger, 999_999).unwrap(),
            SettlementState::Finalized
        );
    }

    #[test]
    fn local_acceptance_within_policy_threshold_is_accepted_but_never_reports_as_final() {
        let claim = sign_claim(&payer(), b"payee", 100, 0, 10_000, b"chain-1", 0).unwrap();
        let policy = LocalAcceptancePolicy {
            max_amount_micro_without_finality: 500,
        };
        let mut watcher = InMemoryClaimWatcher::new();
        let state = evaluate_local_acceptance(&claim, &policy, 0, &mut watcher).unwrap();
        assert_eq!(state, SettlementState::AcceptedLocal);
        assert!(!state.is_final());
    }

    #[test]
    fn local_acceptance_above_policy_threshold_requires_waiting() {
        let claim = sign_claim(&payer(), b"payee", 10_000, 0, 10_000, b"chain-1", 0).unwrap();
        let policy = LocalAcceptancePolicy {
            max_amount_micro_without_finality: 500,
        };
        let mut watcher = InMemoryClaimWatcher::new();
        assert_eq!(
            evaluate_local_acceptance(&claim, &policy, 0, &mut watcher).unwrap(),
            SettlementState::PendingCanonical
        );
    }

    #[test]
    fn never_accept_early_policy_never_returns_accepted_local() {
        let claim = sign_claim(&payer(), b"payee", 1, 0, 10_000, b"chain-1", 0).unwrap();
        let policy = LocalAcceptancePolicy::never_accept_early();
        let mut watcher = InMemoryClaimWatcher::new();
        assert_eq!(
            evaluate_local_acceptance(&claim, &policy, 0, &mut watcher).unwrap(),
            SettlementState::PendingCanonical
        );
    }

    #[test]
    fn local_acceptance_rejects_a_second_conflicting_claim_at_the_same_slot() {
        let claim_a = sign_claim(&payer(), b"merchant-a", 100, 0, 10_000, b"chain-1", 0).unwrap();
        let claim_b = sign_claim(&payer(), b"merchant-b", 100, 0, 10_000, b"chain-1", 0).unwrap();
        let policy = LocalAcceptancePolicy {
            max_amount_micro_without_finality: 500,
        };
        let mut watcher = InMemoryClaimWatcher::new();
        assert!(evaluate_local_acceptance(&claim_a, &policy, 0, &mut watcher).is_ok());
        assert_eq!(
            evaluate_local_acceptance(&claim_b, &policy, 0, &mut watcher).unwrap_err(),
            SettlementError::ConflictsWithKnownClaim
        );
    }

    #[test]
    fn local_acceptance_rejects_a_forged_signature() {
        let mut claim = sign_claim(&payer(), b"payee", 1_000, 0, 10_000, b"chain-1", 0).unwrap();
        claim.amount_micro = 999_999; // tamper after signing
        let policy = LocalAcceptancePolicy::never_accept_early();
        let mut watcher = InMemoryClaimWatcher::new();
        assert_eq!(
            evaluate_local_acceptance(&claim, &policy, 0, &mut watcher).unwrap_err(),
            SettlementError::BadSignature
        );
    }
}
