//! The V3 stealth-address derivation: [`crate::stealth_impl`]'s same
//! CryptoNote-style scheme, over the Gate #72 audit's exact Section 6
//! domain-separated offset hash instead of a bare, undomained one.
//!
//! # What changed from `crate::stealth_impl`, and why
//!
//! [`crate::stealth_impl::derive_output_with_secret`]'s shared scalar is
//! `s = Hs(shared_point)` — no domain tag, no binding to which suite,
//! network, or even which recipient keys and transaction key produced it.
//! Two different protocols (or two different networks) hashing the same
//! raw Diffie-Hellman point would derive the same `s`, which is exactly
//! the "generic transcript framing is too permissive" class of gap the
//! Gate #72 audit's F72-03 names. Section 6 closes it for `PrivatePaymentV3`
//! specifically: `s = Hs("mininet/value/stealth-offset/v3", suite_id ||
//! network_id || A || B || R || shared)`, so a shared point can never be
//! replayed as though it were computed under a different suite, network,
//! or recipient/transaction key pair.
//!
//! Section 6 also specifies resampling `r` on the (astronomically
//! unlikely) degenerate outcomes `s == 0` or `P == identity`, rather than
//! [`crate::stealth_impl`]'s implicit reliance on those cases simply never
//! occurring. [`MAX_RESAMPLE_ATTEMPTS`] bounds the retry loop so a
//! CSPRNG that somehow kept producing degenerate output cannot spin
//! forever — with real entropy this never iterates more than once.
//!
//! [FREEZE reminder — D-0036/D-0037/D-0047] Founder-overridden,
//! AI-authored prototype. Unaudited. Nothing value-bearing may depend on
//! this before Gate #72 closes.

use curve25519_dalek::traits::Identity;

use crate::canonical::{
    canonical_nonidentity_point as decompress_point, canonical_scalar as decompress_scalar,
};
use crate::curve::{basepoint, hash_to_scalar, random_scalar, RistrettoPoint, Scalar};
use crate::stealth::StealthOutput;
use crate::stealth_impl::StealthSharedSecret;

/// Domain separator for the stealth-offset hash — the audit's Section
/// 5.4/6 exact string.
pub const STEALTH_OFFSET_V3_DOMAIN: &[u8] = b"mininet/value/stealth-offset/v3";

/// Bound on how many times [`derive_output_v3`] resamples `r` after a
/// degenerate `s == 0` or `P == identity` outcome, per Section 6 step 5/7.
/// Never expected to iterate more than once in practice — see this
/// module's own docs.
const MAX_RESAMPLE_ATTEMPTS: u32 = 8;

fn offset_scalar(
    suite_id: u16,
    network_id: &[u8; 32],
    a: &RistrettoPoint,
    b: &RistrettoPoint,
    r_pub: &RistrettoPoint,
    shared_point: &RistrettoPoint,
) -> Scalar {
    hash_to_scalar(&[
        STEALTH_OFFSET_V3_DOMAIN,
        &suite_id.to_be_bytes(),
        network_id,
        a.compress().as_bytes(),
        b.compress().as_bytes(),
        r_pub.compress().as_bytes(),
        shared_point.compress().as_bytes(),
    ])
}

/// Derive a V3 stealth output and the shared secret used to build it.
/// `suite_id`/`network_id` bind the offset hash so it can never be
/// replayed as though computed under a different suite or network — see
/// this module's own docs.
pub fn derive_output_v3(
    suite_id: u16,
    network_id: &[u8; 32],
    recipient_spend_public: &[u8],
    recipient_view_public: &[u8],
) -> Option<(StealthOutput, StealthSharedSecret)> {
    let a = decompress_point(recipient_spend_public)?;
    let b = decompress_point(recipient_view_public)?;
    for _ in 0..MAX_RESAMPLE_ATTEMPTS {
        let r = random_scalar().ok()?;
        let r_pub = r * basepoint();
        let shared_point = r * b;
        let s = offset_scalar(suite_id, network_id, &a, &b, &r_pub, &shared_point);
        if s == Scalar::ZERO {
            continue;
        }
        let one_time_address = s * basepoint() + a;
        if one_time_address == RistrettoPoint::identity() {
            continue;
        }
        return Some((
            StealthOutput {
                tx_public_key: r_pub.compress().to_bytes().to_vec(),
                one_time_address: one_time_address.compress().to_bytes().to_vec(),
            },
            StealthSharedSecret::from_point(shared_point),
        ));
    }
    None
}

/// Recover the same shared secret on the recipient's side, and
/// independently recompute/verify the one-time address it should have
/// produced — Section 6's recipient-side steps in one call, since a
/// caller always needs both together to accept an output as genuinely
/// theirs (recomputing `s` without checking `P` would accept an output
/// whose address does not actually match this offset).
pub fn recover_and_verify_v3(
    suite_id: u16,
    network_id: &[u8; 32],
    own_view_secret: &[u8],
    own_spend_public: &[u8],
    output: &StealthOutput,
) -> Option<StealthSharedSecret> {
    let b_secret = decompress_scalar(own_view_secret)?;
    let a = decompress_point(own_spend_public)?;
    let b_pub = b_secret * basepoint();
    let r_pub = decompress_point(&output.tx_public_key)?;
    let shared_point = b_secret * r_pub;
    let s = offset_scalar(suite_id, network_id, &a, &b_pub, &r_pub, &shared_point);
    let expected = s * basepoint() + a;
    if expected.compress().to_bytes().as_slice() != output.one_time_address.as_slice() {
        return None;
    }
    Some(StealthSharedSecret::from_point(shared_point))
}

/// Derive the one-time private scalar `x = s + a` needed to spend
/// `output`, verifying `x*G == P` before returning it (Section 6's
/// recipient-side `require xG == P` and `require x != 0`) — never hands
/// back a scalar that does not actually open the claimed output.
pub fn derive_spend_scalar_v3(
    suite_id: u16,
    network_id: &[u8; 32],
    own_view_secret: &[u8],
    own_spend_secret: &[u8],
    output: &StealthOutput,
) -> Option<Scalar> {
    let b_secret = decompress_scalar(own_view_secret)?;
    let a_secret = decompress_scalar(own_spend_secret)?;
    let a_pub = a_secret * basepoint();
    let b_pub = b_secret * basepoint();
    let r_pub = decompress_point(&output.tx_public_key)?;
    let shared_point = b_secret * r_pub;
    let s = offset_scalar(suite_id, network_id, &a_pub, &b_pub, &r_pub, &shared_point);
    let x = s + a_secret;
    if x == Scalar::ZERO {
        return None;
    }
    let expected = (x * basepoint()).compress().to_bytes();
    if expected.as_slice() != output.one_time_address.as_slice() {
        return None;
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stealth_impl::StealthKeypair;

    const SUITE_ID: u16 = 0x0003;
    const NETWORK: [u8; 32] = [3u8; 32];

    #[test]
    fn sender_and_recipient_derive_the_same_shared_secret() {
        let recipient = StealthKeypair::generate().unwrap();
        let (output, sender_secret) = derive_output_v3(
            SUITE_ID,
            &NETWORK,
            &recipient.spend_public_bytes(),
            &recipient.view_public_bytes(),
        )
        .unwrap();
        let recipient_secret = recover_and_verify_v3(
            SUITE_ID,
            &NETWORK,
            &recipient.view_secret_bytes(),
            &recipient.spend_public_bytes(),
            &output,
        )
        .unwrap();
        assert_eq!(
            sender_secret.as_key_material(),
            recipient_secret.as_key_material()
        );
    }

    #[test]
    fn a_stranger_cannot_recover_or_verify_the_output() {
        let recipient = StealthKeypair::generate().unwrap();
        let stranger = StealthKeypair::generate().unwrap();
        let (output, _) = derive_output_v3(
            SUITE_ID,
            &NETWORK,
            &recipient.spend_public_bytes(),
            &recipient.view_public_bytes(),
        )
        .unwrap();
        assert!(recover_and_verify_v3(
            SUITE_ID,
            &NETWORK,
            &stranger.view_secret_bytes(),
            &stranger.spend_public_bytes(),
            &output,
        )
        .is_none());
    }

    #[test]
    fn the_derived_spend_scalar_opens_the_one_time_address() {
        let recipient = StealthKeypair::generate().unwrap();
        let (output, _) = derive_output_v3(
            SUITE_ID,
            &NETWORK,
            &recipient.spend_public_bytes(),
            &recipient.view_public_bytes(),
        )
        .unwrap();
        let x = derive_spend_scalar_v3(
            SUITE_ID,
            &NETWORK,
            &recipient.view_secret_bytes(),
            &recipient.spend_secret_bytes(),
            &output,
        )
        .unwrap();
        assert_eq!(
            (x * basepoint()).compress().to_bytes().as_slice(),
            output.one_time_address.as_slice()
        );
    }

    #[test]
    fn two_payments_to_one_recipient_are_unlinkable() {
        let recipient = StealthKeypair::generate().unwrap();
        let (a, secret_a) = derive_output_v3(
            SUITE_ID,
            &NETWORK,
            &recipient.spend_public_bytes(),
            &recipient.view_public_bytes(),
        )
        .unwrap();
        let (b, secret_b) = derive_output_v3(
            SUITE_ID,
            &NETWORK,
            &recipient.spend_public_bytes(),
            &recipient.view_public_bytes(),
        )
        .unwrap();
        assert_ne!(a.one_time_address, b.one_time_address);
        assert_ne!(secret_a.as_key_material(), secret_b.as_key_material());
    }

    #[test]
    fn a_different_network_derives_a_different_offset_and_fails_to_verify() {
        // The whole reason this module exists over `crate::stealth_impl`:
        // the offset hash is bound to suite_id/network_id, so a shared
        // point recomputed under a different network must not verify.
        let recipient = StealthKeypair::generate().unwrap();
        let (output, _) = derive_output_v3(
            SUITE_ID,
            &NETWORK,
            &recipient.spend_public_bytes(),
            &recipient.view_public_bytes(),
        )
        .unwrap();
        assert!(recover_and_verify_v3(
            SUITE_ID,
            &[9u8; 32],
            &recipient.view_secret_bytes(),
            &recipient.spend_public_bytes(),
            &output,
        )
        .is_none());
    }

    #[test]
    fn malformed_recipient_keys_are_rejected_without_panicking() {
        assert!(derive_output_v3(SUITE_ID, &NETWORK, b"short", b"also-short").is_none());
    }

    #[test]
    fn an_identity_view_key_is_rejected() {
        let identity = RistrettoPoint::identity().compress().to_bytes();
        let recipient = StealthKeypair::generate().unwrap();
        assert!(derive_output_v3(
            SUITE_ID,
            &NETWORK,
            &recipient.spend_public_bytes(),
            &identity,
        )
        .is_none());
    }
}
