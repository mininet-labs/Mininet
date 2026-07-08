//! Integer square root — the concave curve [`crate::weight::proposer_weight`]
//! is built from. From-scratch, deterministic, no floats: the same "all
//! integer, so exactly reproducible" convention `mini-reward`'s accrual math
//! and `mini-uniqueness`'s trust propagation both use.

/// The largest integer `r` such that `r * r <= n` (Newton's method, integer
/// arithmetic only). `isqrt(0) == 0`.
///
/// Works in `u128` internally so `n + 1` never overflows even at `u64::MAX`
/// — the natural width for a `u64` input is too narrow for Newton's method's
/// own intermediate `x + 1`.
pub fn isqrt(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    let n = n as u128;
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isqrt_of_zero_and_one() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(1), 1);
    }

    #[test]
    fn isqrt_of_perfect_squares() {
        for r in 0u64..1000 {
            assert_eq!(isqrt(r * r), r);
        }
    }

    #[test]
    fn isqrt_rounds_down_for_non_perfect_squares() {
        assert_eq!(isqrt(2), 1);
        assert_eq!(isqrt(3), 1);
        assert_eq!(isqrt(8), 2);
        assert_eq!(isqrt(99), 9);
        assert_eq!(isqrt(100), 10);
        assert_eq!(isqrt(101), 10);
    }

    #[test]
    fn isqrt_of_large_values() {
        let n = u64::MAX;
        let r = isqrt(n);
        assert!(r * r <= n);
        assert!((r + 1).checked_mul(r + 1).map(|sq| sq > n).unwrap_or(true));
    }
}
