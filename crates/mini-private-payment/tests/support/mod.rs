// Each test binary compiles this module separately and uses a different
// subset, so unused-here is expected rather than dead.
#![allow(dead_code)]

//! Shared fixtures. Every payment built here goes through the real
//! primitives — real stealth derivation, a real ring signature, a real
//! Bulletproof, and the protocol's real decoy sampling. Nothing is stubbed,
//! because a privacy test against a stub proves nothing about privacy.

use mini_private_payment::{
    build, InMemoryOutputSet, PaymentPurpose, PaymentRequest, PrivatePaymentClaim, MIN_RING_SIZE,
};
use mini_value::{StealthKeypair, StealthSharedSecret};

pub const NETWORK: [u8; 32] = [0x5a; 32];

/// A recipient's published stealth keys plus the secrets to scan with.
pub fn recipient() -> StealthKeypair {
    StealthKeypair::generate().unwrap()
}

/// A one-time keypair usable as an output: returns (public, secret).
pub fn one_time_key() -> (Vec<u8>, [u8; 32]) {
    let key = StealthKeypair::generate().unwrap();
    (key.spend_public_bytes().to_vec(), key.spend_secret_bytes())
}

/// An output set of `size` real one-time keys, with the caller's own output
/// appended last (newest). Returns the set, the real index, and its secret.
pub fn output_set_with_own(size: usize) -> (InMemoryOutputSet, usize, [u8; 32]) {
    let mut outputs = InMemoryOutputSet::new();
    for _ in 0..size {
        outputs.push(one_time_key().0);
    }
    let (own_public, own_secret) = one_time_key();
    outputs.push(own_public);
    (outputs, size, own_secret)
}

/// A default output set comfortably larger than the ring.
pub fn outputs() -> (InMemoryOutputSet, usize, [u8; 32]) {
    output_set_with_own(64)
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
    let (set, real_index, secret) = output_set_with_own(ring_size * 4);
    let request = request_for(to, amount, purpose, ring_size, real_index, &secret);
    let (claim, shared) = build(&request, &set).unwrap();
    (claim, shared, secret)
}

/// A `PaymentRequest` with fresh entropy, for tests that need to drive
/// `build` themselves.
pub fn request_for(
    to: &StealthKeypair,
    amount: u64,
    purpose: &[u8],
    ring_size: usize,
    real_output_index: usize,
    secret: &[u8; 32],
) -> PaymentRequest {
    PaymentRequest {
        network_id: NETWORK,
        recipient_spend_public: to.spend_public_bytes().to_vec(),
        recipient_view_public: to.view_public_bytes().to_vec(),
        amount_micro: amount,
        purpose: PaymentPurpose::new(purpose.to_vec()),
        valid_until_ms: 10_000,
        last_known_chain: b"height:1".to_vec(),
        ring_size,
        real_output_index,
        secret_key: secret.to_vec(),
        decoy_entropy: mini_crypto::random_32().unwrap(),
        blinding: mini_crypto::random_32().unwrap(),
    }
}
