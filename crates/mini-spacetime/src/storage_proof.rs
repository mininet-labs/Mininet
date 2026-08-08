//! A real (Merkle/PDP-style) [`crate::proof::ProofOfSpaceTimeSource`]
//! implementation — the founder-directed "start simple" interim scheme
//! (D-0037/D-0038): periodic random-challenge, Merkle-proof responses,
//! well-documented (Ateniese et al.'s Provable Data Possession), achievable
//! now.
//!
//! ## What this proves, and what it does not
//!
//! A storage node commits to a [`StorageCommitment`] (a Merkle root over
//! its stored blocks). A verifier challenges a specific block index; the
//! node must return that block's *actual bytes* plus a Merkle proof
//! ([`verify_storage_challenge`]) — producing a valid response requires
//! having genuinely retained the real data, not just the previously-
//! published root. Repeating this over time
//! ([`ProofHistory`]/[`StorageWindowPolicy`]) demonstrates *continuous*
//! possession, the "time" half of proof-of-space-**time**.
//!
//! **What it does not prove: replication uniqueness.** This scheme cannot
//! tell the difference between a thousand honest small devices each
//! holding their own copy and one well-resourced server holding a single
//! copy and answering every challenge on their behalf — exactly the
//! warehouse-consolidation attack the whitepaper's egalitarian thesis
//! ("a thousand cheap, slow, scattered machines genuinely outcompete a
//! single warehouse," §7) depends on resisting. Real proof-of-replication
//! (Filecoin-style sequential/time-locked encoding) is the construction
//! that closes that gap, and is deliberately treated as a separate, later,
//! dedicated project rather than compressed into this pass.

use crate::merkle::MerkleProof;
use crate::proof::ProofOfSpaceTimeSource;

/// A storage node's public commitment: the Merkle root over its claimed
/// blocks, how many blocks it covers, and how large each one is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageCommitment {
    /// Root of the Merkle tree over the committed blocks.
    pub merkle_root: [u8; 32],
    /// How many blocks the commitment covers.
    pub block_count: usize,
    /// Size of every committed block, in bytes.
    ///
    /// Uniform by construction: a commitment over variably-sized blocks
    /// could name any byte total it liked, because `block_count` alone says
    /// nothing about volume. Eight one-byte blocks and eight one-mebibyte
    /// blocks are the same `block_count`.
    ///
    /// This field is **not** merely asserted.
    /// [`verify_storage_challenge`] requires every answered block to be
    /// exactly this long, so a provider claiming large blocks has to serve
    /// large blocks on every challenge or fail them. That is what makes
    /// [`StorageCommitment::committed_bytes`] a derived quantity rather
    /// than a declaration.
    pub block_size_bytes: u32,
}

impl StorageCommitment {
    /// Total bytes this commitment covers, saturating rather than wrapping.
    ///
    /// Every input is inside the commitment and enforced at challenge time,
    /// so this is derived from checked evidence — unlike a caller-supplied
    /// capacity figure, which is derived from a caller's opinion.
    pub fn committed_bytes(&self) -> u64 {
        (self.block_count as u64).saturating_mul(u64::from(self.block_size_bytes))
    }
}

/// How committed bytes convert into the capacity units a weighting layer
/// counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageUnitPolicy {
    bytes_per_unit: u64,
}

impl StorageUnitPolicy {
    /// One unit per `bytes_per_unit` committed bytes. Zero is refused: it
    /// would make every commitment worth infinite capacity.
    pub fn new(bytes_per_unit: u64) -> Option<Self> {
        if bytes_per_unit == 0 {
            return None;
        }
        Some(Self { bytes_per_unit })
    }

    /// One unit per gibibyte.
    pub fn gibibytes() -> Self {
        Self {
            bytes_per_unit: 1024 * 1024 * 1024,
        }
    }

    pub fn bytes_per_unit(&self) -> u64 {
        self.bytes_per_unit
    }
}

/// Capacity that something actually checked.
///
/// **There is deliberately no constructor taking a number.** The only ways
/// to obtain one are [`ProvenCapacity::from_commitment`], which derives it
/// from a [`StorageCommitment`] whose block size is enforced on every
/// challenge, and [`ProvenCapacity::none`]. That is the whole point of the
/// type: [`crate::proposer_weight`] weights block production, and a
/// function that exercises that much authority must not be reachable with a
/// number a caller typed.
///
/// Before this existed, `proposer_weight` took a bare `u64` and its own
/// documentation said it "trusts its input completely" — so a provider
/// could commit a single 32-byte block, prove it honestly, and then declare
/// a million units. That inverts the thesis the storage design rests on:
/// "a thousand cheap machines outcompete one warehouse" holds only while
/// capacity must be proven, since a warehouse and a Raspberry Pi type a
/// large number equally cheaply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProvenCapacity {
    units: u64,
    committed_bytes: u64,
}

impl ProvenCapacity {
    /// Derive capacity from a commitment. Truncating division: a commitment
    /// smaller than one unit counts as zero rather than rounding up into
    /// capacity nobody committed.
    pub fn from_commitment(commitment: &StorageCommitment, policy: &StorageUnitPolicy) -> Self {
        let committed_bytes = commitment.committed_bytes();
        Self {
            units: committed_bytes / policy.bytes_per_unit(),
            committed_bytes,
        }
    }

    /// No proven capacity — what an unproven or lapsed commitment
    /// contributes.
    pub fn none() -> Self {
        Self {
            units: 0,
            committed_bytes: 0,
        }
    }

    /// Capacity units, for a weighting layer to consume.
    pub fn units(&self) -> u64 {
        self.units
    }

    /// The committed bytes those units were derived from.
    pub fn committed_bytes(&self) -> u64 {
        self.committed_bytes
    }

    /// Total two proven capacities, saturating rather than wrapping.
    ///
    /// Sound because both operands were derived: a sum of checked
    /// quantities is itself checked, and no sequence of additions can mint
    /// capacity from nothing — [`ProvenCapacity::none`] added any number of
    /// times is still zero. This is the only arithmetic the type permits,
    /// and it exists so a provider holding several replicas can be totalled
    /// without anyone unwrapping to `u64` and back.
    ///
    /// It does **not** check that the two came from *different*
    /// commitments. Adding one replica's capacity to itself double-counts
    /// it — that is a caller bug the type cannot see, so callers totalling
    /// a set must key it by something unique (`ProviderStanding` keys by
    /// replica root).
    pub fn saturating_add(self, other: Self) -> Self {
        Self {
            units: self.units.saturating_add(other.units),
            committed_bytes: self.committed_bytes.saturating_add(other.committed_bytes),
        }
    }
}

/// A verifier's challenge: prove possession of the block at this index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageChallenge {
    /// The challenged block index.
    pub leaf_index: usize,
}

/// A node's response to a [`StorageChallenge`]: the actual block bytes,
/// plus a Merkle proof they belong at the challenged index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageChallengeResponse {
    /// Must match the challenge's index.
    pub leaf_index: usize,
    /// The actual bytes of the challenged block.
    pub block_bytes: Vec<u8>,
    /// Merkle membership proof for `block_bytes` at `leaf_index`.
    pub proof: MerkleProof,
}

/// Verify a challenge response against the commitment **and the challenge it
/// answers**.
///
/// The challenge argument is load-bearing, and its absence was a real defect.
/// Without it this function could only ask "is this a genuine leaf of the
/// committed tree", which any leaf satisfies. A prover asked for leaf 7 could
/// answer leaf 3 and pass — so proving possession required keeping exactly one
/// leaf and its Merkle path, a few hundred bytes, rather than the data. The
/// whole point of a spot check is that the prover cannot know in advance which
/// index it must hold, and that only bites if the answer is checked against
/// the question.
pub fn verify_storage_challenge(
    commitment: &StorageCommitment,
    challenge: &StorageChallenge,
    response: &StorageChallengeResponse,
) -> bool {
    challenge.leaf_index == response.leaf_index
        && response.leaf_index == response.proof.leaf_index
        // The block must be exactly the size the commitment claims. Without
        // this, `block_size_bytes` would be a number a provider asserts,
        // and capacity derived from it would be no better than the
        // caller-supplied figure this type exists to replace: commit to
        // huge blocks, serve tiny ones, collect the weight.
        && response.block_bytes.len() == commitment.block_size_bytes as usize
        && response.proof.verify(
            &response.block_bytes,
            commitment.merkle_root,
            commitment.block_count,
        )
}

/// How tightly spaced successful challenge responses must be, and how
/// much continuous coverage counts as "proven over time." Tunable, not
/// frozen — the whitepaper specifies the shape (continuous re-proof over
/// a real span of time), not these exact numbers.
#[derive(Debug, Clone, Copy)]
pub struct StorageWindowPolicy {
    /// A gap between successive successful responses larger than this
    /// breaks the streak — the proof has lapsed.
    pub max_interval_ms: u64,
    /// Minimum unbroken coverage required before capacity counts as
    /// currently proven.
    pub min_window_ms: u64,
}

impl StorageWindowPolicy {
    /// A month-scale default: must answer at least every two days, and
    /// needs roughly a month of unbroken coverage.
    pub fn month_scale_default() -> Self {
        StorageWindowPolicy {
            max_interval_ms: 2 * 86_400_000,
            min_window_ms: 30 * 86_400_000,
        }
    }
}

/// A record of successful challenge-response timestamps for one storage
/// commitment.
#[derive(Debug, Clone, Default)]
pub struct ProofHistory {
    /// Kept sorted ascending.
    successes: Vec<u64>,
}

impl ProofHistory {
    /// A new, empty history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful challenge response at `at_ms`.
    pub fn record_success(&mut self, at_ms: u64) {
        match self.successes.binary_search(&at_ms) {
            Ok(_) => {}
            Err(pos) => self.successes.insert(pos, at_ms),
        }
    }

    /// The longest unbroken run of successes ending at or before `now_ms`
    /// — zero if the most recent success is itself already stale (older
    /// than `policy.max_interval_ms`), since a streak from long ago does
    /// not establish *current* possession.
    pub fn covered_window_ms(&self, policy: &StorageWindowPolicy, now_ms: u64) -> u64 {
        let relevant: Vec<u64> = self
            .successes
            .iter()
            .copied()
            .filter(|&t| t <= now_ms)
            .collect();
        let Some(&last) = relevant.last() else {
            return 0;
        };
        if now_ms.saturating_sub(last) > policy.max_interval_ms {
            return 0;
        }
        let mut start = relevant.len() - 1;
        while start > 0 {
            let gap = relevant[start] - relevant[start - 1];
            if gap > policy.max_interval_ms {
                break;
            }
            start -= 1;
        }
        last - relevant[start]
    }

    /// Whether this history currently demonstrates continuous possession
    /// per `policy` at `now_ms`.
    pub fn proven_space_time(&self, policy: &StorageWindowPolicy, now_ms: u64) -> bool {
        self.covered_window_ms(policy, now_ms) >= policy.min_window_ms
    }
}

/// The interim [`ProofOfSpaceTimeSource`] implementation: proven capacity
/// tracks whether this commitment's [`ProofHistory`] currently satisfies
/// its [`StorageWindowPolicy`].
#[derive(Debug, Clone)]
pub struct MerkleStorageProof {
    commitment: StorageCommitment,
    units: StorageUnitPolicy,
    policy: StorageWindowPolicy,
    history: ProofHistory,
}

impl MerkleStorageProof {
    /// A fresh proof tracker for `commitment` under `policy`, counting
    /// capacity at `units`.
    ///
    /// It takes a *conversion policy*, not a capacity figure. The figure is
    /// derived from the commitment, which is checked on every challenge —
    /// this signature previously accepted `capacity_units: u64` from its
    /// caller with nothing tying that number to the commitment beside it.
    pub fn new(
        commitment: StorageCommitment,
        units: StorageUnitPolicy,
        policy: StorageWindowPolicy,
    ) -> Self {
        MerkleStorageProof {
            commitment,
            units,
            policy,
            history: ProofHistory::new(),
        }
    }

    /// The commitment this tracker is proving.
    pub fn commitment(&self) -> &StorageCommitment {
        &self.commitment
    }

    /// Verify `response` against the `challenge` it answers and, if valid,
    /// record a successful proof at `now_ms`. Returns whether the response
    /// was valid.
    ///
    /// The caller must pass the challenge it actually issued. Passing the
    /// response's own index back would restore the defect this signature
    /// exists to prevent.
    pub fn submit_response(
        &mut self,
        challenge: &StorageChallenge,
        response: &StorageChallengeResponse,
        now_ms: u64,
    ) -> bool {
        if verify_storage_challenge(&self.commitment, challenge, response) {
            self.history.record_success(now_ms);
            true
        } else {
            false
        }
    }
}

impl ProofOfSpaceTimeSource for MerkleStorageProof {
    fn proven_capacity(&mut self, now_ms: u64) -> Option<ProvenCapacity> {
        if self.history.proven_space_time(&self.policy, now_ms) {
            Some(ProvenCapacity::from_commitment(
                &self.commitment,
                &self.units,
            ))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merkle::MerkleTree;

    fn blocks(n: usize) -> Vec<Vec<u8>> {
        (0..n).map(|i| vec![i as u8; 16]).collect()
    }

    /// One capacity unit per committed byte, so a commitment's unit count
    /// equals its byte count and the arithmetic stays checkable by hand.
    fn units() -> StorageUnitPolicy {
        StorageUnitPolicy::new(1).unwrap()
    }

    fn commitment_and_tree(n: usize) -> (StorageCommitment, MerkleTree, Vec<Vec<u8>>) {
        let data = blocks(n);
        let tree = MerkleTree::from_blocks(&data).unwrap();
        let commitment = StorageCommitment {
            merkle_root: tree.root(),
            block_count: tree.leaf_count(),
            // blocks() makes every block 16 bytes; the commitment must say
            // so, because verify_storage_challenge now enforces it.
            block_size_bytes: 16,
        };
        (commitment, tree, data)
    }

    fn respond(tree: &MerkleTree, data: &[Vec<u8>], index: usize) -> StorageChallengeResponse {
        StorageChallengeResponse {
            leaf_index: index,
            block_bytes: data[index].clone(),
            proof: tree.prove(index).unwrap(),
        }
    }

    #[test]
    fn a_valid_response_verifies() {
        let (commitment, tree, data) = commitment_and_tree(5);
        let challenge = StorageChallenge { leaf_index: 2 };
        let response = respond(&tree, &data, 2);
        assert!(verify_storage_challenge(&commitment, &challenge, &response));
    }

    #[test]
    fn tampered_block_bytes_fail_verification() {
        let (commitment, tree, data) = commitment_and_tree(5);
        let challenge = StorageChallenge { leaf_index: 2 };
        let mut response = respond(&tree, &data, 2);
        response.block_bytes = b"fabricated".to_vec();
        assert!(!verify_storage_challenge(
            &commitment,
            &challenge,
            &response
        ));
    }

    #[test]
    fn mismatched_index_fails_verification() {
        let (commitment, tree, data) = commitment_and_tree(5);
        let challenge = StorageChallenge { leaf_index: 2 };
        let mut response = respond(&tree, &data, 2);
        response.leaf_index = 3;
        assert!(!verify_storage_challenge(
            &commitment,
            &challenge,
            &response
        ));
    }

    #[test]
    fn answering_a_different_leaf_than_asked_is_refused() {
        // The defect this signature exists to prevent: before the challenge
        // was bound, a prover asked for leaf 2 could answer leaf 4 with a
        // perfectly genuine proof and pass, so holding one leaf and its Merkle
        // path was enough to "prove" possession of the whole tree.
        let (commitment, tree, data) = commitment_and_tree(5);
        let asked = StorageChallenge { leaf_index: 2 };
        let answered_elsewhere = respond(&tree, &data, 4);

        // The response is internally genuine ...
        assert!(answered_elsewhere.proof.verify(
            &answered_elsewhere.block_bytes,
            commitment.merkle_root,
            commitment.block_count,
        ));
        // ... and still refused, because it answers the wrong question.
        assert!(!verify_storage_challenge(
            &commitment,
            &asked,
            &answered_elsewhere
        ));
    }

    #[test]
    fn a_tracker_does_not_credit_a_wrong_leaf_response() {
        let (commitment, tree, data) = commitment_and_tree(5);
        let policy = StorageWindowPolicy::month_scale_default();
        let mut tracker = MerkleStorageProof::new(commitment, units(), policy);
        let asked = StorageChallenge { leaf_index: 1 };
        let elsewhere = respond(&tree, &data, 3);
        assert!(!tracker.submit_response(&asked, &elsewhere, 0));
    }

    #[test]
    fn a_single_success_does_not_establish_a_time_window() {
        let mut history = ProofHistory::new();
        history.record_success(0);
        let policy = StorageWindowPolicy::month_scale_default();
        assert!(!history.proven_space_time(&policy, 0));
    }

    #[test]
    fn sustained_close_successes_establish_the_window() {
        let mut history = ProofHistory::new();
        let policy = StorageWindowPolicy::month_scale_default();
        let mut t = 0u64;
        while t <= policy.min_window_ms {
            history.record_success(t);
            t += policy.max_interval_ms / 2;
        }
        assert!(history.proven_space_time(&policy, t));
    }

    #[test]
    fn a_gap_larger_than_max_interval_breaks_the_streak() {
        let mut history = ProofHistory::new();
        let policy = StorageWindowPolicy::month_scale_default();
        history.record_success(0);
        // Huge gap, then a fresh streak that alone isn't long enough.
        history.record_success(policy.min_window_ms * 10);
        history.record_success(policy.min_window_ms * 10 + 1_000);
        assert!(!history.proven_space_time(&policy, policy.min_window_ms * 10 + 1_000));
    }

    #[test]
    fn a_stale_last_success_counts_as_no_current_proof() {
        let mut history = ProofHistory::new();
        let policy = StorageWindowPolicy::month_scale_default();
        let mut t = 0u64;
        while t <= policy.min_window_ms {
            history.record_success(t);
            t += policy.max_interval_ms / 2;
        }
        // Long after the last success, well past max_interval_ms.
        let stale_now = t + policy.max_interval_ms * 10;
        assert!(!history.proven_space_time(&policy, stale_now));
    }

    #[test]
    fn merkle_storage_proof_reports_capacity_only_once_the_window_is_proven() {
        let (commitment, tree, data) = commitment_and_tree(4);
        let policy = StorageWindowPolicy::month_scale_default();
        let mut proof = MerkleStorageProof::new(commitment, units(), policy);

        assert_eq!(proof.proven_capacity(0), None);

        let mut t = 0u64;
        while t <= policy.min_window_ms {
            let index = (t % 4) as usize;
            let challenge = StorageChallenge { leaf_index: index };
            let response = respond(&tree, &data, index);
            assert!(proof.submit_response(&challenge, &response, t));
            t += policy.max_interval_ms / 2;
        }
        assert_eq!(
            proof.proven_capacity(t).map(|c| c.units()),
            Some(commitment.committed_bytes())
        );
    }

    #[test]
    fn an_invalid_response_is_rejected_and_not_recorded() {
        let (commitment, tree, data) = commitment_and_tree(4);
        let policy = StorageWindowPolicy::month_scale_default();
        let mut proof = MerkleStorageProof::new(commitment, units(), policy);

        let challenge = StorageChallenge { leaf_index: 0 };
        let mut bad_response = respond(&tree, &data, 0);
        bad_response.block_bytes = b"wrong".to_vec();
        assert!(!proof.submit_response(&challenge, &bad_response, 0));
        assert_eq!(proof.proven_capacity(0), None);
    }
    #[test]
    fn a_block_of_the_wrong_size_fails_its_challenge() {
        // block_size_bytes is what makes capacity derivable rather than
        // asserted, so it has to be enforced on the wire. A provider that
        // commits to large blocks and serves small ones fails every
        // challenge instead of collecting the weight.
        let (mut commitment, tree, data) = commitment_and_tree(8);
        commitment.block_size_bytes = 1024; // claims 1 KiB blocks, holds 16 B
        let challenge = StorageChallenge { leaf_index: 3 };
        let response = respond(&tree, &data, 3);
        assert!(!verify_storage_challenge(
            &commitment,
            &challenge,
            &response
        ));
    }

    #[test]
    fn committed_bytes_is_the_product_and_saturates() {
        let commitment = StorageCommitment {
            merkle_root: [0u8; 32],
            block_count: 10,
            block_size_bytes: 64,
        };
        assert_eq!(commitment.committed_bytes(), 640);

        let absurd = StorageCommitment {
            merkle_root: [0u8; 32],
            block_count: usize::MAX,
            block_size_bytes: u32::MAX,
        };
        assert_eq!(absurd.committed_bytes(), u64::MAX, "saturates, never wraps");
    }

    #[test]
    fn a_commitment_smaller_than_one_unit_proves_zero_capacity() {
        // Truncating, not rounding up: rounding would mint capacity nobody
        // committed, which is the same failure in miniature.
        let commitment = StorageCommitment {
            merkle_root: [0u8; 32],
            block_count: 1,
            block_size_bytes: 32,
        };
        let coarse = StorageUnitPolicy::new(1024 * 1024).unwrap();
        assert_eq!(
            ProvenCapacity::from_commitment(&commitment, &coarse).units(),
            0
        );
    }

    #[test]
    fn a_zero_byte_unit_policy_is_refused() {
        assert!(StorageUnitPolicy::new(0).is_none());
    }
}
