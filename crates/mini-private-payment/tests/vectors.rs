//! Golden wire vectors.
//!
//! **If a vector here changes, the wire format changed.** That is a version
//! bump and a decision entry, not a test update. Two nodes that disagree
//! about a transcript byte disagree about what was signed, which for a ring
//! signature means one of them rejects every payment the other makes.
//!
//! Building a *whole* claim is randomized — fresh stealth `r`, fresh ring
//! nonces, fresh blinding — so these vectors pin the deterministic parts:
//! the transcript layout, the constants, and the memo's padded encoding.

use mini_crypto::HashAlgorithm;
use mini_private_payment::{
    PrivatePaymentClaim, SealedMemo, ABSOLUTE_MIN_RING_SIZE, CLAIM_TRANSCRIPT_DOMAIN,
    CLAIM_VERSION, MAX_MEMO_BYTES, MEMO_KDF_INFO, MEMO_PADDED_BYTES, MIN_RING_SIZE,
};
use mini_value::{RangeProof, RingSignature, StealthOutput, RANGE_PROOF_BYTES};

/// Checked by the compiler rather than by a test run: the tunable minimum
/// can never sit below the frozen floor, and a build that tried would not
/// link.
const _: () = assert!(MIN_RING_SIZE >= ABSOLUTE_MIN_RING_SIZE);

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Ring size for the fixture, deliberately a **literal** rather than
/// `MIN_RING_SIZE`.
///
/// These vectors pin the wire *encoding*. `MIN_RING_SIZE` is a Tier-T
/// tunable, and a golden vector that tracked it would move every time the
/// parameter was tuned — reporting "the format changed" when nothing about
/// the format had. That is a vector that cries wolf, and a vector nobody
/// trusts is worse than none. (It moved exactly once for exactly this
/// reason, when D-0449 raised the minimum from 8 to 16.)
const VECTOR_RING_SIZE: usize = 8;

/// A fully deterministic claim — every field fixed, no randomness — so its
/// transcript is a stable vector.
fn fixed_claim() -> PrivatePaymentClaim {
    // A syntactically well-formed range proof of the right length. It is
    // not a *valid* proof and is not meant to be: this vector pins the
    // encoding, and verification is tested elsewhere.
    let proof_bytes: Vec<u8> = (0..RANGE_PROOF_BYTES).map(|i| (i % 251) as u8).collect();
    let range_proof = RangeProof::from_bytes(&proof_bytes).expect("well-formed length");

    let ring: Vec<Vec<u8>> = (0..VECTOR_RING_SIZE as u8)
        .map(|i| {
            let mut member = vec![0u8; 32];
            member[0] = i;
            member
        })
        .collect();

    PrivatePaymentClaim {
        network_id: [0x11; 32],
        output: StealthOutput {
            tx_public_key: vec![0x22; 32],
            one_time_address: vec![0x33; 32],
        },
        amount_commitment: vec![0x44; 32],
        range_proof,
        memo: SealedMemo {
            ciphertext: vec![0x55; MEMO_PADDED_BYTES + 16],
        },
        valid_until_ms: 1_700_000_000_000,
        last_known_chain: b"height:4242".to_vec(),
        ring,
        signature: RingSignature {
            challenge: vec![0x66; 32],
            responses: (0..VECTOR_RING_SIZE).map(|_| vec![0x77; 32]).collect(),
            key_image: vec![0x88; 32],
        },
    }
}

#[test]
fn the_domain_separator_and_version_are_frozen() {
    assert_eq!(
        CLAIM_TRANSCRIPT_DOMAIN,
        b"mininet/mini-private-payment/claim/v1"
    );
    assert_eq!(CLAIM_VERSION, 1);
    assert_eq!(MEMO_KDF_INFO, b"mininet/mini-private-payment/memo-key/v1");
}

#[test]
fn the_size_constants_are_frozen() {
    // A memo whose padded size changed would make old and new payments
    // distinguishable by length -- an anonymity-set split, silently.
    assert_eq!(MEMO_PADDED_BYTES, 256);
    assert_eq!(MAX_MEMO_BYTES, 252);
    // 7 field elements + 6 L + 6 R + 2 folded scalars, all 32 bytes.
    assert_eq!(RANGE_PROOF_BYTES, 672);

    // ABSOLUTE_MIN_RING_SIZE is frozen and may never fall. MIN_RING_SIZE is
    // Tier T: it may be raised, never lowered past the floor. Asserting the
    // relationship rather than the value is what lets the tunable move
    // without letting it move in the wrong direction.
    assert_eq!(ABSOLUTE_MIN_RING_SIZE, 8);
    assert_eq!(MIN_RING_SIZE, 16, "current tuned value (D-0449)");
}

#[test]
fn the_binding_transcript_is_a_stable_byte_string() {
    let claim = fixed_claim();
    let transcript = claim.binding_transcript();

    // Layout, asserted field by field so a reordering is caught at the
    // exact byte rather than as one opaque digest mismatch.
    let mut offset = 0usize;
    assert_eq!(
        &transcript[offset..offset + CLAIM_TRANSCRIPT_DOMAIN.len()],
        CLAIM_TRANSCRIPT_DOMAIN
    );
    offset += CLAIM_TRANSCRIPT_DOMAIN.len();
    assert_eq!(transcript[offset], CLAIM_VERSION);
    offset += 1;
    assert_eq!(&transcript[offset..offset + 32], &[0x11; 32]);
    offset += 32;
    // Length-prefixed from here on.
    assert_eq!(&transcript[offset..offset + 4], &32u32.to_be_bytes());

    // The memo must NOT appear: it is bound *by* this digest, so including
    // it would be circular.
    assert!(
        !transcript.windows(16).any(|w| w == [0x55u8; 16]),
        "the memo leaked into its own binding"
    );

    assert_eq!(
        hex(&HashAlgorithm::Blake3.digest(&transcript)),
        "1eb36f92739c65c921b6b4b2c2fca4a27917ff1f54b21681ee6a1656c9a2a5ea"
    );
}

#[test]
fn the_full_transcript_is_the_binding_transcript_plus_the_memo() {
    let claim = fixed_claim();
    let binding = claim.binding_transcript();
    let full = claim.transcript();
    assert!(full.starts_with(&binding));
    // The memo, length-prefixed, is the whole remainder.
    assert_eq!(full.len(), binding.len() + 4 + MEMO_PADDED_BYTES + 16);
    assert_ne!(claim.binding_digest(), claim.transcript_digest());
}

#[test]
fn a_claim_encodes_to_stable_bytes() {
    let claim = fixed_claim();
    let encoded = claim.encode();
    assert_eq!(
        hex(&HashAlgorithm::Blake3.digest(&encoded)),
        "d5ffcf909e2c21824935e93915920ef873d547c8eda202e058f361cd0f7b5faf"
    );
    // And it round-trips.
    assert_eq!(PrivatePaymentClaim::decode(&encoded).unwrap(), claim);
}

#[test]
fn the_encoded_length_is_the_same_for_every_payment() {
    // Constant size across amounts and purposes is a privacy property, not
    // an aesthetic one: a variable-length payment leaks through its length.
    let mut a = fixed_claim();
    let mut b = fixed_claim();
    a.valid_until_ms = 0;
    b.valid_until_ms = u64::MAX;
    assert_eq!(a.encode().len(), b.encode().len());

    // last_known_chain is the one caller-variable-length field, and it is
    // sender-chosen rather than derived from anything private.
    let mut c = fixed_claim();
    c.last_known_chain = b"height:1".to_vec();
    assert_eq!(
        c.encode().len() + 3,
        a.encode().len(),
        "only the chain reference varies in length"
    );
}
