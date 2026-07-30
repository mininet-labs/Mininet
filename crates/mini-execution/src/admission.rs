//! Bounded local admission for payment claims awaiting proposal.
//!
//! This pool is convenience and denial-of-service containment, never
//! canonical truth. Admission does not reserve or move MINI; every candidate
//! must still pass finalized block execution.

use std::collections::BTreeMap;
use std::fmt;

use mini_settlement::{claim_digest, verify_claim_signature, CanonicalLedgerView, PaymentClaim};

use crate::{LedgerState, MAX_CLAIMS_PER_BLOCK};

/// Conservative defaults for one node's local pending-payment memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionPolicy {
    pub max_claims: usize,
    pub max_total_bytes: usize,
    pub max_claims_per_payer: usize,
}

impl Default for AdmissionPolicy {
    fn default() -> Self {
        Self {
            max_claims: 4_096,
            max_total_bytes: 8 * 1024 * 1024,
            max_claims_per_payer: 64,
        }
    }
}

/// Why a local node refused or evicted a pending claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AdmissionError {
    InvalidPolicy,
    MalformedWire,
    InvalidSignature,
    WrongNetwork,
    UnsupportedPayee,
    Expired,
    AlreadyResolved,
    Duplicate,
    ConflictingSequence,
    InsufficientAvailableBalance,
    TooManyClaims,
    TooManyClaimsForPayer,
    TooManyBytes,
}

impl fmt::Display for AdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::InvalidPolicy => "invalid admission policy",
                Self::MalformedWire => "malformed payment submission",
                Self::InvalidSignature => "claim signature is invalid",
                Self::WrongNetwork => "claim targets another settlement network",
                Self::UnsupportedPayee => "payee is not a supported account",
                Self::Expired => "claim validity window has elapsed",
                Self::AlreadyResolved => "claim is already canonically resolved",
                Self::Duplicate => "claim is already admitted",
                Self::ConflictingSequence => "another claim occupies this payer sequence",
                Self::InsufficientAvailableBalance => {
                    "payer balance cannot cover all locally admitted claims"
                }
                Self::TooManyClaims => "local admission claim limit reached",
                Self::TooManyClaimsForPayer => "local per-payer claim limit reached",
                Self::TooManyBytes => "local admission byte limit reached",
            }
        )
    }
}

impl std::error::Error for AdmissionError {}

/// One deterministic, bounded node-local set of claims awaiting proposal.
#[derive(Debug, Clone)]
pub struct PaymentAdmissionPool {
    policy: AdmissionPolicy,
    claims: BTreeMap<[u8; 32], PaymentClaim>,
    payer_slots: BTreeMap<Vec<u8>, BTreeMap<u64, [u8; 32]>>,
    payer_reserved_micro: BTreeMap<Vec<u8>, u128>,
    encoded_bytes: usize,
}

impl PaymentAdmissionPool {
    pub fn new(policy: AdmissionPolicy) -> Result<Self, AdmissionError> {
        if policy.max_claims == 0
            || policy.max_claims > MAX_CLAIMS_PER_BLOCK
            || policy.max_total_bytes == 0
            || policy.max_claims_per_payer == 0
            || policy.max_claims_per_payer > policy.max_claims
        {
            return Err(AdmissionError::InvalidPolicy);
        }
        Ok(Self {
            policy,
            claims: BTreeMap::new(),
            payer_slots: BTreeMap::new(),
            payer_reserved_micro: BTreeMap::new(),
            encoded_bytes: 0,
        })
    }

    pub fn len(&self) -> usize {
        self.claims.len()
    }

    pub fn is_empty(&self) -> bool {
        self.claims.is_empty()
    }

    pub fn encoded_bytes(&self) -> usize {
        self.encoded_bytes
    }

    /// Decode and admit one untrusted standalone payment submission.
    pub fn admit_wire(
        &mut self,
        bytes: &[u8],
        state: &LedgerState,
        now_ms: u64,
    ) -> Result<[u8; 32], AdmissionError> {
        let claim =
            PaymentClaim::from_wire_bytes(bytes).map_err(|_| AdmissionError::MalformedWire)?;
        self.admit(claim, state, now_ms)
    }

    /// Admit one signed claim against the latest finalized state.
    pub fn admit(
        &mut self,
        claim: PaymentClaim,
        state: &LedgerState,
        now_ms: u64,
    ) -> Result<[u8; 32], AdmissionError> {
        let bytes = claim
            .to_wire_bytes()
            .map_err(|_| AdmissionError::MalformedWire)?
            .len();
        validate_claim(&claim, state, now_ms)?;
        let digest = claim_digest(&claim);
        if self.claims.contains_key(&digest) {
            return Err(AdmissionError::Duplicate);
        }
        if self
            .payer_slots
            .get(&claim.payer)
            .is_some_and(|slots| slots.contains_key(&claim.sequence))
        {
            return Err(AdmissionError::ConflictingSequence);
        }
        if self.claims.len() >= self.policy.max_claims {
            return Err(AdmissionError::TooManyClaims);
        }
        let payer_count = self.payer_slots.get(&claim.payer).map_or(0, BTreeMap::len);
        if payer_count >= self.policy.max_claims_per_payer {
            return Err(AdmissionError::TooManyClaimsForPayer);
        }
        if self
            .encoded_bytes
            .checked_add(bytes)
            .is_none_or(|total| total > self.policy.max_total_bytes)
        {
            return Err(AdmissionError::TooManyBytes);
        }
        let reserved = self
            .payer_reserved_micro
            .get(&claim.payer)
            .copied()
            .unwrap_or(0);
        let next_reserved = reserved
            .checked_add(u128::from(claim.amount_micro))
            .ok_or(AdmissionError::InsufficientAvailableBalance)?;
        if next_reserved > state.balance(&claim.payer).as_micro() {
            return Err(AdmissionError::InsufficientAvailableBalance);
        }

        self.encoded_bytes += bytes;
        self.payer_reserved_micro
            .insert(claim.payer.clone(), next_reserved);
        self.payer_slots
            .entry(claim.payer.clone())
            .or_default()
            .insert(claim.sequence, digest);
        self.claims.insert(digest, claim);
        Ok(digest)
    }

    /// Deterministic candidate order independent of local arrival order.
    pub fn candidates(&self, limit: usize) -> Vec<PaymentClaim> {
        let mut ordered: Vec<_> = self
            .claims
            .iter()
            .map(|(digest, claim)| (claim.payer.as_slice(), claim.sequence, *digest, claim))
            .collect();
        ordered.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));
        ordered
            .into_iter()
            .take(limit.min(MAX_CLAIMS_PER_BLOCK))
            .map(|(_, _, _, claim)| claim.clone())
            .collect()
    }

    /// Remove claims invalidated by new canonical state or elapsed time.
    ///
    /// Returns `(digest, reason)` in digest order for deterministic logging.
    pub fn revalidate(
        &mut self,
        state: &LedgerState,
        now_ms: u64,
    ) -> Vec<([u8; 32], AdmissionError)> {
        let mut removals = Vec::new();
        let mut reserved = BTreeMap::<Vec<u8>, u128>::new();
        for claim in self.candidates(self.len()) {
            let digest = claim_digest(&claim);
            let reason = validate_claim(&claim, state, now_ms).err().or_else(|| {
                let total = reserved
                    .get(&claim.payer)
                    .copied()
                    .unwrap_or(0)
                    .checked_add(u128::from(claim.amount_micro));
                match total {
                    Some(total) if total <= state.balance(&claim.payer).as_micro() => {
                        reserved.insert(claim.payer.clone(), total);
                        None
                    }
                    _ => Some(AdmissionError::InsufficientAvailableBalance),
                }
            });
            if let Some(reason) = reason {
                removals.push((digest, reason));
            }
        }
        removals.sort_by_key(|(digest, _)| *digest);
        for (digest, _) in &removals {
            self.remove(digest);
        }
        removals
    }

    pub fn remove(&mut self, digest: &[u8; 32]) -> Option<PaymentClaim> {
        let claim = self.claims.remove(digest)?;
        let bytes = claim
            .to_wire_bytes()
            .expect("admitted claims were wire-bounded")
            .len();
        self.encoded_bytes -= bytes;
        if let Some(slots) = self.payer_slots.get_mut(&claim.payer) {
            slots.remove(&claim.sequence);
            if slots.is_empty() {
                self.payer_slots.remove(&claim.payer);
            }
        }
        let remaining = self
            .payer_reserved_micro
            .get(&claim.payer)
            .copied()
            .unwrap_or(0)
            .saturating_sub(u128::from(claim.amount_micro));
        if remaining == 0 {
            self.payer_reserved_micro.remove(&claim.payer);
        } else {
            self.payer_reserved_micro
                .insert(claim.payer.clone(), remaining);
        }
        Some(claim)
    }
}

fn validate_claim(
    claim: &PaymentClaim,
    state: &LedgerState,
    now_ms: u64,
) -> Result<(), AdmissionError> {
    verify_claim_signature(claim).map_err(|_| AdmissionError::InvalidSignature)?;
    if claim.network_id != state.network_id() {
        return Err(AdmissionError::WrongNetwork);
    }
    if !crate::state::is_supported_account(&claim.payee) {
        return Err(AdmissionError::UnsupportedPayee);
    }
    if now_ms >= claim.valid_until_ms {
        return Err(AdmissionError::Expired);
    }
    let digest = claim_digest(claim);
    if state.rejected_claim(&digest).is_some()
        || state
            .finalized_sequence(&claim.payer)
            .is_some_and(|sequence| sequence >= claim.sequence)
    {
        return Err(AdmissionError::AlreadyResolved);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_crypto::SigningKey;
    use mini_economy::Amount;
    use mini_settlement::{sign_claim, sign_claim_for_network, MININET_NETWORK_ID};

    fn payer(seed: u8) -> SigningKey {
        SigningKey::from_seed(&[seed; 32])
    }

    fn account(key: &SigningKey) -> Vec<u8> {
        key.verifying_key().to_bytes().to_vec()
    }

    fn recipient(seed: u8) -> Vec<u8> {
        account(&payer(seed))
    }

    fn state(key: &SigningKey, amount: u64) -> LedgerState {
        LedgerState::with_genesis_balances(
            Amount::from(amount),
            vec![(account(key), Amount::from(amount))],
        )
        .unwrap()
    }

    fn claim(key: &SigningKey, payee_seed: u8, amount: u64, sequence: u64) -> PaymentClaim {
        sign_claim(
            key,
            &recipient(payee_seed),
            amount,
            sequence,
            10_000,
            b"head",
            0,
        )
        .unwrap()
    }

    #[test]
    fn invalid_or_excessive_policies_fail_closed() {
        assert_eq!(
            PaymentAdmissionPool::new(AdmissionPolicy {
                max_claims: 0,
                ..AdmissionPolicy::default()
            })
            .unwrap_err(),
            AdmissionError::InvalidPolicy
        );
        assert_eq!(
            PaymentAdmissionPool::new(AdmissionPolicy {
                max_claims: MAX_CLAIMS_PER_BLOCK + 1,
                ..AdmissionPolicy::default()
            })
            .unwrap_err(),
            AdmissionError::InvalidPolicy
        );
    }

    #[test]
    fn candidate_order_is_independent_of_arrival_order() {
        let a = payer(0x31);
        let b = payer(0x32);
        let total = Amount::from(2_000);
        let ledger = LedgerState::with_genesis_balances(
            total,
            vec![
                (account(&a), Amount::from(1_000)),
                (account(&b), Amount::from(1_000)),
            ],
        )
        .unwrap();
        let a0 = claim(&a, 0x41, 100, 0);
        let a1 = claim(&a, 0x42, 100, 1);
        let b0 = claim(&b, 0x43, 100, 0);
        let mut first = PaymentAdmissionPool::new(AdmissionPolicy::default()).unwrap();
        let mut second = PaymentAdmissionPool::new(AdmissionPolicy::default()).unwrap();
        for item in [b0.clone(), a1.clone(), a0.clone()] {
            first.admit(item, &ledger, 1).unwrap();
        }
        for item in [a0, b0, a1] {
            second.admit(item, &ledger, 1).unwrap();
        }
        let first_digests: Vec<_> = first.candidates(10).iter().map(claim_digest).collect();
        let second_digests: Vec<_> = second.candidates(10).iter().map(claim_digest).collect();
        assert_eq!(first_digests, second_digests);
    }

    #[test]
    fn duplicate_conflict_and_aggregate_overspend_are_refused() {
        let key = payer(0x33);
        let ledger = state(&key, 1_000);
        let first = claim(&key, 0x41, 600, 0);
        let conflict = claim(&key, 0x42, 100, 0);
        let aggregate_overspend = claim(&key, 0x43, 401, 1);
        let mut pool = PaymentAdmissionPool::new(AdmissionPolicy::default()).unwrap();
        pool.admit(first.clone(), &ledger, 1).unwrap();
        assert_eq!(
            pool.admit(first, &ledger, 1).unwrap_err(),
            AdmissionError::Duplicate
        );
        assert_eq!(
            pool.admit(conflict, &ledger, 1).unwrap_err(),
            AdmissionError::ConflictingSequence
        );
        assert_eq!(
            pool.admit(aggregate_overspend, &ledger, 1).unwrap_err(),
            AdmissionError::InsufficientAvailableBalance
        );
    }

    #[test]
    fn signature_network_payee_and_expiry_are_checked_before_storage() {
        let key = payer(0x34);
        let ledger = state(&key, 1_000);
        let mut pool = PaymentAdmissionPool::new(AdmissionPolicy::default()).unwrap();

        let mut forged = claim(&key, 0x41, 1, 0);
        forged.amount_micro = 2;
        assert_eq!(
            pool.admit(forged, &ledger, 1).unwrap_err(),
            AdmissionError::InvalidSignature
        );
        let foreign = sign_claim_for_network(
            &key,
            &recipient(0x41),
            1,
            0,
            10_000,
            &[0x99; 32],
            b"head",
            0,
        )
        .unwrap();
        assert_eq!(
            pool.admit(foreign, &ledger, 1).unwrap_err(),
            AdmissionError::WrongNetwork
        );
        let unsupported = sign_claim(&key, b"not-a-key", 1, 0, 10_000, b"head", 0).unwrap();
        assert_eq!(
            pool.admit(unsupported, &ledger, 1).unwrap_err(),
            AdmissionError::UnsupportedPayee
        );
        let oversized_hint = vec![0; mini_settlement::MAX_CLAIM_FIELD_BYTES + 1];
        let oversized =
            sign_claim(&key, &recipient(0x41), 1, 0, 10_000, &oversized_hint, 0).unwrap();
        assert_eq!(
            pool.admit(oversized, &ledger, 1).unwrap_err(),
            AdmissionError::MalformedWire
        );
        let expired = sign_claim_for_network(
            &key,
            &recipient(0x41),
            1,
            0,
            5,
            &MININET_NETWORK_ID,
            b"head",
            0,
        )
        .unwrap();
        assert_eq!(
            pool.admit(expired, &ledger, 5).unwrap_err(),
            AdmissionError::Expired
        );
        assert!(pool.is_empty());
    }

    #[test]
    fn standalone_wire_submission_reaches_the_same_admission_path() {
        let key = payer(0x37);
        let ledger = state(&key, 1_000);
        let claim = claim(&key, 0x41, 100, 0);
        let digest = claim_digest(&claim);
        let mut pool = PaymentAdmissionPool::new(AdmissionPolicy::default()).unwrap();
        assert_eq!(
            pool.admit_wire(&claim.to_wire_bytes().unwrap(), &ledger, 1)
                .unwrap(),
            digest
        );
        assert_eq!(claim_digest(&pool.candidates(1)[0]), digest);
        assert_eq!(
            pool.admit_wire(b"not-a-claim", &ledger, 1).unwrap_err(),
            AdmissionError::MalformedWire
        );
    }

    #[test]
    fn count_per_payer_and_byte_limits_are_independent() {
        let key = payer(0x35);
        let ledger = state(&key, 1_000);
        let sample = claim(&key, 0x41, 1, 0);
        let sample_bytes = sample.to_wire_bytes().unwrap().len();

        let mut payer_limited = PaymentAdmissionPool::new(AdmissionPolicy {
            max_claims: 2,
            max_total_bytes: sample_bytes * 2,
            max_claims_per_payer: 1,
        })
        .unwrap();
        payer_limited.admit(sample, &ledger, 1).unwrap();
        assert_eq!(
            payer_limited
                .admit(claim(&key, 0x42, 1, 1), &ledger, 1)
                .unwrap_err(),
            AdmissionError::TooManyClaimsForPayer
        );

        let mut byte_limited = PaymentAdmissionPool::new(AdmissionPolicy {
            max_claims: 2,
            max_total_bytes: sample_bytes,
            max_claims_per_payer: 2,
        })
        .unwrap();
        byte_limited
            .admit(claim(&key, 0x41, 1, 0), &ledger, 1)
            .unwrap();
        assert_eq!(
            byte_limited
                .admit(claim(&key, 0x42, 1, 1), &ledger, 1)
                .unwrap_err(),
            AdmissionError::TooManyBytes
        );
    }

    #[test]
    fn revalidation_evicts_finalized_expired_and_now_unaffordable_claims() {
        let key = payer(0x36);
        let initial = state(&key, 1_000);
        let finalized_claim = claim(&key, 0x41, 500, 0);
        let later = claim(&key, 0x42, 400, 1);
        let expiring = sign_claim(&key, &recipient(0x43), 100, 2, 2, b"head", 0).unwrap();
        let mut pool = PaymentAdmissionPool::new(AdmissionPolicy::default()).unwrap();
        pool.admit(finalized_claim.clone(), &initial, 1).unwrap();
        pool.admit(later.clone(), &initial, 1).unwrap();
        pool.admit(expiring, &initial, 1).unwrap();

        let winning_conflict = claim(&key, 0x44, 700, 0);
        let next = crate::apply_block(
            &initial,
            &crate::SettlementBlockBody::new(vec![winning_conflict]),
        )
        .unwrap();
        let removed = pool.revalidate(&next, 2);
        assert_eq!(removed.len(), 3);
        assert!(removed
            .iter()
            .any(|(_, reason)| *reason == AdmissionError::AlreadyResolved));
        assert!(removed
            .iter()
            .any(|(_, reason)| { *reason == AdmissionError::InsufficientAvailableBalance }));
        assert!(removed
            .iter()
            .any(|(_, reason)| *reason == AdmissionError::Expired));
        assert!(pool.is_empty());
        assert_eq!(pool.encoded_bytes(), 0);
    }
}
