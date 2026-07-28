//! Proves the whole point of this crate, not just its unit-level
//! invariants: a file erasure-coded by `mini_erasure`, placed onto
//! distinct holders by this crate, survives losing holders (not just
//! shards in the abstract), gets repaired onto fresh distinct holders, and
//! is still reconstructible by querying only the holders
//! `select_retrieval_set` names.

use did_mini::Controller;
use mini_erasure::{digest, encode, plan_repair, reconstruct, repair, ErasureParams, Shard};
use mini_replication_policy::{plan_placement, plan_repair_placement, select_retrieval_set};

fn did(seed: u8) -> did_mini::Did {
    Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32])
        .unwrap()
        .did()
}

#[test]
fn a_placed_and_partially_lost_file_is_still_retrievable_after_repair() {
    let params = ErasureParams::new(4, 3).unwrap(); // total = 7, tolerates losing 3
    let data = b"suppression-resistant replication: no single holder can take this down".to_vec();
    let encoded = encode(&data, params).unwrap();
    let digests: Vec<[u8; 32]> = encoded.shards.iter().map(|s| digest(&s.bytes)).collect();

    let candidates: Vec<did_mini::Did> = (0..7).map(did).collect();
    let plan = plan_placement(params, &candidates).unwrap();

    // Simulate three holders disappearing (still within tolerance).
    let lost_indices = [1usize, 3, 5];
    let mut held_shards: Vec<Option<Shard>> = encoded.shards.iter().cloned().map(Some).collect();
    for &i in &lost_indices {
        held_shards[i] = None;
    }

    let assessment = plan_repair(params, &held_shards, &digests);
    assert!(assessment.reconstructable);
    assert_eq!(assessment.missing, lost_indices);

    // Repair the shard bytes...
    let regenerated = repair(params, &held_shards, &digests, encoded.original_len).unwrap();
    assert_eq!(regenerated.len(), lost_indices.len());

    // ...and place the regenerated shards onto fresh, distinct holders.
    let fresh_holders: Vec<did_mini::Did> = (50..53).map(did).collect();
    let repaired_plan = plan_repair_placement(&plan, &lost_indices, &fresh_holders).unwrap();
    assert_eq!(repaired_plan.distinct_holder_count(), 7);

    // The fresh holders now hold exactly the regenerated shards, and the
    // untouched holders are unchanged from the original plan.
    for shard in &regenerated {
        let holder = repaired_plan.holder_for(shard.index).unwrap();
        assert!(fresh_holders.contains(&holder.0));
    }
    for i in [0usize, 2, 4, 6] {
        assert_eq!(repaired_plan.holder_for(i), plan.holder_for(i));
    }

    // Finally: a retrieval client following select_retrieval_set's
    // deterministic default only needs to query those data_shards holders
    // -- reconstruct the file purely from that subset's shards, proving
    // the placement/repair bookkeeping actually lines up with real,
    // reconstructible shard bytes end to end.
    let retrieval_set = select_retrieval_set(&repaired_plan);
    assert_eq!(retrieval_set.len(), params.data_shards);

    let mut all_shards_by_index: Vec<Option<Shard>> =
        encoded.shards.into_iter().map(Some).collect();
    for shard in &regenerated {
        all_shards_by_index[shard.index] = Some(shard.clone());
    }
    let mut queried: Vec<Option<Shard>> = vec![None; params.total_shards()];
    for assignment in &retrieval_set {
        queried[assignment.shard_index] = all_shards_by_index[assignment.shard_index].clone();
    }

    let recovered = reconstruct(params, &queried, encoded.original_len).unwrap();
    assert_eq!(recovered, data);
}

#[test]
fn placement_refuses_to_start_with_too_few_distinct_holders_for_the_chosen_redundancy() {
    let params = ErasureParams::new(6, 4).unwrap(); // total = 10
    let candidates: Vec<did_mini::Did> = (0..8).map(did).collect();
    let err = plan_placement(params, &candidates).unwrap_err();
    assert_eq!(
        err,
        mini_replication_policy::ReplicationError::InsufficientDistinctCandidates {
            needed: 10,
            available: 8
        }
    );
}
