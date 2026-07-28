use std::collections::HashSet;

use did_mini::Did;
use mini_erasure::ErasureParams;

use crate::error::{ReplicationError, Result};

/// A distinct identity holding one shard. A thin, typed wrapper over
/// [`did_mini::Did`] rather than a bare `Did` parameter, matching this
/// workspace's typed-domain convention (`CLAUDE.md`): this crate's whole
/// job is enforcing that holder identities stay distinct across a plan, so
/// the type that carries "this is a shard holder" is worth naming.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HolderId(pub Did);

/// One shard's current holder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardAssignment {
    pub shard_index: usize,
    pub holder: HolderId,
}

/// Which distinct identity holds each shard of one [`mini_erasure`]-encoded
/// file. The invariant this whole crate exists to hold: every shard index
/// in `0..params.total_shards()` has exactly one assignment, and no two
/// assignments share a holder. [`plan_placement`] and
/// [`plan_repair_placement`] are the only ways to build one, and both
/// enforce it before returning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicationPlan {
    pub params: ErasureParams,
    assignments: Vec<ShardAssignment>,
}

impl ReplicationPlan {
    /// All assignments, sorted by shard index.
    pub fn assignments(&self) -> &[ShardAssignment] {
        &self.assignments
    }

    /// The holder currently assigned to `shard_index`, if that index is
    /// valid for this plan's parameters.
    pub fn holder_for(&self, shard_index: usize) -> Option<&HolderId> {
        self.assignments
            .iter()
            .find(|a| a.shard_index == shard_index)
            .map(|a| &a.holder)
    }

    /// Always equal to `params.total_shards()` by construction — exposed as
    /// a named, testable invariant rather than left implicit.
    pub fn distinct_holder_count(&self) -> usize {
        self.holder_set().len()
    }

    fn holder_set(&self) -> HashSet<&Did> {
        self.assignments.iter().map(|a| &a.holder.0).collect()
    }
}

fn require_distinct(dids: &[Did]) -> Result<()> {
    let mut seen = HashSet::with_capacity(dids.len());
    for did in dids {
        if !seen.insert(did) {
            return Err(ReplicationError::DuplicateCandidate);
        }
    }
    Ok(())
}

/// Assign each of `params.total_shards()` shards to its own distinct
/// candidate holder — the first `total_shards()` entries of `candidates`,
/// in the order given. Suppression resistance comes directly from this
/// distinctness: removing, freezing, or coercing any single holder can
/// cost at most one shard, never more, so no single party's cooperation is
/// required to keep the content retrievable (as long as
/// [`mini_erasure::code::reconstruct`]'s `data_shards`-of-`total_shards`
/// threshold still holds).
///
/// **Honest limit:** distinctness is checked by `Did` equality only. This
/// crate has no way to detect that two different `Did`s are controlled by
/// the same operator behind the scenes — that is the general Sybil-
/// resistance problem `docs/INVARIANTS.md`'s hard limitations already name
/// as unsolved, not something a placement policy can fix.
pub fn plan_placement(params: ErasureParams, candidates: &[Did]) -> Result<ReplicationPlan> {
    require_distinct(candidates)?;
    let total = params.total_shards();
    if candidates.len() < total {
        return Err(ReplicationError::InsufficientDistinctCandidates {
            needed: total,
            available: candidates.len(),
        });
    }
    let assignments = candidates
        .iter()
        .take(total)
        .enumerate()
        .map(|(shard_index, did)| ShardAssignment {
            shard_index,
            holder: HolderId(did.clone()),
        })
        .collect();
    Ok(ReplicationPlan {
        params,
        assignments,
    })
}

/// Replace exactly the holders of `missing_shard_indices` with fresh,
/// distinct candidates, leaving every other shard's holder unchanged.
/// Pairs with [`mini_erasure::health::plan_repair`]/`repair`: those
/// functions decide *which* shard bytes need regenerating and produce the
/// new bytes; this function decides *who* takes over holding each one,
/// preserving the same distinctness invariant [`plan_placement`]
/// established. Neither this function nor [`plan_placement`] touches shard
/// bytes or performs any network transfer — see the crate doc comment.
pub fn plan_repair_placement(
    plan: &ReplicationPlan,
    missing_shard_indices: &[usize],
    fresh_candidates: &[Did],
) -> Result<ReplicationPlan> {
    let total = plan.params.total_shards();

    let mut seen_indices = HashSet::with_capacity(missing_shard_indices.len());
    for &index in missing_shard_indices {
        if index >= total {
            return Err(ReplicationError::UnknownShardIndex { index, total });
        }
        if !seen_indices.insert(index) {
            return Err(ReplicationError::DuplicateShardIndex { index });
        }
    }

    require_distinct(fresh_candidates)?;
    let existing_holders = plan.holder_set();
    for candidate in fresh_candidates {
        if existing_holders.contains(candidate) {
            return Err(ReplicationError::CandidateAlreadyHolder);
        }
    }
    if fresh_candidates.len() < missing_shard_indices.len() {
        return Err(ReplicationError::InsufficientDistinctCandidates {
            needed: missing_shard_indices.len(),
            available: fresh_candidates.len(),
        });
    }

    let mut assignments = plan.assignments.clone();
    let mut fresh = fresh_candidates.iter();
    for &index in missing_shard_indices {
        let new_holder = fresh.next().expect("length checked above").clone();
        let slot = assignments
            .iter_mut()
            .find(|a| a.shard_index == index)
            .expect("index range checked above");
        slot.holder = HolderId(new_holder);
    }

    Ok(ReplicationPlan {
        params: plan.params,
        assignments,
    })
}

/// A deterministic default retrieval set: the `params.data_shards` lowest-
/// indexed assignments. Any `data_shards`-sized subset of a plan's
/// `total_shards()` assignments reconstructs the original data (the
/// erasure code's MDS property, `mini_erasure::code`) — this is a
/// deterministic default, not a latency- or reliability-weighted
/// recommendation. A real retrieval client may have better information
/// (recent health checks, measured round-trip time) this crate does not
/// have access to and should prefer that instead.
pub fn select_retrieval_set(plan: &ReplicationPlan) -> Vec<&ShardAssignment> {
    let mut sorted: Vec<&ShardAssignment> = plan.assignments.iter().collect();
    sorted.sort_by_key(|a| a.shard_index);
    sorted.truncate(plan.params.data_shards);
    sorted
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;

    fn did(seed: u8) -> Did {
        Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32])
            .unwrap()
            .did()
    }

    fn candidates(n: u8) -> Vec<Did> {
        (0..n).map(did).collect()
    }

    #[test]
    fn placement_assigns_one_distinct_holder_per_shard() {
        let params = ErasureParams::new(4, 2).unwrap();
        let plan = plan_placement(params, &candidates(6)).unwrap();
        assert_eq!(plan.assignments().len(), 6);
        assert_eq!(plan.distinct_holder_count(), 6);
        for i in 0..6 {
            assert!(plan.holder_for(i).is_some());
        }
        assert!(plan.holder_for(6).is_none());
    }

    #[test]
    fn extra_candidates_beyond_total_shards_are_not_used() {
        let params = ErasureParams::new(4, 2).unwrap();
        let plan = plan_placement(params, &candidates(10)).unwrap();
        assert_eq!(plan.assignments().len(), 6);
        // The 7th..10th candidates never appear as holders.
        let used: HashSet<Did> = plan
            .assignments()
            .iter()
            .map(|a| a.holder.0.clone())
            .collect();
        let unused = candidates(10);
        for c in &unused[6..] {
            assert!(!used.contains(c));
        }
    }

    #[test]
    fn duplicate_candidates_are_rejected() {
        let params = ErasureParams::new(2, 1).unwrap();
        let a = did(1);
        let candidates = vec![a.clone(), did(2), a];
        assert_eq!(
            plan_placement(params, &candidates),
            Err(ReplicationError::DuplicateCandidate)
        );
    }

    #[test]
    fn too_few_distinct_candidates_is_rejected() {
        let params = ErasureParams::new(4, 3).unwrap(); // needs 7
        let err = plan_placement(params, &candidates(5)).unwrap_err();
        assert_eq!(
            err,
            ReplicationError::InsufficientDistinctCandidates {
                needed: 7,
                available: 5
            }
        );
    }

    #[test]
    fn repair_replaces_exactly_the_missing_holders() {
        let params = ErasureParams::new(4, 2).unwrap();
        let plan = plan_placement(params, &candidates(6)).unwrap();
        let original_holder_2 = plan.holder_for(2).cloned();
        let original_holder_4 = plan.holder_for(4).cloned();

        let fresh = vec![did(100), did(101)];
        let repaired = plan_repair_placement(&plan, &[2, 4], &fresh).unwrap();

        assert_eq!(repaired.distinct_holder_count(), 6);
        assert_ne!(repaired.holder_for(2), original_holder_2.as_ref());
        assert_ne!(repaired.holder_for(4), original_holder_4.as_ref());
        // Every other holder is untouched.
        for i in [0usize, 1, 3, 5] {
            assert_eq!(repaired.holder_for(i), plan.holder_for(i));
        }
    }

    #[test]
    fn repair_rejects_a_fresh_candidate_already_holding_another_shard() {
        let params = ErasureParams::new(3, 2).unwrap();
        let plan = plan_placement(params, &candidates(5)).unwrap();
        let still_holding = plan.holder_for(1).unwrap().0.clone();
        assert_eq!(
            plan_repair_placement(&plan, &[0], &[still_holding]),
            Err(ReplicationError::CandidateAlreadyHolder)
        );
    }

    #[test]
    fn repair_rejects_too_few_fresh_candidates() {
        let params = ErasureParams::new(3, 2).unwrap();
        let plan = plan_placement(params, &candidates(5)).unwrap();
        assert_eq!(
            plan_repair_placement(&plan, &[0, 1], &[did(200)]),
            Err(ReplicationError::InsufficientDistinctCandidates {
                needed: 2,
                available: 1
            })
        );
    }

    #[test]
    fn repair_rejects_an_out_of_range_shard_index() {
        let params = ErasureParams::new(3, 2).unwrap(); // total = 5
        let plan = plan_placement(params, &candidates(5)).unwrap();
        assert_eq!(
            plan_repair_placement(&plan, &[5], &[did(200)]),
            Err(ReplicationError::UnknownShardIndex { index: 5, total: 5 })
        );
    }

    #[test]
    fn repair_rejects_a_duplicated_missing_shard_index() {
        let params = ErasureParams::new(3, 2).unwrap();
        let plan = plan_placement(params, &candidates(5)).unwrap();
        assert_eq!(
            plan_repair_placement(&plan, &[1, 1], &[did(200), did(201)]),
            Err(ReplicationError::DuplicateShardIndex { index: 1 })
        );
    }

    #[test]
    fn retrieval_set_is_exactly_data_shards_sized_and_index_sorted() {
        let params = ErasureParams::new(4, 3).unwrap();
        let plan = plan_placement(params, &candidates(7)).unwrap();
        let set = select_retrieval_set(&plan);
        assert_eq!(set.len(), 4);
        let indices: Vec<usize> = set.iter().map(|a| a.shard_index).collect();
        assert_eq!(indices, vec![0, 1, 2, 3]);
    }

    #[test]
    fn retrieval_set_reflects_repaired_holders() {
        let params = ErasureParams::new(4, 2).unwrap();
        let plan = plan_placement(params, &candidates(6)).unwrap();
        let repaired = plan_repair_placement(&plan, &[0], &[did(50)]).unwrap();
        let set = select_retrieval_set(&repaired);
        let holder_0 = set.iter().find(|a| a.shard_index == 0).unwrap();
        assert_eq!(holder_0.holder.0, did(50));
    }
}
