//! `PrivatePaymentV3`: the Gate #72 external cryptography audit's Section
//! 9-11 wire format, three-digest scheme, and consensus-facing shape —
//! [`crate::claim`]'s same private-payment composition (stealth outputs,
//! a two-column MLSAG per input, Bulletproof range proofs, a sealed
//! memo carrying the spendable opening) over the audit's exact byte
//! layout and the [`mini_value::bp_range_v2`]/[`mini_value::mlsag_v3`]/
//! [`mini_value::stealth_v3`] primitives instead of `crate::claim`'s V1
//! basis and framing.
//!
//! # Why additive, not an in-place V2 rewrite
//!
//! `crate::claim::PrivatePaymentClaim` (V2) is real, working, tested code
//! with its own golden wire vectors. Nothing here changes it, exactly
//! the same stance [`mini_value::bp_range_v2`] took toward `bp_range`
//! (D-0518): this module is a new, real, working format available for
//! new callers, not a breaking change to an existing one. `mini-bounty`
//! and `mini-shielded-verify`, which build on V2 today, are untouched.
//!
//! # What V3 removes relative to V2, per the audit
//!
//! - **No caller-selected ring size.** [`RING_SIZE_V3`] is the only ring
//!   size; `crate::claim::PaymentRequest::ring_size` has no V3 analogue
//!   (F72-11).
//! - **No caller-supplied decoy entropy.** [`build_v3`] draws it from the
//!   OS CSPRNG internally (F72-11); nothing in [`PaymentRequestV3`]
//!   exposes a seed.
//! - **No `valid_until_ms`/`last_known_chain`.** A device clock and an
//!   opaque chain hint are not canonical validity anchors (F72-12); V3
//!   claims are pending until canonical finality decides them, exactly
//!   like every other claim in this tree.
//! - **No arbitrary fee bid.** A claim signs `fee_policy_id`; the actual
//!   `fee_micro` is a deterministic function of that policy and the
//!   claim's own shape (F72-13) — see [`quote_fee_micro`]'s own docs for
//!   how provisional that function currently is.
//!
//! # The three digests (Section 10.3), and why one becomes three
//!
//! `crate::claim` has two: `binding_digest` (everything except memos and
//! signatures) and `transcript_digest` (adds memos). V3 adds a third,
//! `claim_id`, that also covers the signatures — the audit's own
//! rationale (Section 10.3/F72-07): a cache, mempool, or block keyed by
//! `crate::claim`'s `transcript_digest` cannot distinguish two
//! differently-signed encodings of the same unsigned transaction, which
//! matters once full claim bytes (not just a derived nullifier list)
//! become canonical block data — see [`PrivatePaymentClaimV3::claim_id`].
//!
//! # What this module does not yet do
//!
//! It is not wired into `mini-shielded-verify`, `mini-execution`, or any
//! consensus path (Gate #72's "canonical claim bytes in consensus" item,
//! Section 11, remains separate follow-up). [`quote_fee_policy_id`]'s fee
//! derivation is an honest placeholder, not a calibrated economic
//! function. Decoy selection uses [`crate::decoy::select_ring_indices_v3`],
//! the audit's OSPEAD log-GB2 recalibration (F72-10) — see that function's
//! docs, and [`crate::decoy::OSPEAD_AGE_WEIGHTS`], for the derivation and
//! an honest disclosure of the one unit-ambiguity judgment call it makes.
//!
//! [FREEZE reminder — D-0036/D-0037/D-0047] Founder-overridden,
//! AI-authored prototype. Unaudited. Nothing value-bearing may depend on
//! this before Gate #72 closes.

use mini_crypto::HashAlgorithm;
use mini_value::{
    balancing_blinding_v3, derive_output_v3, prove_range_v2_from_bytes,
    public_amount_commitment_v2, reblind_v3, recover_and_verify_v3, sign_spend_v3,
    verify_balance_v2, verify_range_v2, verify_spend_v3, MlsagSignatureV3, RangeProofV2,
    SpendWitness, StealthSharedSecret, MLSAG_V3_RING_SIZE, RANGE_PROOF_V2_BYTES,
};

use crate::codec::{Reader, Writer};
use crate::decoy::select_ring_indices_v3;
use crate::error::{DecodeFailure, PrivatePaymentError, Result};
use crate::memo::{PaymentNote, PaymentPurpose};
use crate::memo_v3::{MemoContextV3, SealedMemoV3};
use crate::OutputSet;

/// Fixed ASCII header magic (Section 9).
pub const CLAIM_V3_MAGIC: &[u8] = b"mininet-private-payment";

/// Wire format version.
pub const CLAIM_V3_VERSION: u8 = 3;

/// The audit's Section 5.1 suite identifier for
/// `MININET_VALUE_V3_RISTRETTO`.
pub const SUITE_ID_V3: u16 = 0x0003;

/// The one and only ring size (Section 7.2/9): re-exported from
/// [`mini_value::mlsag_v3`] so the two crates cannot silently drift.
pub const RING_SIZE_V3: usize = MLSAG_V3_RING_SIZE;

/// Most inputs one V3 claim may spend (Section 9: `input_count u8, 1..16`).
pub const MAX_INPUTS_V3: usize = 16;

/// Most outputs one V3 claim may create (Section 9: `output_count u8, 1..16`).
pub const MAX_OUTPUTS_V3: usize = 16;

/// Maximum encoded size of one standalone V3 claim (Section 9).
pub const MAX_CLAIM_V3_BYTES: usize = 65_536;

const MEMO_CONTEXT_DOMAIN: &[u8] = b"mininet/private-payment/memo-context/v3";
const SIGNING_DOMAIN: &[u8] = b"mininet/private-payment/signing/v3";
const CLAIM_ID_DOMAIN: &[u8] = b"mininet/private-payment/claim-id/v3";

/// One spent input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimInputV3 {
    /// One-time public keys, canonically sorted and deduplicated, exactly
    /// [`RING_SIZE_V3`] of them.
    pub ring: Vec<Vec<u8>>,
    /// Parallel amount commitments, same order as `ring`.
    pub ring_commitments: Vec<Vec<u8>>,
    /// The re-blinded commitment entering the balance equation.
    pub pseudo_commitment: Vec<u8>,
    /// Proof of ring membership and matching commitment value.
    pub signature: MlsagSignatureV3,
}

/// One created output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimOutputV3 {
    /// `R = r*G`, the sender's ephemeral transaction key.
    pub tx_public_key: Vec<u8>,
    /// `P`, the fresh one-time address.
    pub one_time_address: Vec<u8>,
    /// Pedersen commitment to the amount, over the [`mini_value::
    /// bp_range_v2`] basis.
    pub amount_commitment: Vec<u8>,
    /// Bulletproof that the committed amount is in `[0, 2^64)`.
    pub range_proof: RangeProofV2,
    /// The opening and purpose, sealed to this output's recipient.
    pub sealed_memo: SealedMemoV3,
}

/// A `PrivatePaymentV3` claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivatePaymentClaimV3 {
    pub network_id: [u8; 32],
    /// The signed fee policy identifier (Section 9/11.5) — not an
    /// arbitrary bid. See [`quote_fee_micro`] for how the actual fee
    /// amount is derived.
    pub fee_policy_id: [u8; 32],
    pub inputs: Vec<ClaimInputV3>,
    pub outputs: Vec<ClaimOutputV3>,
}

fn write_header(
    w: &mut Writer,
    network_id: &[u8; 32],
    fee_policy_id: &[u8; 32],
    input_count: u8,
    output_count: u8,
) {
    w.raw(CLAIM_V3_MAGIC);
    w.u8(CLAIM_V3_VERSION);
    w.raw(&SUITE_ID_V3.to_be_bytes());
    w.raw(network_id);
    w.raw(fee_policy_id);
    w.u8(input_count);
    w.u8(output_count);
}

fn write_input_unsigned(w: &mut Writer, input: &ClaimInputV3) {
    w.u8(RING_SIZE_V3 as u8);
    for key in &input.ring {
        w.raw(key);
    }
    for commitment in &input.ring_commitments {
        w.raw(commitment);
    }
    w.raw(&input.pseudo_commitment);
}

fn write_input_signature(w: &mut Writer, signature: &MlsagSignatureV3) {
    w.raw(&signature.challenge);
    for response in &signature.key_responses {
        w.raw(response);
    }
    for response in &signature.blinding_responses {
        w.raw(response);
    }
    w.raw(&signature.key_image);
}

fn write_output_unsigned(w: &mut Writer, output: &ClaimOutputV3) {
    w.raw(&output.tx_public_key);
    w.raw(&output.one_time_address);
    w.raw(&output.amount_commitment);
    w.raw(&output.range_proof.to_bytes());
}

impl PrivatePaymentClaimV3 {
    fn header_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        write_header(
            &mut w,
            &self.network_id,
            &self.fee_policy_id,
            self.inputs.len() as u8,
            self.outputs.len() as u8,
        );
        w.finish()
    }

    /// Section 10.2's memo binding: everything that exists before any
    /// memo is sealed (the header and every input/output's unsigned
    /// public fields), plus this specific output's index.
    pub fn memo_context_digest(&self, output_index: u32) -> [u8; 32] {
        let mut w = Writer::new();
        w.raw(MEMO_CONTEXT_DOMAIN);
        w.raw(&self.header_bytes());
        for input in &self.inputs {
            write_input_unsigned(&mut w, input);
        }
        for output in &self.outputs {
            write_output_unsigned(&mut w, output);
        }
        w.u32(output_index);
        HashAlgorithm::Blake3.digest(&w.finish())
    }

    /// Section 10.3.B: everything through the sealed memos, excluding
    /// every input's MLSAG signature — what each input's signature
    /// actually authorizes.
    pub fn signing_digest(&self) -> [u8; 32] {
        let mut w = Writer::new();
        w.raw(SIGNING_DOMAIN);
        w.raw(&self.header_bytes());
        for input in &self.inputs {
            write_input_unsigned(&mut w, input);
        }
        for output in &self.outputs {
            write_output_unsigned(&mut w, output);
            output.sealed_memo.write_into(&mut w);
        }
        HashAlgorithm::Blake3.digest(&w.finish())
    }

    /// Section 10.3.C: the entire canonical transaction, signatures
    /// included — the complete transaction's identity, and the id
    /// caches/mempools/blocks use (see this module's own docs on why
    /// this differs from [`Self::signing_digest`]).
    pub fn claim_id(&self) -> [u8; 32] {
        let mut w = Writer::new();
        w.raw(CLAIM_ID_DOMAIN);
        w.raw(&self.encode());
        HashAlgorithm::Blake3.digest(&w.finish())
    }

    /// Canonical wire encoding (Section 9): header, every input's
    /// unsigned fields, every output, then every input's signature.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(&self.header_bytes());
        for input in &self.inputs {
            write_input_unsigned(&mut w, input);
        }
        for output in &self.outputs {
            write_output_unsigned(&mut w, output);
            output.sealed_memo.write_into(&mut w);
        }
        for input in &self.inputs {
            write_input_signature(&mut w, &input.signature);
        }
        w.finish()
    }

    /// Decode a claim. Structural validation only — [`verify_v3`] is the
    /// only thing that decides whether it is cryptographically valid.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_CLAIM_V3_BYTES {
            return Err(DecodeFailure::LimitExceeded.into());
        }
        let mut r = Reader::new(bytes);
        let magic = r.array::<{ CLAIM_V3_MAGIC.len() }>()?;
        if magic != *CLAIM_V3_MAGIC {
            return Err(DecodeFailure::UnsupportedVersion.into());
        }
        if r.u8()? != CLAIM_V3_VERSION {
            return Err(DecodeFailure::UnsupportedVersion.into());
        }
        let suite_id = u16::from_be_bytes(r.array::<2>()?);
        if suite_id != SUITE_ID_V3 {
            return Err(DecodeFailure::UnsupportedVersion.into());
        }
        let network_id = r.array::<32>()?;
        let fee_policy_id = r.array::<32>()?;
        let input_count = r.u8()? as usize;
        let output_count = r.u8()? as usize;
        if input_count == 0 || input_count > MAX_INPUTS_V3 {
            return Err(DecodeFailure::LimitExceeded.into());
        }
        if output_count == 0 || output_count > MAX_OUTPUTS_V3 {
            return Err(DecodeFailure::LimitExceeded.into());
        }

        let mut partial_inputs = Vec::with_capacity(input_count);
        for _ in 0..input_count {
            let ring_size = r.u8()? as usize;
            if ring_size != RING_SIZE_V3 {
                return Err(DecodeFailure::LimitExceeded.into());
            }
            let mut ring = Vec::with_capacity(RING_SIZE_V3);
            for _ in 0..RING_SIZE_V3 {
                ring.push(r.array::<32>()?.to_vec());
            }
            let mut ring_commitments = Vec::with_capacity(RING_SIZE_V3);
            for _ in 0..RING_SIZE_V3 {
                ring_commitments.push(r.array::<32>()?.to_vec());
            }
            let pseudo_commitment = r.array::<32>()?.to_vec();
            partial_inputs.push((ring, ring_commitments, pseudo_commitment));
        }

        let mut outputs = Vec::with_capacity(output_count);
        for _ in 0..output_count {
            let tx_public_key = r.array::<32>()?.to_vec();
            let one_time_address = r.array::<32>()?.to_vec();
            let amount_commitment = r.array::<32>()?.to_vec();
            let proof_bytes = r.array::<RANGE_PROOF_V2_BYTES>()?;
            let range_proof =
                RangeProofV2::from_bytes(&proof_bytes).ok_or(DecodeFailure::BadRangeProof)?;
            let sealed_memo = SealedMemoV3::read_from(&mut r)?;
            outputs.push(ClaimOutputV3 {
                tx_public_key,
                one_time_address,
                amount_commitment,
                range_proof,
                sealed_memo,
            });
        }

        let mut inputs = Vec::with_capacity(input_count);
        for (ring, ring_commitments, pseudo_commitment) in partial_inputs {
            let challenge = r.array::<32>()?.to_vec();
            let mut key_responses = Vec::with_capacity(RING_SIZE_V3);
            for _ in 0..RING_SIZE_V3 {
                key_responses.push(r.array::<32>()?.to_vec());
            }
            let mut blinding_responses = Vec::with_capacity(RING_SIZE_V3);
            for _ in 0..RING_SIZE_V3 {
                blinding_responses.push(r.array::<32>()?.to_vec());
            }
            let key_image = r.array::<32>()?.to_vec();
            inputs.push(ClaimInputV3 {
                ring,
                ring_commitments,
                pseudo_commitment,
                signature: MlsagSignatureV3 {
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
            fee_policy_id,
            inputs,
            outputs,
        })
    }
}

/// A claim that has passed every structural and cryptographic check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedPrivateClaimV3 {
    claim: PrivatePaymentClaimV3,
    claim_id: [u8; 32],
}

impl VerifiedPrivateClaimV3 {
    pub fn claim(&self) -> &PrivatePaymentClaimV3 {
        &self.claim
    }

    pub fn claim_id(&self) -> &[u8; 32] {
        &self.claim_id
    }

    /// The double-spend nullifiers, one per input.
    pub fn key_images(&self) -> impl Iterator<Item = &[u8]> {
        self.claim
            .inputs
            .iter()
            .map(|input| input.signature.key_image.as_slice())
    }

    /// Open output `index`'s memo, if it is addressed to the holder of
    /// `shared`.
    pub fn open_memo(&self, index: usize, shared: &StealthSharedSecret) -> Result<PaymentNote> {
        let output = self
            .claim
            .outputs
            .get(index)
            .ok_or(PrivatePaymentError::MalformedMemo)?;
        let digest = self.claim.memo_context_digest(index as u32);
        let context = MemoContextV3 {
            network_id: &self.claim.network_id,
            memo_context_digest: &digest,
            output_index: index as u32,
            tx_public_key: &output.tx_public_key,
            one_time_address: &output.one_time_address,
            amount_commitment: &output.amount_commitment,
        };
        output.sealed_memo.open(shared, &context)
    }
}

/// Provisional fee-policy quote: `fee_micro` as a deterministic function
/// of a claim's own shape, so both a builder and a verifier compute the
/// identical value without an arbitrary bid on the wire (F72-13).
///
/// This is **not** the calibrated economic fee mechanism Section 11.5
/// describes — that "depends on real block cadence and market
/// conditions this crate stays decoupled from" (mirroring `mini_settlement
/// ::claim_v2`'s own `MAX_VALIDITY_HEIGHT_SPAN` precedent for the same
/// reason). It exists so the cryptographic rule the audit actually fixes
/// — one canonical policy, a deterministic amount, the balance equation
/// including it, no fee auction — has *something* concrete to check
/// today. `fee_policy_id` is accepted but not yet interpreted: a future
/// real fee-policy registry is what gives it meaning.
pub fn quote_fee_micro(_fee_policy_id: &[u8; 32], input_count: usize, output_count: usize) -> u64 {
    const BASE_FEE_MICRO: u64 = 1_000;
    const PER_INPUT_MICRO: u64 = 200;
    const PER_OUTPUT_MICRO: u64 = 100;
    BASE_FEE_MICRO + PER_INPUT_MICRO * input_count as u64 + PER_OUTPUT_MICRO * output_count as u64
}

/// An output this wallet controls and is about to spend.
#[derive(Clone)]
pub struct SpendableOutputV3 {
    pub set_index: usize,
    pub one_time_secret: [u8; 32],
    pub value_micro: u64,
    pub blinding: [u8; 32],
}

impl core::fmt::Debug for SpendableOutputV3 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SpendableOutputV3(<redacted>)")
    }
}

/// One party being paid.
#[derive(Clone)]
pub struct RecipientV3 {
    pub spend_public: Vec<u8>,
    pub view_public: Vec<u8>,
    pub amount_micro: u64,
    pub purpose: PaymentPurpose,
}

impl core::fmt::Debug for RecipientV3 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("RecipientV3(<redacted>)")
    }
}

/// Everything needed to build one V3 claim. No `ring_size` or
/// `decoy_entropy` field: see this module's own docs on why (F72-11).
#[derive(Debug, Clone)]
pub struct PaymentRequestV3 {
    pub network_id: [u8; 32],
    pub fee_policy_id: [u8; 32],
    pub spends: Vec<SpendableOutputV3>,
    pub recipients: Vec<RecipientV3>,
}

/// What the payer keeps after building a claim.
#[derive(Debug)]
pub struct BuiltOutputV3 {
    pub shared: StealthSharedSecret,
    pub blinding: [u8; 32],
}

/// Build a V3 claim. The value equation (`Σ spends = Σ recipients +
/// fee`, where `fee` is [`quote_fee_micro`]'s output) is enforced before
/// anything is signed.
pub fn build_v3(
    request: &PaymentRequestV3,
    outputs: &impl OutputSet,
) -> Result<(PrivatePaymentClaimV3, Vec<BuiltOutputV3>)> {
    if request.spends.is_empty() || request.spends.len() > MAX_INPUTS_V3 {
        return Err(PrivatePaymentError::InputCountOutOfRange {
            got: request.spends.len(),
            max: MAX_INPUTS_V3,
        });
    }
    if request.recipients.is_empty() || request.recipients.len() > MAX_OUTPUTS_V3 {
        return Err(PrivatePaymentError::OutputCountOutOfRange {
            got: request.recipients.len(),
            max: MAX_OUTPUTS_V3,
        });
    }

    let fee_micro = quote_fee_micro(
        &request.fee_policy_id,
        request.spends.len(),
        request.recipients.len(),
    );

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
        .checked_add(fee_micro)
        .ok_or(PrivatePaymentError::UnbalancedAmounts)?;
    if spent != required {
        return Err(PrivatePaymentError::UnbalancedAmounts);
    }

    // Outputs first (without memos -- sealed once the memo-context digest
    // exists, which needs every output's public fields already fixed).
    let mut output_blindings = Vec::with_capacity(request.recipients.len());
    let mut built = Vec::with_capacity(request.recipients.len());
    let mut claim_outputs = Vec::with_capacity(request.recipients.len());
    for recipient in &request.recipients {
        let blinding = mini_value::random_scalar_bytes()
            .map_err(|_| PrivatePaymentError::CryptoUnavailable)?;
        let (output, shared) = derive_output_v3(
            SUITE_ID_V3,
            &request.network_id,
            &recipient.spend_public,
            &recipient.view_public,
        )
        .ok_or(PrivatePaymentError::CryptoUnavailable)?;
        let (amount_commitment, range_proof) =
            prove_range_v2_from_bytes(recipient.amount_micro, &blinding)
                .ok_or(PrivatePaymentError::CryptoUnavailable)?
                .map_err(|_| PrivatePaymentError::CryptoUnavailable)?;
        output_blindings.push(blinding);
        claim_outputs.push(ClaimOutputV3 {
            tx_public_key: output.tx_public_key,
            one_time_address: output.one_time_address,
            amount_commitment: amount_commitment.to_vec(),
            range_proof,
            sealed_memo: SealedMemoV3 {
                ciphertext: Vec::new(),
            },
        });
        built.push(BuiltOutputV3 { shared, blinding });
    }

    let pseudo_blindings = pseudo_blindings_for(request.spends.len(), &output_blindings)?;

    let mut claim_inputs = Vec::with_capacity(request.spends.len());
    let mut witnesses = Vec::with_capacity(request.spends.len());
    for (spend, pseudo_blinding) in request.spends.iter().zip(pseudo_blindings.iter()) {
        let mut entropy = [0u8; 32];
        entropy.copy_from_slice(
            &mini_crypto::random_32().map_err(|_| PrivatePaymentError::CryptoUnavailable)?,
        );
        let (indices, position) =
            select_ring_indices_v3(outputs, spend.set_index, RING_SIZE_V3, &entropy)?;
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
            reblind_v3(spend.value_micro, &spend.blinding, pseudo_blinding)
                .ok_or(PrivatePaymentError::CryptoUnavailable)?;
        claim_inputs.push(ClaimInputV3 {
            ring,
            ring_commitments,
            pseudo_commitment: pseudo_commitment.to_vec(),
            signature: MlsagSignatureV3 {
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

    let mut claim = PrivatePaymentClaimV3 {
        network_id: request.network_id,
        fee_policy_id: request.fee_policy_id,
        inputs: claim_inputs,
        outputs: claim_outputs,
    };

    for (index, recipient) in request.recipients.iter().enumerate() {
        let digest = claim.memo_context_digest(index as u32);
        let note = PaymentNote::new(
            recipient.purpose.clone(),
            recipient.amount_micro,
            built[index].blinding,
        );
        let context = MemoContextV3 {
            network_id: &claim.network_id,
            memo_context_digest: &digest,
            output_index: index as u32,
            tx_public_key: &claim.outputs[index].tx_public_key,
            one_time_address: &claim.outputs[index].one_time_address,
            amount_commitment: &claim.outputs[index].amount_commitment,
        };
        claim.outputs[index].sealed_memo =
            SealedMemoV3::seal(&note, &built[index].shared, &context)?;
    }

    let signing_digest = claim.signing_digest();
    for (index, witness) in witnesses.iter().enumerate() {
        let input = &claim.inputs[index];
        let signature = sign_spend_v3(
            SUITE_ID_V3,
            &claim.network_id,
            &signing_digest,
            index as u32,
            &input.ring,
            &input.ring_commitments,
            &input.pseudo_commitment,
            witness,
        )
        .ok_or(PrivatePaymentError::CryptoUnavailable)?;
        claim.inputs[index].signature = signature;
    }

    Ok((claim, built))
}

fn pseudo_blindings_for(count: usize, output_blindings: &[[u8; 32]]) -> Result<Vec<[u8; 32]>> {
    let mut chosen = Vec::with_capacity(count);
    for _ in 0..count.saturating_sub(1) {
        chosen.push(
            mini_value::random_scalar_bytes()
                .map_err(|_| PrivatePaymentError::CryptoUnavailable)?,
        );
    }
    chosen.push(balancing_blinding_v3(output_blindings, &chosen));
    Ok(chosen)
}

/// Verify a V3 private payment completely.
pub fn verify_v3(
    claim: &PrivatePaymentClaimV3,
    network_id: &[u8; 32],
) -> Result<VerifiedPrivateClaimV3> {
    if &claim.network_id != network_id {
        return Err(PrivatePaymentError::NetworkMismatch);
    }
    if claim.inputs.is_empty() || claim.inputs.len() > MAX_INPUTS_V3 {
        return Err(PrivatePaymentError::InputCountOutOfRange {
            got: claim.inputs.len(),
            max: MAX_INPUTS_V3,
        });
    }
    if claim.outputs.is_empty() || claim.outputs.len() > MAX_OUTPUTS_V3 {
        return Err(PrivatePaymentError::OutputCountOutOfRange {
            got: claim.outputs.len(),
            max: MAX_OUTPUTS_V3,
        });
    }
    for input in &claim.inputs {
        if input.ring.len() != RING_SIZE_V3 || input.ring_commitments.len() != RING_SIZE_V3 {
            return Err(PrivatePaymentError::RingTooSmall {
                got: input.ring.len(),
                min: RING_SIZE_V3,
            });
        }
        if !crate::ring_is_canonical(&input.ring) {
            return Err(PrivatePaymentError::DuplicateRingMember);
        }
    }

    for output in &claim.outputs {
        let mut commitment = [0u8; 32];
        if output.amount_commitment.len() != 32 {
            return Err(PrivatePaymentError::BadRangeProof);
        }
        commitment.copy_from_slice(&output.amount_commitment);
        if !verify_range_v2(commitment, &output.range_proof) {
            return Err(PrivatePaymentError::BadRangeProof);
        }
    }

    let fee_micro = quote_fee_micro(
        &claim.fee_policy_id,
        claim.inputs.len(),
        claim.outputs.len(),
    );
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
    sinks.push(public_amount_commitment_v2(fee_micro).to_vec());
    if !verify_balance_v2(&pseudo, &sinks) {
        return Err(PrivatePaymentError::UnbalancedAmounts);
    }

    let signing_digest = claim.signing_digest();
    for (index, input) in claim.inputs.iter().enumerate() {
        if !verify_spend_v3(
            SUITE_ID_V3,
            &claim.network_id,
            &signing_digest,
            index as u32,
            &input.ring,
            &input.ring_commitments,
            &input.pseudo_commitment,
            &input.signature,
        ) {
            return Err(PrivatePaymentError::BadSpendProof);
        }
    }

    for (index, input) in claim.inputs.iter().enumerate() {
        if claim.inputs[..index]
            .iter()
            .any(|earlier| earlier.signature.key_image == input.signature.key_image)
        {
            return Err(PrivatePaymentError::RepeatedKeyImage);
        }
    }

    Ok(VerifiedPrivateClaimV3 {
        claim_id: claim.claim_id(),
        claim: claim.clone(),
    })
}

/// Recover the shared secret for a V3 output using a recipient's view
/// key, if the output is theirs. Thin wrapper over
/// [`mini_value::recover_and_verify_v3`] using this module's fixed
/// [`SUITE_ID_V3`].
pub fn recover_shared_secret_v3(
    network_id: &[u8; 32],
    own_view_secret: &[u8],
    own_spend_public: &[u8],
    output: &ClaimOutputV3,
) -> Option<StealthSharedSecret> {
    recover_and_verify_v3(
        SUITE_ID_V3,
        network_id,
        own_view_secret,
        own_spend_public,
        &mini_value::StealthOutput {
            tx_public_key: output.tx_public_key.clone(),
            one_time_address: output.one_time_address.clone(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoy::InMemoryOutputSet;
    use mini_value::{pedersen_commitment_v2, StealthKeypair};

    const NETWORK: [u8; 32] = [0x5a; 32];
    const FEE_POLICY: [u8; 32] = [0x11; 32];

    struct Ledger {
        outputs: InMemoryOutputSet,
    }

    impl Ledger {
        fn new() -> Self {
            Self {
                outputs: InMemoryOutputSet::new(),
            }
        }

        fn fill(&mut self, count: usize) {
            for _ in 0..count {
                let key = StealthKeypair::generate().unwrap();
                let blinding = mini_value::random_scalar_bytes().unwrap();
                let commitment = pedersen_commitment_v2(1_000, &blinding).unwrap();
                self.outputs
                    .push(key.spend_public_bytes().to_vec(), commitment.to_vec());
            }
        }

        fn mint(&mut self, value_micro: u64) -> SpendableOutputV3 {
            let key = StealthKeypair::generate().unwrap();
            let blinding = mini_value::random_scalar_bytes().unwrap();
            let commitment = pedersen_commitment_v2(value_micro, &blinding).unwrap();
            self.outputs
                .push(key.spend_public_bytes().to_vec(), commitment.to_vec());
            SpendableOutputV3 {
                set_index: self.outputs.len() - 1,
                one_time_secret: key.spend_secret_bytes(),
                value_micro,
                blinding,
            }
        }

        fn with_funds(value_micro: u64) -> (Self, SpendableOutputV3) {
            let mut ledger = Ledger::new();
            ledger.fill(RING_SIZE_V3 * 4);
            let spend = ledger.mint(value_micro);
            ledger.fill(4);
            (ledger, spend)
        }
    }

    impl OutputSet for Ledger {
        fn len(&self) -> usize {
            self.outputs.len()
        }
        fn key_at(&self, index: usize) -> Option<Vec<u8>> {
            self.outputs.key_at(index)
        }
        fn commitment_at(&self, index: usize) -> Option<Vec<u8>> {
            self.outputs.commitment_at(index)
        }
    }

    fn pay(to: &StealthKeypair, amount: u64, purpose: &[u8]) -> RecipientV3 {
        RecipientV3 {
            spend_public: to.spend_public_bytes().to_vec(),
            view_public: to.view_public_bytes().to_vec(),
            amount_micro: amount,
            purpose: PaymentPurpose::new(purpose.to_vec()),
        }
    }

    fn request_for(
        spends: Vec<SpendableOutputV3>,
        recipients: Vec<RecipientV3>,
    ) -> PaymentRequestV3 {
        PaymentRequestV3 {
            network_id: NETWORK,
            fee_policy_id: FEE_POLICY,
            spends,
            recipients,
        }
    }

    /// One input, one recipient, exactly funded (amount + the quoted fee).
    fn payment_to(
        to: &StealthKeypair,
        amount: u64,
        purpose: &[u8],
    ) -> (PrivatePaymentClaimV3, Vec<BuiltOutputV3>) {
        let fee = quote_fee_micro(&FEE_POLICY, 1, 1);
        let (ledger, spend) = Ledger::with_funds(amount + fee);
        let request = request_for(vec![spend], vec![pay(to, amount, purpose)]);
        build_v3(&request, &ledger).unwrap()
    }

    #[test]
    fn a_balanced_payment_builds_and_verifies() {
        let to = StealthKeypair::generate().unwrap();
        let (claim, _) = payment_to(&to, 5_000, b"invoice");
        assert!(verify_v3(&claim, &NETWORK).is_ok());
    }

    #[test]
    fn the_recipient_opens_the_memo_and_recovers_the_spendable_opening() {
        let to = StealthKeypair::generate().unwrap();
        let (claim, _) = payment_to(&to, 5_000, b"for-you");
        let verified = verify_v3(&claim, &NETWORK).unwrap();
        let shared = recover_shared_secret_v3(
            &NETWORK,
            &to.view_secret_bytes(),
            &to.spend_public_bytes(),
            &claim.outputs[0],
        )
        .unwrap();
        let note = verified.open_memo(0, &shared).unwrap();
        assert_eq!(note.amount_micro, 5_000);
        assert_eq!(note.purpose.reference, b"for-you");
    }

    #[test]
    fn a_stranger_cannot_recover_the_output_or_open_the_memo() {
        let to = StealthKeypair::generate().unwrap();
        let stranger = StealthKeypair::generate().unwrap();
        let (claim, _) = payment_to(&to, 1_000, b"private");
        assert!(recover_shared_secret_v3(
            &NETWORK,
            &stranger.view_secret_bytes(),
            &stranger.spend_public_bytes(),
            &claim.outputs[0],
        )
        .is_none());
    }

    #[test]
    fn encode_decode_round_trips_and_preserves_the_claim_id() {
        let to = StealthKeypair::generate().unwrap();
        let (claim, _) = payment_to(&to, 2_000, b"round-trip");
        let bytes = claim.encode();
        assert!(bytes.len() <= MAX_CLAIM_V3_BYTES);
        let decoded = PrivatePaymentClaimV3::decode(&bytes).unwrap();
        assert_eq!(claim, decoded);
        assert_eq!(claim.claim_id(), decoded.claim_id());
    }

    #[test]
    fn a_tampered_pseudo_commitment_fails_verification() {
        // Tampering the pseudo-commitment breaks the balance sum before
        // verify_v3 ever reaches the spend-proof check (the same ordering
        // crate::claim::verify documents: conservation before signatures).
        let to = StealthKeypair::generate().unwrap();
        let (mut claim, _) = payment_to(&to, 1_000, b"x");
        claim.inputs[0].pseudo_commitment[0] ^= 0xff;
        assert_eq!(
            verify_v3(&claim, &NETWORK).unwrap_err(),
            PrivatePaymentError::UnbalancedAmounts
        );
    }

    #[test]
    fn a_wrong_network_is_rejected() {
        let to = StealthKeypair::generate().unwrap();
        let (claim, _) = payment_to(&to, 1_000, b"x");
        assert_eq!(
            verify_v3(&claim, &[0xee; 32]).unwrap_err(),
            PrivatePaymentError::NetworkMismatch
        );
    }

    #[test]
    fn an_unbalanced_request_cannot_be_built() {
        let to = StealthKeypair::generate().unwrap();
        let (ledger, spend) = Ledger::with_funds(1_000);
        let request = request_for(vec![spend], vec![pay(&to, 999, b"short")]);
        assert_eq!(
            build_v3(&request, &ledger).unwrap_err(),
            PrivatePaymentError::UnbalancedAmounts
        );
    }

    #[test]
    fn a_tampered_range_proof_fails_verification() {
        let to = StealthKeypair::generate().unwrap();
        let (mut claim, _) = payment_to(&to, 1_000, b"x");
        let other = mini_value::prove_range_v2_from_bytes(1, &[3u8; 32])
            .unwrap()
            .unwrap();
        claim.outputs[0].range_proof = other.1;
        assert_eq!(
            verify_v3(&claim, &NETWORK).unwrap_err(),
            PrivatePaymentError::BadRangeProof
        );
    }

    #[test]
    fn repeating_a_key_image_within_one_claim_is_rejected() {
        // A real double-spend attempt: the same one-time output, spent
        // twice as two distinct inputs of the same claim. Each MLSAG is
        // independently valid (both really do open that output), and the
        // balance sum is made to work out (2x the real value in, matching
        // outputs), so only the key-image check catches this.
        let to = StealthKeypair::generate().unwrap();
        let value = 1_000u64;
        let (ledger, spend) = Ledger::with_funds(value);
        let spend_again = SpendableOutputV3 {
            set_index: spend.set_index,
            one_time_secret: spend.one_time_secret,
            value_micro: spend.value_micro,
            blinding: spend.blinding,
        };
        let fee = quote_fee_micro(&FEE_POLICY, 2, 1);
        let paid = 2 * value - fee;
        let request = request_for(
            vec![spend, spend_again],
            vec![pay(&to, paid, b"double-spend")],
        );
        let (claim, _) = build_v3(&request, &ledger).unwrap();
        assert_eq!(
            verify_v3(&claim, &NETWORK).unwrap_err(),
            PrivatePaymentError::RepeatedKeyImage
        );
    }
}
