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
    public_amount_commitment, reblind, sign_spend, verify_spend, ConfidentialAmountScheme,
    MininetConfidentialAmount, MlsagSignature, RangeProof, SpendWitness, StealthOutput,
    StealthSharedSecret, RANGE_PROOF_BYTES,
};

use crate::codec::{Reader, Writer};
use crate::error::{DecodeFailure, PrivatePaymentError, Result};
use crate::memo::{PaymentNote, PaymentPurpose, SealedMemo};

/// Domain separator for the claim transcript — the bytes a ring signature
/// actually signs.
pub const CLAIM_TRANSCRIPT_DOMAIN: &[u8] = b"mininet/mini-private-payment/claim/v2";

/// Wire format version. A decoder that does not recognize this refuses the
/// claim rather than guessing at a layout.
pub const CLAIM_VERSION: u8 = 2;

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

/// Most inputs one claim may spend. Verification cost is
/// `inputs × ring_size` scalar multiplications, so the two bounds multiply
/// — an unbounded input count would defeat [`MAX_RING_SIZE`]'s purpose.
pub const MAX_INPUTS: usize = 16;

/// Most outputs one claim may create, bounding both verification cost and
/// the scanning work every recipient in the network performs per claim.
pub const MAX_OUTPUTS: usize = 16;

/// One spent input: a ring of candidate outputs, the re-blinded
/// commitment that enters the balance equation, and the proof tying them
/// together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimInput {
    /// One-time public keys, canonically sorted and deduplicated.
    pub ring: Vec<Vec<u8>>,
    /// The amount commitments of those same outputs, in the same order.
    /// Parallel to `ring`: member `j` is `(ring[j], ring_commitments[j])`.
    pub ring_commitments: Vec<Vec<u8>>,
    /// A commitment to the spent value under a fresh blinding factor.
    ///
    /// This is what appears in the balance sum. The real member's own
    /// commitment cannot, because publishing it would say which member was
    /// real and the ring would stop hiding anything.
    pub pseudo_commitment: Vec<u8>,
    /// Proof that the signer controls one ring member **and** that
    /// `pseudo_commitment` hides that member's value.
    pub signature: MlsagSignature,
}

/// One created output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimOutput {
    /// The fresh one-time address this output pays.
    pub output: StealthOutput,
    /// Pedersen commitment to the amount.
    pub amount_commitment: Vec<u8>,
    /// Bulletproof that the committed amount is in `[0, 2^64)`.
    ///
    /// Without it a "negative" amount would balance the equation while
    /// minting value — the range proof is what makes conservation mean
    /// anything.
    pub range_proof: RangeProof,
    /// Purpose **and** the commitment opening, sealed to this output's
    /// recipient. See [`PaymentNote`] for why the opening travels here.
    pub memo: SealedMemo,
}

/// A payment that hides its payer, payee, amounts, and purpose, and proves
/// it created no value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivatePaymentClaim {
    /// Exact settlement network. A claim built for a test network must
    /// never be replayable onto the real one.
    pub network_id: [u8; 32],
    /// The outputs being spent, each hidden in its own ring.
    pub inputs: Vec<ClaimInput>,
    /// The outputs being created — recipients and change alike. Change is
    /// not a special case: it is an output paying yourself, which is what
    /// keeps it indistinguishable from any other output.
    pub outputs: Vec<ClaimOutput>,
    /// The fee, in the clear.
    ///
    /// Public on purpose. A fee must be checkable — a verifier has to
    /// confirm the fee charged is the fee declared — and a hidden fee would
    /// need its own range proof and still leave the network unable to
    /// prioritize. The cost is stated rather than hidden: see the crate
    /// docs on what a public fee leaks.
    pub fee_micro: u64,
    /// The claim expires if it has not reached canonical inclusion by this
    /// device-clock time, in ms. Self-reported, like everywhere else in
    /// this tree that lacks a time anchor.
    pub valid_until_ms: u64,
    /// Opaque reference to the canonical chain state the payer last
    /// observed.
    pub last_known_chain: Vec<u8>,
}

/// A claim that has passed every structural and cryptographic check.
///
/// Constructing one outside [`verify`] is impossible: the fields are
/// private and there is no public constructor.
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

    /// The double-spend nullifiers, one per input.
    ///
    /// Each is deterministic in the one-time key it spends, so the same
    /// output can never be spent twice — see this module's docs on what
    /// that costs in linkability.
    pub fn key_images(&self) -> impl Iterator<Item = &[u8]> {
        self.claim
            .inputs
            .iter()
            .map(|input| input.signature.key_image.as_slice())
    }

    /// The digest every input's spend proof committed to — this claim's
    /// identity, and the key a ledger records it under.
    pub fn transcript_digest(&self) -> &[u8; 32] {
        &self.transcript_digest
    }

    /// The claim-wide AAD root the memos were sealed under.
    pub fn binding_digest(&self) -> &[u8; 32] {
        &self.binding_digest
    }

    /// Open output `index`'s memo, if it is addressed to the holder of
    /// `shared`.
    pub fn open_memo(&self, index: usize, shared: &StealthSharedSecret) -> Result<PaymentNote> {
        let output = self
            .claim
            .outputs
            .get(index)
            .ok_or(PrivatePaymentError::MalformedMemo)?;
        output
            .memo
            .open(shared, &output_binding_digest(&self.binding_digest, index))
    }

    /// Fabricate the one claim shape [`build`] cannot produce: a payment
    /// that really is addressed to a recipient, whose memo that recipient
    /// cannot open.
    ///
    /// Reachable in the wild, unreachable through this crate. A hostile
    /// *encoder* can derive a correct stealth output, seal the memo under a
    /// key the recipient will never derive, and sign that transcript
    /// honestly; the result verifies, is recognized, and will not open.
    /// Crate-internal and test-only.
    #[cfg(test)]
    pub(crate) fn fabricate_unopenable_memo(mut self) -> Self {
        let tail = self.claim.outputs[0].memo.ciphertext.len() - 1;
        self.claim.outputs[0].memo.ciphertext[tail] ^= 0xff;
        self
    }
}

/// Per-output memo AAD: the claim-wide binding digest, plus the output's
/// own index.
///
/// Binding to the claim alone would let a memo move between outputs of the
/// same claim. Those memos are sealed under different shared secrets so
/// neither would open — but "fails for the wrong reason" is not a security
/// argument, and an index costs four bytes.
fn output_binding_digest(binding: &[u8; 32], index: usize) -> [u8; 32] {
    let mut w = Writer::new();
    w.raw(binding);
    w.u32(index as u32);
    HashAlgorithm::Blake3.digest(&w.finish())
}

impl PrivatePaymentClaim {
    /// Everything the memos are bound to: every field **except** the memos
    /// themselves and the spend proofs.
    ///
    /// A memo cannot be sealed against the full transcript, because the
    /// full transcript contains the memos — the two would define each
    /// other. Splitting resolves that without weakening either binding:
    /// the memo is sealed with this digest (plus its output index) as AEAD
    /// additional data, so it cannot be moved onto a claim paying a
    /// different address; and the spend proofs cover [`Self::transcript`],
    /// which does include the memos, so a memo cannot be swapped or
    /// stripped either.
    pub fn binding_transcript(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(CLAIM_TRANSCRIPT_DOMAIN);
        w.u8(CLAIM_VERSION);
        w.raw(&self.network_id);
        w.u64(self.fee_micro);
        w.u64(self.valid_until_ms);
        w.bytes(&self.last_known_chain);
        w.u32(self.inputs.len() as u32);
        for input in &self.inputs {
            w.u32(input.ring.len() as u32);
            for member in &input.ring {
                w.bytes(member);
            }
            for commitment in &input.ring_commitments {
                w.bytes(commitment);
            }
            w.bytes(&input.pseudo_commitment);
        }
        w.u32(self.outputs.len() as u32);
        for output in &self.outputs {
            w.bytes(&output.output.tx_public_key);
            w.bytes(&output.output.one_time_address);
            w.bytes(&output.amount_commitment);
            w.bytes(&output.range_proof.to_bytes());
        }
        w.finish()
    }

    /// BLAKE3 of [`Self::binding_transcript`].
    pub fn binding_digest(&self) -> [u8; 32] {
        HashAlgorithm::Blake3.digest(&self.binding_transcript())
    }

    /// The exact bytes every input's spend proof signs: the binding
    /// transcript plus every memo.
    ///
    /// All inputs sign the same message. That is what makes them one
    /// transaction rather than several: an input's proof cannot be lifted
    /// into a different claim, because the claim's whole shape — its other
    /// inputs, its outputs, its fee — is inside what it signed.
    pub fn transcript(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(&self.binding_transcript());
        for output in &self.outputs {
            output.memo.write_into(&mut w);
        }
        w.finish()
    }

    /// BLAKE3 of [`Self::transcript`].
    pub fn transcript_digest(&self) -> [u8; 32] {
        HashAlgorithm::Blake3.digest(&self.transcript())
    }

    /// Canonical wire encoding.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(&self.transcript());
        for input in &self.inputs {
            w.bytes(&input.signature.challenge);
            w.u32(input.signature.key_responses.len() as u32);
            for response in &input.signature.key_responses {
                w.bytes(response);
            }
            for response in &input.signature.blinding_responses {
                w.bytes(response);
            }
            w.bytes(&input.signature.key_image);
        }
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
        let fee_micro = r.u64()?;
        let valid_until_ms = r.u64()?;
        let last_known_chain = r.bytes()?;

        let input_count = bounded(r.u32()?, MAX_INPUTS)?;
        let mut partial_inputs = Vec::with_capacity(input_count);
        for _ in 0..input_count {
            let ring_size = bounded(r.u32()?, MAX_RING_SIZE)?;
            let mut ring = Vec::with_capacity(ring_size);
            for _ in 0..ring_size {
                ring.push(r.field_element()?);
            }
            let mut ring_commitments = Vec::with_capacity(ring_size);
            for _ in 0..ring_size {
                ring_commitments.push(r.field_element()?);
            }
            let pseudo_commitment = r.field_element()?;
            partial_inputs.push((ring, ring_commitments, pseudo_commitment));
        }

        let output_count = bounded(r.u32()?, MAX_OUTPUTS)?;
        let mut partial_outputs = Vec::with_capacity(output_count);
        for _ in 0..output_count {
            let tx_public_key = r.field_element()?;
            let one_time_address = r.field_element()?;
            let amount_commitment = r.field_element()?;
            let proof_bytes = r.bytes()?;
            if proof_bytes.len() != RANGE_PROOF_BYTES {
                return Err(DecodeFailure::BadRangeProof.into());
            }
            let range_proof =
                RangeProof::from_bytes(&proof_bytes).ok_or(DecodeFailure::BadRangeProof)?;
            partial_outputs.push((
                StealthOutput {
                    tx_public_key,
                    one_time_address,
                },
                amount_commitment,
                range_proof,
            ));
        }

        let mut outputs = Vec::with_capacity(output_count);
        for (output, amount_commitment, range_proof) in partial_outputs {
            outputs.push(ClaimOutput {
                output,
                amount_commitment,
                range_proof,
                memo: SealedMemo::read_from(&mut r)?,
            });
        }

        let mut inputs = Vec::with_capacity(input_count);
        for (ring, ring_commitments, pseudo_commitment) in partial_inputs {
            let challenge = r.field_element()?;
            let response_count = bounded(r.u32()?, MAX_RING_SIZE)?;
            if response_count != ring.len() {
                return Err(DecodeFailure::LimitExceeded.into());
            }
            let mut key_responses = Vec::with_capacity(response_count);
            for _ in 0..response_count {
                key_responses.push(r.field_element()?);
            }
            let mut blinding_responses = Vec::with_capacity(response_count);
            for _ in 0..response_count {
                blinding_responses.push(r.field_element()?);
            }
            let key_image = r.field_element()?;
            inputs.push(ClaimInput {
                ring,
                ring_commitments,
                pseudo_commitment,
                signature: MlsagSignature {
                    challenge,
                    key_responses,
                    blinding_responses,
                    key_image,
                },
            });
        }
        r.finish()?;

        for input in &inputs {
            if !crate::ring_is_canonical(&input.ring) {
                return Err(DecodeFailure::NoncanonicalRingOrder.into());
            }
        }

        Ok(Self {
            network_id,
            inputs,
            outputs,
            fee_micro,
            valid_until_ms,
            last_known_chain,
        })
    }
}

fn bounded(count: u32, max: usize) -> Result<usize> {
    let value = usize::try_from(count).map_err(|_| DecodeFailure::LengthOutOfRange)?;
    if value > max {
        return Err(DecodeFailure::LimitExceeded.into());
    }
    Ok(value)
}

/// An output this wallet controls and is about to spend.
#[derive(Clone)]
pub struct SpendableOutput {
    /// Where this output sits in the caller's local [`crate::OutputSet`].
    pub set_index: usize,
    /// The one-time private key that opens it.
    pub one_time_secret: [u8; 32],
    /// Its value — from the [`PaymentNote`] the sender sealed.
    pub value_micro: u64,
    /// Its blinding factor — from the same note.
    pub blinding: [u8; 32],
}

impl core::fmt::Debug for SpendableOutput {
    /// Redacted: this struct is entirely spending material.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SpendableOutput(<redacted>)")
    }
}

/// One party being paid — including yourself, when the output is change.
#[derive(Clone)]
pub struct Recipient {
    pub spend_public: Vec<u8>,
    pub view_public: Vec<u8>,
    pub amount_micro: u64,
    pub purpose: PaymentPurpose,
}

impl core::fmt::Debug for Recipient {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Recipient(<redacted>)")
    }
}

/// Everything needed to build one claim.
///
/// Change is **not** a field. A wallet that wants change adds itself to
/// `recipients`, which is what makes change indistinguishable from any
/// other output — a dedicated change field would mark one output as the
/// payer's on every claim in the network.
#[derive(Debug, Clone)]
pub struct PaymentRequest {
    pub network_id: [u8; 32],
    /// The outputs being spent.
    pub spends: Vec<SpendableOutput>,
    /// The outputs being created, change included.
    pub recipients: Vec<Recipient>,
    /// The fee, which is public.
    pub fee_micro: u64,
    /// Ring size for every input. One value for all of them: per-input
    /// ring sizes would be a per-claim fingerprint.
    pub ring_size: usize,
    pub valid_until_ms: u64,
    pub last_known_chain: Vec<u8>,
    /// Per-claim decoy entropy. The protocol picks the members; this only
    /// seeds the draw (D-0449).
    pub decoy_entropy: [u8; 32],
}

/// What the payer keeps after building a claim: enough to recognize and
/// later spend each output they created for themselves.
#[derive(Debug)]
pub struct BuiltOutput {
    /// The shared secret for this output, so the payer can open its memo.
    pub shared: StealthSharedSecret,
    /// The blinding factor sealed into the note.
    pub blinding: [u8; 32],
}

/// Build a claim.
///
/// The value equation is enforced here before anything is signed:
/// `Σ spends = Σ recipients + fee`. A caller whose numbers do not add up
/// gets [`PrivatePaymentError::UnbalancedAmounts`] rather than a claim that
/// no verifier will accept.
pub fn build(
    request: &PaymentRequest,
    outputs: &impl crate::OutputSet,
) -> Result<(PrivatePaymentClaim, Vec<BuiltOutput>)> {
    if request.spends.is_empty() || request.spends.len() > MAX_INPUTS {
        return Err(PrivatePaymentError::InputCountOutOfRange {
            got: request.spends.len(),
            max: MAX_INPUTS,
        });
    }
    if request.recipients.is_empty() || request.recipients.len() > MAX_OUTPUTS {
        return Err(PrivatePaymentError::OutputCountOutOfRange {
            got: request.recipients.len(),
            max: MAX_OUTPUTS,
        });
    }

    // Conservation, checked in integers before any curve arithmetic.
    // Saturating sums would hide an overflow as a balanced claim, so these
    // are checked adds.
    let spent = request
        .spends
        .iter()
        .try_fold(0u64, |acc, s| acc.checked_add(s.value_micro))
        .ok_or(PrivatePaymentError::UnbalancedAmounts)?;
    let paid = request
        .recipients
        .iter()
        .try_fold(0u64, |acc, r| acc.checked_add(r.amount_micro))
        .ok_or(PrivatePaymentError::UnbalancedAmounts)?;
    let required = paid
        .checked_add(request.fee_micro)
        .ok_or(PrivatePaymentError::UnbalancedAmounts)?;
    if spent != required {
        return Err(PrivatePaymentError::UnbalancedAmounts);
    }

    // Output commitments first: their blinding factors determine what the
    // last pseudo-commitment's blinding has to be.
    let mut confidential = MininetConfidentialAmount;
    let mut output_blindings = Vec::with_capacity(request.recipients.len());
    let mut built = Vec::with_capacity(request.recipients.len());
    let mut claim_outputs = Vec::with_capacity(request.recipients.len());
    for recipient in &request.recipients {
        let blinding =
            mini_crypto::random_32().map_err(|_| PrivatePaymentError::CryptoUnavailable)?;
        let (output, shared) =
            mini_value::derive_output_with_secret(&recipient.spend_public, &recipient.view_public)
                .ok_or(PrivatePaymentError::CryptoUnavailable)?;
        let (amount_commitment, range_proof) = confidential
            .commit_with_proof(recipient.amount_micro, &blinding)
            .ok_or(PrivatePaymentError::CryptoUnavailable)?;
        output_blindings.push(blinding);
        claim_outputs.push(ClaimOutput {
            output,
            amount_commitment,
            range_proof,
            // Placeholder; sealed below once the binding digest exists.
            memo: SealedMemo {
                ciphertext: Vec::new(),
            },
        });
        built.push(BuiltOutput { shared, blinding });
    }

    // Pseudo-commitment blindings: free for every input but the last, whose
    // value is forced so that Σ b' = Σ b_out. That single constraint is
    // what makes the public balance sum come out to the identity when, and
    // only when, the amounts conserve.
    let pseudo_blindings = pseudo_blindings_for(request.spends.len(), &output_blindings)?;

    let mut claim_inputs = Vec::with_capacity(request.spends.len());
    let mut witnesses = Vec::with_capacity(request.spends.len());
    for (spend, pseudo_blinding) in request.spends.iter().zip(pseudo_blindings.iter()) {
        let (indices, position) = crate::select_ring_indices(
            outputs,
            spend.set_index,
            request.ring_size,
            &request.decoy_entropy,
        )?;
        let ring = indices
            .iter()
            .map(|index| outputs.key_at(*index))
            .collect::<Option<Vec<_>>>()
            .ok_or(PrivatePaymentError::RealOutputNotInSet)?;
        let ring_commitments = indices
            .iter()
            .map(|index| outputs.commitment_at(*index))
            .collect::<Option<Vec<_>>>()
            .ok_or(PrivatePaymentError::RealOutputNotInSet)?;
        let (pseudo_commitment, blinding_difference) =
            reblind(spend.value_micro, &spend.blinding, pseudo_blinding)
                .ok_or(PrivatePaymentError::CryptoUnavailable)?;
        claim_inputs.push(ClaimInput {
            ring,
            ring_commitments,
            pseudo_commitment: pseudo_commitment.to_vec(),
            signature: MlsagSignature {
                challenge: Vec::new(),
                key_responses: Vec::new(),
                blinding_responses: Vec::new(),
                key_image: Vec::new(),
            },
        });
        witnesses.push(SpendWitness {
            secret_index: position,
            one_time_secret: spend.one_time_secret,
            blinding_difference,
        });
    }

    let mut claim = PrivatePaymentClaim {
        network_id: request.network_id,
        inputs: claim_inputs,
        outputs: claim_outputs,
        fee_micro: request.fee_micro,
        valid_until_ms: request.valid_until_ms,
        last_known_chain: request.last_known_chain.clone(),
    };

    // Memos next: sealed against the binding digest, which excludes them.
    let binding = claim.binding_digest();
    for (index, recipient) in request.recipients.iter().enumerate() {
        let note = PaymentNote::new(
            recipient.purpose.clone(),
            recipient.amount_micro,
            built[index].blinding,
        );
        claim.outputs[index].memo = SealedMemo::seal(
            &note,
            &built[index].shared,
            &output_binding_digest(&binding, index),
        )?;
    }
    debug_assert_eq!(
        claim.binding_digest(),
        binding,
        "the memos must never be part of their own binding"
    );

    // Signed last, over the full transcript -- which does include the
    // memos, so a swapped or stripped memo breaks every input's proof.
    let message = claim.transcript();
    for (index, witness) in witnesses.iter().enumerate() {
        let input = &claim.inputs[index];
        let signature = sign_spend(
            &input.ring,
            &input.ring_commitments,
            &input.pseudo_commitment,
            &message,
            witness,
        )
        .ok_or(PrivatePaymentError::CryptoUnavailable)?;
        claim.inputs[index].signature = signature;
    }

    Ok((claim, built))
}

/// Blinding factors for the pseudo-commitments: random for all but the
/// last, which absorbs whatever is needed for the sum to match the
/// outputs'.
fn pseudo_blindings_for(count: usize, output_blindings: &[[u8; 32]]) -> Result<Vec<[u8; 32]>> {
    let mut chosen = Vec::with_capacity(count);
    for _ in 0..count.saturating_sub(1) {
        chosen.push(mini_crypto::random_32().map_err(|_| PrivatePaymentError::CryptoUnavailable)?);
    }
    chosen.push(mini_value::balancing_blinding(output_blindings, &chosen));
    Ok(chosen)
}

/// Verify a private payment completely.
///
/// Checks run cheapest-first so a malformed or obviously-unsafe claim costs
/// a verifier no curve arithmetic:
///
/// 1. network binding,
/// 2. input/output counts and every ring's size, shape and canonical order,
/// 3. every output's range proof — is each amount even a number?
/// 4. **conservation**: `Σ pseudo-commitments = Σ output commitments + fee`,
/// 5. every input's spend proof over the exact transcript,
/// 6. key images distinct within the claim.
///
/// Steps 3 and 4 are worthless apart and only meaningful together. The
/// balance sum alone is satisfied by a "negative" output that mints value;
/// the range proofs alone say every amount is a number without saying they
/// add up. Step 6 catches a claim spending the same output twice inside
/// itself — cross-claim double spends are [`crate::KeyImageSet`]'s job,
/// because they need state this function does not have.
pub fn verify(claim: &PrivatePaymentClaim, network_id: &[u8; 32]) -> Result<VerifiedPrivateClaim> {
    if &claim.network_id != network_id {
        return Err(PrivatePaymentError::NetworkMismatch);
    }
    if claim.inputs.is_empty() || claim.inputs.len() > MAX_INPUTS {
        return Err(PrivatePaymentError::InputCountOutOfRange {
            got: claim.inputs.len(),
            max: MAX_INPUTS,
        });
    }
    if claim.outputs.is_empty() || claim.outputs.len() > MAX_OUTPUTS {
        return Err(PrivatePaymentError::OutputCountOutOfRange {
            got: claim.outputs.len(),
            max: MAX_OUTPUTS,
        });
    }

    for input in &claim.inputs {
        if input.ring.len() < MIN_RING_SIZE {
            return Err(PrivatePaymentError::RingTooSmall {
                got: input.ring.len(),
                min: MIN_RING_SIZE,
            });
        }
        if input.ring.len() > MAX_RING_SIZE {
            return Err(PrivatePaymentError::RingTooLarge {
                got: input.ring.len(),
                max: MAX_RING_SIZE,
            });
        }
        if input.ring_commitments.len() != input.ring.len() {
            return Err(DecodeFailure::LimitExceeded.into());
        }
        if !crate::ring_is_canonical(&input.ring) {
            return Err(PrivatePaymentError::DuplicateRingMember);
        }
    }

    let confidential = MininetConfidentialAmount;
    for output in &claim.outputs {
        if !confidential.verify_range_proof(&output.amount_commitment, &output.range_proof) {
            return Err(PrivatePaymentError::BadRangeProof);
        }
    }

    // Conservation. The fee enters as a commitment to a publicly known
    // amount with a zero blinding factor, which is what makes it auditable:
    // a verifier recomputes it from the cleartext `fee_micro` and would
    // reject any other value.
    let pseudo: Vec<Vec<u8>> = claim
        .inputs
        .iter()
        .map(|input| input.pseudo_commitment.clone())
        .collect();
    let mut sinks: Vec<Vec<u8>> = claim
        .outputs
        .iter()
        .map(|output| output.amount_commitment.clone())
        .collect();
    sinks.push(public_amount_commitment(claim.fee_micro).to_vec());
    if !confidential.verify_balance(&pseudo, &sinks) {
        return Err(PrivatePaymentError::UnbalancedAmounts);
    }

    let message = claim.transcript();
    for input in &claim.inputs {
        if !verify_spend(
            &input.ring,
            &input.ring_commitments,
            &input.pseudo_commitment,
            &message,
            &input.signature,
        ) {
            return Err(PrivatePaymentError::BadSpendProof);
        }
    }

    // A claim that spends the same output twice within itself would pass
    // every check above -- both proofs are valid, and the balance sums
    // happily -- while creating value from one output counted twice.
    for (index, input) in claim.inputs.iter().enumerate() {
        if claim.inputs[..index]
            .iter()
            .any(|earlier| earlier.signature.key_image == input.signature.key_image)
        {
            return Err(PrivatePaymentError::RepeatedKeyImage);
        }
    }

    Ok(VerifiedPrivateClaim {
        transcript_digest: claim.transcript_digest(),
        binding_digest: claim.binding_digest(),
        claim: claim.clone(),
    })
}

/// Sort and deduplicate a ring, and its parallel commitments with it.
///
/// Canonical order means one membership has exactly one encoding, so the
/// same payment cannot be re-serialized into a different transcript — and
/// the draw order, which would leak where the real member landed, never
/// reaches the wire.
///
/// Both vectors are taken together on purpose. A ring member is the *pair*
/// `(key, commitment)`, so sorting the keys alone would silently pair every
/// member with a stranger's commitment — a claim that still decodes, still
/// looks canonical, and can no longer be verified by anyone. Making the
/// desynced call impossible to write is cheaper than catching it later.
///
/// Returns `false`, changing nothing, when the two vectors are not the same
/// length — that is a caller bug, and canonicalizing half of it would turn
/// a detectable mistake into a subtle one.
#[must_use]
pub fn canonicalize_ring(ring: &mut Vec<Vec<u8>>, ring_commitments: &mut Vec<Vec<u8>>) -> bool {
    if ring.len() != ring_commitments.len() {
        return false;
    }
    let mut members: Vec<(Vec<u8>, Vec<u8>)> = ring
        .drain(..)
        .zip(ring_commitments.drain(..))
        .collect::<Vec<_>>();
    members.sort_by(|left, right| left.0.cmp(&right.0));
    members.dedup_by(|left, right| left.0 == right.0);
    for (key, commitment) in members {
        ring.push(key);
        ring_commitments.push(commitment);
    }
    true
}

/// Whether a ring is sorted and free of duplicates.
pub fn ring_is_canonical(ring: &[Vec<u8>]) -> bool {
    ring.windows(2).all(|pair| pair[0] < pair[1])
}
