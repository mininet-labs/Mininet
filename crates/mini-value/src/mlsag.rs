//! The spend proof that makes value conservation possible: a two-column
//! MLSAG over `(one-time key, commitment difference)` pairs.
//!
//! # Why a second column exists at all
//!
//! [`crate::ring_impl`]'s single-column ring signature proves *somebody in
//! this ring authorized this message*. That is enough to hide a payer, and
//! it is **not** enough to hide an amount, because it says nothing about
//! how much the spent output was worth. A payment built on it alone can
//! commit to any amount it likes: nothing ties the new commitment to the
//! old one, so value can be created from nothing.
//!
//! Conservation needs a public equation. Every spendable output carries a
//! Pedersen commitment `C = b·G_blind + v·H_val`, and the additively
//! homomorphic property means a verifier can check
//! `Σ inputs = Σ outputs + fee` by summing points — see
//! [`crate::confidential::ConfidentialAmountScheme::verify_balance`]. But
//! the *inputs* to that sum cannot be the real ring members' commitments,
//! because publishing which commitments were spent identifies which ring
//! member was real, and the ring stops hiding anything.
//!
//! The standard resolution, and the one used here: the spender publishes a
//! **pseudo-commitment** `C'` to the same value under a fresh blinding
//! factor, puts `C'` in the balance equation, and proves in zero knowledge
//! that `C'` commits to the same value as one of the ring's commitments —
//! without saying which. That proof is this module.
//!
//! Since both commit to the same `v`, their difference cancels the value
//! term entirely:
//!
//! ```text
//! C_real − C' = (b_real − b')·G_blind + (v − v)·H_val
//!             = (b_real − b')·G_blind
//! ```
//!
//! which is a public key on `G_blind` whose private key the spender knows.
//! So "this pseudo-commitment matches one of these commitments" becomes
//! "I know the discrete log of one of these differences" — a ring
//! signature again, on a second column, chained to the same challenge as
//! the first so a signer cannot mix and match rows.
//!
//! # This is not new cryptography
//!
//! MLSAG is published (Noether–Mackenzie, *Ring Confidential Transactions*)
//! and has secured a live network for years. Implementing it in-house
//! rather than depending on another project's code follows D-0063 and the
//! same reasoning as Bulletproofs (D-0036/D-0040) and SDR proof-of-
//! replication (D-0064): compose and implement prior art the wider field
//! has already analyzed; never invent a construction nobody outside this
//! repository has looked at.
//!
//! # Why the commitment column has no key image
//!
//! Column 0 produces a key image `I = x·Hp(P)`, which is what makes double
//! spending detectable — the same output always yields the same image.
//! Column 1 deliberately produces none, and that omission is load-bearing:
//! a key image on the blinding column would be deterministic in
//! `b_real − b'`, and every spend of outputs sharing a blinding
//! relationship would link. The column proves knowledge, not uniqueness;
//! uniqueness is column 0's job and only column 0's.
//!
//! # Honest limits [D-0036/D-0037/D-0047]
//!
//! Founder-overridden, AI-authored prototype. Unaudited. A flaw here does
//! not fail loudly — it produces payments that look conserved and are not,
//! or rings that look anonymous and are not. Nothing value-bearing may
//! depend on this before the external review #72 gates.
//!
//! This module proves *one* input against *one* ring. Summing the pseudo-
//! commitments and checking them against outputs and fee is the caller's
//! job, and a caller that verifies every MLSAG and forgets the balance
//! check has verified nothing about conservation.

use zeroize::Zeroize;

use crate::bp_generators::blinding_generator;
use crate::curve::{basepoint, hash_to_point, hash_to_scalar, CompressedRistretto, Scalar};
use crate::curve::{random_scalar, RistrettoPoint};

/// Domain separator for this scheme's Fiat-Shamir challenges. Distinct
/// from [`crate::ring_impl`]'s so a signature over one scheme's transcript
/// can never be replayed as the other's.
pub const MLSAG_DOMAIN: &[u8] = b"mininet/mini-value/mlsag/v1";

/// A two-column MLSAG: proof that the signer controls one ring member's
/// one-time key **and** that the accompanying pseudo-commitment hides the
/// same value as that member's commitment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MlsagSignature {
    /// The Fiat-Shamir challenge chain's anchor.
    pub challenge: Vec<u8>,
    /// Column 0 responses — one per ring member, ring order.
    pub key_responses: Vec<Vec<u8>>,
    /// Column 1 responses — one per ring member, ring order.
    pub blinding_responses: Vec<Vec<u8>>,
    /// The double-spend nullifier, from column 0 only.
    pub key_image: Vec<u8>,
}

/// The secrets that authorize one spend.
///
/// Debug-redacted and zeroized on drop: a leaked `one_time_secret` spends
/// the output, and a leaked `blinding_difference` reveals which ring member
/// was real, which is the whole property the ring exists to protect.
#[derive(Clone)]
pub struct SpendWitness {
    /// Which ring position the signer actually controls.
    pub secret_index: usize,
    /// `x`, the one-time private key for `ring_keys[secret_index]`.
    pub one_time_secret: [u8; 32],
    /// `b_real − b_pseudo`: the discrete log, on `G_blind`, of
    /// `ring_commitments[secret_index] − pseudo_commitment`.
    pub blinding_difference: [u8; 32],
}

impl core::fmt::Debug for SpendWitness {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SpendWitness")
            .field("secret_index", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl Drop for SpendWitness {
    fn drop(&mut self) {
        self.one_time_secret.zeroize();
        self.blinding_difference.zeroize();
    }
}

fn decompress_point(bytes: &[u8]) -> Option<RistrettoPoint> {
    let arr: [u8; 32] = bytes.try_into().ok()?;
    CompressedRistretto(arr).decompress()
}

fn decompress_scalar(bytes: &[u8]) -> Option<Scalar> {
    let arr: [u8; 32] = bytes.try_into().ok()?;
    Some(Scalar::from_bytes_mod_order(arr))
}

/// One link of the challenge chain. Both columns' commitments enter the
/// same hash, which is what binds them: a signer cannot satisfy column 0
/// at one ring position and column 1 at another.
fn challenge_hash(
    message: &[u8],
    key_l: &RistrettoPoint,
    key_r: &RistrettoPoint,
    blinding_l: &RistrettoPoint,
    key_image_bytes: &[u8],
) -> Scalar {
    hash_to_scalar(&[
        MLSAG_DOMAIN,
        message,
        key_l.compress().as_bytes(),
        key_r.compress().as_bytes(),
        blinding_l.compress().as_bytes(),
        key_image_bytes,
    ])
}

/// The commitment column's ring: `C_j − C'` for each member.
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

/// Sign one spend.
///
/// `ring_keys` and `ring_commitments` are parallel: member `j` is the
/// output whose one-time address is `ring_keys[j]` and whose amount
/// commitment is `ring_commitments[j]`. `pseudo_commitment` is the
/// spender's re-blinded commitment to the same value as the real member's.
///
/// Returns `None` rather than a bad signature on any malformed input: an
/// empty or mismatched ring, an out-of-range index, an undecodable point,
/// or a witness whose `blinding_difference` does not actually open
/// `ring_commitments[secret_index] − pseudo_commitment`. That last check
/// is deliberate — a silent mismatch would produce a signature that
/// verifies nowhere, and the failure would surface as an unexplained
/// rejection far from its cause.
pub fn sign_spend(
    ring_keys: &[Vec<u8>],
    ring_commitments: &[Vec<u8>],
    pseudo_commitment: &[u8],
    message: &[u8],
    witness: &SpendWitness,
) -> Option<MlsagSignature> {
    let n = ring_keys.len();
    if n == 0 || ring_commitments.len() != n || witness.secret_index >= n {
        return None;
    }
    let keys: Vec<RistrettoPoint> = ring_keys
        .iter()
        .map(|k| decompress_point(k))
        .collect::<Option<_>>()?;
    let differences = difference_ring(ring_commitments, pseudo_commitment)?;

    let pi = witness.secret_index;
    let x = decompress_scalar(&witness.one_time_secret)?;
    let z = decompress_scalar(&witness.blinding_difference)?;

    // Refuse to sign a witness that does not open what it claims to.
    if x * basepoint() != keys[pi] || z * blinding_generator() != differences[pi] {
        return None;
    }

    let image_base = hash_to_point(&[keys[pi].compress().as_bytes()]);
    let key_image = x * image_base;
    let key_image_bytes = key_image.compress().to_bytes();

    let mut c = vec![Scalar::ZERO; n];
    let mut s_key = vec![Scalar::ZERO; n];
    let mut s_blind = vec![Scalar::ZERO; n];

    let alpha_key = random_scalar().ok()?;
    let alpha_blind = random_scalar().ok()?;
    c[(pi + 1) % n] = challenge_hash(
        message,
        &(alpha_key * basepoint()),
        &(alpha_key * image_base),
        &(alpha_blind * blinding_generator()),
        &key_image_bytes,
    );

    let mut j = (pi + 1) % n;
    while j != pi {
        s_key[j] = random_scalar().ok()?;
        s_blind[j] = random_scalar().ok()?;
        let hp_j = hash_to_point(&[keys[j].compress().as_bytes()]);
        let key_l = s_key[j] * basepoint() + c[j] * keys[j];
        let key_r = s_key[j] * hp_j + c[j] * key_image;
        let blinding_l = s_blind[j] * blinding_generator() + c[j] * differences[j];
        let next = (j + 1) % n;
        c[next] = challenge_hash(message, &key_l, &key_r, &blinding_l, &key_image_bytes);
        j = next;
    }

    s_key[pi] = alpha_key - c[pi] * x;
    s_blind[pi] = alpha_blind - c[pi] * z;

    Some(MlsagSignature {
        challenge: c[0].to_bytes().to_vec(),
        key_responses: s_key.iter().map(|v| v.to_bytes().to_vec()).collect(),
        blinding_responses: s_blind.iter().map(|v| v.to_bytes().to_vec()).collect(),
        key_image: key_image_bytes.to_vec(),
    })
}

/// Verify one spend proof.
///
/// A `true` result means: some member of `ring_keys` authorized `message`,
/// and `pseudo_commitment` hides the same value as that same member's
/// commitment. It says **nothing** about whether the pseudo-commitments
/// balance the outputs — that is a separate, public sum the caller must
/// also check, and skipping it leaves value conservation unproven.
pub fn verify_spend(
    ring_keys: &[Vec<u8>],
    ring_commitments: &[Vec<u8>],
    pseudo_commitment: &[u8],
    message: &[u8],
    signature: &MlsagSignature,
) -> bool {
    let n = ring_keys.len();
    if n == 0
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

    let mut c = c0;
    for j in 0..n {
        let hp_j = hash_to_point(&[keys[j].compress().as_bytes()]);
        let key_l = s_key[j] * basepoint() + c * keys[j];
        let key_r = s_key[j] * hp_j + c * key_image;
        let blinding_l = s_blind[j] * blinding_generator() + c * differences[j];
        c = challenge_hash(message, &key_l, &key_r, &blinding_l, &signature.key_image);
    }
    c == c0
}

/// Re-blind a commitment: given the real output's value and blinding, and
/// a fresh blinding, produce the pseudo-commitment and the difference the
/// witness needs.
///
/// Returns `(pseudo_commitment, blinding_difference)`. The difference is
/// `b_real − b_pseudo`, which is exactly [`SpendWitness::blinding_difference`].
pub fn reblind(
    value: u64,
    real_blinding: &[u8],
    pseudo_blinding: &[u8],
) -> Option<([u8; 32], [u8; 32])> {
    let b_real = decompress_scalar(real_blinding)?;
    let b_pseudo = decompress_scalar(pseudo_blinding)?;
    let pseudo = b_pseudo * blinding_generator()
        + Scalar::from(value) * crate::bp_generators::value_generator();
    Some((pseudo.compress().to_bytes(), (b_real - b_pseudo).to_bytes()))
}

/// The blinding factor the last pseudo-commitment must use for a
/// transaction to balance.
///
/// Value conservation is checked as a sum of curve points, and that sum
/// only cancels when the blinding factors cancel too. So a spender may
/// choose freely for every input but one; the last is forced to
/// `Σ b_out − Σ b'_chosen`. Given that, the summed pseudo-commitments equal
/// the summed outputs plus fee **exactly when the amounts conserve** — the
/// blinding terms drop out, leaving the value terms to agree or not.
///
/// Returning it as a named function rather than exposing scalar arithmetic
/// keeps this one constraint in a place where it can be explained and
/// tested, instead of open-coded at each call site where getting the sign
/// backwards would produce a claim nothing accepts.
pub fn balancing_blinding(
    output_blindings: &[[u8; 32]],
    chosen_pseudo_blindings: &[[u8; 32]],
) -> [u8; 32] {
    let sum = |values: &[[u8; 32]]| {
        values.iter().fold(Scalar::ZERO, |acc, bytes| {
            acc + Scalar::from_bytes_mod_order(*bytes)
        })
    };
    (sum(output_blindings) - sum(chosen_pseudo_blindings)).to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bp_generators::value_generator;

    fn commitment(value: u64, blinding: Scalar) -> Vec<u8> {
        (blinding * blinding_generator() + Scalar::from(value) * value_generator())
            .compress()
            .to_bytes()
            .to_vec()
    }

    struct Spend {
        ring_keys: Vec<Vec<u8>>,
        ring_commitments: Vec<Vec<u8>>,
        pseudo: Vec<u8>,
        witness: SpendWitness,
    }

    /// A ring of `n` members with a real, spendable output at `pi`.
    fn spend_of(value: u64, n: usize, pi: usize) -> Spend {
        let mut ring_keys = Vec::new();
        let mut ring_commitments = Vec::new();
        let mut real_secret = Scalar::ZERO;
        let mut real_blinding = Scalar::ZERO;
        for j in 0..n {
            let secret = random_scalar().unwrap();
            let blinding = random_scalar().unwrap();
            // Decoys hold arbitrary other values; only the real one matters.
            let member_value = if j == pi { value } else { value + 7 + j as u64 };
            if j == pi {
                real_secret = secret;
                real_blinding = blinding;
            }
            ring_keys.push((secret * basepoint()).compress().to_bytes().to_vec());
            ring_commitments.push(commitment(member_value, blinding));
        }
        let pseudo_blinding = random_scalar().unwrap();
        let (pseudo, difference) = reblind(
            value,
            &real_blinding.to_bytes(),
            &pseudo_blinding.to_bytes(),
        )
        .unwrap();
        Spend {
            ring_keys,
            ring_commitments,
            pseudo: pseudo.to_vec(),
            witness: SpendWitness {
                secret_index: pi,
                one_time_secret: real_secret.to_bytes(),
                blinding_difference: difference,
            },
        }
    }

    fn sign(spend: &Spend, message: &[u8]) -> MlsagSignature {
        sign_spend(
            &spend.ring_keys,
            &spend.ring_commitments,
            &spend.pseudo,
            message,
            &spend.witness,
        )
        .unwrap()
    }

    fn verify(spend: &Spend, message: &[u8], signature: &MlsagSignature) -> bool {
        verify_spend(
            &spend.ring_keys,
            &spend.ring_commitments,
            &spend.pseudo,
            message,
            signature,
        )
    }

    #[test]
    fn a_valid_spend_verifies() {
        let spend = spend_of(1_000, 8, 3);
        let signature = sign(&spend, b"transcript");
        assert!(verify(&spend, b"transcript", &signature));
    }

    #[test]
    fn it_verifies_from_every_ring_position() {
        // If verification depended on where the real member sat, the ring
        // would leak the signer's index through nothing more than which
        // signatures happen to pass.
        for pi in 0..6 {
            let spend = spend_of(500, 6, pi);
            let signature = sign(&spend, b"transcript");
            assert!(verify(&spend, b"transcript", &signature), "position {pi}");
        }
    }

    #[test]
    fn a_tampered_message_fails() {
        let spend = spend_of(1_000, 8, 2);
        let signature = sign(&spend, b"transcript");
        assert!(!verify(&spend, b"different transcript", &signature));
    }

    #[test]
    fn a_pseudo_commitment_to_a_different_value_cannot_be_signed() {
        // The heart of conservation: re-blinding is allowed, re-valuing is
        // not. A spender who inflates the pseudo-commitment no longer knows
        // the discrete log of the difference, so no signature exists.
        let value = 1_000;
        let spend = spend_of(value, 8, 4);
        let inflated_blinding = random_scalar().unwrap();
        let (inflated, difference) = reblind(
            value + 1_000_000,
            &inflated_blinding.to_bytes(),
            &inflated_blinding.to_bytes(),
        )
        .unwrap();
        let witness = SpendWitness {
            secret_index: spend.witness.secret_index,
            one_time_secret: spend.witness.one_time_secret,
            blinding_difference: difference,
        };
        assert_eq!(
            sign_spend(
                &spend.ring_keys,
                &spend.ring_commitments,
                &inflated,
                b"transcript",
                &witness,
            ),
            None,
            "signing must refuse a witness that does not open the difference"
        );
    }

    #[test]
    fn a_swapped_pseudo_commitment_fails_verification() {
        // Even a well-formed signature is bound to the exact pseudo-
        // commitment it was made for; substituting another breaks column 1.
        let spend = spend_of(1_000, 8, 1);
        let signature = sign(&spend, b"transcript");
        let other = spend_of(1_000, 8, 1);
        assert!(!verify_spend(
            &spend.ring_keys,
            &spend.ring_commitments,
            &other.pseudo,
            b"transcript",
            &signature,
        ));
    }

    #[test]
    fn knowing_only_the_one_time_key_is_not_enough() {
        // Column 0 alone used to be the whole signature. This asserts the
        // second column actually carries weight: a signer who controls the
        // output but cannot open the commitment difference cannot sign.
        let spend = spend_of(1_000, 8, 5);
        let witness = SpendWitness {
            secret_index: spend.witness.secret_index,
            one_time_secret: spend.witness.one_time_secret,
            blinding_difference: random_scalar().unwrap().to_bytes(),
        };
        assert_eq!(
            sign_spend(
                &spend.ring_keys,
                &spend.ring_commitments,
                &spend.pseudo,
                b"transcript",
                &witness,
            ),
            None
        );
    }

    #[test]
    fn knowing_only_the_blinding_difference_is_not_enough() {
        let spend = spend_of(1_000, 8, 5);
        let witness = SpendWitness {
            secret_index: spend.witness.secret_index,
            one_time_secret: random_scalar().unwrap().to_bytes(),
            blinding_difference: spend.witness.blinding_difference,
        };
        assert_eq!(
            sign_spend(
                &spend.ring_keys,
                &spend.ring_commitments,
                &spend.pseudo,
                b"transcript",
                &witness,
            ),
            None
        );
    }

    #[test]
    fn the_key_image_is_deterministic_in_the_spent_output() {
        // Double-spend detection depends on this exactly: the same output
        // must always yield the same image, whatever ring or pseudo-
        // commitment surrounds it.
        let spend = spend_of(1_000, 8, 0);
        let first = sign(&spend, b"one");
        let second = sign(&spend, b"two");
        assert_eq!(first.key_image, second.key_image);
    }

    #[test]
    fn different_outputs_produce_different_key_images() {
        let a = spend_of(1_000, 8, 0);
        let b = spend_of(1_000, 8, 0);
        assert_ne!(sign(&a, b"m").key_image, sign(&b, b"m").key_image);
    }

    #[test]
    fn a_substituted_key_image_fails() {
        let spend = spend_of(1_000, 8, 2);
        let mut signature = sign(&spend, b"transcript");
        signature.key_image = sign(&spend_of(1_000, 8, 2), b"transcript").key_image;
        assert!(!verify(&spend, b"transcript", &signature));
    }

    #[test]
    fn a_tampered_response_fails() {
        let spend = spend_of(1_000, 8, 3);
        let mut signature = sign(&spend, b"transcript");
        signature.blinding_responses[0] = random_scalar().unwrap().to_bytes().to_vec();
        assert!(!verify(&spend, b"transcript", &signature));

        let mut signature = sign(&spend, b"transcript");
        signature.key_responses[5] = random_scalar().unwrap().to_bytes().to_vec();
        assert!(!verify(&spend, b"transcript", &signature));
    }

    #[test]
    fn malformed_input_is_rejected_without_panicking() {
        let spend = spend_of(1_000, 4, 0);
        let signature = sign(&spend, b"m");
        // Mismatched column lengths.
        assert!(!verify_spend(
            &spend.ring_keys,
            &[],
            &spend.pseudo,
            b"m",
            &signature
        ));
        // Undecodable ring member.
        assert!(!verify_spend(
            &vec![vec![0u8; 4]; 4],
            &spend.ring_commitments,
            &spend.pseudo,
            b"m",
            &signature,
        ));
        // Undecodable pseudo-commitment.
        assert!(!verify_spend(
            &spend.ring_keys,
            &spend.ring_commitments,
            b"short",
            b"m",
            &signature,
        ));
        // Empty ring, and an out-of-range secret index.
        assert_eq!(
            sign_spend(&[], &[], &spend.pseudo, b"m", &spend.witness),
            None
        );
        let out_of_range = SpendWitness {
            secret_index: 99,
            one_time_secret: spend.witness.one_time_secret,
            blinding_difference: spend.witness.blinding_difference,
        };
        assert_eq!(
            sign_spend(
                &spend.ring_keys,
                &spend.ring_commitments,
                &spend.pseudo,
                b"m",
                &out_of_range,
            ),
            None
        );
    }

    #[test]
    fn the_balancing_blinding_makes_the_sums_cancel() {
        // The property the whole balance check rests on: with the last
        // pseudo-blinding chosen this way, summed pseudo-commitments equal
        // summed outputs plus fee exactly when the amounts conserve.
        let value_a = 700u64;
        let value_b = 300u64;
        let fee = 25u64;
        let out_a = 600u64;
        let out_b = 375u64;
        assert_eq!(value_a + value_b, out_a + out_b + fee);

        let b_out = [
            random_scalar().unwrap().to_bytes(),
            random_scalar().unwrap().to_bytes(),
        ];
        let chosen = [random_scalar().unwrap().to_bytes()];
        let last = balancing_blinding(&b_out, &chosen);

        let pseudo_a = Scalar::from_bytes_mod_order(chosen[0]) * blinding_generator()
            + Scalar::from(value_a) * value_generator();
        let pseudo_b = Scalar::from_bytes_mod_order(last) * blinding_generator()
            + Scalar::from(value_b) * value_generator();
        let commit_a = Scalar::from_bytes_mod_order(b_out[0]) * blinding_generator()
            + Scalar::from(out_a) * value_generator();
        let commit_b = Scalar::from_bytes_mod_order(b_out[1]) * blinding_generator()
            + Scalar::from(out_b) * value_generator();
        let fee_point = Scalar::from(fee) * value_generator();

        assert_eq!(pseudo_a + pseudo_b, commit_a + commit_b + fee_point);
    }

    #[test]
    fn an_unbalanced_amount_does_not_cancel() {
        let b_out = [random_scalar().unwrap().to_bytes()];
        let chosen: [[u8; 32]; 0] = [];
        let last = balancing_blinding(&b_out, &chosen);
        // Spender claims to spend 1000 but only pays out 900 with no fee:
        // the blinding terms still cancel, so the mismatch shows up as a
        // pure value difference and the points differ.
        let pseudo = Scalar::from_bytes_mod_order(last) * blinding_generator()
            + Scalar::from(1000u64) * value_generator();
        let commit = Scalar::from_bytes_mod_order(b_out[0]) * blinding_generator()
            + Scalar::from(900u64) * value_generator();
        assert_ne!(pseudo, commit);
    }

    #[test]
    fn a_witness_never_prints_its_secrets() {
        let spend = spend_of(1_000, 4, 1);
        let rendered = format!("{:?}", spend.witness);
        assert!(rendered.contains("<redacted>"), "{rendered}");
        assert!(!rendered.contains("one_time_secret: ["), "{rendered}");
    }

    #[test]
    fn a_signature_over_one_scheme_does_not_verify_as_the_other() {
        // MLSAG_DOMAIN separates this transcript from ring_impl's. Without
        // it, a single-column signature and a two-column one could collide
        // on a shared challenge and one could stand in for the other.
        let spend = spend_of(1_000, 8, 2);
        let signature = sign(&spend, b"transcript");
        let single = crate::ring::RingSignature {
            challenge: signature.challenge.clone(),
            responses: signature.key_responses.clone(),
            key_image: signature.key_image.clone(),
        };
        use crate::ring::RingSignatureScheme;
        assert!(!crate::ring_impl::MininetRingSignature::verifier().verify(
            &spend.ring_keys,
            b"transcript",
            &single,
        ));
    }
}
