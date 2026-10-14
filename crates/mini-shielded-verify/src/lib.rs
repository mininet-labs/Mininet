//! The concrete "whoever cares" validator-side shielded-spend verifier
//! (D-0474, [roadmap R8](../../../docs/ROADMAP_TO_RELEASE.md)) —
//! `mini_execution::nullifier`'s own docs name this exact split:
//! "Verification belongs where it already is — `mini_private_payment
//! ::verify`, run by whoever cares — and the chain orders the results."
//! This crate is that "whoever cares," kept in its own crate specifically
//! so `mini-execution`/`mini-consensus`/`mini-chain` never link
//! `mini-private-payment`/`mini-value` (the voice/value wall, P1,
//! Directive 16) — the same reasoning `mini_chain::ValidatorOracle`'s own
//! split already established for identity/KEL material.
//!
//! ## How a validator uses this
//!
//! ```ignore
//! let evidence = Arc::new(ClaimEvidencePool::new());
//! // ... elsewhere, as full claims arrive over the network:
//! evidence.insert(claim_wire_bytes)?;
//! // ... constructing the node:
//! let verifier = Arc::new(ShieldedClaimVerifier::new(network_id, evidence.clone()));
//! let node = ConsensusNode::new(config).with_claim_verifier(verifier);
//! ```
//!
//! ## Honest limits
//!
//! - **No claim-evidence gossip protocol.** [`ClaimEvidencePool`] is a
//!   plain local store; how full claim bytes actually reach a validator
//!   (a dedicated gossip topic, a request/response protocol, an existing
//!   mempool) is not this crate's job — it only defines the shape a
//!   validator's own evidence-gathering component must fill.
//! - **Bounded admission, no pruning/eviction policy.** Finite local entry,
//!   total-byte and per-claim budgets fail closed at capacity. Defaults are
//!   operational budgets, not protocol validity rules. Safe historical
//!   retention, persistence and recovery remain host responsibilities.
//! - **No accountability/measurement layer.** Roadmap R8 asks for a
//!   validator set that "verifies claims and is measured for it" — this
//!   crate provides the verification; recording *which* validators ran
//!   it (an evidence trail analogous to `mini_consensus::evidence`'s
//!   equivocation proofs) remains unbuilt.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock};

use mini_execution::{
    ClaimVerifier, NullifierRecord, ShieldedClaimEffects, ShieldedGenesisAllocation, ShieldedOutput,
};
use mini_private_payment::PrivatePaymentClaim;

/// A claim's wire bytes failed structural decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MalformedClaim;

/// Local resource budgets, not canonical claim-validity rules.
/// Zero limits deliberately disable admission; no claim is evicted implicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvidenceLimits {
    pub max_claims: usize,
    pub max_total_bytes: usize,
    pub max_claim_bytes: usize,
}

impl Default for EvidenceLimits {
    fn default() -> Self {
        Self {
            max_claims: 1024,
            max_total_bytes: 64 * 1024 * 1024,
            max_claim_bytes: 1024 * 1024,
        }
    }
}

/// Admission failures never mutate retained evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceError {
    MalformedClaim(MalformedClaim),
    ClaimTooLarge,
    ClaimCountLimit,
    TotalBytesLimit,
}

impl core::fmt::Display for EvidenceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "shielded evidence admission refused: {self:?}")
    }
}

impl std::error::Error for EvidenceError {}

#[derive(Debug, Default)]
struct EvidenceState {
    claims: BTreeMap<[u8; 32], Vec<u8>>,
    total_bytes: usize,
}

/// A bounded local store of structurally decoded shielded claim bytes.
/// Admission is not cryptographic verification. The verifier still checks
/// the complete proof and canonical execution still checks membership.
/// Limits bound retained wire bytes and entry count, not allocator overhead,
/// caller-owned buffers, transient decoding, or concurrent lookup copies.
/// Historical evidence is never evicted: a full pool fails closed, and the
/// host must reconcile retention/recovery requirements before replacing it.
#[derive(Debug)]
pub struct ClaimEvidencePool {
    limits: EvidenceLimits,
    state: RwLock<EvidenceState>,
}

impl Default for ClaimEvidencePool {
    fn default() -> Self {
        Self::with_limits(EvidenceLimits::default())
    }
}

impl ClaimEvidencePool {
    /// A fresh pool with finite default local budgets.
    pub fn new() -> Self {
        Self::default()
    }

    /// Use explicit host-selected local budgets.
    pub fn with_limits(limits: EvidenceLimits) -> Self {
        Self {
            limits,
            state: RwLock::new(EvidenceState::default()),
        }
    }

    /// Store structurally decoded bytes under their computed transcript digest.
    /// Exact retries consume no capacity. Replacement under an existing digest
    /// consumes only the replacement byte size, preserving prior semantics.
    /// Admission is structural: invalid proof bytes can replace valid evidence,
    /// so retained bytes never establish trust without verification.
    /// Capacity checks and admission share one write lock so concurrent
    /// insertions cannot exceed a budget.
    pub fn insert(&self, claim_bytes: Vec<u8>) -> Result<[u8; 32], EvidenceError> {
        if claim_bytes.len() > self.limits.max_claim_bytes {
            return Err(EvidenceError::ClaimTooLarge);
        }
        let claim = PrivatePaymentClaim::decode(&claim_bytes)
            .map_err(|_| EvidenceError::MalformedClaim(MalformedClaim))?;
        let digest = claim.transcript_digest();
        let mut state = self.state.write().expect("claim evidence pool lock poisoned");
        let existing_len = match state.claims.get(&digest) {
            Some(existing) if existing == &claim_bytes => return Ok(digest),
            Some(existing) => Some(existing.len()),
            None => None,
        };
        if existing_len.is_none() && state.claims.len() >= self.limits.max_claims {
            return Err(EvidenceError::ClaimCountLimit);
        }
        let total_bytes = state
            .total_bytes
            .checked_sub(existing_len.unwrap_or(0))
            .and_then(|total| total.checked_add(claim_bytes.len()))
            .filter(|total| *total <= self.limits.max_total_bytes)
            .ok_or(EvidenceError::TotalBytesLimit)?;
        state.claims.insert(digest, claim_bytes);
        state.total_bytes = total_bytes;
        Ok(digest)
    }

    /// Copy retained bytes, if present.
    pub fn get(&self, digest: &[u8; 32]) -> Option<Vec<u8>> {
        self.state
            .read()
            .expect("claim evidence pool lock poisoned")
            .claims
            .get(digest)
            .cloned()
    }

    /// Retained claim count.
    pub fn len(&self) -> usize {
        self.state
            .read()
            .expect("claim evidence pool lock poisoned")
            .claims
            .len()
    }

    /// Retained wire-byte total, excluding allocator overhead and copies.
    pub fn total_bytes(&self) -> usize {
        self.state
            .read()
            .expect("claim evidence pool lock poisoned")
            .total_bytes
    }

    /// Whether no evidence is retained.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The concrete [`ClaimVerifier`]: looks a claim up in its own
/// [`ClaimEvidencePool`], decodes and verifies it with
/// `mini_private_payment::verify`, and confirms the result's own key
/// images and transcript digest exactly match what it is being asked to
/// vouch for — a claim existing under the right digest is necessary but
/// never sufficient; it must also actually verify and its key images must
/// exactly match the group, no more and no fewer.
#[derive(Debug)]
pub struct ShieldedClaimVerifier {
    network_id: [u8; 32],
    evidence: Arc<ClaimEvidencePool>,
}

impl ShieldedClaimVerifier {
    /// A verifier checking claims against `network_id`, reading evidence
    /// from `evidence` — typically shared (via the same `Arc`) with
    /// whatever component populates the pool as claims arrive.
    pub fn new(network_id: [u8; 32], evidence: Arc<ClaimEvidencePool>) -> Self {
        ShieldedClaimVerifier {
            network_id,
            evidence,
        }
    }
}

impl ClaimVerifier for ShieldedClaimVerifier {
    fn verify_claim(
        &self,
        network_id: &[u8; 32],
        digest: &[u8; 32],
        group: &[NullifierRecord],
    ) -> Option<ShieldedClaimEffects> {
        if network_id != &self.network_id {
            return None;
        }
        let Some(bytes) = self.evidence.get(digest) else {
            // No evidence, no trust — the honest default a caller-injected
            // verifier must have (see mini-execution::ClaimVerifier's own
            // docs: never on the claim's mere presence, and here there is
            // not even that).
            return None;
        };
        let Ok(claim) = PrivatePaymentClaim::decode(&bytes) else {
            return None;
        };
        let Ok(verified) = mini_private_payment::verify(&claim, &self.network_id) else {
            return None;
        };
        if verified.transcript_digest() != digest {
            return None;
        }
        let mut expected: BTreeSet<Vec<u8>> = verified.key_images().map(|k| k.to_vec()).collect();
        if expected.len() != group.len() {
            return None;
        }
        for record in group {
            // Defense in depth: `apply_nullifiers` already only ever
            // groups records sharing one digest before calling this, but
            // a verifier is a trust boundary — it must not assume every
            // future caller upholds that precondition perfectly.
            if record.claim_digest != *digest {
                return None;
            }
            if !expected.remove(&record.key_image) {
                return None;
            }
        }
        if !expected.is_empty() {
            return None;
        }
        if !claim.outputs.iter().all(|output| {
            mini_value::one_time_key_is_well_formed(&output.output.one_time_address)
                && mini_value::one_time_key_is_well_formed(&output.output.tx_public_key)
        }) {
            return None;
        }
        Some(ShieldedClaimEffects {
            ring_members: claim
                .inputs
                .iter()
                .flat_map(|input| {
                    input
                        .ring
                        .iter()
                        .zip(&input.ring_commitments)
                        .map(|(key, commitment)| ShieldedOutput {
                            public_key: key.clone(),
                            amount_commitment: commitment.clone(),
                        })
                })
                .collect(),
            outputs: claim
                .outputs
                .iter()
                .map(|output| ShieldedOutput {
                    public_key: output.output.one_time_address.clone(),
                    amount_commitment: output.amount_commitment.clone(),
                })
                .collect(),
            fee_micro: claim.fee_micro,
        })
    }
    fn verify_genesis_allocation(
        &self,
        network_id: &[u8; 32],
        allocation: &ShieldedGenesisAllocation,
    ) -> bool {
        network_id == &self.network_id
            && mini_value::one_time_key_is_well_formed(&allocation.output.public_key)
            && allocation.output.amount_commitment
                == mini_value::public_amount_commitment(allocation.amount_micro)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_pool_holds_nothing() {
        let pool = ClaimEvidencePool::new();
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
        assert_eq!(pool.get(&[0u8; 32]), None);
    }

    #[test]
    fn malformed_bytes_are_rejected_before_storage() {
        let pool = ClaimEvidencePool::new();
        assert_eq!(
            pool.insert(vec![0xff; 4]),
            Err(EvidenceError::MalformedClaim(MalformedClaim))
        );
        assert!(pool.is_empty());
    }

    #[test]
    fn a_verifier_with_no_evidence_never_trusts_a_claim() {
        let evidence = Arc::new(ClaimEvidencePool::new());
        let verifier = ShieldedClaimVerifier::new([7u8; 32], evidence);
        let group = [NullifierRecord::new(vec![1u8; 32], [9u8; 32])];
        assert!(verifier
            .verify_claim(&[7; 32], &[9u8; 32], &group)
            .is_none());
    }

    #[test]
    fn a_verifier_rejects_a_group_whose_own_digest_does_not_match_the_records() {
        // Defense-in-depth path: even if a caller mismatches digest and
        // group (which honest callers never do), the verifier must not
        // trust it.
        let evidence = Arc::new(ClaimEvidencePool::new());
        let verifier = ShieldedClaimVerifier::new([7u8; 32], evidence);
        let mismatched_group = [NullifierRecord::new(vec![1u8; 32], [1u8; 32])];
        assert!(verifier
            .verify_claim(&[7; 32], &[9u8; 32], &mismatched_group)
            .is_none());
    }
}
