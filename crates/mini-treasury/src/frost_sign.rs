//! FROST (Flexible Round-Optimized Schnorr Threshold signatures, Komlo &
//! Goldberg) signing: two rounds that let any `threshold`-sized subset of
//! [`crate::frost_keygen`]'s participants jointly produce one ordinary
//! Schnorr signature under the group public key — without ever
//! reconstructing the group secret key at any single point, on any single
//! device, at any time.
//!
//! ## Why two rounds, and why a *binding factor*
//!
//! Round 1: every participant who might sign publishes a pair of nonce
//! commitments `(D_i, E_i) = (d_i*G, e_i*G)` for fresh random `d_i, e_i` —
//! before anyone knows which message will be signed. Round 2: once the
//! message and the final signing set are fixed, each participant computes
//! their response using both nonces, weighted by a *binding factor*
//! `rho_i = H(i, message, all commitments)`. The binding factor is what
//! stops a subtle attack on naive two-round Schnorr aggregation (Drijvers
//! et al.): without it, a coalition of signers can adaptively choose their
//! own nonces after seeing everyone else's, and forge a signature over a
//! different message than any honest signer agreed to. Binding every
//! signer's contribution to the *entire* commitment list and the message
//! closes that gap.
//!
//! ## The two identities this module's correctness rests on
//!
//! Both were hand-derived and checked term-by-term before writing this
//! code, the same discipline `mini_value::bp_range` used for Bulletproofs.
//!
//! **Individual share verification** — for signer `i` with Lagrange
//! coefficient `lambda_i` (see [`lagrange_coefficient`]) and per-signer
//! group-commitment contribution `R_i = D_i + rho_i*E_i`:
//!
//! ```text
//! z_i = d_i + e_i*rho_i + lambda_i*s_i*c
//! z_i*G = d_i*G + rho_i*e_i*G + lambda_i*c*s_i*G
//!       = D_i + rho_i*E_i + c*lambda_i*Y_i
//!       = R_i + c*lambda_i*Y_i
//! ```
//!
//! **Aggregate signature validity** — summing every signer's `z_i` and
//! `R_i`, and using Shamir reconstruction-in-the-exponent
//! (`sum_i lambda_i*s_i = f(0) = s`, the same identity
//! `frost_keygen`'s tests check directly):
//!
//! ```text
//! z = sum_i z_i = sum_i d_i + sum_i(e_i*rho_i) + c * sum_i(lambda_i*s_i)
//!   = sum_i d_i + sum_i(e_i*rho_i) + c*s
//! R = sum_i R_i = sum_i D_i + sum_i(rho_i*E_i) = (sum_i d_i + sum_i e_i*rho_i)*G
//! z*G = R + c*s*G = R + c*Y
//! ```
//!
//! — exactly the ordinary single-key Schnorr verification equation
//! (`z*G == R + c*Y`), which is why the *output* of FROST is an entirely
//! ordinary Schnorr signature: anyone verifying it later needs no idea
//! FROST, or a threshold scheme, or multiple signers, were ever involved.

use std::collections::BTreeMap;

use curve25519_dalek::traits::Identity;

use crate::curve::{
    basepoint, hash_to_scalar, random_scalar, CompressedRistretto, RistrettoPoint, Scalar,
};
use crate::error::{Result, TreasuryError};
use crate::frost_keygen::{KeyPackage, PublicKeyPackage};

/// A participant's private round-1 nonces (`d_i`, `e_i`). Held only by that
/// participant, between round 1 and round 2 — never transmitted, never
/// reused across a second signature (reusing them leaks the secret share,
/// the same catastrophic failure mode as nonce reuse in plain Schnorr/
/// ECDSA). This prototype does not zeroize them on drop; a production
/// implementation should.
#[derive(Debug, Clone, Copy)]
pub struct SigningNonces {
    hiding: Scalar,
    binding: Scalar,
}

/// A participant's public round-1 commitment `(D_i, E_i)`, safe to publish.
#[derive(Debug, Clone, Copy)]
pub struct NonceCommitment {
    /// Which participant this commitment belongs to.
    pub index: u16,
    hiding: RistrettoPoint,
    binding: RistrettoPoint,
}

/// Round 1: generate a fresh nonce pair and its public commitment for
/// `index`. Must be called again for every new signature — see
/// [`SigningNonces`]'s honest limit on reuse.
pub fn round1_commit(index: u16) -> Result<(SigningNonces, NonceCommitment)> {
    let hiding = random_scalar()?;
    let binding = random_scalar()?;
    let commitment = NonceCommitment {
        index,
        hiding: basepoint() * hiding,
        binding: basepoint() * binding,
    };
    Ok((SigningNonces { hiding, binding }, commitment))
}

/// The coordinator-assembled bundle every round-2 signer needs: the
/// message being signed, and every participating signer's round-1
/// commitment. Constructing one enforces that at least `threshold`
/// distinct signers are present — signing with fewer is rejected here,
/// not discovered later as an unverifiable aggregate signature.
#[derive(Debug, Clone)]
pub struct SigningPackage {
    message: Vec<u8>,
    commitments: BTreeMap<u16, NonceCommitment>,
}

impl SigningPackage {
    /// Bundle `message` with `commitments` (one round-1 commitment per
    /// participating signer). Rejects duplicate indices and a signing set
    /// smaller than `threshold`.
    pub fn new(
        threshold: u16,
        message: Vec<u8>,
        commitments: Vec<NonceCommitment>,
    ) -> Result<Self> {
        if commitments.len() < threshold as usize {
            return Err(TreasuryError::NotEnoughSigners);
        }
        let mut map = BTreeMap::new();
        for commitment in commitments {
            if map.insert(commitment.index, commitment).is_some() {
                return Err(TreasuryError::InvalidFrostParticipant);
            }
        }
        Ok(SigningPackage {
            message,
            commitments: map,
        })
    }

    fn indices(&self) -> Vec<Scalar> {
        self.commitments.keys().map(|&i| index_scalar(i)).collect()
    }

    /// Every binding factor `rho_j = H(j, message, all commitments)`, one
    /// per participating signer, keyed by index.
    fn binding_factors(&self) -> BTreeMap<u16, Scalar> {
        // Bind to the whole sorted commitment list so no signer can change
        // their own or anyone else's contribution after the fact.
        let mut transcript = Vec::new();
        for commitment in self.commitments.values() {
            transcript.extend_from_slice(&commitment.index.to_be_bytes());
            transcript.extend_from_slice(commitment.hiding.compress().as_bytes());
            transcript.extend_from_slice(commitment.binding.compress().as_bytes());
        }

        self.commitments
            .keys()
            .map(|&j| {
                let rho_j = hash_to_scalar(&[
                    b"mini-treasury/frost/binding-factor",
                    &j.to_be_bytes(),
                    &self.message,
                    &transcript,
                ]);
                (j, rho_j)
            })
            .collect()
    }

    /// The group commitment `R = sum_i (D_i + rho_i*E_i)`.
    fn group_commitment(&self, binding_factors: &BTreeMap<u16, Scalar>) -> RistrettoPoint {
        let mut r = RistrettoPoint::identity();
        for commitment in self.commitments.values() {
            let rho = binding_factors[&commitment.index];
            r += commitment.hiding + commitment.binding * rho;
        }
        r
    }

    /// Signer `index`'s own contribution `R_i = D_i + rho_i*E_i` to the
    /// group commitment.
    fn per_signer_commitment(
        &self,
        index: u16,
        binding_factors: &BTreeMap<u16, Scalar>,
    ) -> RistrettoPoint {
        let commitment = &self.commitments[&index];
        let rho = binding_factors[&index];
        commitment.hiding + commitment.binding * rho
    }
}

/// The Schnorr challenge `c = H(R, Y, message)`.
fn challenge(
    group_commitment: RistrettoPoint,
    group_public_key: RistrettoPoint,
    message: &[u8],
) -> Scalar {
    hash_to_scalar(&[
        b"mini-treasury/frost/challenge",
        group_commitment.compress().as_bytes(),
        group_public_key.compress().as_bytes(),
        message,
    ])
}

/// This signer's Shamir/Lagrange coefficient for reconstruction at `x=0`,
/// given the full set of participating indices: `lambda_i = prod_{j != i}
/// x_j / (x_j - x_i)`. Every participant in a signing round computes the
/// *same* value for the *same* signing set — it depends only on which
/// indices are signing, not on any secret.
pub(crate) fn lagrange_coefficient(index: Scalar, all_indices: &[Scalar]) -> Scalar {
    let mut numerator = Scalar::ONE;
    let mut denominator = Scalar::ONE;
    for &j in all_indices {
        if j == index {
            continue;
        }
        numerator *= j;
        denominator *= j - index;
    }
    numerator * denominator.invert()
}

fn index_scalar(index: u16) -> Scalar {
    Scalar::from(index as u64)
}

/// Round 2: compute this signer's response `z_i` to `signing_package`,
/// using the nonces generated for it in round 1. Calling this twice with
/// the same `nonces` for two different signing packages doubly-spends the
/// nonce and leaks the secret share — round 1 must be re-run per signature.
pub fn round2_sign(
    key_package: &KeyPackage,
    nonces: &SigningNonces,
    signing_package: &SigningPackage,
) -> Result<Scalar> {
    if !signing_package.commitments.contains_key(&key_package.index) {
        return Err(TreasuryError::InvalidFrostParticipant);
    }
    let indices = signing_package.indices();
    let binding_factors = signing_package.binding_factors();
    let r = signing_package.group_commitment(&binding_factors);
    let c = challenge(r, key_package.group_public_key, &signing_package.message);
    let rho_i = binding_factors[&key_package.index];
    let lambda_i = lagrange_coefficient(index_scalar(key_package.index), &indices);

    Ok(nonces.hiding + nonces.binding * rho_i + lambda_i * key_package.secret_share * c)
}

/// Verify signer `index`'s share `z_i` against their public verification
/// share, *before* aggregating — catches a faulty or malicious signer
/// immediately, with attribution, instead of only learning the final
/// aggregate signature doesn't verify.
pub fn verify_signature_share(
    index: u16,
    z_i: Scalar,
    signing_package: &SigningPackage,
    public_key_package: &PublicKeyPackage,
) -> Result<bool> {
    let Some(&y_i) = public_key_package.verifying_shares.get(&index) else {
        return Err(TreasuryError::InvalidFrostParticipant);
    };
    let indices = signing_package.indices();
    let binding_factors = signing_package.binding_factors();
    let r = signing_package.group_commitment(&binding_factors);
    let c = challenge(
        r,
        public_key_package.group_public_key,
        &signing_package.message,
    );
    let lambda_i = lagrange_coefficient(index_scalar(index), &indices);
    let r_i = signing_package.per_signer_commitment(index, &binding_factors);

    Ok((basepoint() * z_i).compress() == (r_i + (c * lambda_i) * y_i).compress())
}

/// An ordinary Schnorr signature `(R, z)` — the output of FROST looks
/// exactly like a signature from a single key, by design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signature {
    r: CompressedRistretto,
    z: Scalar,
}

impl Signature {
    /// Serialize to the 64-byte wire format (`R || z`, both 32 bytes).
    pub fn to_bytes(self) -> [u8; 64] {
        let mut out = [0u8; 64];
        out[..32].copy_from_slice(self.r.as_bytes());
        out[32..].copy_from_slice(self.z.as_bytes());
        out
    }

    /// Deserialize from the 64-byte wire format. `None` if malformed (wrong
    /// length, or the first 32 bytes are not a valid compressed Ristretto
    /// point). The scalar half is reduced mod the group order rather than
    /// canonical-checked, the same choice this workspace already makes for
    /// scalar decoding elsewhere (`mini_value::confidential_impl`).
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let r_bytes: [u8; 32] = bytes.get(..32)?.try_into().ok()?;
        let z_bytes: [u8; 32] = bytes.get(32..64)?.try_into().ok()?;
        if bytes.len() != 64 {
            return None;
        }
        // Confirm it decompresses to a real point now, so a malformed
        // signature is rejected here rather than surfacing later as a
        // confusing verification failure.
        CompressedRistretto(r_bytes).decompress()?;
        let z = Scalar::from_bytes_mod_order(z_bytes);
        Some(Signature {
            r: CompressedRistretto(r_bytes),
            z,
        })
    }
}

/// Combine per-signer shares into the final signature. Every share is
/// verified individually first (see [`verify_signature_share`]) so a bad
/// share is caught and attributed rather than silently producing an
/// aggregate that fails to verify.
pub fn aggregate(
    signing_package: &SigningPackage,
    shares: &BTreeMap<u16, Scalar>,
    public_key_package: &PublicKeyPackage,
) -> Result<Signature> {
    if shares.len() != signing_package.commitments.len() {
        return Err(TreasuryError::NotEnoughSigners);
    }
    let mut z = Scalar::ZERO;
    for (&index, &z_i) in shares {
        if !verify_signature_share(index, z_i, signing_package, public_key_package)? {
            return Err(TreasuryError::InvalidFrostSignatureShare);
        }
        z += z_i;
    }

    let binding_factors = signing_package.binding_factors();
    let r = signing_package.group_commitment(&binding_factors);
    Ok(Signature { r: r.compress(), z })
}

/// Verify a completed FROST signature exactly as any ordinary Schnorr
/// verifier would, with no knowledge that a threshold scheme was involved:
/// `z*G == R + c*Y`.
pub fn verify(signature: &Signature, message: &[u8], group_public_key: RistrettoPoint) -> bool {
    let Some(r) = signature.r.decompress() else {
        return false;
    };
    let c = challenge(r, group_public_key, message);
    (basepoint() * signature.z).compress() == (r + c * group_public_key).compress()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frost_keygen::trusted_dealer_keygen;

    fn sign_with(
        signer_indices: &[u16],
        shares: &[KeyPackage],
        public: &PublicKeyPackage,
        threshold: u16,
        message: &[u8],
    ) -> Signature {
        let mut nonces_by_index = BTreeMap::new();
        let mut commitments = Vec::new();
        for &i in signer_indices {
            let (nonces, commitment) = round1_commit(i).unwrap();
            nonces_by_index.insert(i, nonces);
            commitments.push(commitment);
        }
        let signing_package =
            SigningPackage::new(threshold, message.to_vec(), commitments).unwrap();

        let mut z_shares = BTreeMap::new();
        for &i in signer_indices {
            let key_package = shares.iter().find(|s| s.index == i).unwrap();
            let z_i = round2_sign(key_package, &nonces_by_index[&i], &signing_package).unwrap();
            z_shares.insert(i, z_i);
        }

        aggregate(&signing_package, &z_shares, public).unwrap()
    }

    #[test]
    fn a_threshold_sized_subset_produces_a_valid_signature() {
        let (shares, public) = trusted_dealer_keygen(5, 3).unwrap();
        let message = b"send 10 BTC-equivalent MINI to treasury payout #42";
        let signature = sign_with(&[1, 2, 3], &shares, &public, 3, message);
        assert!(verify(&signature, message, public.group_public_key));
    }

    #[test]
    fn a_different_threshold_sized_subset_also_produces_a_valid_signature() {
        let (shares, public) = trusted_dealer_keygen(5, 3).unwrap();
        let message = b"treasury payout #43";
        let signature = sign_with(&[2, 4, 5], &shares, &public, 3, message);
        assert!(verify(&signature, message, public.group_public_key));
    }

    #[test]
    fn fewer_than_threshold_signers_are_rejected_at_package_construction() {
        let (_, _public) = trusted_dealer_keygen(5, 3).unwrap();
        let (_, c1) = round1_commit(1).unwrap();
        let (_, c2) = round1_commit(2).unwrap();
        let err = SigningPackage::new(3, b"msg".to_vec(), vec![c1, c2]).unwrap_err();
        assert_eq!(err, TreasuryError::NotEnoughSigners);
    }

    #[test]
    fn duplicate_signer_index_is_rejected() {
        let (_, c1) = round1_commit(1).unwrap();
        let (_, c2) = round1_commit(1).unwrap();
        let err = SigningPackage::new(2, b"msg".to_vec(), vec![c1, c2]).unwrap_err();
        assert_eq!(err, TreasuryError::InvalidFrostParticipant);
    }

    #[test]
    fn a_tampered_signature_share_is_caught_before_aggregation() {
        let (shares, public) = trusted_dealer_keygen(5, 3).unwrap();
        let message = b"treasury payout #44";
        let signer_indices = [1, 2, 3];

        let mut nonces_by_index = BTreeMap::new();
        let mut commitments = Vec::new();
        for &i in &signer_indices {
            let (nonces, commitment) = round1_commit(i).unwrap();
            nonces_by_index.insert(i, nonces);
            commitments.push(commitment);
        }
        let signing_package = SigningPackage::new(3, message.to_vec(), commitments).unwrap();

        let mut z_shares = BTreeMap::new();
        for &i in &signer_indices {
            let key_package = shares.iter().find(|s| s.index == i).unwrap();
            let z_i = round2_sign(key_package, &nonces_by_index[&i], &signing_package).unwrap();
            z_shares.insert(i, z_i);
        }
        // Tamper with one signer's share.
        *z_shares.get_mut(&2).unwrap() += Scalar::ONE;

        let err = aggregate(&signing_package, &z_shares, &public).unwrap_err();
        assert_eq!(err, TreasuryError::InvalidFrostSignatureShare);
    }

    #[test]
    fn signature_fails_verification_under_a_different_message() {
        let (shares, public) = trusted_dealer_keygen(5, 3).unwrap();
        let message = b"treasury payout #45";
        let signature = sign_with(&[1, 2, 3], &shares, &public, 3, message);
        assert!(!verify(
            &signature,
            b"treasury payout #46 (attacker-modified)",
            public.group_public_key
        ));
    }

    #[test]
    fn signature_fails_verification_under_a_different_group_key() {
        let (shares, public_a) = trusted_dealer_keygen(5, 3).unwrap();
        let (_, public_b) = trusted_dealer_keygen(5, 3).unwrap();
        let message = b"treasury payout #47";
        let signature = sign_with(&[1, 2, 3], &shares, &public_a, 3, message);
        assert!(!verify(&signature, message, public_b.group_public_key));
    }

    #[test]
    fn wrong_length_signature_bytes_are_rejected_without_panicking() {
        assert!(Signature::from_bytes(&[0u8; 10]).is_none());
        assert!(Signature::from_bytes(&[0u8; 63]).is_none());
        assert!(Signature::from_bytes(&[0u8; 65]).is_none());
    }

    #[test]
    fn an_invalid_curve_point_in_signature_bytes_is_rejected_without_panicking() {
        // 0xFF repeated is not a valid compressed Ristretto encoding (it is
        // not the canonical little-endian encoding of any coset
        // representative), so decompression must fail rather than the
        // decoder silently accepting garbage as a point.
        let bytes = [0xFFu8; 64];
        assert!(Signature::from_bytes(&bytes).is_none());
    }

    #[test]
    fn signature_round_trips_through_bytes() {
        let (shares, public) = trusted_dealer_keygen(5, 3).unwrap();
        let message = b"treasury payout #48";
        let signature = sign_with(&[1, 2, 3], &shares, &public, 3, message);
        let bytes = signature.to_bytes();
        let decoded = Signature::from_bytes(&bytes).unwrap();
        assert!(verify(&decoded, message, public.group_public_key));
    }
}
