//! Self-healing storage — closes roadmap #32. Erasure coding
//! ([`crate::code`]) already tolerates losing up to `parity_shards` of the
//! `total_shards()` shards; this module is what turns that tolerance into
//! *healing*: detect which shards a network of holders can no longer
//! vouch for (missing outright, or present but corrupted), reconstruct the
//! original data from whatever verified shards remain, and regenerate
//! exactly the missing ones so a fresh holder can take over -- restoring
//! full redundancy before a second loss compounds into an unrecoverable
//! one.
//!
//! **Scope boundary**, the same one every networked-vs-logic split in this
//! tree draws: this module proves the *detection and reconstruction*
//! logic is correct. Deciding which peer should hold a regenerated shard,
//! and actually transferring it to them, is `mini-net`/`mini-store`'s job
//! (a distribution problem, not an erasure-coding one) — out of scope
//! here, unstarted.

use crate::code::{reconstruct, regenerate_shard, ErasureParams, Shard};
use crate::error::{ErasureError, Result};

/// The BLAKE3 digest a shard's bytes should hash to. Computed once when a
/// shard is first distributed, and checked again whenever the shard's
/// current holder is asked to produce it — the same "don't just trust
/// presence, verify content" discipline `mini_spacetime::storage_proof`
/// applies to whole files, applied here per shard.
pub fn digest(bytes: &[u8]) -> [u8; 32] {
    blake3::hash(bytes).into()
}

/// Whether `shard`'s bytes actually match `expected_digest` — a shard that
/// is present but corrupted must be treated the same as one that is
/// simply missing, never trusted as "available."
pub fn verify_shard(shard: &Shard, expected_digest: [u8; 32]) -> bool {
    digest(&shard.bytes) == expected_digest
}

/// A health assessment: which shard indices currently cannot be trusted
/// (missing or failed their integrity check), and whether enough verified
/// shards remain to reconstruct at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairPlan {
    pub missing: Vec<usize>,
    pub reconstructable: bool,
}

/// Assess `shards` (length must be `params.total_shards()`, aligned with
/// `expected_digests`) without doing any reconstruction work yet.
pub fn plan_repair(
    params: ErasureParams,
    shards: &[Option<Shard>],
    expected_digests: &[[u8; 32]],
) -> RepairPlan {
    let is_verified: Vec<bool> = shards
        .iter()
        .zip(expected_digests)
        .map(|(maybe_shard, expected)| {
            maybe_shard
                .as_ref()
                .is_some_and(|shard| verify_shard(shard, *expected))
        })
        .collect();
    let missing: Vec<usize> = is_verified
        .iter()
        .enumerate()
        .filter_map(|(i, &ok)| if ok { None } else { Some(i) })
        .collect();
    let verified_count = is_verified.iter().filter(|&&ok| ok).count();
    RepairPlan {
        missing,
        reconstructable: verified_count >= params.data_shards,
    }
}

/// Reconstruct the original data from whichever shards in `shards` verify
/// against `expected_digests`, then regenerate exactly the shards that
/// were missing or failed verification. Returns the regenerated shards,
/// ready for a caller to redistribute to fresh holders — empty if nothing
/// needed repair.
pub fn repair(
    params: ErasureParams,
    shards: &[Option<Shard>],
    expected_digests: &[[u8; 32]],
    original_len: usize,
) -> Result<Vec<Shard>> {
    let n = params.total_shards();
    if shards.len() != n || expected_digests.len() != n {
        return Err(ErasureError::WrongShardCount {
            expected: n,
            got: shards.len(),
        });
    }

    let verified: Vec<Option<Shard>> = shards
        .iter()
        .zip(expected_digests)
        .map(|(maybe_shard, expected)| {
            maybe_shard
                .as_ref()
                .filter(|shard| verify_shard(shard, *expected))
                .cloned()
        })
        .collect();

    let missing: Vec<usize> = verified
        .iter()
        .enumerate()
        .filter_map(|(i, s)| if s.is_none() { Some(i) } else { None })
        .collect();
    if missing.is_empty() {
        return Ok(Vec::new());
    }

    let original_data = reconstruct(params, &verified, original_len)?;
    let shard_len = verified
        .iter()
        .flatten()
        .next()
        .expect("reconstruct() already proved at least data_shards verified shards exist")
        .bytes
        .len();

    // reconstruct() truncates to original_len; regenerating a shard needs
    // the exact zero-padded working matrix encode() used, which truncating
    // then re-padding with zeros reproduces bit-for-bit (encode's own
    // padding was zeros in exactly those trailing positions).
    let mut padded = original_data;
    padded.resize(shard_len * params.data_shards, 0);

    Ok(missing
        .into_iter()
        .map(|index| Shard {
            index,
            bytes: regenerate_shard(params, &padded, shard_len, index),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::encode;

    fn encode_and_digests(
        data: &[u8],
        params: ErasureParams,
    ) -> (Vec<Shard>, Vec<[u8; 32]>, usize) {
        let encoded = encode(data, params).unwrap();
        let digests = encoded.shards.iter().map(|s| digest(&s.bytes)).collect();
        (encoded.shards, digests, encoded.original_len)
    }

    #[test]
    fn a_fully_healthy_set_needs_no_repair() {
        let params = ErasureParams::new(4, 2).unwrap();
        let data = b"nothing is missing, nothing needs healing here".to_vec();
        let (shards, digests, len) = encode_and_digests(&data, params);
        let available: Vec<Option<Shard>> = shards.into_iter().map(Some).collect();

        let plan = plan_repair(params, &available, &digests);
        assert!(plan.missing.is_empty());
        assert!(plan.reconstructable);

        let repaired = repair(params, &available, &digests, len).unwrap();
        assert!(repaired.is_empty());
    }

    #[test]
    fn missing_shards_are_regenerated_identically_to_the_originals() {
        let params = ErasureParams::new(4, 3).unwrap();
        let data = b"lose two shards, heal them back to exactly what they were before".to_vec();
        let (shards, digests, len) = encode_and_digests(&data, params);

        let mut available: Vec<Option<Shard>> = shards.iter().cloned().map(Some).collect();
        available[1] = None;
        available[5] = None;

        let plan = plan_repair(params, &available, &digests);
        assert_eq!(plan.missing, vec![1, 5]);
        assert!(plan.reconstructable);

        let repaired = repair(params, &available, &digests, len).unwrap();
        assert_eq!(repaired.len(), 2);
        for shard in &repaired {
            assert_eq!(shard.bytes, shards[shard.index].bytes);
        }
    }

    #[test]
    fn a_corrupted_shard_is_treated_the_same_as_a_missing_one() {
        let params = ErasureParams::new(4, 2).unwrap();
        let data = b"corruption must be caught, not silently trusted as valid data".to_vec();
        let (shards, digests, len) = encode_and_digests(&data, params);

        let mut available: Vec<Option<Shard>> = shards.iter().cloned().map(Some).collect();
        available[2].as_mut().unwrap().bytes[0] ^= 0xff;

        let plan = plan_repair(params, &available, &digests);
        assert_eq!(plan.missing, vec![2]);

        let repaired = repair(params, &available, &digests, len).unwrap();
        assert_eq!(repaired.len(), 1);
        assert_eq!(repaired[0].index, 2);
        assert_eq!(repaired[0].bytes, shards[2].bytes);
    }

    #[test]
    fn too_few_verified_shards_reports_unreconstructable_and_repair_fails() {
        let params = ErasureParams::new(4, 2).unwrap();
        let data = b"three good shards is one short of the four we need".to_vec();
        let (shards, digests, len) = encode_and_digests(&data, params);

        let mut available: Vec<Option<Shard>> = shards.into_iter().map(Some).collect();
        available[0] = None;
        available[1] = None;
        available[2] = None;

        let plan = plan_repair(params, &available, &digests);
        assert!(!plan.reconstructable);
        assert!(matches!(
            repair(params, &available, &digests, len),
            Err(ErasureError::TooManyMissingShards { .. })
        ));
    }

    #[test]
    fn regenerated_shards_pass_their_own_integrity_check() {
        let params = ErasureParams::new(5, 2).unwrap();
        let data = b"a repaired shard must verify against the same digest as the original".to_vec();
        let (shards, digests, len) = encode_and_digests(&data, params);

        let mut available: Vec<Option<Shard>> = shards.into_iter().map(Some).collect();
        available[6] = None;

        let repaired = repair(params, &available, &digests, len).unwrap();
        assert!(verify_shard(&repaired[0], digests[6]));
    }
}
