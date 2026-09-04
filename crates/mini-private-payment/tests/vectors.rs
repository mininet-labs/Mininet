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
    AcknowledgedIrreversibleDisclosure, ClaimInput, ClaimOutput, PrivatePaymentClaim, SealedMemo,
    ViewKeyDisclosure, ABSOLUTE_MIN_RING_SIZE, CLAIM_TRANSCRIPT_DOMAIN, CLAIM_VERSION,
    DISCLOSURE_DOMAIN, DISCLOSURE_VERSION, MAX_INPUTS, MAX_MEMO_BYTES, MAX_OUTPUTS, MEMO_KDF_INFO,
    MEMO_PADDED_BYTES, MIN_RING_SIZE, NOTE_OVERHEAD_BYTES,
};
use mini_value::{MlsagSignature, RangeProof, StealthOutput, RANGE_PROOF_BYTES};

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

/// One deterministic input, distinguished from its siblings by `tag` so a
/// multi-input vector cannot pass while silently encoding one input twice.
fn fixed_input(tag: u8) -> ClaimInput {
    let ring: Vec<Vec<u8>> = (0..VECTOR_RING_SIZE as u8)
        .map(|i| {
            let mut member = vec![0u8; 32];
            member[0] = i;
            member[1] = tag;
            member
        })
        .collect();
    let ring_commitments: Vec<Vec<u8>> = (0..VECTOR_RING_SIZE as u8)
        .map(|i| {
            let mut commitment = vec![0x99u8; 32];
            commitment[0] = i;
            commitment[1] = tag;
            commitment
        })
        .collect();

    ClaimInput {
        ring,
        ring_commitments,
        pseudo_commitment: vec![0xaa ^ tag; 32],
        signature: MlsagSignature {
            challenge: vec![0x66 ^ tag; 32],
            key_responses: (0..VECTOR_RING_SIZE).map(|_| vec![0x77; 32]).collect(),
            blinding_responses: (0..VECTOR_RING_SIZE).map(|_| vec![0x78; 32]).collect(),
            key_image: vec![0x88 ^ tag; 32],
        },
    }
}

/// One deterministic output.
fn fixed_output(tag: u8) -> ClaimOutput {
    // A syntactically well-formed range proof of the right length. It is
    // not a *valid* proof and is not meant to be: this vector pins the
    // encoding, and verification is tested elsewhere.
    let proof_bytes: Vec<u8> = (0..RANGE_PROOF_BYTES)
        .map(|i| ((i + tag as usize) % 251) as u8)
        .collect();

    ClaimOutput {
        output: StealthOutput {
            tx_public_key: vec![0x22 ^ tag; 32],
            one_time_address: vec![0x33 ^ tag; 32],
        },
        amount_commitment: vec![0x44 ^ tag; 32],
        range_proof: RangeProof::from_bytes(&proof_bytes).expect("well-formed length"),
        memo: SealedMemo {
            ciphertext: vec![0x55 ^ tag; MEMO_PADDED_BYTES + 16],
        },
    }
}

/// A fully deterministic claim — every field fixed, no randomness — so its
/// transcript is a stable vector.
///
/// Two inputs and two outputs rather than one of each: a single-input,
/// single-output vector would pin the format for exactly the shape that
/// existed before conservation did, and would not catch a change to how
/// the counts, or the per-input and per-output repetitions, are laid out.
fn fixed_claim() -> PrivatePaymentClaim {
    PrivatePaymentClaim {
        network_id: [0x11; 32],
        inputs: vec![fixed_input(0), fixed_input(1)],
        outputs: vec![fixed_output(0), fixed_output(1)],
        fee_micro: 250,
        valid_until_ms: 1_700_000_000_000,
        last_known_chain: b"height:4242".to_vec(),
    }
}

#[test]
fn the_domain_separator_and_version_are_frozen() {
    // v2 is the conservation format: many inputs, many outputs, a public
    // fee, and an MLSAG per input. v1 claims do not decode under it and
    // are not meant to -- a v1 claim proved no balance, so accepting one
    // would accept a payment that could mint value.
    assert_eq!(
        CLAIM_TRANSCRIPT_DOMAIN,
        b"mininet/mini-private-payment/claim/v2"
    );
    assert_eq!(CLAIM_VERSION, 2);
    assert_eq!(MEMO_KDF_INFO, b"mininet/mini-private-payment/memo-key/v1");
}

#[test]
fn the_size_constants_are_frozen() {
    // A memo whose padded size changed would make old and new payments
    // distinguishable by length -- an anonymity-set split, silently.
    assert_eq!(MEMO_PADDED_BYTES, 256);
    // The note now carries the commitment opening as well as the purpose,
    // so the room left for caller bytes is smaller by exactly that.
    assert_eq!(NOTE_OVERHEAD_BYTES, 4 + 8 + 32);
    assert_eq!(MAX_MEMO_BYTES, MEMO_PADDED_BYTES - NOTE_OVERHEAD_BYTES);
    assert_eq!(MAX_MEMO_BYTES, 212);

    // Bounds on how much work one claim can impose on every verifier and
    // every scanning wallet in the network.
    assert_eq!(MAX_INPUTS, 16);
    assert_eq!(MAX_OUTPUTS, 16);
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
    // The fee is in the clear and inside the signed transcript, so it can
    // be neither hidden from a verifier nor edited after signing.
    assert_eq!(&transcript[offset..offset + 8], &250u64.to_be_bytes());
    offset += 8;
    assert_eq!(
        &transcript[offset..offset + 8],
        &1_700_000_000_000u64.to_be_bytes()
    );

    // The memos must NOT appear: they are bound *by* this digest, so
    // including them would be circular.
    for tag in [0x55u8, 0x54] {
        assert!(
            !transcript.windows(16).any(|w| w == [tag; 16]),
            "a memo leaked into its own binding"
        );
    }

    assert_eq!(
        hex(&HashAlgorithm::Blake3.digest(&transcript)),
        "ee42bc644e0bd5d8946da878747aadb209aed1d6d36a0af52af6828766d591c9"
    );
}

#[test]
fn the_full_transcript_is_the_binding_transcript_plus_the_memo() {
    let claim = fixed_claim();
    let binding = claim.binding_transcript();
    let full = claim.transcript();
    assert!(full.starts_with(&binding));
    // Every memo, length-prefixed, is the whole remainder -- one per
    // output, so a claim paying three parties signs all three.
    assert_eq!(
        full.len(),
        binding.len() + claim.outputs.len() * (4 + MEMO_PADDED_BYTES + 16)
    );
    assert_ne!(claim.binding_digest(), claim.transcript_digest());
}

#[test]
fn a_claim_encodes_to_stable_bytes() {
    let claim = fixed_claim();
    let encoded = claim.encode();
    assert_eq!(
        hex(&HashAlgorithm::Blake3.digest(&encoded)),
        "ccc115160ab70d867d39d64b352b4cd3cc16ca27fe9a0ce2f9ecdb4eece943db"
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

    // Input and output *counts* do vary the length, and that is the honest
    // trade: a claim that always encoded a fixed number of inputs would pad
    // every payment to the maximum, and a claim that hid its own structure
    // could not be verified at all. What is bounded is how much that
    // leaks -- see the crate docs.
    let mut d = fixed_claim();
    d.outputs.push(fixed_output(2));
    assert!(d.encode().len() > a.encode().len());
}

/// A fully deterministic disclosure (D-0451). The keys are not a real
/// keypair — `verify_disclosure` would reject them, and correctly — because
/// what is pinned here is the *encoding*, and a vector built from a
/// generated keypair would be a vector built from randomness.
fn fixed_disclosure() -> ViewKeyDisclosure {
    let acknowledged = AcknowledgedIrreversibleDisclosure::new(
        AcknowledgedIrreversibleDisclosure::REQUIRED_ACKNOWLEDGEMENT,
    )
    .expect("the exact phrase");
    ViewKeyDisclosure::create(
        vec![0x44; 32],
        vec![0x55; 32],
        vec![0x66; 32],
        b"treasury disbursements".to_vec(),
        1_700_000_000_000,
        &acknowledged,
    )
}

#[test]
fn a_disclosure_encodes_to_stable_bytes() {
    // A published view key is quoted, archived, and referred to by digest
    // long after it is published, and a disclosure whose bytes shifted under
    // it would break every reference. Pinned like any other wire object.
    let disclosure = fixed_disclosure();
    let encoded = disclosure.encode();
    assert!(encoded.starts_with(DISCLOSURE_DOMAIN));
    assert_eq!(encoded[DISCLOSURE_DOMAIN.len()], DISCLOSURE_VERSION);
    assert_eq!(
        hex(&HashAlgorithm::Blake3.digest(&encoded)),
        "633f8dc9fa1f847c6ec528777e93a02d77a1ca8cafcb21977ac68f7c0fe9120d"
    );
    assert_eq!(ViewKeyDisclosure::decode(&encoded).unwrap(), disclosure);
    assert_eq!(disclosure.digest(), HashAlgorithm::Blake3.digest(&encoded));
}

#[test]
fn the_disclosure_domain_cannot_collide_with_the_claim_domain() {
    // Two domain-separated objects in one crate: neither may ever be a
    // prefix of the other, or a decoder could be walked from one into the
    // other by a caller who controls the bytes.
    assert!(!DISCLOSURE_DOMAIN.starts_with(CLAIM_TRANSCRIPT_DOMAIN));
    assert!(!CLAIM_TRANSCRIPT_DOMAIN.starts_with(DISCLOSURE_DOMAIN));
}
