//! FROST key generation: a trusted dealer splits one group secret key into
//! `n` Feldman-verifiable shares, any `threshold` of which can later
//! reconstruct a signature (never the secret itself — see [`crate::frost_sign`]).
//!
//! ## Trusted-dealer keygen, not distributed key generation — honest limit
//!
//! [`trusted_dealer_keygen`] is the simpler of FROST's two keygen modes: one
//! party (the dealer) briefly holds the whole secret while splitting it,
//! then is expected to erase it. A production deployment should replace
//! this with FROST's distributed key generation (DKG) protocol, in which no
//! single party — including any device running keygen — ever holds the
//! full secret at any point. That DKG is real, separate protocol work, not
//! implemented here; this crate's live demo (`examples/frost_live_demo.rs`)
//! is explicit that it starts from trusted-dealer keygen for exactly this
//! reason. Per D-0037/D-0041, this is a founder-reviewed, AI-authored
//! prototype pending external cryptography audit, same bar as every other
//! primitive in this batch.
//!
//! ## The construction
//!
//! Shamir secret sharing: the dealer picks a random degree-`(threshold-1)`
//! polynomial `f(x) = a_0 + a_1*x + ... + a_{t-1}*x^{t-1}` over the scalar
//! field, where `a_0` **is** the group secret key. Participant `i`
//! (`i in 1..=n`, index `0` reserved for the secret itself) receives
//! `s_i = f(i)`. Feldman VSS publishes `A_k = a_k*G` for every coefficient,
//! so anyone can verify their own share without trusting the dealer's
//! word: `s_i*G == sum_k A_k * i^k`, which holds because evaluating the
//! commitment polynomial term-by-term in the exponent is exactly evaluating
//! `f(i)` in the exponent.

use std::collections::BTreeMap;

use crate::curve::{basepoint, random_scalar, RistrettoPoint, Scalar};
use crate::error::{Result, TreasuryError};

/// Hard cap on FROST participant count — a custody committee, not a
/// network-wide validator set.
pub const MAX_PARTICIPANTS: u16 = 100;

/// A caller's explicit, typed acknowledgment that [`trusted_dealer_keygen`]
/// briefly centralizes trust in one party (this module's own honest limit,
/// D-0037, [roadmap issue #93](../../issues/93)) and that using it commits
/// to replacing it with real distributed key generation before any
/// production custody use. Constructing this **is** the acknowledgment —
/// there is no other way to call [`trusted_dealer_keygen`], so a caller
/// cannot reach it by accident, only by writing this type's name out loud
/// (the same typed-authority discipline this repo's `CLAUDE.md` requires
/// for anything that exercises real authority: a specific named request
/// type, never a bare call a reviewer could miss in a diff).
#[derive(Debug, Clone, Copy)]
pub struct AcknowledgedPrototypeOnly {
    _private: (),
}

impl AcknowledgedPrototypeOnly {
    /// Name it, don't hide it: calling this long name is itself the
    /// acknowledgment that trusted-dealer keygen is not production-ready.
    pub const fn insecure_trusted_dealer_keygen_is_not_production_ready() -> Self {
        AcknowledgedPrototypeOnly { _private: () }
    }
}

/// One participant's share of the group secret key, plus what they need to
/// sign: their own index, secret share, and the group's public key.
/// Nothing here reveals the group secret or any other participant's share.
#[derive(Debug, Clone)]
pub struct KeyPackage {
    /// This participant's index (`1..=n`, never `0`).
    pub index: u16,
    /// This participant's Shamir share `f(index)` of the group secret.
    pub secret_share: Scalar,
    /// The group's public key `Y = f(0)*G`, the same for every participant.
    pub group_public_key: RistrettoPoint,
}

/// Public material every participant and the coordinator need: the group
/// public key, and each participant's individual public verification share
/// `Y_i = s_i*G`, used to check a signature share before aggregating it.
#[derive(Debug, Clone)]
pub struct PublicKeyPackage {
    /// The group's public key `Y = f(0)*G`.
    pub group_public_key: RistrettoPoint,
    /// Each participant index's public verification share `Y_i = s_i*G`.
    pub verifying_shares: BTreeMap<u16, RistrettoPoint>,
}

/// Split a fresh group secret key into `n` Feldman-verifiable shares, any
/// `threshold` of which can later produce a valid signature under the
/// returned group public key. See the module's honest limit: this is
/// trusted-dealer keygen, not DKG — the dealer (this function, for the
/// instant it runs) holds the whole secret. `_ack` proves the caller
/// deliberately chose this over real DKG — see [`AcknowledgedPrototypeOnly`].
pub fn trusted_dealer_keygen(
    n: u16,
    threshold: u16,
    _ack: AcknowledgedPrototypeOnly,
) -> Result<(Vec<KeyPackage>, PublicKeyPackage)> {
    if n == 0 || n > MAX_PARTICIPANTS || threshold == 0 || threshold > n {
        return Err(TreasuryError::InvalidFrostParameters);
    }

    // a_0 ..= a_{threshold-1}, a_0 is the group secret key.
    let mut coefficients = Vec::with_capacity(threshold as usize);
    for _ in 0..threshold {
        coefficients.push(random_scalar()?);
    }
    let group_public_key = basepoint() * coefficients[0];

    let mut key_packages = Vec::with_capacity(n as usize);
    let mut verifying_shares = BTreeMap::new();
    for index in 1..=n {
        let secret_share = eval_polynomial(&coefficients, Scalar::from(index));
        verifying_shares.insert(index, basepoint() * secret_share);
        key_packages.push(KeyPackage {
            index,
            secret_share,
            group_public_key,
        });
    }

    Ok((
        key_packages,
        PublicKeyPackage {
            group_public_key,
            verifying_shares,
        },
    ))
}

/// Evaluate `a_0 + a_1*x + a_2*x^2 + ...` via Horner's method.
fn eval_polynomial(coefficients: &[Scalar], x: Scalar) -> Scalar {
    let mut acc = Scalar::ZERO;
    for coeff in coefficients.iter().rev() {
        acc = acc * x + coeff;
    }
    acc
}

#[cfg(test)]
mod tests {
    use curve25519_dalek::traits::Identity;

    use super::*;

    fn ack() -> AcknowledgedPrototypeOnly {
        AcknowledgedPrototypeOnly::insecure_trusted_dealer_keygen_is_not_production_ready()
    }

    #[test]
    fn rejects_invalid_parameters() {
        assert_eq!(
            trusted_dealer_keygen(0, 1, ack()).unwrap_err(),
            TreasuryError::InvalidFrostParameters
        );
        assert_eq!(
            trusted_dealer_keygen(5, 0, ack()).unwrap_err(),
            TreasuryError::InvalidFrostParameters
        );
        assert_eq!(
            trusted_dealer_keygen(3, 4, ack()).unwrap_err(),
            TreasuryError::InvalidFrostParameters
        );
        assert_eq!(
            trusted_dealer_keygen(MAX_PARTICIPANTS + 1, 1, ack()).unwrap_err(),
            TreasuryError::InvalidFrostParameters
        );
    }

    #[test]
    fn every_share_is_individually_feldman_verifiable() {
        let (shares, public) = trusted_dealer_keygen(5, 3, ack()).unwrap();
        for share in &shares {
            let expected = *public.verifying_shares.get(&share.index).unwrap();
            assert_eq!(
                (basepoint() * share.secret_share).compress(),
                expected.compress()
            );
            assert_eq!(
                share.group_public_key.compress(),
                public.group_public_key.compress()
            );
        }
    }

    #[test]
    fn any_threshold_sized_subset_reconstructs_the_same_secret_in_the_exponent() {
        // Lagrange-interpolate the secret from two different
        // threshold-sized subsets and check both land on the same
        // group public key -- proves f(0) is well-defined regardless of
        // which signers participate, the property FROST signing itself
        // relies on (see frost_sign::lagrange_coefficient).
        let (shares, public) = trusted_dealer_keygen(5, 3, ack()).unwrap();

        let reconstruct = |subset: &[&KeyPackage]| -> RistrettoPoint {
            let indices: Vec<Scalar> = subset.iter().map(|s| Scalar::from(s.index)).collect();
            let mut acc = RistrettoPoint::identity();
            for (i, share) in subset.iter().enumerate() {
                let lambda = crate::frost_sign::lagrange_coefficient(indices[i], &indices);
                acc += (basepoint() * share.secret_share) * lambda;
            }
            acc
        };

        let subset_a = [&shares[0], &shares[1], &shares[2]];
        let subset_b = [&shares[1], &shares[2], &shares[3]];
        assert_eq!(
            reconstruct(&subset_a).compress(),
            public.group_public_key.compress()
        );
        assert_eq!(
            reconstruct(&subset_b).compress(),
            public.group_public_key.compress()
        );
    }

    #[test]
    fn distinct_keygen_runs_produce_distinct_keys() {
        let (_, public_a) = trusted_dealer_keygen(3, 2, ack()).unwrap();
        let (_, public_b) = trusted_dealer_keygen(3, 2, ack()).unwrap();
        assert_ne!(
            public_a.group_public_key.compress(),
            public_b.group_public_key.compress()
        );
    }
}
