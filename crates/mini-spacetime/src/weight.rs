//! Block-production selection weight from committed storage capacity
//! (whitepaper SS8.1): "a deliberately concave reward curve, caps per
//! identity, and bonuses for geographic and network diversity, so that
//! doubling one's capacity yields less than double the reward."
//!
//! **This is a scoring formula, not the proof itself.** Computing "how much
//! should this much *proven* capacity count for" is ordinary deterministic
//! arithmetic, the same risk class as `mini-reward`'s diversity-weighting.
//! *Proving* a node genuinely holds that capacity over time is a real
//! cryptographic protocol (proof-of-space-time / proof-of-replication) and
//! is deliberately not attempted here — see [`crate::proof`]'s honest limit
//! and D-0035 point 5.

use crate::isqrt::isqrt;
use crate::storage_proof::ProvenCapacity;

/// Parameters governing the weight formula. All integer, so weight is
/// exactly reproducible from the same proven-capacity input.
#[derive(Debug, Clone, Copy)]
pub struct ProposerParams {
    /// Per-identity cap on raw capacity counted (in whatever unit the
    /// caller's proof-of-space-time layer measures, e.g. GiB). Capacity
    /// beyond this contributes nothing further — the anti-concentration
    /// floor alongside the concave curve itself.
    pub capacity_cap_units: u64,
    /// Maximum bonus, as a percentage added on top of the base weight, for
    /// geographic/network diversity (e.g. spreading capacity across
    /// multiple distinct regions/network paths rather than one location).
    pub max_diversity_bonus_percent: u32,
    /// Bonus percentage granted per distinct region beyond the first,
    /// before the max cap above is applied.
    pub bonus_percent_per_extra_region: u32,
}

impl ProposerParams {
    /// A starting-point profile: a cap that keeps any single identity from
    /// dominating block production, and a modest diversity bonus. Tunable —
    /// the whitepaper specifies the *shape* (concave, capped, diversity-
    /// bonused), not these exact numbers.
    pub fn default_params() -> Self {
        ProposerParams {
            capacity_cap_units: 1_000_000,
            max_diversity_bonus_percent: 50,
            bonus_percent_per_extra_region: 10,
        }
    }
}

/// This identity's block-production selection weight, given its proven
/// capacity and how many distinct regions it spreads that capacity across.
///
/// Takes a [`ProvenCapacity`], never a bare number. That is the whole
/// signature change: this function previously accepted
/// `raw_capacity_units: u64` and said in its own documentation that it
/// "trusts its input completely" — so a provider could commit a single
/// 32-byte block, prove it honestly, and declare a million units into the
/// function that weights block production. There was no defense anywhere
/// against a node simply asserting capacity it did not hold.
///
/// `ProvenCapacity` has no numeric constructor. The only way to obtain one
/// is to derive it from a [`crate::StorageCommitment`], whose block size is
/// re-checked against the served bytes on every single challenge. So the
/// number reaching this curve is a consequence of what a provider actually
/// answered, not of what it typed.
///
/// The curve: capacity is capped per identity, then square-rooted (concave:
/// doubling capacity yields roughly 1.41x weight, never 2x), then a bounded
/// diversity bonus is added on top.
pub fn proposer_weight(
    capacity: ProvenCapacity,
    distinct_regions: u32,
    params: &ProposerParams,
) -> u64 {
    let capped = capacity.units().min(params.capacity_cap_units);
    let base = isqrt(capped);
    let bonus_percent = diversity_bonus_percent(distinct_regions, params);
    base + (base * u64::from(bonus_percent) / 100)
}

fn diversity_bonus_percent(distinct_regions: u32, params: &ProposerParams) -> u32 {
    let extra_regions = distinct_regions.saturating_sub(1);
    extra_regions
        .saturating_mul(params.bonus_percent_per_extra_region)
        .min(params.max_diversity_bonus_percent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_proof::{StorageCommitment, StorageUnitPolicy};

    /// `units` units of genuinely-derived capacity.
    ///
    /// Deliberately built through the real derivation rather than a
    /// test-only numeric constructor: a backdoor here would be a backdoor
    /// anywhere, since `#[cfg(test)]` is not a security boundary for
    /// anything in the same crate.
    fn capacity(units: u64) -> ProvenCapacity {
        let commitment = StorageCommitment {
            merkle_root: [0u8; 32],
            block_count: units as usize,
            block_size_bytes: 1,
        };
        ProvenCapacity::from_commitment(&commitment, &StorageUnitPolicy::new(1).unwrap())
    }

    #[test]
    fn doubling_capacity_yields_less_than_double_weight() {
        let params = ProposerParams::default_params();
        let w1 = proposer_weight(capacity(10_000), 1, &params);
        let w2 = proposer_weight(capacity(20_000), 1, &params);
        assert!(w2 > w1, "more capacity should still weigh more");
        assert!(
            w2 < 2 * w1,
            "concave curve: doubling capacity must not double weight (w1={w1} w2={w2})"
        );
    }

    #[test]
    fn capacity_beyond_the_cap_contributes_nothing_further() {
        let params = ProposerParams::default_params();
        let at_cap = proposer_weight(capacity(params.capacity_cap_units), 1, &params);
        let way_over = proposer_weight(capacity(params.capacity_cap_units * 100), 1, &params);
        assert_eq!(at_cap, way_over);
    }

    #[test]
    fn diversity_bonus_increases_weight_but_is_capped() {
        let params = ProposerParams::default_params();
        let one_region = proposer_weight(capacity(10_000), 1, &params);
        // 3 regions: 2 extra * 10%/region = 20%, still below the 50% cap.
        let three_regions = proposer_weight(capacity(10_000), 3, &params);
        // 10 and 100 regions both push well past the cap (9 and 99 extra
        // regions respectively), so both land on the same capped bonus.
        let ten_regions = proposer_weight(capacity(10_000), 10, &params);
        let hundred_regions = proposer_weight(capacity(10_000), 100, &params);

        assert!(three_regions > one_region);
        assert!(ten_regions > three_regions);
        // Bonus caps at max_diversity_bonus_percent regardless of how many
        // regions beyond that are reported.
        assert_eq!(ten_regions, hundred_regions);

        let base = isqrt(10_000);
        let expected_capped_bonus =
            base + (base * u64::from(params.max_diversity_bonus_percent) / 100);
        assert_eq!(hundred_regions, expected_capped_bonus);
    }

    #[test]
    fn zero_capacity_weighs_zero_regardless_of_diversity() {
        let params = ProposerParams::default_params();
        assert_eq!(proposer_weight(capacity(0), 1, &params), 0);
        assert_eq!(proposer_weight(capacity(0), 50, &params), 0);
    }

    #[test]
    fn a_tiny_commitment_cannot_weigh_like_a_large_one() {
        // The hole this signature closed, stated as a test. Before
        // `ProvenCapacity`, a provider committing a single small block
        // could pass any `u64` it liked to this function. Now the number
        // is a consequence of the commitment, so a one-block provider
        // weighs like a one-block provider no matter what it wants.
        let params = ProposerParams::default_params();
        let tiny = StorageCommitment {
            merkle_root: [0u8; 32],
            block_count: 1,
            block_size_bytes: 32,
        };
        let large = StorageCommitment {
            merkle_root: [1u8; 32],
            block_count: 1_000_000,
            block_size_bytes: 32,
        };
        let policy = StorageUnitPolicy::new(32).unwrap();

        let tiny_weight =
            proposer_weight(ProvenCapacity::from_commitment(&tiny, &policy), 1, &params);
        let large_weight =
            proposer_weight(ProvenCapacity::from_commitment(&large, &policy), 1, &params);
        assert_eq!(tiny_weight, 1, "one 32-byte block is one unit, weight 1");
        assert!(large_weight > tiny_weight * 100);
    }

    #[test]
    fn summed_capacity_weighs_more_but_still_concavely() {
        // Totalling several replicas is the one arithmetic ProvenCapacity
        // allows, and it must not become a way around the curve.
        let params = ProposerParams::default_params();
        let single = capacity(10_000);
        let doubled = single.saturating_add(single);
        let w1 = proposer_weight(single, 1, &params);
        let w2 = proposer_weight(doubled, 1, &params);
        assert!(w2 > w1);
        assert!(w2 < 2 * w1, "the concave curve still applies to a sum");
    }

    #[test]
    fn no_sequence_of_additions_mints_capacity_from_nothing() {
        let mut total = ProvenCapacity::none();
        for _ in 0..1_000 {
            total = total.saturating_add(ProvenCapacity::none());
        }
        assert_eq!(total.units(), 0);
        assert_eq!(total.committed_bytes(), 0);
        assert_eq!(
            proposer_weight(total, 100, &ProposerParams::default_params()),
            0
        );
    }
}
