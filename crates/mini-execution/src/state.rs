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

use mini_crypto::{HashAlgorithm, SignatureSuite, VerifyingKey};
use mini_economy::{Amount, IssuancePolicy, MonetaryLedger};
use mini_settlement::{verify_claim_signature, CanonicalLedgerView, PaymentClaim};

use crate::body::SettlementBlockBody;
use crate::error::{ExecutionError, Result};

/// Supports current/PQ opaque account identifiers while bounding state-memory
/// amplification from an untrusted payee field.
pub const MAX_ACCOUNT_BYTES: usize = 4_096;

/// The deterministic result of applying every finalized block up to some
/// height: one `(sequence, digest)` high-water-mark per payer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerState {
    finalized: BTreeMap<Vec<u8>, (u64, [u8; 32])>,
    monetary: MonetaryLedger,
    balances: BTreeMap<Vec<u8>, Amount>,
    allocated_circulating: Amount,
    unallocated_circulating: Amount,
}

impl LedgerState {
    /// The empty state — genesis, nothing settled yet.
    pub fn new() -> Self {
        Self::with_genesis_supply(Amount::ZERO)
    }

    /// Construct genesis with an explicitly supplied circulating MINI amount.
    pub fn with_genesis_supply(genesis_circulating: Amount) -> Self {
        Self {
            finalized: BTreeMap::new(),
            monetary: MonetaryLedger::new(genesis_circulating),
            balances: BTreeMap::new(),
            allocated_circulating: Amount::ZERO,
            unallocated_circulating: genesis_circulating,
        }
    }

    /// Construct a transparent Tier-0 account allocation. The allocations
    /// must account for the exact genesis circulating supply.
    pub fn with_genesis_balances(
        genesis_circulating: Amount,
        allocations: Vec<(Vec<u8>, Amount)>,
    ) -> Result<Self> {
        let mut balances = BTreeMap::new();
        let mut allocated = Amount::ZERO;
        for (account, amount) in allocations {
            if !is_supported_account(&account)
                || amount == Amount::ZERO
                || balances.insert(account, amount).is_some()
            {
                return Err(ExecutionError::InvalidGenesisAllocation);
            }
            allocated = allocated
                .checked_add(amount)
                .map_err(|_| ExecutionError::AmountOverflow)?;
        }
        if allocated != genesis_circulating {
            return Err(ExecutionError::InvalidGenesisAllocation);
        }
        Ok(Self {
            finalized: BTreeMap::new(),
            monetary: MonetaryLedger::new(genesis_circulating),
            balances,
            allocated_circulating: allocated,
            unallocated_circulating: Amount::ZERO,
        })
    }

    pub fn monetary(&self) -> &MonetaryLedger {
        &self.monetary
    }

    pub fn balance(&self, account: &[u8]) -> Amount {
        self.balances.get(account).copied().unwrap_or(Amount::ZERO)
    }

    pub fn unallocated_circulating(&self) -> Amount {
        self.unallocated_circulating
    }

    pub fn allocated_circulating(&self) -> Amount {
        self.allocated_circulating
    }

    pub fn balances(&self) -> &BTreeMap<Vec<u8>, Amount> {
        &self.balances
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
        w.extend_from_slice(b"mini-execution/ledger-state/v2");
        w.extend_from_slice(&(self.finalized.len() as u64).to_be_bytes());
        for (payer, (sequence, digest)) in &self.finalized {
            w.extend_from_slice(&(payer.len() as u32).to_be_bytes());
            w.extend_from_slice(payer);
            w.extend_from_slice(&sequence.to_be_bytes());
            w.extend_from_slice(digest);
        }
        w.extend_from_slice(&self.monetary.commitment().to_bytes());
        w.extend_from_slice(&(self.balances.len() as u64).to_be_bytes());
        for (account, balance) in &self.balances {
            w.extend_from_slice(&(account.len() as u32).to_be_bytes());
            w.extend_from_slice(account);
            w.extend_from_slice(&balance.as_micro().to_be_bytes());
        }
        w.extend_from_slice(&self.allocated_circulating.as_micro().to_be_bytes());
        w.extend_from_slice(&self.unallocated_circulating.as_micro().to_be_bytes());
        HashAlgorithm::Blake3.digest(&w)
    }

    pub fn verify_supply_conservation(&self) -> Result<()> {
        let accounted = self
            .allocated_circulating
            .checked_add(self.unallocated_circulating)
            .map_err(|_| ExecutionError::AmountOverflow)?;
        let circulating = self
            .monetary
            .circulating_supply()
            .map_err(ExecutionError::InvalidMonetaryEpoch)?;
        if accounted != circulating {
            return Err(ExecutionError::SupplyConservationViolation);
        }
        Ok(())
    }

    /// Expensive audit/recovery check. Ordinary block execution uses the
    /// consensus-tracked `allocated_circulating` total in O(1).
    pub fn verify_balance_map_total(&self) -> Result<()> {
        let mut recomputed = Amount::ZERO;
        for balance in self.balances.values() {
            recomputed = recomputed
                .checked_add(*balance)
                .map_err(|_| ExecutionError::AmountOverflow)?;
        }
        if recomputed != self.allocated_circulating {
            return Err(ExecutionError::SupplyConservationViolation);
        }
        Ok(())
    }
}

impl Default for LedgerState {
    fn default() -> Self {
        Self::new()
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
    if body.monetary_epochs.len() > 1 {
        return Err(ExecutionError::TooManyMonetaryEpochs);
    }
    let mut next = prev.clone();
    for claim in &body.claims {
        apply_one_claim(&mut next, claim)?;
    }
    if let Some(epoch) = body.monetary_epochs.first() {
        let before = next
            .monetary
            .circulating_supply()
            .map_err(ExecutionError::InvalidMonetaryEpoch)?;
        next.monetary = next
            .monetary
            .apply_epoch(epoch, &IssuancePolicy::d0074())
            .map_err(ExecutionError::InvalidMonetaryEpoch)?;
        let after = next
            .monetary
            .circulating_supply()
            .map_err(ExecutionError::InvalidMonetaryEpoch)?;
        let newly_circulating = after
            .checked_sub(before)
            .map_err(|_| ExecutionError::SupplyConservationViolation)?;
        next.unallocated_circulating = next
            .unallocated_circulating
            .checked_add(newly_circulating)
            .map_err(|_| ExecutionError::AmountOverflow)?;
    }
    next.verify_supply_conservation()?;
    Ok(next)
}

fn apply_one_claim(state: &mut LedgerState, claim: &PaymentClaim) -> Result<()> {
    if verify_claim_signature(claim).is_err() || !is_supported_account(&claim.payee) {
        return Ok(());
    }
    let current = state.finalized_sequence(&claim.payer);
    let wins = match current {
        None => true,
        Some(existing_sequence) => claim.sequence > existing_sequence,
    };
    if !wins {
        return Ok(());
    }
    let amount = Amount::from(claim.amount_micro);
    let payer_balance = state.balance(&claim.payer);
    if payer_balance < amount {
        return Ok(());
    }
    if claim.payer != claim.payee {
        let payee_balance = state.balance(&claim.payee);
        let debited = payer_balance
            .checked_sub(amount)
            .map_err(|_| ExecutionError::SupplyConservationViolation)?;
        let credited = payee_balance
            .checked_add(amount)
            .map_err(|_| ExecutionError::AmountOverflow)?;
        if debited == Amount::ZERO {
            state.balances.remove(&claim.payer);
        } else {
            state.balances.insert(claim.payer.clone(), debited);
        }
        state.balances.insert(claim.payee.clone(), credited);
    }
    let digest = mini_settlement::claim_digest(claim);
    state
        .finalized
        .insert(claim.payer.clone(), (claim.sequence, digest));
    Ok(())
}

fn is_supported_account(account: &[u8]) -> bool {
    !account.is_empty()
        && account.len() <= MAX_ACCOUNT_BYTES
        && VerifyingKey::from_suite_bytes(SignatureSuite::DEFAULT, account).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_crypto::SigningKey;
    use mini_settlement::sign_claim;

    fn payer() -> SigningKey {
        SigningKey::from_seed(&[0x33; 32])
    }

    fn funded_state(amount_micro: u64) -> LedgerState {
        let account = payer().verifying_key().to_bytes().to_vec();
        let amount = Amount::from(amount_micro);
        LedgerState::with_genesis_balances(amount, vec![(account, amount)]).unwrap()
    }

    fn recipient(seed: u8) -> Vec<u8> {
        SigningKey::from_seed(&[seed; 32])
            .verifying_key()
            .to_bytes()
            .to_vec()
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
        let claim =
            sign_claim(&payer(), &recipient(0x41), 1_000, 0, 10_000, b"chain-1", 0).unwrap();
        let body = SettlementBlockBody::new(vec![claim.clone()]);
        let next = apply_block(&funded_state(1_000), &body).unwrap();
        assert_eq!(next.finalized_sequence(&claim.payer), Some(0));
        assert_eq!(
            next.finalized_claim_digest(&claim.payer, 0),
            Some(mini_settlement::claim_digest(&claim))
        );
    }

    #[test]
    fn a_tampered_claim_is_dropped_not_finalized() {
        let mut claim =
            sign_claim(&payer(), &recipient(0x41), 1_000, 0, 10_000, b"chain-1", 0).unwrap();
        claim.amount_micro = 999_999; // invalidates the signature
        let body = SettlementBlockBody::new(vec![claim.clone()]);
        let next = apply_block(&funded_state(1_000), &body).unwrap();
        assert_eq!(next.finalized_sequence(&claim.payer), None);
    }

    #[test]
    fn two_conflicting_claims_in_one_body_the_first_in_order_wins() {
        let claim_a =
            sign_claim(&payer(), &recipient(0x41), 500, 0, 10_000, b"chain-1", 0).unwrap();
        let claim_b =
            sign_claim(&payer(), &recipient(0x42), 500, 0, 10_000, b"chain-1", 0).unwrap();
        assert_ne!(
            mini_settlement::claim_digest(&claim_a),
            mini_settlement::claim_digest(&claim_b)
        );

        let body = SettlementBlockBody::new(vec![claim_a.clone(), claim_b.clone()]);
        let next = apply_block(&funded_state(1_000), &body).unwrap();
        assert_eq!(
            next.finalized_claim_digest(&claim_a.payer, 0),
            Some(mini_settlement::claim_digest(&claim_a)),
            "the first claim at a slot wins; the second is dropped, never merged"
        );
    }

    #[test]
    fn a_higher_sequence_in_a_later_block_supersedes_the_previous_finalized_entry() {
        let first =
            sign_claim(&payer(), &recipient(0x41), 1_000, 0, 10_000, b"chain-1", 0).unwrap();
        let second =
            sign_claim(&payer(), &recipient(0x42), 2_000, 1, 10_000, b"chain-2", 0).unwrap();

        let after_first = apply_block(
            &funded_state(3_000),
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
        let first =
            sign_claim(&payer(), &recipient(0x41), 1_000, 0, 10_000, b"chain-1", 0).unwrap();
        let replay_attempt = sign_claim(
            &payer(),
            &recipient(0x42),
            999_999,
            0,
            10_000,
            b"chain-1",
            0,
        )
        .unwrap();

        let after_first = apply_block(
            &funded_state(1_000),
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
        let claim =
            sign_claim(&payer(), &recipient(0x41), 1_000, 0, 10_000, b"chain-1", 0).unwrap();
        let body = SettlementBlockBody::new(vec![claim]);
        let a = apply_block(&funded_state(1_000), &body).unwrap();
        let b = apply_block(&funded_state(1_000), &body).unwrap();
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

    #[test]
    fn transfer_debits_credits_and_conserves_supply() {
        let claim = sign_claim(&payer(), &recipient(0x41), 400, 0, 10_000, b"chain-1", 0).unwrap();
        let next = apply_block(
            &funded_state(1_000),
            &SettlementBlockBody::new(vec![claim.clone()]),
        )
        .unwrap();
        assert_eq!(next.balance(&claim.payer), Amount::from(600));
        assert_eq!(next.balance(&claim.payee), Amount::from(400));
        assert_eq!(next.unallocated_circulating(), Amount::ZERO);
        next.verify_supply_conservation().unwrap();
        next.verify_balance_map_total().unwrap();
    }

    #[test]
    fn overspend_is_not_finalized_and_does_not_consume_sequence() {
        let too_large =
            sign_claim(&payer(), &recipient(0x41), 1_001, 0, 10_000, b"chain-1", 0).unwrap();
        let valid =
            sign_claim(&payer(), &recipient(0x42), 1_000, 0, 10_000, b"chain-1", 0).unwrap();
        let after_rejected = apply_block(
            &funded_state(1_000),
            &SettlementBlockBody::new(vec![too_large.clone()]),
        )
        .unwrap();
        assert_eq!(after_rejected.finalized_sequence(&too_large.payer), None);
        assert_eq!(
            after_rejected.balance(&too_large.payer),
            Amount::from(1_000)
        );

        let after_valid = apply_block(
            &after_rejected,
            &SettlementBlockBody::new(vec![valid.clone()]),
        )
        .unwrap();
        assert_eq!(after_valid.finalized_sequence(&valid.payer), Some(0));
        assert_eq!(after_valid.balance(&valid.payee), Amount::from(1_000));
    }

    #[test]
    fn canonical_body_order_prevents_aggregate_overspend() {
        let first = sign_claim(&payer(), &recipient(0x41), 700, 0, 10_000, b"chain-1", 0).unwrap();
        let second = sign_claim(&payer(), &recipient(0x42), 700, 1, 10_000, b"chain-1", 0).unwrap();
        let next = apply_block(
            &funded_state(1_000),
            &SettlementBlockBody::new(vec![first.clone(), second.clone()]),
        )
        .unwrap();
        assert_eq!(next.balance(&first.payee), Amount::from(700));
        assert_eq!(next.balance(&second.payee), Amount::ZERO);
        assert_eq!(next.finalized_sequence(&first.payer), Some(0));
        next.verify_supply_conservation().unwrap();
    }

    #[test]
    fn self_transfer_changes_sequence_but_never_supply_or_balance() {
        let payer_key = payer();
        let payer_account = payer_key.verifying_key().to_bytes().to_vec();
        let claim = sign_claim(&payer_key, &payer_account, 500, 0, 10_000, b"chain-1", 0).unwrap();
        let next = apply_block(
            &funded_state(1_000),
            &SettlementBlockBody::new(vec![claim.clone()]),
        )
        .unwrap();
        assert_eq!(next.balance(&payer_account), Amount::from(1_000));
        assert_eq!(next.finalized_sequence(&payer_account), Some(0));
        next.verify_supply_conservation().unwrap();
    }

    #[test]
    fn genesis_allocations_must_be_exact_unique_nonzero_and_bounded() {
        assert_eq!(
            LedgerState::with_genesis_balances(
                Amount::from(10),
                vec![(b"alice".to_vec(), Amount::from(9))]
            )
            .unwrap_err(),
            ExecutionError::InvalidGenesisAllocation
        );
        assert_eq!(
            LedgerState::with_genesis_balances(
                Amount::from(10),
                vec![
                    (b"alice".to_vec(), Amount::from(5)),
                    (b"alice".to_vec(), Amount::from(5))
                ]
            )
            .unwrap_err(),
            ExecutionError::InvalidGenesisAllocation
        );
        assert_eq!(
            LedgerState::with_genesis_balances(
                Amount::from(1),
                vec![(vec![7; MAX_ACCOUNT_BYTES + 1], Amount::from(1))]
            )
            .unwrap_err(),
            ExecutionError::InvalidGenesisAllocation
        );
    }
}
