// Each test binary compiles this module separately and uses a different
// subset, so unused-here is expected rather than dead.
#![allow(dead_code)]

//! Shared fixtures. Every payment built here goes through the real
//! primitives — real stealth derivation, a real ring signature, a real
//! Bulletproof. Nothing is stubbed, because a privacy test against a stub
//! proves nothing about privacy.

use mini_private_payment::{
    build, canonicalize_ring, PaymentPurpose, PaymentRequest, PrivatePaymentClaim, MIN_RING_SIZE,
};
use mini_value::{StealthKeypair, StealthSharedSecret};

pub const NETWORK: [u8; 32] = [0x5a; 32];

/// A recipient's published stealth keys plus the secrets to scan with.
pub fn recipient() -> StealthKeypair {
    StealthKeypair::generate().unwrap()
}

/// A one-time keypair usable as a ring member: returns (public, secret).
pub fn one_time_key() -> (Vec<u8>, [u8; 32]) {
    let key = StealthKeypair::generate().unwrap();
    (key.spend_public_bytes().to_vec(), key.spend_secret_bytes())
}

/// A canonical ring of `size` members containing `real_public`, and the
/// index the real key ends up at after sorting.
pub fn ring_containing(real_public: &[u8], size: usize) -> (Vec<Vec<u8>>, usize) {
    let mut ring: Vec<Vec<u8>> = (0..size - 1).map(|_| one_time_key().0).collect();
    ring.push(real_public.to_vec());
    canonicalize_ring(&mut ring);
    let index = ring
        .iter()
        .position(|member| member.as_slice() == real_public)
        .expect("real key is in its own ring");
    (ring, index)
}

/// A complete, valid private payment to `to`, for `amount` micro-MINI,
/// referencing `purpose`.
pub fn payment_to(
    to: &StealthKeypair,
    amount: u64,
    purpose: &[u8],
) -> (PrivatePaymentClaim, StealthSharedSecret, [u8; 32]) {
    payment_with_ring(to, amount, purpose, MIN_RING_SIZE)
}

/// The same, with an explicit ring size.
pub fn payment_with_ring(
    to: &StealthKeypair,
    amount: u64,
    purpose: &[u8],
    ring_size: usize,
) -> (PrivatePaymentClaim, StealthSharedSecret, [u8; 32]) {
    let (real_public, real_secret) = one_time_key();
    let (ring, secret_index) = ring_containing(&real_public, ring_size);
    let blinding = mini_crypto::random_32().unwrap();
    let request = PaymentRequest {
        network_id: NETWORK,
        recipient_spend_public: to.spend_public_bytes().to_vec(),
        recipient_view_public: to.view_public_bytes().to_vec(),
        amount_micro: amount,
        purpose: PaymentPurpose::new(purpose.to_vec()),
        valid_until_ms: 10_000,
        last_known_chain: b"height:1".to_vec(),
        ring,
        secret_index,
        secret_key: real_secret.to_vec(),
        blinding,
    };
    let (claim, shared) = build(&request).unwrap();
    (claim, shared, real_secret)
}
