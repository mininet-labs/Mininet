//! End-to-end adversarial scenario: a file loses shards to two separate,
//! non-overlapping outages over time, gets healed after each, and still
//! reconstructs to the exact original bytes at the end -- the actual
//! claim `mini-erasure` exists to support (roadmap #30/#32), not just the
//! individual encode/reconstruct/repair unit properties tested in-crate.

use mini_erasure::{digest, encode, plan_repair, reconstruct, repair, ErasureParams, Shard};

fn store(shards: Vec<Shard>) -> Vec<Option<Shard>> {
    shards.into_iter().map(Some).collect()
}

#[test]
fn a_file_survives_two_separate_partial_outages_and_still_heals_completely() {
    let params = ErasureParams::new(6, 4).unwrap();
    let data: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).collect();

    let encoded = encode(&data, params).unwrap();
    let digests: Vec<[u8; 32]> = encoded.shards.iter().map(|s| digest(&s.bytes)).collect();
    let mut holders = store(encoded.shards);

    // Outage 1: three holders go dark at once (within the parity budget).
    for i in [0, 3, 7] {
        holders[i] = None;
    }
    let plan = plan_repair(params, &holders, &digests);
    assert_eq!(plan.missing, vec![0, 3, 7]);
    assert!(plan.reconstructable);

    let repaired = repair(params, &holders, &digests, encoded.original_len).unwrap();
    assert_eq!(repaired.len(), 3);
    for shard in repaired {
        let index = shard.index;
        holders[index] = Some(shard);
    }

    // Fully healed: no shard should be reported missing anymore.
    assert!(plan_repair(params, &holders, &digests).missing.is_empty());

    // Outage 2: a *different* set of holders (including one that was
    // freshly repaired) goes dark.
    for i in [0, 5, 9] {
        holders[i] = None;
    }
    let repaired_again = repair(params, &holders, &digests, encoded.original_len).unwrap();
    for shard in repaired_again {
        let index = shard.index;
        holders[index] = Some(shard);
    }
    assert!(plan_repair(params, &holders, &digests).missing.is_empty());

    // The file itself, reconstructed after two healing cycles, is still
    // bit-identical to what was originally encoded.
    let recovered = reconstruct(params, &holders, encoded.original_len).unwrap();
    assert_eq!(recovered, data);
}

#[test]
fn an_outage_beyond_the_parity_budget_is_reported_unreconstructable_not_silently_wrong() {
    let params = ErasureParams::new(4, 2).unwrap();
    let data = b"losing more than parity_shards holders is unrecoverable, and must say so".to_vec();
    let encoded = encode(&data, params).unwrap();
    let digests: Vec<[u8; 32]> = encoded.shards.iter().map(|s| digest(&s.bytes)).collect();
    let mut holders = store(encoded.shards);

    // parity_shards = 2, so losing 3 is one too many.
    holders[0] = None;
    holders[1] = None;
    holders[2] = None;

    let plan = plan_repair(params, &holders, &digests);
    assert!(!plan.reconstructable);
    assert!(repair(params, &holders, &digests, encoded.original_len).is_err());
}
