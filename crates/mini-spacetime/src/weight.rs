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
/// `raw_capacity_units` must already be **proven** capacity from a real
/// proof-of-space-time protocol ([`crate::proof::ProofOfSpaceTimeSource`]) —
/// this function trusts its input completely; it is not itself a defense
/// against a node merely claiming capacity it does not hold.
///
/// The curve: capacity is capped per identity, then square-rooted (concave:
/// doubling capacity yields roughly 1.41x weight, never 2x), then a bounded
/// diversity bonus is added on top.
pub fn proposer_weight(
    raw_capacity_units: u64,
    distinct_regions: u32,
    params: &ProposerParams,
) -> u64 {
    let capped = raw_capacity_units.min(params.capacity_cap_units);
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

    #[test]
    fn doubling_capacity_yields_less_than_double_weight() {
        let params = ProposerParams::default_params();
        let w1 = proposer_weight(10_000, 1, &params);
        let w2 = proposer_weight(20_000, 1, &params);
        assert!(w2 > w1, "more capacity should still weigh more");
        assert!(
            w2 < 2 * w1,
            "concave curve: doubling capacity must not double weight (w1={w1} w2={w2})"
        );
    }

    #[test]
    fn capacity_beyond_the_cap_contributes_nothing_further() {
        let params = ProposerParams::default_params();
        let at_cap = proposer_weight(params.capacity_cap_units, 1, &params);
        let way_over = proposer_weight(params.capacity_cap_units * 100, 1, &params);
        assert_eq!(at_cap, way_over);
    }

    #[test]
    fn diversity_bonus_increases_weight_but_is_capped() {
        let params = ProposerParams::default_params();
        let one_region = proposer_weight(10_000, 1, &params);
        // 3 regions: 2 extra * 10%/region = 20%, still below the 50% cap.
        let three_regions = proposer_weight(10_000, 3, &params);
        // 10 and 100 regions both push well past the cap (9 and 99 extra
        // regions respectively), so both land on the same capped bonus.
        let ten_regions = proposer_weight(10_000, 10, &params);
        let hundred_regions = proposer_weight(10_000, 100, &params);

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
        assert_eq!(proposer_weight(0, 1, &params), 0);
        assert_eq!(proposer_weight(0, 50, &params), 0);
    }
}
