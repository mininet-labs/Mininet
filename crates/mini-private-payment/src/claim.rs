//! The shielded payment claim: what a payment looks like when the network
//! is not entitled to know who paid, who was paid, how much, or why.
//!
//! # What each transparent field became
//!
//! | `mini_settlement::PaymentClaim` | here |
//! |---|---|
//! | `payer: Vec<u8>` — a stable key | *absent*; a ring signature proves some member of an anonymity set authorized this |
//! | `payee: Vec<u8>` — a stable address | [`mini_value::StealthOutput`] — a fresh one-time address per payment |
//! | `amount_micro: u64` — cleartext | a Pedersen commitment plus a Bulletproof that it is in range |
//! | `sequence: u64` — a per-payer counter | *absent*; the key image is the conflict key |
//! | (no purpose field) | [`SealedMemo`], readable only by the recipient |
//!
//! The `sequence` removal is the one worth dwelling on. In the transparent
//! claim, every payment by one payer shares a payer key *and* carries an
//! incrementing counter — so an observer gets the payer's complete ordered
//! payment history for free, without breaking anything. There is no
//! equivalent here, because there is nothing stable to order.
//!
//! # The key image is linkable, and has to be
//!
//! [`VerifiedPrivateClaim::key_image`] is the one value that is *designed*
//! to be comparable across payments: it is deterministic in the spent
//! one-time key, so spending the same output twice produces the same key
//! image and the second spend is refused. That is what stops double-spends
//! without a public payer, and it is the standard CryptoNote trade-off. Its
//! cost is real and must not be understated: two spends of the same output
//! are linkable to each other. They are not linkable to a person, an
//! identity root, or any other payment — but "unlinkable" is too strong a
//! word for what this achieves, and this crate never uses it unqualified.

use mini_crypto::HashAlgorithm;
use mini_value::{
    ConfidentialAmountScheme, MininetConfidentialAmount, MininetRingSignature, RangeProof,
    RingSignature, RingSignatureScheme, StealthOutput, StealthSharedSecret, RANGE_PROOF_BYTES,
};

use crate::codec::{Reader, Writer};
use crate::error::{DecodeFailure, PrivatePaymentError, Result};
use crate::memo::{PaymentPurpose, SealedMemo};

/// Domain separator for the claim transcript — the bytes a ring signature
/// actually signs.
pub const CLAIM_TRANSCRIPT_DOMAIN: &[u8] = b"mininet/mini-private-payment/claim/v1";

/// Wire format version. A decoder that does not recognize this refuses the
/// claim rather than guessing at a layout.
pub const CLAIM_VERSION: u8 = 1;

/// Smallest ring this crate will verify.
///
/// A ring of one names its signer outright, and a ring of two gives even
/// odds. Sixteen costs sixteen scalar multiplications to verify instead of
/// eight — negligible even on the weakest device this project targets
/// (Directive 11) — and ring size is the cheapest anonymity lever available,
/// so spending it is close to free.
///
/// **Tier T, with [`ABSOLUTE_MIN_RING_SIZE`] as the frozen floor beneath
/// it.** This value may be raised by ordinary tunable-parameter process; it
/// may never be lowered past the floor. That asymmetry is deliberate: a
/// future change arguing for a smaller ring on performance grounds is
/// exactly how this protection dies quietly elsewhere, and a floor makes
/// that a constitutional conversation rather than a patch.
///
/// It is a legible figure, **not** one derived from a deanonymization
/// analysis of real traffic — no such traffic exists yet, and the design
/// document records that as the open question it is.
pub const MIN_RING_SIZE: usize = 16;

/// The frozen floor: no ring size below this is admissible, ever, whatever
/// [`MIN_RING_SIZE`] is tuned to.
pub const ABSOLUTE_MIN_RING_SIZE: usize = 8;

const _: () = assert!(
    MIN_RING_SIZE >= ABSOLUTE_MIN_RING_SIZE,
    "the tunable minimum can never be set below the frozen floor"
);

/// Largest ring, bounding verification cost. Ring signature verification is
/// linear in ring size, so an unbounded ring is a denial-of-service vector
/// against exactly the weak devices Directive 11 protects.
pub const MAX_RING_SIZE: usize = 128;

/// A payment that hides its payer, payee, amount, and purpose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivatePaymentClaim {
    /// Exact settlement network. A claim built for a test network must
    /// never be replayable onto the real one.
    pub network_id: [u8; 32],
    /// The one-time output this payment goes to.
    pub output: StealthOutput,
    /// Pedersen commitment to the amount.
    pub amount_commitment: Vec<u8>,
    /// Bulletproof that the committed amount is in `[0, 2^64)`.
    pub range_proof: RangeProof,
    /// The purpose, sealed to the recipient.
    pub memo: SealedMemo,
    /// The claim expires if it has not reached canonical inclusion by this
    /// device-clock time, in ms. Self-reported, like everywhere else in
    /// this tree that lacks a time anchor.
    pub valid_until_ms: u64,
    /// Opaque reference to the canonical chain state the payer last
    /// observed. Carried so a reconciler can reason about plausibility at
    /// signing time without this crate knowing the chain's representation.
    pub last_known_chain: Vec<u8>,
    /// The anonymity set: one-time public keys, one of which the signer
    /// actually controls. Canonically sorted and deduplicated.
    pub ring: Vec<Vec<u8>>,
    /// Proof that some member of `ring` authorized this exact claim.
    pub signature: RingSignature,
}

/// A claim that has passed every structural and cryptographic check.
///
/// Constructing one outside [`verify`] is impossible: the fields are
/// private and there is no public constructor. A caller holding a
/// `VerifiedPrivateClaim` therefore knows the checks ran, rather than
/// having to remember to run them — the same reason
/// `mini_storage_fraud::VerifiedReplicaClaim` exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedPrivateClaim {
    claim: PrivatePaymentClaim,
    transcript_digest: [u8; 32],
    binding_digest: [u8; 32],
}

impl VerifiedPrivateClaim {
    pub fn claim(&self) -> &PrivatePaymentClaim {
        &self.claim
    }

    /// The double-spend nullifier. Deterministic in the spent one-time key,
    /// so the same output can never be spent twice — see this module's
    /// docs on what that costs in linkability.
    pub fn key_image(&self) -> &[u8] {
        &self.claim.signature.key_image
    }

    /// The digest the ring signature committed to — this claim's identity,
    /// and the key a ledger or nullifier set records it under.
    pub fn transcript_digest(&self) -> &[u8; 32] {
        &self.transcript_digest
    }

    /// The AAD the memo was sealed under: everything about this payment
    /// except the memo itself.
    pub fn binding_digest(&self) -> &[u8; 32] {
        &self.binding_digest
    }

    /// Open this payment's memo, if it is addressed to the holder of
    /// `shared`.
    pub fn open_memo(&self, shared: &StealthSharedSecret) -> Result<PaymentPurpose> {
        self.claim.memo.open(shared, &self.binding_digest)
    }

    /// Fabricate the one claim shape [`build`] cannot produce: a payment
    /// that really is addressed to a recipient, whose memo that recipient
    /// cannot open.
    ///
    /// Reachable in the wild, unreachable through this crate. [`build`]
    /// seals every memo with the same shared secret it derived the output
    /// from, and [`verify`] rejects a memo edited afterwards because the
    /// signature covers it. But a hostile *encoder* is bound by neither: it
    /// can derive a correct stealth output, seal the memo under a key the
    /// recipient will never derive, and sign that transcript honestly. The
    /// result verifies, is recognized, and will not open.
    ///
    /// Crate-internal and test-only, because it exists solely so
    /// [`crate::scan`] can be tested against a claim a stranger could
    /// actually send. Exposing it would be handing out a memo-corrupting
    /// constructor for no legitimate purpose.
    #[cfg(test)]
    pub(crate) fn fabricate_unopenable_memo(mut self) -> Self {
        let tail = self.claim.memo.ciphertext.len() - 1;
        self.claim.memo.ciphertext[tail] ^= 0xff;
        self
    }
}

impl PrivatePaymentClaim {
    /// Everything the memo is bound to: every field **except** the memo
    /// itself and the signature.
    ///
    /// The memo cannot be sealed against the full transcript, because the
    /// full transcript contains the memo — the two would define each other.
    /// Splitting the transcript resolves that without weakening either
    /// binding:
    ///
    /// - the **memo** is sealed with this digest as AEAD additional data,
    ///   so it cannot be moved onto a claim paying a different address or
    ///   committing a different amount;
    /// - the **signature** covers [`Self::transcript`], which does include
    ///   the memo, so the memo cannot be swapped or stripped either.
    ///
    /// Binding the memo to the full transcript instead would have been
    /// circular; binding it to nothing would have made it transplantable.
    pub fn binding_transcript(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(CLAIM_TRANSCRIPT_DOMAIN);
        w.u8(CLAIM_VERSION);
        w.raw(&self.network_id);
        w.bytes(&self.output.tx_public_key);
        w.bytes(&self.output.one_time_address);
        w.bytes(&self.amount_commitment);
        w.bytes(&self.range_proof.to_bytes());
        w.u64(self.valid_until_ms);
        w.bytes(&self.last_known_chain);
        w.u32(self.ring.len() as u32);
        for member in &self.ring {
            w.bytes(member);
        }
        w.finish()
    }

    /// BLAKE3 of [`Self::binding_transcript`] — the memo's AAD.
    pub fn binding_digest(&self) -> [u8; 32] {
        HashAlgorithm::Blake3.digest(&self.binding_transcript())
    }

    /// The exact bytes a ring signature signs: the binding transcript plus
    /// the memo.
    ///
    /// The signature is excluded for the obvious reason, and the ring is
    /// included for the less obvious one: without it, a signature could be
    /// replayed against a *different* ring that happens to contain the same
    /// real signer, changing who the claim appears to hide among.
    pub fn transcript(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(&self.binding_transcript());
        self.memo.write_into(&mut w);
        w.finish()
    }

    /// BLAKE3 of [`Self::transcript`] — the claim's identity, and the key
    /// a ledger or nullifier set records it under.
    pub fn transcript_digest(&self) -> [u8; 32] {
        HashAlgorithm::Blake3.digest(&self.transcript())
    }

    /// Canonical wire encoding.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(&self.transcript());
        w.bytes(&self.signature.challenge);
        w.u32(self.signature.responses.len() as u32);
        for response in &self.signature.responses {
            w.bytes(response);
        }
        w.bytes(&self.signature.key_image);
        w.finish()
    }

    /// Decode a claim. Structural validation only — a decoded claim is not
    /// a verified one, and [`verify`] is the only thing that decides that.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let domain = r.array::<{ CLAIM_TRANSCRIPT_DOMAIN.len() }>()?;
        if domain != CLAIM_TRANSCRIPT_DOMAIN {
            return Err(DecodeFailure::UnsupportedVersion.into());
        }
        if r.u8()? != CLAIM_VERSION {
            return Err(DecodeFailure::UnsupportedVersion.into());
        }
        let network_id = r.array::<32>()?;
        let output = StealthOutput {
            tx_public_key: r.field_element()?,
            one_time_address: r.field_element()?,
        };
        let amount_commitment = r.field_element()?;
        let proof_bytes = r.bytes()?;
        let range_proof =
            RangeProof::from_bytes(&proof_bytes).ok_or(DecodeFailure::BadRangeProof)?;
        let valid_until_ms = r.u64()?;
        let last_known_chain = r.bytes()?;

        let ring_len = usize::try_from(r.u32()?).map_err(|_| DecodeFailure::LengthOutOfRange)?;
        if ring_len > MAX_RING_SIZE {
            return Err(DecodeFailure::LimitExceeded.into());
        }
        let mut ring = Vec::with_capacity(ring_len);
        for _ in 0..ring_len {
            ring.push(r.field_element()?);
        }
        if !ring_is_canonical(&ring) {
            return Err(DecodeFailure::NoncanonicalRingOrder.into());
        }

        // The memo follows the binding fields, mirroring the transcript's
        // own layout: binding transcript first, then the memo, then the
        // signature. Decoding in transcript order is what lets a decoder's
        // output re-derive byte-identical transcripts.
        let memo = SealedMemo::read_from(&mut r)?;

        let challenge = r.field_element()?;
        let response_len =
            usize::try_from(r.u32()?).map_err(|_| DecodeFailure::LengthOutOfRange)?;
        if response_len > MAX_RING_SIZE {
            return Err(DecodeFailure::LimitExceeded.into());
        }
        let mut responses = Vec::with_capacity(response_len);
        for _ in 0..response_len {
            responses.push(r.field_element()?);
        }
        let key_image = r.field_element()?;
        r.finish()?;

        Ok(Self {
            network_id,
            output,
            amount_commitment,
            range_proof,
            memo,
            valid_until_ms,
            last_known_chain,
            ring,
            signature: RingSignature {
                challenge,
                responses,
                key_image,
            },
        })
    }
}

/// Whether a ring is sorted and free of duplicates.
///
/// Canonical ordering means one payment has one encoding; deduplication
/// means the ring size a verifier counts is the anonymity it actually
/// provides. A ring padded with one key repeated eight times looks like a
/// ring of eight and hides nobody.
pub fn ring_is_canonical(ring: &[Vec<u8>]) -> bool {
    ring.windows(2).all(|pair| pair[0] < pair[1])
}

/// Sort and deduplicate a caller's ring into canonical order.
pub fn canonicalize_ring(ring: &mut Vec<Vec<u8>>) {
    ring.sort();
    ring.dedup();
}

/// What a sender supplies to build a payment.
#[derive(Debug, Clone)]
pub struct PaymentRequest {
    pub network_id: [u8; 32],
    /// The recipient's published stealth spend key.
    pub recipient_spend_public: Vec<u8>,
    /// The recipient's published stealth view key.
    pub recipient_view_public: Vec<u8>,
    /// The amount in micro-MINI. Committed to, never published.
    pub amount_micro: u64,
    /// What the payment is for. Sealed to the recipient.
    pub purpose: PaymentPurpose,
    pub valid_until_ms: u64,
    pub last_known_chain: Vec<u8>,
    /// How many members the anonymity set should have. Must be at least
    /// [`MIN_RING_SIZE`].
    ///
    /// The *members* are not a caller choice. [`crate::select_ring`] picks
    /// them under the protocol's one sampling rule, because a per-wallet
    /// choice fingerprints that wallet's users — see [`crate::decoy`].
    pub ring_size: usize,
    /// Index of the sender's real output within `outputs`.
    pub real_output_index: usize,
    /// The sender's one-time secret key for that output.
    pub secret_key: Vec<u8>,
    /// Fresh per payment. Reusing it reproduces the same ring, and two
    /// payments sharing a ring are visibly related.
    pub decoy_entropy: [u8; 32],
    /// Blinding factor for the amount commitment. Must be fresh per
    /// payment: a reused blinding makes two equal amounts produce equal
    /// commitments, which is a linkability channel as loud as publishing
    /// the amount.
    pub blinding: [u8; 32],
}

/// Build a signed private payment.
///
/// The order matters and is not arbitrary: the stealth output and its
/// shared secret come first, then the amount commitment, then the memo is
/// sealed against a transcript that already contains both — so the memo is
/// bound to a claim that cannot subsequently change without invalidating
/// it. Sealing before committing would leave the memo transplantable.
pub fn build(
    request: &PaymentRequest,
    outputs: &impl crate::OutputSet,
) -> Result<(PrivatePaymentClaim, StealthSharedSecret)> {
    // The ring is chosen here, by the protocol's rule, from the caller's
    // local output set. It is not a caller parameter: a wallet that samples
    // differently from its peers marks its own users, so this cannot be an
    // implementation choice even when the implementation means well.
    let (ring, secret_index) = crate::select_ring(
        outputs,
        request.real_output_index,
        request.ring_size,
        &request.decoy_entropy,
    )?;

    let (output, shared) = mini_value::derive_output_with_secret(
        &request.recipient_spend_public,
        &request.recipient_view_public,
    )
    .ok_or(PrivatePaymentError::CryptoUnavailable)?;

    let mut confidential = MininetConfidentialAmount;
    let (amount_commitment, range_proof) = confidential
        .commit_with_proof(request.amount_micro, &request.blinding)
        .ok_or(PrivatePaymentError::CryptoUnavailable)?;

    // An empty placeholder memo, present only so the claim can compute its
    // own binding transcript. That transcript excludes the memo, so this
    // value never reaches the digest the memo is sealed against.
    let mut claim = PrivatePaymentClaim {
        network_id: request.network_id,
        output,
        amount_commitment,
        range_proof,
        memo: SealedMemo {
            ciphertext: Vec::new(),
        },
        valid_until_ms: request.valid_until_ms,
        last_known_chain: request.last_known_chain.clone(),
        ring,
        signature: RingSignature {
            challenge: Vec::new(),
            responses: Vec::new(),
            key_image: Vec::new(),
        },
    };

    let binding = claim.binding_digest();
    claim.memo = SealedMemo::seal(&request.purpose, &shared, &binding)?;
    debug_assert_eq!(
        claim.binding_digest(),
        binding,
        "the memo must never be part of its own binding"
    );

    // Signed last, over the full transcript -- which does include the
    // memo, so a swapped or stripped memo breaks the signature.
    let mut scheme = MininetRingSignature::new(secret_index, &request.secret_key)
        .ok_or(PrivatePaymentError::CryptoUnavailable)?;
    let signature = scheme
        .sign(&claim.ring, &claim.transcript())
        .ok_or(PrivatePaymentError::CryptoUnavailable)?;
    claim.signature = signature;

    Ok((claim, shared))
}

/// Verify a private payment completely.
///
/// Checks run cheapest-first so a malformed or obviously-unsafe claim
/// costs a verifier no curve arithmetic:
/// 1. network binding,
/// 2. ring size, shape, and canonical order,
/// 3. the range proof (is the amount even a number?),
/// 4. the ring signature over the exact transcript,
/// 5. the key image the claim carries against the one the signature proves.
pub fn verify(claim: &PrivatePaymentClaim, network_id: &[u8; 32]) -> Result<VerifiedPrivateClaim> {
    if &claim.network_id != network_id {
        return Err(PrivatePaymentError::NetworkMismatch);
    }
    if claim.ring.len() < MIN_RING_SIZE {
        return Err(PrivatePaymentError::RingTooSmall {
            got: claim.ring.len(),
            min: MIN_RING_SIZE,
        });
    }
    if claim.ring.len() > MAX_RING_SIZE {
        return Err(PrivatePaymentError::RingTooLarge {
            got: claim.ring.len(),
            max: MAX_RING_SIZE,
        });
    }
    if !ring_is_canonical(&claim.ring) {
        // Sorted-and-distinct is one check; report the duplicate case
        // separately because it is an anonymity claim, not a formatting one.
        let mut sorted = claim.ring.clone();
        sorted.sort();
        let mut deduped = sorted.clone();
        deduped.dedup();
        if deduped.len() != sorted.len() {
            return Err(PrivatePaymentError::DuplicateRingMember);
        }
        return Err(DecodeFailure::NoncanonicalRingOrder.into());
    }
    if claim.signature.responses.len() != claim.ring.len() {
        return Err(PrivatePaymentError::BadRingSignature);
    }
    if claim.amount_commitment.len() != 32 || claim.signature.key_image.len() != 32 {
        return Err(DecodeFailure::BadFieldElement.into());
    }
    if claim.range_proof.to_bytes().len() != RANGE_PROOF_BYTES {
        return Err(DecodeFailure::BadRangeProof.into());
    }

    let confidential = MininetConfidentialAmount;
    if !confidential.verify_range_proof(&claim.amount_commitment, &claim.range_proof) {
        return Err(PrivatePaymentError::BadRangeProof);
    }

    let transcript = claim.transcript();
    let scheme = MininetRingSignature::verifier();
    if !scheme.verify(&claim.ring, &transcript, &claim.signature) {
        return Err(PrivatePaymentError::BadRingSignature);
    }

    Ok(VerifiedPrivateClaim {
        claim: claim.clone(),
        transcript_digest: HashAlgorithm::Blake3.digest(&transcript),
        binding_digest: claim.binding_digest(),
    })
}
