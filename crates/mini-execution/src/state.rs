//! The settlement ledger state: for each payer, only the *latest*
//! finalized `(sequence, claim_digest)` pair. That is deliberately all
//! [`mini_settlement::reconcile`] ever needs from a
//! [`mini_settlement::CanonicalLedgerView`] — it only ever asks
//! `finalized_claim_digest` for the sequence `finalized_sequence` itself
//! just returned — so this state carries no more history than the
//! protocol it backs actually reads, the same "don't build more than the
//! seam requires" discipline `mini_settlement::InMemoryLedgerView` already
//! modeled as a test double. This module makes that seam real.

use std::collections::BTreeMap;

use mini_crypto::HashAlgorithm;
use mini_settlement::{verify_claim_signature, CanonicalLedgerView, PaymentClaim};

use crate::body::SettlementBlockBody;
use crate::error::{ExecutionError, Result};

/// The deterministic result of applying every finalized block up to some
/// height: one `(sequence, digest)` high-water-mark per payer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LedgerState {
    finalized: BTreeMap<Vec<u8>, (u64, [u8; 32])>,
}

impl LedgerState {
    /// The empty state — genesis, nothing settled yet.
    pub fn new() -> Self {
        LedgerState::default()
    }

    /// A commitment to this exact state, suitable for a block header's
    /// `state_root`: BLAKE3 over the canonically-sorted (payer, sequence,
    /// digest) triples. `BTreeMap` iteration is already key-sorted, so two
    /// states with the same entries always produce the same commitment
    /// regardless of the order those entries were inserted in — the
    /// property that makes "two honest nodes reconcile to one answer"
    /// (Directive 4) checkable as a plain equality on this one hash.
    pub fn commitment(&self) -> [u8; 32] {
        let mut w = Vec::new();
        w.extend_from_slice(b"mini-execution/ledger-state/v1");
        w.extend_from_slice(&(self.finalized.len() as u64).to_be_bytes());
        for (payer, (sequence, digest)) in &self.finalized {
            w.extend_from_slice(&(payer.len() as u32).to_be_bytes());
            w.extend_from_slice(payer);
            w.extend_from_slice(&sequence.to_be_bytes());
            w.extend_from_slice(digest);
        }
        HashAlgorithm::Blake3.digest(&w)
    }
}

impl CanonicalLedgerView for LedgerState {
    fn finalized_sequence(&self, payer: &[u8]) -> Option<u64> {
        self.finalized.get(payer).map(|(sequence, _)| *sequence)
    }

    fn finalized_claim_digest(&self, payer: &[u8], sequence: u64) -> Option<[u8; 32]> {
        self.finalized
            .get(payer)
            .filter(|(finalized_sequence, _)| *finalized_sequence == sequence)
            .map(|(_, digest)| *digest)
    }
}

/// Apply a finalized block's body to `prev`, producing the next state.
///
/// Per claim, in body order (canonical order — M3): a claim wins its
/// `(payer, sequence)` slot only if it strictly exceeds that payer's
/// current high-water-mark, which then becomes the new mark. Everything
/// else is silently dropped, never merged, never partially honored (M1):
/// a claim with a bad signature, an already-claimed sequence (whether the
/// digest matches or not — first inclusion wins, permanently), or a
/// sequence at or below what's already finalized. This mirrors
/// [`mini_settlement::reconcile`]'s own rules exactly, because this
/// function is what makes `reconcile`'s answers real instead of
/// hypothetical: whatever this produces *is* what a [`LedgerState`]-backed
/// `CanonicalLedgerView` reports afterward.
pub fn apply_block(prev: &LedgerState, body: &SettlementBlockBody) -> Result<LedgerState> {
    if body.claims.len() > crate::body::MAX_CLAIMS_PER_BLOCK {
        return Err(ExecutionError::TooManyClaims);
    }
    let mut next = prev.clone();
    for claim in &body.claims {
        apply_one_claim(&mut next, claim);
    }
    Ok(next)
}

fn apply_one_claim(state: &mut LedgerState, claim: &PaymentClaim) {
    if verify_claim_signature(claim).is_err() {
        return;
    }
    let current = state.finalized_sequence(&claim.payer);
    let wins = match current {
        None => true,
        Some(existing_sequence) => claim.sequence > existing_sequence,
    };
    if wins {
        let digest = mini_settlement::claim_digest(claim);
        state
            .finalized
            .insert(claim.payer.clone(), (claim.sequence, digest));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_crypto::SigningKey;
    use mini_settlement::sign_claim;

    fn payer() -> SigningKey {
        SigningKey::from_seed(&[0x33; 32])
    }

    #[test]
    fn an_empty_body_leaves_state_unchanged() {
        let prev = LedgerState::new();
        let body = SettlementBlockBody::new(vec![]);
        let next = apply_block(&prev, &body).unwrap();
        assert_eq!(prev, next);
    }

    #[test]
    fn a_single_valid_claim_finalizes() {
        let claim = sign_claim(&payer(), b"payee", 1_000, 0, 10_000, b"chain-1", 0).unwrap();
        let body = SettlementBlockBody::new(vec![claim.clone()]);
        let next = apply_block(&LedgerState::new(), &body).unwrap();
        assert_eq!(next.finalized_sequence(&claim.payer), Some(0));
        assert_eq!(
            next.finalized_claim_digest(&claim.payer, 0),
            Some(mini_settlement::claim_digest(&claim))
        );
    }

    #[test]
    fn a_tampered_claim_is_dropped_not_finalized() {
        let mut claim = sign_claim(&payer(), b"payee", 1_000, 0, 10_000, b"chain-1", 0).unwrap();
        claim.amount_micro = 999_999; // invalidates the signature
        let body = SettlementBlockBody::new(vec![claim.clone()]);
        let next = apply_block(&LedgerState::new(), &body).unwrap();
        assert_eq!(next.finalized_sequence(&claim.payer), None);
    }

    #[test]
    fn two_conflicting_claims_in_one_body_the_first_in_order_wins() {
        let claim_a = sign_claim(&payer(), b"merchant-a", 500, 0, 10_000, b"chain-1", 0).unwrap();
        let claim_b = sign_claim(&payer(), b"merchant-b", 500, 0, 10_000, b"chain-1", 0).unwrap();
        assert_ne!(
            mini_settlement::claim_digest(&claim_a),
            mini_settlement::claim_digest(&claim_b)
        );

        let body = SettlementBlockBody::new(vec![claim_a.clone(), claim_b.clone()]);
        let next = apply_block(&LedgerState::new(), &body).unwrap();
        assert_eq!(
            next.finalized_claim_digest(&claim_a.payer, 0),
            Some(mini_settlement::claim_digest(&claim_a)),
            "the first claim at a slot wins; the second is dropped, never merged"
        );
    }

    #[test]
    fn a_higher_sequence_in_a_later_block_supersedes_the_previous_finalized_entry() {
        let first = sign_claim(&payer(), b"payee", 1_000, 0, 10_000, b"chain-1", 0).unwrap();
        let second = sign_claim(&payer(), b"payee-2", 2_000, 1, 10_000, b"chain-2", 0).unwrap();

        let after_first = apply_block(
            &LedgerState::new(),
            &SettlementBlockBody::new(vec![first.clone()]),
        )
        .unwrap();
        let after_second = apply_block(
            &after_first,
            &SettlementBlockBody::new(vec![second.clone()]),
        )
        .unwrap();

        assert_eq!(after_second.finalized_sequence(&first.payer), Some(1));
        assert_eq!(
            after_second.finalized_claim_digest(&first.payer, 1),
            Some(mini_settlement::claim_digest(&second))
        );
    }

    #[test]
    fn a_stale_or_repeated_sequence_in_a_later_block_never_overwrites_the_finalized_entry() {
        let first = sign_claim(&payer(), b"payee", 1_000, 0, 10_000, b"chain-1", 0).unwrap();
        let replay_attempt =
            sign_claim(&payer(), b"attacker", 999_999, 0, 10_000, b"chain-1", 0).unwrap();

        let after_first = apply_block(
            &LedgerState::new(),
            &SettlementBlockBody::new(vec![first.clone()]),
        )
        .unwrap();
        let after_replay = apply_block(
            &after_first,
            &SettlementBlockBody::new(vec![replay_attempt]),
        )
        .unwrap();

        assert_eq!(
            after_replay.finalized_claim_digest(&first.payer, 0),
            Some(mini_settlement::claim_digest(&first)),
            "an already-finalized slot can never be overwritten by a later block"
        );
    }

    #[test]
    fn state_commitment_is_deterministic_and_content_sensitive() {
        let claim = sign_claim(&payer(), b"payee", 1_000, 0, 10_000, b"chain-1", 0).unwrap();
        let body = SettlementBlockBody::new(vec![claim]);
        let a = apply_block(&LedgerState::new(), &body).unwrap();
        let b = apply_block(&LedgerState::new(), &body).unwrap();
        assert_eq!(a.commitment(), b.commitment());
        assert_ne!(a.commitment(), LedgerState::new().commitment());
    }

    #[test]
    fn too_many_claims_is_rejected_before_processing() {
        use mini_crypto::{Signature, SignatureSuite};

        // Cheap placeholder claims (garbage, unsigned-in-effect) are fine
        // here: the cap check must happen before any per-claim signature
        // verification, so an over-cap body is rejected regardless of
        // content.
        let placeholder = PaymentClaim {
            payer: vec![0u8; 32],
            payee: vec![1u8; 32],
            amount_micro: 1,
            sequence: 0,
            valid_until_ms: u64::MAX,
            last_known_chain: vec![],
            signature: Signature::from_suite_bytes(SignatureSuite::Ed25519, &[0u8; 64]).unwrap(),
        };
        let claims = vec![placeholder; crate::body::MAX_CLAIMS_PER_BLOCK + 1];
        let body = SettlementBlockBody::new(claims);
        assert_eq!(
            apply_block(&LedgerState::new(), &body).unwrap_err(),
            ExecutionError::TooManyClaims
        );
    }
}
