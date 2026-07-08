//! Deterministic, nothing-up-my-sleeve generators for the Bulletproofs
//! range proof (`bp_range`), independent from `curve::basepoint()` — the
//! generator `stealth_impl`/`ring_impl` build signing keys from — so the
//! commitment scheme never shares a discrete-log relationship with
//! anything signature-related. Every generator here is derived by hashing
//! a fixed, human-readable domain tag, so anyone can recompute and verify
//! there is no hidden trapdoor in their construction.

use crate::curve::{hash_to_point, RistrettoPoint};

/// Bit-length every range proof in this crate covers: values are proven
/// to lie in `[0, 2^64)`, matching this crate's `u64` amount type.
pub const BIT_LENGTH: usize = 64;

/// The blinding-factor axis generator ("G" in the Bulletproofs paper's
/// notation).
pub fn blinding_generator() -> RistrettoPoint {
    hash_to_point(&[b"mini-value/bulletproofs/blinding-generator"])
}

/// The value axis generator ("H" in the Bulletproofs paper's notation).
pub fn value_generator() -> RistrettoPoint {
    hash_to_point(&[b"mini-value/bulletproofs/value-generator"])
}

/// The inner-product argument's cross-term generator ("Q"/"u" in various
/// papers' notation).
pub fn ipa_generator() -> RistrettoPoint {
    hash_to_point(&[b"mini-value/bulletproofs/ipa-generator"])
}

/// The per-bit `G` vector generators, length [`BIT_LENGTH`].
pub fn g_vec() -> Vec<RistrettoPoint> {
    (0..BIT_LENGTH)
        .map(|i| hash_to_point(&[b"mini-value/bulletproofs/g-vec", &(i as u64).to_be_bytes()]))
        .collect()
}

/// The per-bit `H` vector generators, length [`BIT_LENGTH`].
pub fn h_vec() -> Vec<RistrettoPoint> {
    (0..BIT_LENGTH)
        .map(|i| hash_to_point(&[b"mini-value/bulletproofs/h-vec", &(i as u64).to_be_bytes()]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generators_are_deterministic() {
        assert_eq!(blinding_generator(), blinding_generator());
        assert_eq!(value_generator(), value_generator());
        assert_eq!(ipa_generator(), ipa_generator());
        assert_eq!(g_vec(), g_vec());
        assert_eq!(h_vec(), h_vec());
    }

    #[test]
    fn generators_are_all_distinct() {
        let mut all = vec![blinding_generator(), value_generator(), ipa_generator()];
        all.extend(g_vec());
        all.extend(h_vec());
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(all[i], all[j], "generators at {i} and {j} collided");
            }
        }
    }

    #[test]
    fn vectors_have_the_expected_length() {
        assert_eq!(g_vec().len(), BIT_LENGTH);
        assert_eq!(h_vec().len(), BIT_LENGTH);
    }
}
