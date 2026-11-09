//! The V3 validator-side shielded-spend verifier (research PR #429, P0-1; Gate
//! #72 audit Section 11 "canonical claim bytes in consensus").
//!
//! A separate type from the V2 [`crate::ShieldedClaimVerifier`] on purpose. V2
//! and V3 use different commitment generator bases, so one ledger can never
//! serve both: a network's shielded genesis is committed under exactly one
//! basis, and a verifier of the other basis refuses it (research test T05).
//! This verifier therefore accepts only V3 claims and only genesis
//! allocations committed under the V3 (`bp_range_v2`) basis; it never converts
//! between bases and has no fallback.
//!
//! What is different from V2:
//!
//! - Evidence is keyed by `claim_id`, which hashes the **complete** encoded
//!   claim including signatures (T08/T15). Two different byte strings can
//!   therefore never share a key, so evidence under a key cannot be replaced
//!   by different bytes, unlike the V2 pool's transcript digest.
//! - Admission requires canonical encoding: bytes that decode but do not
//!   re-encode to themselves are refused before anything is stored.
//! - The fee in the returned effects is the deterministic
//!   [`mini_private_payment::quote_fee_micro`] value the proof balanced
//!   against, not a number the claim carries.
//!
//! Admission is still only structural; verification (range proofs, every MLSAG,
//! balance including the fee, repeated key images) happens in
//! [`ClaimVerifier::verify_claim`], and canonical membership, unspent key
//! images and atomic application are `mini-execution`'s job. Missing evidence
//! is a failure, never a pass.
//!
//! Honest limits: unaudited (Gate #72/#93 still open); the fee function is the
//! placeholder `quote_fee_micro` documents; there is no gossip or durable
//! retention of evidence here (host responsibility, as for V2).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock};

use mini_execution::{
    ClaimVerifier, NullifierRecord, ShieldedClaimEffects, ShieldedGenesisAllocation, ShieldedOutput,
};
use mini_private_payment::{quote_fee_micro, verify_v3, PrivatePaymentClaimV3, MAX_CLAIM_V3_BYTES};

use crate::{EvidenceError, EvidenceLimits, MalformedClaim};

#[derive(Debug, Default)]
struct EvidenceState {
    claims: BTreeMap<[u8; 32], Vec<u8>>,
    total_bytes: usize,
}

/// A bounded local store of canonically encoded V3 claim bytes, keyed by
/// `claim_id`. Admission is structural, never trust.
#[derive(Debug)]
pub struct ClaimEvidencePoolV3 {
    limits: EvidenceLimits,
    state: RwLock<EvidenceState>,
}

impl Default for ClaimEvidencePoolV3 {
    fn default() -> Self {
        Self::with_limits(EvidenceLimits::default())
    }
}

impl ClaimEvidencePoolV3 {
    /// A pool with finite default local budgets.
    pub fn new() -> Self {
        Self::default()
    }

    /// A pool with explicit host-selected local budgets.
    pub fn with_limits(limits: EvidenceLimits) -> Self {
        Self {
            limits,
            state: RwLock::new(EvidenceState::default()),
        }
    }

    /// Admit canonically encoded V3 claim bytes under their `claim_id`.
    ///
    /// Refuses oversize bytes, anything that does not decode strictly, and
    /// anything that does not re-encode to exactly the same bytes. Because the
    /// key covers every byte, an existing key can only ever hold identical
    /// bytes, so a retry costs nothing and nothing can be replaced.
    pub fn insert(&self, claim_bytes: Vec<u8>) -> Result<[u8; 32], EvidenceError> {
        if claim_bytes.len() > self.limits.max_claim_bytes.min(MAX_CLAIM_V3_BYTES) {
            return Err(EvidenceError::ClaimTooLarge);
        }
        let claim = PrivatePaymentClaimV3::decode(&claim_bytes)
            .map_err(|_| EvidenceError::MalformedClaim(MalformedClaim))?;
        if claim.encode() != claim_bytes {
            return Err(EvidenceError::MalformedClaim(MalformedClaim));
        }
        let claim_id = claim.claim_id();
        let mut state = self
            .state
            .write()
            .expect("claim evidence pool lock poisoned");
        if state.claims.contains_key(&claim_id) {
            return Ok(claim_id);
        }
        if state.claims.len() >= self.limits.max_claims {
            return Err(EvidenceError::ClaimCountLimit);
        }
        let total_bytes = state
            .total_bytes
            .checked_add(claim_bytes.len())
            .filter(|total| *total <= self.limits.max_total_bytes)
            .ok_or(EvidenceError::TotalBytesLimit)?;
        state.claims.insert(claim_id, claim_bytes);
        state.total_bytes = total_bytes;
        Ok(claim_id)
    }

    /// Copy retained bytes, if present.
    pub fn get(&self, claim_id: &[u8; 32]) -> Option<Vec<u8>> {
        self.state
            .read()
            .expect("claim evidence pool lock poisoned")
            .claims
            .get(claim_id)
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

    /// Retained wire-byte total.
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

    /// Write every retained claim to `path` (atomic replace, fsync-ordered),
    /// in `claim_id` order so the file is deterministic. A crash leaves the old
    /// or the new file, never a torn one.
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let state = self
            .state
            .read()
            .expect("claim evidence pool lock poisoned");
        let mut out = Vec::with_capacity(8 + state.total_bytes + state.claims.len() * 4);
        out.extend_from_slice(DURABLE_MAGIC);
        out.extend_from_slice(&(state.claims.len() as u32).to_be_bytes());
        for (id, bytes) in &state.claims {
            out.extend_from_slice(id);
            out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            out.extend_from_slice(bytes);
        }
        mini_durable::atomic_replace(path, &out)
    }

    /// Rebuild a pool from a file written by [`ClaimEvidencePoolV3::save`]. A
    /// missing file is an empty pool. Every entry goes back through
    /// [`ClaimEvidencePoolV3::insert`], so the file is no more trusted than
    /// live gossip: it must decode strictly, re-encode to itself, fit the
    /// limits, and match the `claim_id` recorded beside it, in order. A
    /// truncated, padded, duplicated, out-of-order or otherwise corrupt file
    /// fails closed with `InvalidData`; nothing is partially loaded.
    pub fn load(path: &std::path::Path, limits: EvidenceLimits) -> std::io::Result<Self> {
        let bad = |why: &'static str| std::io::Error::new(std::io::ErrorKind::InvalidData, why);
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::with_limits(limits))
            }
            Err(e) => return Err(e),
        };
        let pool = Self::with_limits(limits);
        let mut cursor = bytes.as_slice();
        let take = |cursor: &mut &[u8], n: usize| -> Option<Vec<u8>> {
            if cursor.len() < n {
                return None;
            }
            let (head, rest) = cursor.split_at(n);
            *cursor = rest;
            Some(head.to_vec())
        };
        if take(&mut cursor, DURABLE_MAGIC.len()).as_deref() != Some(DURABLE_MAGIC) {
            return Err(bad("not a claim evidence file"));
        }
        let count_bytes = take(&mut cursor, 4).ok_or_else(|| bad("truncated"))?;
        let count = u32::from_be_bytes(count_bytes.try_into().expect("four bytes")) as usize;
        if count > pool.limits.max_claims {
            return Err(bad("more claims than the limit"));
        }
        let mut previous: Option<[u8; 32]> = None;
        for _ in 0..count {
            let stored_id = take(&mut cursor, 32).ok_or_else(|| bad("truncated"))?;
            let len_bytes = take(&mut cursor, 4).ok_or_else(|| bad("truncated"))?;
            let len = u32::from_be_bytes(len_bytes.try_into().expect("four bytes")) as usize;
            let claim = take(&mut cursor, len).ok_or_else(|| bad("truncated"))?;
            let id = pool
                .insert(claim)
                .map_err(|_| bad("an entry was refused on reload"))?;
            // The recorded id covers every byte of the claim, so any bit rot or
            // edit inside an entry is caught here, not only framing damage.
            if stored_id.as_slice() != id.as_slice() {
                return Err(bad("an entry does not match its recorded claim id"));
            }
            if previous.is_some_and(|p| p >= id) {
                return Err(bad("entries duplicated or out of order"));
            }
            previous = Some(id);
        }
        if !cursor.is_empty() {
            return Err(bad("trailing bytes"));
        }
        Ok(pool)
    }
}

const DURABLE_MAGIC: &[u8] = b"mininet/claim-evidence/v3\n";

/// The V3 [`ClaimVerifier`]. The `digest` execution passes is the V3
/// `claim_id`; the nullifier records execution groups under it must be exactly
/// the claim's key images.
#[derive(Debug)]
pub struct ShieldedClaimVerifierV3 {
    network_id: [u8; 32],
    evidence: Arc<ClaimEvidencePoolV3>,
}

impl ShieldedClaimVerifierV3 {
    /// Verify claims for `network_id` using evidence from `evidence`.
    pub fn new(network_id: [u8; 32], evidence: Arc<ClaimEvidencePoolV3>) -> Self {
        Self {
            network_id,
            evidence,
        }
    }
}

impl ClaimVerifier for ShieldedClaimVerifierV3 {
    fn verify_claim(
        &self,
        network_id: &[u8; 32],
        digest: &[u8; 32],
        group: &[NullifierRecord],
    ) -> Option<ShieldedClaimEffects> {
        if network_id != &self.network_id {
            return None;
        }
        // No evidence, no trust.
        let bytes = self.evidence.get(digest)?;
        let claim = PrivatePaymentClaimV3::decode(&bytes).ok()?;
        // Exact bytes must still be canonical and bound to the id asked about.
        if claim.encode() != bytes || &claim.claim_id() != digest {
            return None;
        }
        let verified = verify_v3(&claim, &self.network_id).ok()?;
        if verified.claim_id() != digest {
            return None;
        }
        let mut expected: BTreeSet<Vec<u8>> = verified.key_images().map(<[u8]>::to_vec).collect();
        if expected.len() != group.len() {
            return None;
        }
        for record in group {
            if &record.claim_digest != digest || !expected.remove(&record.key_image) {
                return None;
            }
        }
        if !expected.is_empty() {
            return None;
        }
        if !claim.outputs.iter().all(|output| {
            mini_value::one_time_key_is_well_formed(&output.one_time_address)
                && mini_value::one_time_key_is_well_formed(&output.tx_public_key)
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
                    public_key: output.one_time_address.clone(),
                    amount_commitment: output.amount_commitment.clone(),
                })
                .collect(),
            fee_micro: quote_fee_micro(
                &claim.fee_policy_id,
                claim.inputs.len(),
                claim.outputs.len(),
            ),
        })
    }

    fn verify_genesis_allocation(
        &self,
        network_id: &[u8; 32],
        allocation: &ShieldedGenesisAllocation,
    ) -> bool {
        // The V3 basis only: a genesis committed under the V2 basis cannot
        // balance against V3 proofs, so it is refused here, not converted.
        network_id == &self.network_id
            && mini_value::one_time_key_is_well_formed(&allocation.output.public_key)
            && allocation.output.amount_commitment
                == mini_value::public_amount_commitment_v2(allocation.amount_micro)
    }
}
