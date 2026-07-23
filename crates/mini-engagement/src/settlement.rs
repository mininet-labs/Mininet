//! Bridging an [`Engagement`]'s local state to real canonical settlement
//! truth (roadmap issue #226).
//!
//! Before this module, `transitions::complete` only ever produced
//! [`EngagementState::Completed`] in memory -- nothing in this crate ever
//! asked a real [`CanonicalLedgerView`] whether `escrow_claim` actually
//! reached canonical finality (M2/M3, `docs/INVARIANTS.md` §4). A caller
//! who only read `Engagement::state` could not honestly distinguish "I
//! locally recorded this as done" from "the canonical ledger agrees value
//! moved" -- exactly the gap FD-18 Wave 4's `mini-attest` must not paper
//! over (PR #220's research proposal, §3.1: attestation must not infer
//! completion from a serialized struct a reviewer supplied).
//!
//! [`canonical_completion_status`] closes that gap the same way
//! `mini_settlement::reconcile` itself works: strictly read-only, no
//! merging (M1), no authority to finalize anything on its own. It answers
//! one question -- does the canonical ledger's view of `escrow_claim`
//! agree with this engagement's local state? -- and nothing more.
//! Actually getting a claim in front of a canonical ledger (broadcasting
//! it toward consensus so it *can* finalize) is real, separate networked-
//! consensus wiring (roadmap #36-#45) this module does not perform; it
//! only reads whatever a [`CanonicalLedgerView`] already reports.

use mini_settlement::{reconcile, CanonicalLedgerView, SettlementState};

use crate::error::{EngagementError, Result};
use crate::state::{Engagement, EngagementState};

/// Whether an engagement's escrow claim -- the only thing that ever
/// actually moves value here (FD-05: a signed promise is never final
/// ownership by itself) -- has reached canonical resolution, read from a
/// real [`CanonicalLedgerView`] rather than trusted from whatever local
/// [`EngagementState`] a caller was handed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CanonicalCompletionStatus {
    /// The canonical ledger has not yet resolved this engagement's escrow
    /// claim's `(payer, sequence)` slot one way or another.
    Pending,
    /// The escrow claim finalized on the canonical ledger *and* the
    /// engagement's own local state has reached [`EngagementState::Completed`]
    /// -- the only combination a caller (e.g. `mini-attest`, FD-18 Wave 4)
    /// may treat as a genuinely provable, canonical completion.
    CanonicallyCompleted,
    /// The escrow claim finalized, but the local state never reached
    /// `Completed` (still `Accepted`/`Milestone`, or even `Disputed`).
    /// Value moved without local bookkeeping agreeing the work was done --
    /// surfaced explicitly rather than silently folded into either
    /// "complete" or "pending", since local state and canonical truth are
    /// allowed to diverge and a caller must be told when they do.
    FinalizedWithoutLocalCompletion,
    /// The canonical ledger resolved the escrow claim's `(payer,
    /// sequence)` slot with a **different** claim (M3, canonical ordering
    /// alone decides conflicts). This engagement's escrow never moved
    /// value, regardless of local state.
    RejectedConflict,
    /// The escrow claim's validity window passed before canonical
    /// inclusion, and the ledger never referenced it.
    Expired,
}

impl CanonicalCompletionStatus {
    /// The single question `mini-attest` (or any other consumer) should
    /// ever ask: is this engagement provably, canonically done? `true`
    /// only for [`CanonicalCompletionStatus::CanonicallyCompleted`] --
    /// mirroring [`mini_settlement::SettlementState::is_final`]'s
    /// deliberately narrow contract.
    pub const fn is_canonically_complete(self) -> bool {
        matches!(self, CanonicalCompletionStatus::CanonicallyCompleted)
    }
}

/// Reconcile `engagement.escrow_claim` against a real canonical ledger and
/// combine that truth with the engagement's local state.
///
/// Read-only, exactly mirroring [`mini_settlement::reconcile`]: never
/// writes to `ledger`, never submits `engagement.escrow_claim` anywhere,
/// and never finalizes anything on its own authority. A caller who wants
/// to actually get the claim in front of consensus does that separately,
/// through whatever real `mini-net`/`mini-chain` submission path exists;
/// this function only ever reads what a [`CanonicalLedgerView`] already
/// knows.
pub fn canonical_completion_status(
    engagement: &Engagement,
    ledger: &impl CanonicalLedgerView,
    now_ms: u64,
) -> Result<CanonicalCompletionStatus> {
    let settlement =
        reconcile(&engagement.escrow_claim, ledger, now_ms).map_err(EngagementError::Settlement)?;

    Ok(match settlement {
        SettlementState::Finalized => {
            if matches!(engagement.state, EngagementState::Completed { .. }) {
                CanonicalCompletionStatus::CanonicallyCompleted
            } else {
                CanonicalCompletionStatus::FinalizedWithoutLocalCompletion
            }
        }
        SettlementState::RejectedConflict => CanonicalCompletionStatus::RejectedConflict,
        SettlementState::Expired => CanonicalCompletionStatus::Expired,
        SettlementState::SignedLocal
        | SettlementState::AcceptedLocal
        | SettlementState::PendingCanonical => CanonicalCompletionStatus::Pending,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;
    use mini_crypto::SigningKey;
    use mini_objects::{ObjectBuilder, ObjectType, Payload};
    use mini_settlement::{claim_digest, sign_claim, InMemoryLedgerView};

    fn sample_object_id() -> mini_objects::ObjectId {
        let root = Controller::incept_single_from_seeds(&[1u8; 32], &[2u8; 32]).unwrap();
        let device =
            Controller::incept_device_single_from_seeds(&root.did(), &[3u8; 32], &[4u8; 32])
                .unwrap();
        let obj = ObjectBuilder::new(ObjectType::Custom("terms".to_string()))
            .payload(Payload::Public(vec![1, 2, 3]))
            .sign(&root.did(), &device)
            .unwrap();
        obj.id().clone()
    }

    fn sample_engagement_with_key(
        payer_key: &SigningKey,
        amount_micro: u64,
        deadline_ms: u64,
    ) -> Engagement {
        let payer = Controller::incept_single().unwrap().did();
        let performer = Controller::incept_single().unwrap().did();
        let claim = sign_claim(
            payer_key,
            b"performer-payee-bytes",
            amount_micro,
            1,
            u64::MAX,
            b"chain-state",
            0,
        )
        .unwrap();
        Engagement::offer(sample_object_id(), payer, performer, claim, deadline_ms)
    }

    #[test]
    fn nothing_finalized_yet_is_pending() {
        let payer_key = SigningKey::from_seed(&[9u8; 32]);
        let e = sample_engagement_with_key(&payer_key, 1_000, 10_000);
        let ledger = InMemoryLedgerView::new();
        assert_eq!(
            canonical_completion_status(&e, &ledger, 100).unwrap(),
            CanonicalCompletionStatus::Pending
        );
    }

    #[test]
    fn finalized_claim_with_local_completion_is_canonically_completed() {
        let payer_key = SigningKey::from_seed(&[9u8; 32]);
        let e = sample_engagement_with_key(&payer_key, 1_000, 10_000);
        let by = Controller::incept_single().unwrap().did();
        let e = crate::transitions::accept(e, by, 500).unwrap();
        let e = crate::transitions::complete(e, 700).unwrap();

        let mut ledger = InMemoryLedgerView::new();
        ledger.finalize(
            &e.escrow_claim.payer,
            e.escrow_claim.sequence,
            claim_digest(&e.escrow_claim),
        );

        let status = canonical_completion_status(&e, &ledger, 800).unwrap();
        assert_eq!(status, CanonicalCompletionStatus::CanonicallyCompleted);
        assert!(status.is_canonically_complete());
    }

    #[test]
    fn finalized_claim_without_local_completion_is_flagged_not_silently_completed() {
        let payer_key = SigningKey::from_seed(&[9u8; 32]);
        let e = sample_engagement_with_key(&payer_key, 1_000, 10_000);

        let mut ledger = InMemoryLedgerView::new();
        ledger.finalize(
            &e.escrow_claim.payer,
            e.escrow_claim.sequence,
            claim_digest(&e.escrow_claim),
        );

        // Local state is still `Offered` -- never accepted, never completed.
        let status = canonical_completion_status(&e, &ledger, 800).unwrap();
        assert_eq!(
            status,
            CanonicalCompletionStatus::FinalizedWithoutLocalCompletion
        );
        assert!(!status.is_canonically_complete());
    }

    #[test]
    fn a_conflicting_finalized_claim_is_rejected_regardless_of_local_completion() {
        let payer_key = SigningKey::from_seed(&[9u8; 32]);
        let e = sample_engagement_with_key(&payer_key, 1_000, 10_000);
        let by = Controller::incept_single().unwrap().did();
        let e = crate::transitions::accept(e, by, 500).unwrap();
        let e = crate::transitions::complete(e, 700).unwrap();

        // A *different* claim from the same payer won this exact sequence.
        let other = sign_claim(
            &payer_key,
            b"someone-else",
            50,
            e.escrow_claim.sequence,
            u64::MAX,
            b"chain-state",
            0,
        )
        .unwrap();
        let mut ledger = InMemoryLedgerView::new();
        ledger.finalize(&other.payer, other.sequence, claim_digest(&other));

        let status = canonical_completion_status(&e, &ledger, 800).unwrap();
        assert_eq!(status, CanonicalCompletionStatus::RejectedConflict);
        assert!(!status.is_canonically_complete());
    }

    #[test]
    fn an_unresolved_claim_past_its_validity_window_is_expired() {
        let payer_key = SigningKey::from_seed(&[9u8; 32]);
        let payer = Controller::incept_single().unwrap().did();
        let performer = Controller::incept_single().unwrap().did();
        let claim = sign_claim(
            &payer_key,
            b"performer-payee-bytes",
            1_000,
            1,
            500,
            b"chain-state",
            0,
        )
        .unwrap();
        let e = Engagement::offer(sample_object_id(), payer, performer, claim, 10_000);

        let ledger = InMemoryLedgerView::new();
        let status = canonical_completion_status(&e, &ledger, 600).unwrap();
        assert_eq!(status, CanonicalCompletionStatus::Expired);
    }

    #[test]
    fn a_bad_signature_on_the_escrow_claim_is_rejected_not_silently_pending() {
        let payer_key = SigningKey::from_seed(&[9u8; 32]);
        let mut e = sample_engagement_with_key(&payer_key, 1_000, 10_000);
        e.escrow_claim.amount_micro = 999_999; // tamper after signing

        let ledger = InMemoryLedgerView::new();
        assert_eq!(
            canonical_completion_status(&e, &ledger, 100).unwrap_err(),
            EngagementError::Settlement(mini_settlement::SettlementError::BadSignature)
        );
    }
}
