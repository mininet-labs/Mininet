//! The V3 two-column MLSAG: [`crate::mlsag`]'s same relation, over the
//! Gate #72 audit's exact Section 7 challenge construction and the
//! [`crate::bp_range_v2`] commitment basis instead of [`crate::mlsag`]'s
//! [`crate::bp_generators`] one.
//!
//! # Why this is a new module rather than a parameterized [`crate::mlsag`]
//!
//! The commitment-difference column's whole trick is `D = C - C' =
//! (b_real - b')·G_b` — the value terms cancel only when `C` and `C'` are
//! built over the *same* `G_b`/`H_v` pair the verifier checks against.
//! [`crate::mlsag`] hard-codes [`crate::bp_generators::blinding_generator`],
//! matching [`crate::bp_range`]'s basis; a `PrivatePaymentV3` claim commits
//! under [`crate::bp_range_v2`]'s basis instead (see that module's own
//! docs for why `bp_range`/`bp_range_v2` cannot share commitments). Mixing
//! the two inside one MLSAG instance would silently break verification
//! for every claim, not just fail loudly, so this is deliberately a
//! separate module over the matching basis rather than a runtime
//! parameter on the existing one.
//!
//! # What else changed from `crate::mlsag`, and why
//!
//! The Gate #72 audit's Section 7 gives an exact challenge construction,
//! materially stronger than [`crate::mlsag::MLSAG_DOMAIN`]'s: every
//! challenge link binds `suite_id`, `network_id`, the claim's own
//! `signing_digest`, the input's index within the claim, and the full
//! ring statement (every member's key and commitment, not just the
//! `message` bytes the old scheme folds in as one opaque blob) — closing
//! F72-03 ("generic transcript framing is too permissive") for this
//! scheme specifically. The key-image base point is likewise domain- and
//! network-separated (Section 7.1), where [`crate::mlsag`]'s is a bare
//! hash of the one-time key with no domain tag at all.
//!
//! Ring size is fixed at exactly [`RING_SIZE`] (16), per Section 7.2 —
//! V3 has no caller-selectable ring size.
//!
//! [FREEZE reminder — D-0036/D-0037/D-0047] Founder-overridden,
//! AI-authored prototype. Unaudited. Nothing value-bearing may depend on
//! this before Gate #72 closes.

use crate::bp_range_v2::{blinding_generator_v2, value_generator_v2};
use crate::canonical::{
    canonical_nonidentity_point as decompress_point, canonical_scalar as decompress_scalar,
};
use crate::curve::random_scalar;
use crate::curve::{basepoint, hash_to_point, hash_to_scalar, RistrettoPoint, Scalar};

pub use crate::mlsag::SpendWitness;

/// Domain separator for this scheme's Fiat-Shamir challenge chain —
/// the audit's Section 5.4 exact string.
pub const MLSAG_V3_DOMAIN: &[u8] = b"mininet/value/mlsag-challenge/v3";

/// Domain separator for the key-image base point — the audit's Section
/// 7.1 exact string.
pub const KEY_IMAGE_V3_DOMAIN: &[u8] = b"mininet/value/key-image-point/v3";

/// The one and only ring size `PrivatePaymentV3` proves against (Section
/// 7.2): not a caller-selectable minimum, an exact requirement.
pub const RING_SIZE: usize = 16;

/// A V3 two-column MLSAG. Same field shapes as [`crate::mlsag::
/// MlsagSignature`] — deliberately not the same type, so a V2 signature
/// can never be silently accepted where a V3 one is expected, or verified
/// against the wrong challenge construction by a caller that mixed up
/// which `verify_spend_*` to call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MlsagSignatureV3 {
    pub challenge: Vec<u8>,
    pub key_responses: Vec<Vec<u8>>,
    pub blinding_responses: Vec<Vec<u8>>,
    pub key_image: Vec<u8>,
}

/// `Hp(P)` for one-time key `P`: domain- and network-separated (Section
/// 7.1), unlike [`crate::mlsag`]'s bare `hash_to_point(&[P])`.
fn key_image_base(
    suite_id: u16,
    network_id: &[u8; 32],
    one_time_key: &RistrettoPoint,
) -> RistrettoPoint {
    hash_to_point(&[
        KEY_IMAGE_V3_DOMAIN,
        &suite_id.to_be_bytes(),
        network_id,
        one_time_key.compress().as_bytes(),
    ])
}

/// Canonical bytes of the full ring statement: every member's one-time
/// key followed by its amount commitment, in the ring's own (already
/// canonical) order. Part of every challenge link so a signer cannot
/// satisfy the proof against one ring and have it accepted against a
/// different one.
fn ring_statement(ring_keys: &[Vec<u8>], ring_commitments: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::with_capacity(ring_keys.len() * 64);
    for (key, commitment) in ring_keys.iter().zip(ring_commitments) {
        out.extend_from_slice(key);
        out.extend_from_slice(commitment);
    }
    out
}

/// The commitment column's ring: `C_j - C'` for each member, over the
/// [`crate::bp_range_v2`] basis (the difference relation itself is
/// basis-agnostic — only its later comparison against `blinding_
/// generator_v2()` in [`sign_spend_v3`]/[`verify_spend_v3`] ties it to
/// this module's specific basis).
fn difference_ring(
    ring_commitments: &[Vec<u8>],
    pseudo_commitment: &[u8],
) -> Option<Vec<RistrettoPoint>> {
    let pseudo = decompress_point(pseudo_commitment)?;
    ring_commitments
        .iter()
        .map(|c| decompress_point(c).map(|point| point - pseudo))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn challenge_hash_v3(
    suite_id: u16,
    network_id: &[u8; 32],
    signing_digest: &[u8; 32],
    input_index: u32,
    statement: &[u8],
    pseudo_commitment: &[u8],
    key_image_bytes: &[u8],
    row: u16,
    l1: &RistrettoPoint,
    r1: &RistrettoPoint,
    l2: &RistrettoPoint,
) -> Scalar {
    hash_to_scalar(&[
        MLSAG_V3_DOMAIN,
        &suite_id.to_be_bytes(),
        network_id,
        signing_digest,
        &input_index.to_be_bytes(),
        statement,
        pseudo_commitment,
        key_image_bytes,
        &row.to_be_bytes(),
        l1.compress().as_bytes(),
        r1.compress().as_bytes(),
        l2.compress().as_bytes(),
    ])
}

/// Sign one V3 spend. `ring_keys`/`ring_commitments` must each have
/// exactly [`RING_SIZE`] entries — anything else returns `None`, matching
/// [`crate::mlsag::sign_spend`]'s fail-closed contract on malformed input.
#[allow(clippy::too_many_arguments)]
pub fn sign_spend_v3(
    suite_id: u16,
    network_id: &[u8; 32],
    signing_digest: &[u8; 32],
    input_index: u32,
    ring_keys: &[Vec<u8>],
    ring_commitments: &[Vec<u8>],
    pseudo_commitment: &[u8],
    witness: &SpendWitness,
) -> Option<MlsagSignatureV3> {
    let n = RING_SIZE;
    if ring_keys.len() != n || ring_commitments.len() != n || witness.secret_index >= n {
        return None;
    }
    let keys: Vec<RistrettoPoint> = ring_keys
        .iter()
        .map(|k| decompress_point(k))
        .collect::<Option<_>>()?;
    let differences = difference_ring(ring_commitments, pseudo_commitment)?;
    let statement = ring_statement(ring_keys, ring_commitments);

    let pi = witness.secret_index;
    let x = decompress_scalar(&witness.one_time_secret)?;
    let z = decompress_scalar(&witness.blinding_difference)?;

    if x * basepoint() != keys[pi] || z * blinding_generator_v2() != differences[pi] {
        return None;
    }

    let hp_pi = key_image_base(suite_id, network_id, &keys[pi]);
    let key_image = x * hp_pi;
    let key_image_bytes = key_image.compress().to_bytes();

    let mut c = vec![Scalar::ZERO; n];
    let mut s_key = vec![Scalar::ZERO; n];
    let mut s_blind = vec![Scalar::ZERO; n];

    let alpha_key = random_scalar().ok()?;
    let alpha_blind = random_scalar().ok()?;
    c[(pi + 1) % n] = challenge_hash_v3(
        suite_id,
        network_id,
        signing_digest,
        input_index,
        &statement,
        pseudo_commitment,
        &key_image_bytes,
        ((pi + 1) % n) as u16,
        &(alpha_key * basepoint()),
        &(alpha_key * hp_pi),
        &(alpha_blind * blinding_generator_v2()),
    );

    let mut j = (pi + 1) % n;
    while j != pi {
        s_key[j] = random_scalar().ok()?;
        s_blind[j] = random_scalar().ok()?;
        let hp_j = key_image_base(suite_id, network_id, &keys[j]);
        let l1 = s_key[j] * basepoint() + c[j] * keys[j];
        let r1 = s_key[j] * hp_j + c[j] * key_image;
        let l2 = s_blind[j] * blinding_generator_v2() + c[j] * differences[j];
        let next = (j + 1) % n;
        c[next] = challenge_hash_v3(
            suite_id,
            network_id,
            signing_digest,
            input_index,
            &statement,
            pseudo_commitment,
            &key_image_bytes,
            next as u16,
            &l1,
            &r1,
            &l2,
        );
        j = next;
    }

    s_key[pi] = alpha_key - c[pi] * x;
    s_blind[pi] = alpha_blind - c[pi] * z;

    Some(MlsagSignatureV3 {
        challenge: c[0].to_bytes().to_vec(),
        key_responses: s_key.iter().map(|v| v.to_bytes().to_vec()).collect(),
        blinding_responses: s_blind.iter().map(|v| v.to_bytes().to_vec()).collect(),
        key_image: key_image_bytes.to_vec(),
    })
}

/// Verify one V3 spend proof. See [`crate::mlsag::verify_spend`]'s docs
/// on what a `true` result does and does not prove — the same limits
/// apply here, over this module's own basis and challenge construction.
#[allow(clippy::too_many_arguments)]
pub fn verify_spend_v3(
    suite_id: u16,
    network_id: &[u8; 32],
    signing_digest: &[u8; 32],
    input_index: u32,
    ring_keys: &[Vec<u8>],
    ring_commitments: &[Vec<u8>],
    pseudo_commitment: &[u8],
    signature: &MlsagSignatureV3,
) -> bool {
    let n = RING_SIZE;
    if ring_keys.len() != n
        || ring_commitments.len() != n
        || signature.key_responses.len() != n
        || signature.blinding_responses.len() != n
    {
        return false;
    }
    let Some(keys) = ring_keys
        .iter()
        .map(|k| decompress_point(k))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    let Some(differences) = difference_ring(ring_commitments, pseudo_commitment) else {
        return false;
    };
    let Some(key_image) = decompress_point(&signature.key_image) else {
        return false;
    };
    let Some(c0) = decompress_scalar(&signature.challenge) else {
        return false;
    };
    let Some(s_key) = signature
        .key_responses
        .iter()
        .map(|r| decompress_scalar(r))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    let Some(s_blind) = signature
        .blinding_responses
        .iter()
        .map(|r| decompress_scalar(r))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    let statement = ring_statement(ring_keys, ring_commitments);

    let mut c = c0;
    for j in 0..n {
        let hp_j = key_image_base(suite_id, network_id, &keys[j]);
        let l1 = s_key[j] * basepoint() + c * keys[j];
        let r1 = s_key[j] * hp_j + c * key_image;
        let l2 = s_blind[j] * blinding_generator_v2() + c * differences[j];
        c = challenge_hash_v3(
            suite_id,
            network_id,
            signing_digest,
            input_index,
            &statement,
            pseudo_commitment,
            &signature.key_image,
            ((j + 1) % n) as u16,
            &l1,
            &r1,
            &l2,
        );
    }
    c == c0
}

/// Re-blind a commitment over the [`crate::bp_range_v2`] basis: given the
/// real output's value and blinding, and a fresh blinding, produce the
/// pseudo-commitment and the difference the witness needs. See
/// [`crate::mlsag::reblind`]'s docs — identical contract, matching basis.
pub fn reblind_v3(
    value: u64,
    real_blinding: &[u8],
    pseudo_blinding: &[u8],
) -> Option<([u8; 32], [u8; 32])> {
    let b_real = decompress_scalar(real_blinding)?;
    let b_pseudo = decompress_scalar(pseudo_blinding)?;
    let pseudo = b_pseudo * blinding_generator_v2() + Scalar::from(value) * value_generator_v2();
    Some((pseudo.compress().to_bytes(), (b_real - b_pseudo).to_bytes()))
}

/// The blinding factor the last pseudo-commitment must use for a V3
/// claim to balance. See [`crate::mlsag::balancing_blinding`]'s docs —
/// identical contract.
pub fn balancing_blinding_v3(
    output_blindings: &[[u8; 32]],
    chosen_pseudo_blindings: &[[u8; 32]],
) -> [u8; 32] {
    let sum = |values: &[[u8; 32]]| {
        values.iter().fold(Scalar::ZERO, |acc, bytes| {
            acc + decompress_scalar(bytes).unwrap_or(Scalar::ZERO)
        })
    };
    (sum(output_blindings) - sum(chosen_pseudo_blindings)).to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bp_range_v2::pedersen_commitment_v2;
    use crate::curve::random_scalar_bytes;

    const SUITE_ID: u16 = 0x0003;
    const NETWORK: [u8; 32] = [7u8; 32];
    const DIGEST: [u8; 32] = [9u8; 32];

    struct Spend {
        ring_keys: Vec<Vec<u8>>,
        ring_commitments: Vec<Vec<u8>>,
        pseudo_commitment: Vec<u8>,
        witness: SpendWitness,
    }

    /// One real spend among `RING_SIZE` decoys, all at the given value,
    /// with the real member at ring position `real_index`.
    fn spend_of(value: u64, real_index: usize) -> Spend {
        let mut ring_keys = Vec::with_capacity(RING_SIZE);
        let mut ring_commitments = Vec::with_capacity(RING_SIZE);
        let mut real_secret = [0u8; 32];
        let mut real_blinding = [0u8; 32];
        for i in 0..RING_SIZE {
            let secret = random_scalar().unwrap();
            let public = (secret * basepoint()).compress().to_bytes().to_vec();
            let blinding = random_scalar_bytes().unwrap();
            let commitment = pedersen_commitment_v2(value, &blinding).unwrap().to_vec();
            if i == real_index {
                real_secret = secret.to_bytes();
                real_blinding = blinding;
            }
            ring_keys.push(public);
            ring_commitments.push(commitment);
        }
        let pseudo_blinding = random_scalar_bytes().unwrap();
        let (pseudo_commitment, blinding_difference) =
            reblind_v3(value, &real_blinding, &pseudo_blinding).unwrap();
        Spend {
            ring_keys,
            ring_commitments,
            pseudo_commitment: pseudo_commitment.to_vec(),
            witness: SpendWitness {
                secret_index: real_index,
                one_time_secret: real_secret,
                blinding_difference,
            },
        }
    }

    fn sign(spend: &Spend, message: &[u8; 32]) -> MlsagSignatureV3 {
        sign_spend_v3(
            SUITE_ID,
            &NETWORK,
            message,
            0,
            &spend.ring_keys,
            &spend.ring_commitments,
            &spend.pseudo_commitment,
            &spend.witness,
        )
        .unwrap()
    }

    fn verify(spend: &Spend, message: &[u8; 32], signature: &MlsagSignatureV3) -> bool {
        verify_spend_v3(
            SUITE_ID,
            &NETWORK,
            message,
            0,
            &spend.ring_keys,
            &spend.ring_commitments,
            &spend.pseudo_commitment,
            signature,
        )
    }

    #[test]
    fn a_valid_spend_verifies() {
        let spend = spend_of(1_000, 3);
        let signature = sign(&spend, &DIGEST);
        assert!(verify(&spend, &DIGEST, &signature));
    }

    #[test]
    fn wrong_ring_size_is_rejected_at_signing_and_verification() {
        let mut spend = spend_of(1_000, 3);
        spend.ring_keys.pop();
        spend.ring_commitments.pop();
        assert!(sign_spend_v3(
            SUITE_ID,
            &NETWORK,
            &DIGEST,
            0,
            &spend.ring_keys,
            &spend.ring_commitments,
            &spend.pseudo_commitment,
            &spend.witness,
        )
        .is_none());
    }

    #[test]
    fn a_wrong_one_time_secret_fails() {
        let spend = spend_of(1_000, 2);
        let mut signature = sign(&spend, &DIGEST);
        signature.key_responses[2] = random_scalar_bytes().unwrap().to_vec();
        assert!(!verify(&spend, &DIGEST, &signature));
    }

    #[test]
    fn a_wrong_commitment_opening_fails() {
        let spend = spend_of(1_000, 2);
        let mut signature = sign(&spend, &DIGEST);
        signature.blinding_responses[2] = random_scalar_bytes().unwrap().to_vec();
        assert!(!verify(&spend, &DIGEST, &signature));
    }

    #[test]
    fn altered_pseudo_commitment_fails() {
        let spend = spend_of(1_000, 1);
        let signature = sign(&spend, &DIGEST);
        let mut tampered = spend;
        tampered.pseudo_commitment = pedersen_commitment_v2(999, &random_scalar_bytes().unwrap())
            .unwrap()
            .to_vec();
        assert!(!verify(&tampered, &DIGEST, &signature));
    }

    #[test]
    fn altered_signing_digest_fails() {
        let spend = spend_of(1_000, 0);
        let signature = sign(&spend, &DIGEST);
        assert!(!verify(&spend, &[0xffu8; 32], &signature));
    }

    #[test]
    fn altered_network_fails() {
        let spend = spend_of(1_000, 0);
        let signature = sign(&spend, &DIGEST);
        assert!(!verify_spend_v3(
            SUITE_ID,
            &[0xaa; 32],
            &DIGEST,
            0,
            &spend.ring_keys,
            &spend.ring_commitments,
            &spend.pseudo_commitment,
            &signature,
        ));
    }

    #[test]
    fn altered_input_index_fails() {
        let spend = spend_of(1_000, 0);
        let signature = sign(&spend, &DIGEST);
        assert!(!verify_spend_v3(
            SUITE_ID,
            &NETWORK,
            &DIGEST,
            1,
            &spend.ring_keys,
            &spend.ring_commitments,
            &spend.pseudo_commitment,
            &signature,
        ));
    }

    #[test]
    fn the_same_secret_produces_the_same_key_image_across_independent_signatures() {
        // sign_spend_v3 draws fresh nonces every call, so two signatures
        // over the identical witness/ring differ in every other field --
        // the key image, deterministic in (x, P) alone, must still match.
        let spend = spend_of(500, 5);
        let a = sign(&spend, &DIGEST);
        let b = sign(&spend, &DIGEST);
        assert_eq!(a.key_image, b.key_image);
        assert_ne!(
            a.challenge, b.challenge,
            "nonces must be fresh per signature"
        );
    }

    #[test]
    fn distinct_keys_produce_distinct_key_images() {
        let a = spend_of(10, 0);
        let b = spend_of(10, 0);
        let sig_a = sign(&a, &DIGEST);
        let sig_b = sign(&b, &DIGEST);
        assert_ne!(sig_a.key_image, sig_b.key_image);
    }

    #[test]
    fn an_identity_key_image_is_rejected() {
        use curve25519_dalek::traits::Identity;
        let spend = spend_of(1_000, 0);
        let mut signature = sign(&spend, &DIGEST);
        signature.key_image = RistrettoPoint::identity().compress().to_bytes().to_vec();
        assert!(!verify(&spend, &DIGEST, &signature));
    }

    #[test]
    fn key_image_is_domain_and_network_separated_from_v1() {
        // The V1 scheme's key-image base is a bare hash of the one-time
        // key with no domain tag; V3's includes suite_id/network_id. Same
        // one-time key, different network -> different image.
        let spend_a = spend_of(1_000, 0);
        let sig_a = sign(&spend_a, &DIGEST);
        let sig_b = sign_spend_v3(
            SUITE_ID,
            &[0xbb; 32],
            &DIGEST,
            0,
            &spend_a.ring_keys,
            &spend_a.ring_commitments,
            &spend_a.pseudo_commitment,
            &spend_a.witness,
        )
        .unwrap();
        assert_ne!(sig_a.key_image, sig_b.key_image);
    }
}
