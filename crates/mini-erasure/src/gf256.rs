//! Arithmetic in `GF(2^8)`, the finite field Reed-Solomon erasure coding is
//! built on -- the same field (with the same reduction polynomial) used by
//! QR codes, PDF417, RAID6, and RFC 5510's Reed-Solomon FEC scheme. Byte
//! addition is XOR (no carry, characteristic 2); multiplication reduces by
//! the standard primitive polynomial `x^8 + x^4 + x^3 + x^2 + 1` (`0x11D`,
//! low byte `0x1D` since the leading `x^8` term is implicit in an 8-bit
//! representation).

/// Multiply two field elements via the standard "Russian peasant"
/// carry-less multiply-and-reduce, the textbook `GF(2^8)` algorithm.
pub fn mul(mut a: u8, mut b: u8) -> u8 {
    let mut product: u8 = 0;
    for _ in 0..8 {
        if b & 1 != 0 {
            product ^= a;
        }
        let carry = a & 0x80;
        a <<= 1;
        if carry != 0 {
            a ^= 0x1D;
        }
        b >>= 1;
    }
    product
}

/// The multiplicative inverse of a nonzero field element (`a * inv(a) ==
/// 1`). Brute-force search over the 255 nonzero elements -- this crate
/// only inverts small (`data_shards`-sized) matrices, not a per-byte hot
/// path, so simplicity wins over building log/antilog tables.
///
/// # Panics
/// Panics if `a == 0` -- zero has no multiplicative inverse, and every
/// call site here only ever inverts a matrix already known to be
/// non-singular.
pub fn inv(a: u8) -> u8 {
    assert!(a != 0, "0 has no multiplicative inverse in GF(2^8)");
    for candidate in 1..=255u8 {
        if mul(a, candidate) == 1 {
            return candidate;
        }
    }
    unreachable!("every nonzero byte in GF(2^8) has an inverse")
}

/// `base` raised to the `exp`-th power by repeated multiplication.
pub fn pow(base: u8, exp: u32) -> u8 {
    let mut result: u8 = 1;
    for _ in 0..exp {
        result = mul(result, base);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplying_by_zero_is_zero() {
        for a in 0..=255u8 {
            assert_eq!(mul(a, 0), 0);
            assert_eq!(mul(0, a), 0);
        }
    }

    #[test]
    fn multiplying_by_one_is_identity() {
        for a in 0..=255u8 {
            assert_eq!(mul(a, 1), a);
            assert_eq!(mul(1, a), a);
        }
    }

    #[test]
    fn multiplication_is_commutative() {
        for a in 0..=255u8 {
            for b in 0..=255u8 {
                assert_eq!(mul(a, b), mul(b, a));
            }
        }
    }

    #[test]
    fn every_nonzero_element_has_a_true_inverse() {
        for a in 1..=255u8 {
            let inverse = inv(a);
            assert_eq!(
                mul(a, inverse),
                1,
                "inv({a}) = {inverse} did not satisfy a * inv(a) == 1"
            );
        }
    }

    #[test]
    #[should_panic]
    fn inverting_zero_panics() {
        inv(0);
    }

    #[test]
    fn pow_matches_repeated_multiplication() {
        for base in [2u8, 3, 7, 200] {
            let mut expected = 1u8;
            for exp in 0..6u32 {
                assert_eq!(pow(base, exp), expected);
                expected = mul(expected, base);
            }
        }
    }
}
