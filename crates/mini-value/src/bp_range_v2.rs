//! Gate #72 remediation, Bulletproofs half: a single-value range proof
//! over the vendored `bulletproofs` crate (Bünz, Bootle, Boneh, Poelstra,
//! Wuille, Maxwell, "Bulletproofs: Short Proofs for Confidential
//! Transactions and More", the same construction [`crate::bp_range`]/
//! [`crate::bp_ipa`] re-derive by hand), composed here rather than
//! maintaining a second bespoke implementation of the same protocol.
//!
//! ## Why a new module, not an in-place rewrite of `bp_range`
//!
//! Unlike `mini_treasury::frost_sign` (Gate #72's signing half, D-0517),
//! `bp_range`/`bp_ipa`'s hand-rolled Pedersen commitment basis
//! (`bp_generators::{blinding_generator, value_generator, g_vec, h_vec}`)
//! is load-bearing wire format for real, wide callers today:
//! `mini-private-payment`'s claim/amount/scan modules, `mini-bounty`, and
//! `mini-shielded-verify` all commit to and verify against that exact
//! generator basis, and `mini-private-payment/tests/vectors.rs`'s own
//! docs are explicit that a wire-format change there "is a version bump
//! and a decision entry, not a test update." The vendored `bulletproofs`
//! crate uses its own, different `PedersenGens`/`BulletproofGens` basis
//! (audited, but not the *same* basis) — commitments made under one basis
//! cannot be homomorphically summed against commitments made under the
//! other, so swapping `bp_range` in place would silently break every
//! existing balance check across those three crates rather than just
//! changing which library computes the proof.
//!
//! This module is therefore additive: a real, working, audited-library-
//! backed range-proof implementation, available for new code (starting
//! with a future `PrivatePaymentV3` wire format) without touching or
//! breaking `bp_range`'s existing callers or golden wire vectors. It is
//! not yet wired into any consensus-checked path — see this module's own
//! `Result`/entropy-failure semantics, matching `bp_range`'s.
//!
//! [FREEZE reminder — D-0036/D-0037] A founder-overridden, AI-authored
//! prototype pending external cryptography audit, exactly like every
//! other value-layer primitive in this crate.

use bulletproofs::{BulletproofGens, PedersenGens, RangeProof as InnerRangeProof};
use curve25519_dalek::traits::Identity;
use rand_core::OsRng;

use crate::canonical::{canonical_point, canonical_scalar};
use crate::curve::{RistrettoPoint, Scalar};
use crate::error::{Result, ValueError};

/// Bit width every range proof in this module proves membership in
/// `[0, 2^BIT_LENGTH)` for — matching [`crate::bp_range::BIT_LENGTH`].
pub const BIT_LENGTH: usize = 64;

/// Fixed encoded size of a [`RangeProofV2`] at this module's fixed
/// `BIT_LENGTH`/aggregation-size-1 (Section 9's "expected 64-bit single
/// proof encoding is 672 bytes"): 4 compressed points (`A`, `S`, `T_1`,
/// `T_2`) + `2*log2(64)=12` compressed IPA points (`L`/`R`) + 5 scalars
/// (`t_x`, `t_x_blinding`, `e_blinding`, and the IPA's final `a`/`b`) =
/// `16*32 + 5*32`. Checked against the real vendored encoding by
/// [`tests::proof_encoding_is_exactly_672_bytes`] rather than trusted as
/// arithmetic alone.
pub const RANGE_PROOF_V2_BYTES: usize = 672;

/// Domain-separating transcript label. Distinct from anything else in
/// this tree (and from `bp_range`'s own, unrelated transcript, which is a
/// plain hash chain rather than a Merlin transcript) so a proof made here
/// can never be replayed as though it were made under a different
/// protocol's challenge derivation.
const TRANSCRIPT_LABEL: &[u8] = b"mininet/mini-value/bp-range-v2/v1";

/// The blinding-axis generator ("G_b" in the Gate #72 audit's Section
/// 8.1 notation): `H2G("mininet/value/pedersen/blinding-generator/v3")`,
/// the exact domain string that section specifies. `pub(crate)` so
/// [`crate::mlsag_v3`]'s commitment-difference column can be built over
/// this exact same axis — the two must agree, or the MLSAG relation
/// `D = C - C'` on `G_b` never cancels the value term.
pub(crate) fn blinding_generator_v2() -> RistrettoPoint {
    crate::curve::hash_to_point(&[b"mininet/value/pedersen/blinding-generator/v3"])
}

/// The value-axis generator ("H_v" in the same notation):
/// `H2G("mininet/value/pedersen/value-generator/v3")`.
pub(crate) fn value_generator_v2() -> RistrettoPoint {
    crate::curve::hash_to_point(&[b"mininet/value/pedersen/value-generator/v3"])
}

/// Domain-hashed Pedersen generators, **not** `PedersenGens::default()`.
///
/// `bulletproofs::PedersenGens::default()` sets `B` to the raw Ristretto
/// basepoint and `B_blinding` to a SHA3-512 hash of it — a real,
/// documented nothing-up-my-sleeve pair, but not an *independent* one for
/// this protocol: `B` is then the exact same point [`crate::curve::
/// basepoint`] uses everywhere else in this crate as the signing
/// generator (one-time keys, stealth addresses, the MLSAG ownership
/// column). The Gate #72 audit's Section 8.1 requires `H_v != signing
/// base point G` precisely because sharing a generator between a
/// commitment's value axis and the key/signature layer breaks the
/// "nothing up my sleeve, no relation to anything else" property a
/// Pedersen commitment's hiding proof depends on -- the same reasoning
/// [`crate::bp_generators`]'s own docs already give for keeping *its*
/// generators independent of `basepoint()`. Using the library default
/// unexamined here was a defect in this module's initial version: it
/// composed the vendored proving/verifying *algorithm* correctly but
/// inherited a basis the audit itself forbids. `BulletproofGens::new`
/// derives its own per-bit generator vectors from a fixed internal label
/// independent of whatever `PedersenGens` is paired with it (the crate's
/// own docs advertise "pluggable bases" for exactly this reason), so
/// supplying a custom `PedersenGens` here changes nothing else about the
/// proof system's soundness.
fn generators() -> (PedersenGens, BulletproofGens) {
    (
        PedersenGens {
            B: value_generator_v2(),
            B_blinding: blinding_generator_v2(),
        },
        BulletproofGens::new(BIT_LENGTH, 1),
    )
}

/// A Bulletproofs range proof for one value committed elsewhere, produced
/// by the vendored `bulletproofs` crate.
#[derive(Debug, Clone)]
pub struct RangeProofV2 {
    inner: InnerRangeProof,
}

/// The vendored `bulletproofs::RangeProof` has no `PartialEq`/`Eq` of its
/// own; comparing the canonical encoding is exact (this module's own
/// `to_bytes`/`from_bytes` round-trip has no lossy step) and lets
/// `PrivatePaymentClaimV3` (which embeds a `RangeProofV2` per output)
/// derive `PartialEq`/`Eq` the same way `crate::bp_range::RangeProof`'s
/// callers already do.
impl PartialEq for RangeProofV2 {
    fn eq(&self, other: &Self) -> bool {
        self.to_bytes() == other.to_bytes()
    }
}

impl Eq for RangeProofV2 {}

impl RangeProofV2 {
    /// Canonical variable-width encoding (fixed for a given `BIT_LENGTH`,
    /// via the vendored crate's own `to_bytes`).
    pub fn to_bytes(&self) -> Vec<u8> {
        self.inner.to_bytes()
    }

    /// Decode a [`RangeProofV2`]. Rejects malformed field elements and
    /// the wrong length; well-formed is not the same as valid --
    /// [`verify_range_v2`] remains the only thing that decides that.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        InnerRangeProof::from_bytes(bytes)
            .ok()
            .map(|inner| Self { inner })
    }
}

/// Commit to `value` with `blinding`, and prove `value ∈ [0, 2^64)`.
/// Returns the compressed commitment and the proof. Errors only on a
/// local CSPRNG failure or a proving-library internal error (an
/// out-of-range bit length, which `BIT_LENGTH`'s fixed value never
/// triggers).
pub fn prove_range_v2(value: u64, blinding: Scalar) -> Result<([u8; 32], RangeProofV2)> {
    let (pc_gens, bp_gens) = generators();
    let mut transcript = merlin::Transcript::new(TRANSCRIPT_LABEL);
    // `OsRng` (via `getrandom`) rather than the convenience API's internal
    // `rand::thread_rng()`, matching this tree's `mini-custody`/`mini-
    // crypto` OS-entropy convention throughout.
    let mut rng = OsRng;
    let (proof, commitment) = InnerRangeProof::prove_single_with_rng(
        &bp_gens,
        &pc_gens,
        &mut transcript,
        value,
        &blinding,
        BIT_LENGTH,
        &mut rng,
    )
    .map_err(|_| ValueError::InvalidInput)?;
    Ok((commitment.to_bytes(), RangeProofV2 { inner: proof }))
}

/// [`prove_range_v2`] for a caller outside this crate, which cannot name
/// `curve25519_dalek::Scalar` (this crate does not re-export it — every
/// other cross-crate entry point here, [`pedersen_commitment_v2`]
/// included, takes a blinding factor as bytes for the same reason).
/// `None` on a non-canonical blinding encoding, matching [`canonical_scalar`]'s
/// contract everywhere else in this crate.
pub fn prove_range_v2_from_bytes(
    value: u64,
    blinding: &[u8],
) -> Option<Result<([u8; 32], RangeProofV2)>> {
    let scalar = canonical_scalar(blinding)?;
    Some(prove_range_v2(value, scalar))
}

/// Verify `proof` shows `commitment` hides a non-negative, in-bounds
/// amount under this module's Pedersen basis. `false` on any malformed
/// input rather than panicking.
pub fn verify_range_v2(commitment: [u8; 32], proof: &RangeProofV2) -> bool {
    let (pc_gens, bp_gens) = generators();
    let mut transcript = merlin::Transcript::new(TRANSCRIPT_LABEL);
    let compressed = curve25519_dalek::ristretto::CompressedRistretto(commitment);
    proof
        .inner
        .verify_single(&bp_gens, &pc_gens, &mut transcript, &compressed, BIT_LENGTH)
        .is_ok()
}

/// The Pedersen commitment `blinding·B_blinding + value·B`, under this
/// module's own basis ([`PedersenGens::default`]) -- **with no range
/// proof**. See [`crate::confidential_impl::pedersen_commitment`]'s own
/// docs for why a bare commitment is sometimes exactly what a caller
/// needs and exactly what it must never substitute for on an unproven
/// output.
pub fn pedersen_commitment_v2(value: u64, blinding_factor: &[u8]) -> Option<[u8; 32]> {
    let blinding = canonical_scalar(blinding_factor)?;
    let (pc_gens, _) = generators();
    Some(
        pc_gens
            .commit(Scalar::from(value), blinding)
            .compress()
            .to_bytes(),
    )
}

/// A commitment to a **publicly known** amount under this module's basis:
/// `value · B`, zero blinding. See
/// [`crate::confidential_impl::public_amount_commitment`]'s own docs --
/// the fee-commitment use case is identical here.
pub fn public_amount_commitment_v2(value: u64) -> [u8; 32] {
    let (pc_gens, _) = generators();
    pc_gens
        .commit(Scalar::from(value), Scalar::ZERO)
        .compress()
        .to_bytes()
}

/// Sum a list of compressed commitment points, `None` if any is
/// malformed. An empty list sums to the identity.
fn sum_commitments_v2(commitments: &[Vec<u8>]) -> Option<RistrettoPoint> {
    let mut sum = RistrettoPoint::identity();
    for c in commitments {
        sum += canonical_point(c)?;
    }
    Some(sum)
}

/// Verify that the sum of `input_commitments` equals the sum of
/// `output_commitments`, under this module's Pedersen basis -- the same
/// additively-homomorphic balance check
/// [`crate::confidential_impl::MininetConfidentialAmount::verify_balance`]
/// performs under the old basis. Mixing commitments from the two bases in
/// one balance check will never cancel, by design (see this module's own
/// top-level docs) -- that is the entire reason this is a new module
/// rather than an in-place change to the old one.
pub fn verify_balance_v2(input_commitments: &[Vec<u8>], output_commitments: &[Vec<u8>]) -> bool {
    let (Some(sum_in), Some(sum_out)) = (
        sum_commitments_v2(input_commitments),
        sum_commitments_v2(output_commitments),
    ) else {
        return false;
    };
    sum_in == sum_out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_committed_amount_verifies_its_own_range_proof() {
        let blinding = crate::curve::random_scalar().unwrap();
        let (commitment, proof) = prove_range_v2(1_000, blinding).unwrap();
        assert!(verify_range_v2(commitment, &proof));
    }

    #[test]
    fn a_tampered_commitment_fails_verification() {
        let blinding = crate::curve::random_scalar().unwrap();
        let (mut commitment, proof) = prove_range_v2(1_000, blinding).unwrap();
        commitment[0] ^= 0xff;
        assert!(!verify_range_v2(commitment, &proof));
    }

    #[test]
    fn a_proof_for_a_different_value_does_not_verify_this_commitment() {
        let blinding = crate::curve::random_scalar().unwrap();
        let (commitment, _) = prove_range_v2(1_000, blinding).unwrap();
        let (_, other_proof) = prove_range_v2(2_000, blinding).unwrap();
        assert!(!verify_range_v2(commitment, &other_proof));
    }

    #[test]
    fn proof_bytes_round_trip_through_encode_decode() {
        let blinding = crate::curve::random_scalar().unwrap();
        let (commitment, proof) = prove_range_v2(42, blinding).unwrap();
        let decoded = RangeProofV2::from_bytes(&proof.to_bytes()).unwrap();
        assert!(verify_range_v2(commitment, &decoded));
    }

    #[test]
    fn truncated_proof_bytes_are_rejected_without_panicking() {
        let blinding = crate::curve::random_scalar().unwrap();
        let (_, proof) = prove_range_v2(42, blinding).unwrap();
        let bytes = proof.to_bytes();
        assert!(RangeProofV2::from_bytes(&bytes[..bytes.len() - 1]).is_none());
    }

    #[test]
    fn a_bare_commitment_is_the_same_point_the_proving_path_produces() {
        for value in [0u64, 1, 1_000, u64::MAX] {
            let blinding = crate::curve::random_scalar().unwrap();
            let (proven, _) = prove_range_v2(value, blinding).unwrap();
            let bare = pedersen_commitment_v2(value, &blinding.to_bytes()).unwrap();
            assert_eq!(proven, bare, "value {value}");
        }
    }

    #[test]
    fn a_bare_commitment_rejects_a_malformed_blinding_factor() {
        assert_eq!(pedersen_commitment_v2(1, b"too-short"), None);
    }

    #[test]
    fn balanced_inputs_and_outputs_verify() {
        let b_in1 = crate::curve::random_scalar().unwrap();
        let b_in2 = crate::curve::random_scalar().unwrap();
        let b_out = b_in1 + b_in2;
        let (in1, _) = prove_range_v2(30, b_in1).unwrap();
        let (in2, _) = prove_range_v2(12, b_in2).unwrap();
        let (out, _) = prove_range_v2(42, b_out).unwrap();
        assert!(verify_balance_v2(
            &[in1.to_vec(), in2.to_vec()],
            &[out.to_vec()]
        ));
    }

    #[test]
    fn unbalanced_inputs_and_outputs_fail_verification() {
        let b_in = crate::curve::random_scalar().unwrap();
        let b_out = crate::curve::random_scalar().unwrap();
        let (input, _) = prove_range_v2(50, b_in).unwrap();
        let (output, _) = prove_range_v2(50, b_out).unwrap();
        assert!(!verify_balance_v2(&[input.to_vec()], &[output.to_vec()]));
    }

    #[test]
    fn empty_inputs_and_outputs_are_vacuously_balanced() {
        assert!(verify_balance_v2(&[], &[]));
    }

    #[test]
    fn malformed_commitment_in_balance_check_fails_without_panicking() {
        assert!(!verify_balance_v2(&[vec![0u8; 4]], &[vec![0u8; 32]]));
    }

    #[test]
    fn a_fee_commitment_participates_correctly_in_the_balance_equation() {
        let b_in = crate::curve::random_scalar().unwrap();
        let (input, _) = prove_range_v2(110, b_in).unwrap();
        let (output, _) = prove_range_v2(100, b_in).unwrap();
        let fee = public_amount_commitment_v2(10);
        assert!(verify_balance_v2(
            &[input.to_vec()],
            &[output.to_vec(), fee.to_vec()]
        ));
    }

    #[test]
    fn v1_and_v2_commitments_to_the_same_value_and_blinding_do_not_match() {
        // The whole reason this is a new module: the two bases are
        // different points, so the "same" opening produces two different
        // commitments that can never be balanced against each other.
        let blinding = crate::curve::random_scalar().unwrap();
        let v2 = pedersen_commitment_v2(1_000, &blinding.to_bytes()).unwrap();
        let v1 =
            crate::confidential_impl::pedersen_commitment(1_000, &blinding.to_bytes()).unwrap();
        assert_ne!(v1, v2);
    }

    #[test]
    /// Gate #72 Section 8.1's own requirement: neither commitment axis may
    /// be identity, they must differ from each other, and neither may
    /// coincide with the signing base point `G` -- the exact property
    /// `PedersenGens::default()` violated (its `B` *is* `G`) before this
    /// module built its own generators. See [`generators`]'s doc comment.
    fn generator_independence() {
        let g_b = blinding_generator_v2();
        let h_v = value_generator_v2();
        let g = crate::curve::basepoint();
        assert_ne!(g_b, RistrettoPoint::identity());
        assert_ne!(h_v, RistrettoPoint::identity());
        assert_ne!(g_b, h_v);
        assert_ne!(g_b, g);
        assert_ne!(h_v, g, "value axis must not be the signing base point");
    }

    #[test]
    fn proof_encoding_is_exactly_672_bytes() {
        let blinding = crate::curve::random_scalar().unwrap();
        let (_, proof) = prove_range_v2(1_000, blinding).unwrap();
        assert_eq!(proof.to_bytes().len(), RANGE_PROOF_V2_BYTES);
    }

    #[test]
    fn generators_are_deterministic_across_calls() {
        assert_eq!(blinding_generator_v2(), blinding_generator_v2());
        assert_eq!(value_generator_v2(), value_generator_v2());
    }
}
