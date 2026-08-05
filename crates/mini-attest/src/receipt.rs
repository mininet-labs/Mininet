//! Provider-signed, content-addressed Tier-0 completion receipts.

use did_mini::{Controller, Did, IndexedSig, Kel};
use mini_crypto::{encoding, HashAlgorithm, Multihash, Signature, SignatureSuite};
use mini_engagement::{
    canonical_completion_status, CanonicalCompletionStatus, Engagement, EngagementState,
};
use mini_objects::ObjectId;
use mini_provider::EngagementGrant;
use mini_settlement::{claim_digest, CanonicalLedgerView};

use crate::codec::{Reader, Writer};
use crate::{AttestError, HolderCommitment, Result};

pub const RECEIPT_VERSION: u8 = 1;

const RECEIPT_SIGNING_DOMAIN: &[u8] = b"mininet/mini-attest/engagement-completion-receipt/v1";
const ENGAGEMENT_ID_DOMAIN: &[u8] = b"mininet/mini-attest/engagement-id/v1";
const COMPLETION_STATE_DOMAIN: &[u8] = b"mininet/mini-attest/completion-state-commitment/v1";
const REVIEW_SUBJECT_DOMAIN: &[u8] = b"mininet/mini-attest/review-subject/v1";

const MAX_DID_BYTES: usize = 256;
const MAX_OBJECT_ID_BYTES: usize = 128;
/// Mirrors `did_mini::MAX_SIGNATURES` rather than restating a smaller
/// number: a cap below did-mini's own would let a legitimate threshold
/// identity sign an object it could not then decode.
const MAX_SIGNATURES: usize = did_mini::MAX_SIGNATURES;
// ML-DSA signatures are intentionally supported by the crypto-agile KEL API.
const MAX_SIGNATURE_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReviewSubjectCommitment([u8; 32]);

impl ReviewSubjectCommitment {
    pub fn derive(subject: &[u8]) -> Result<Self> {
        if subject.is_empty() || subject.len() > 4096 {
            return Err(AttestError::LimitExceeded);
        }
        let mut writer = Writer::new();
        writer.raw(REVIEW_SUBJECT_DOMAIN);
        writer.bytes(subject);
        Ok(Self(HashAlgorithm::Blake3.digest(&writer.finish())))
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SettlementReference {
    CanonicalClaimDigest([u8; 32]),
    NoSettlementRequired,
}

impl SettlementReference {
    fn encode(self, writer: &mut Writer) {
        match self {
            SettlementReference::CanonicalClaimDigest(digest) => {
                writer.u8(1);
                writer.raw(&digest);
            }
            SettlementReference::NoSettlementRequired => {
                writer.u8(2);
                writer.raw(&[0; 32]);
            }
        }
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self> {
        let tag = reader.u8()?;
        let digest = reader.raw_array::<32>()?;
        match tag {
            1 => Ok(SettlementReference::CanonicalClaimDigest(digest)),
            2 if digest == [0; 32] => Ok(SettlementReference::NoSettlementRequired),
            _ => Err(AttestError::SettlementReferenceMismatch),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReceiptId(ObjectId);

impl ReceiptId {
    pub fn as_object_id(&self) -> &ObjectId {
        &self.0
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    fn derive(bytes: &[u8]) -> Result<Self> {
        let multihash = Multihash::of(HashAlgorithm::Blake3, bytes);
        let encoded = encoding::encode(encoding::BASE58BTC, &multihash.to_bytes())?;
        Ok(Self(ObjectId::parse(&encoded)?))
    }

    fn parse(value: &[u8]) -> Result<Self> {
        let value = core::str::from_utf8(value).map_err(|_| AttestError::InvalidReceiptId)?;
        Ok(Self(
            ObjectId::parse(value).map_err(|_| AttestError::InvalidReceiptId)?,
        ))
    }
}

/// The minimal public receipt. It intentionally contains commitments and
/// content ids, not payment addresses, amount, free-form terms, or reviewer
/// root identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngagementCompletionReceiptV1 {
    pub engagement_id: [u8; 32],
    pub terms_object_id: ObjectId,
    pub provider_declaration_id: ObjectId,
    pub provider: Did,
    pub holder_commitment: HolderCommitment,
    pub completed_at_epoch: u64,
    pub completion_state_hash: [u8; 32],
    pub settlement_reference: SettlementReference,
    pub reviewable_subject: ReviewSubjectCommitment,
    pub expiry_epoch: u64,
    signature: Vec<IndexedSig>,
}

impl EngagementCompletionReceiptV1 {
    fn signing_bytes(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.raw(RECEIPT_SIGNING_DOMAIN);
        self.encode_fields(&mut writer);
        writer.finish()
    }

    fn encode_fields(&self, writer: &mut Writer) {
        writer.u8(RECEIPT_VERSION);
        writer.raw(&self.engagement_id);
        writer.bytes(self.terms_object_id.as_str().as_bytes());
        writer.bytes(self.provider_declaration_id.as_str().as_bytes());
        writer.bytes(self.provider.as_str().as_bytes());
        writer.raw(self.holder_commitment.as_bytes());
        writer.u64(self.completed_at_epoch);
        writer.raw(&self.completion_state_hash);
        self.settlement_reference.encode(writer);
        writer.raw(self.reviewable_subject.as_bytes());
        writer.u64(self.expiry_epoch);
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        self.encode_fields(&mut writer);
        encode_signatures(&mut writer, &self.signature);
        writer.finish()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        if reader.u8()? != RECEIPT_VERSION {
            return Err(AttestError::UnsupportedReceiptVersion);
        }
        let engagement_id = reader.raw_array::<32>()?;
        let terms_object_id = parse_object_id(reader.bytes_limited(MAX_OBJECT_ID_BYTES)?)?;
        let provider_declaration_id = parse_object_id(reader.bytes_limited(MAX_OBJECT_ID_BYTES)?)?;
        let provider = parse_did(reader.bytes_limited(MAX_DID_BYTES)?)?;
        let holder_commitment = HolderCommitment::from_bytes(reader.raw_array::<32>()?);
        let completed_at_epoch = reader.u64()?;
        let completion_state_hash = reader.raw_array::<32>()?;
        let settlement_reference = SettlementReference::decode(&mut reader)?;
        let reviewable_subject = ReviewSubjectCommitment::from_bytes(reader.raw_array::<32>()?);
        let expiry_epoch = reader.u64()?;
        let signature = decode_signatures(&mut reader)?;
        reader.finish()?;
        if signature.is_empty() {
            return Err(AttestError::BadProviderSignature);
        }
        Ok(Self {
            engagement_id,
            terms_object_id,
            provider_declaration_id,
            provider,
            holder_commitment,
            completed_at_epoch,
            completion_state_hash,
            settlement_reference,
            reviewable_subject,
            expiry_epoch,
            signature,
        })
    }

    pub fn id(&self) -> Result<ReceiptId> {
        ReceiptId::derive(&self.to_bytes())
    }

    pub fn verify_provider_signature(&self, provider_kel: &Kel) -> Result<()> {
        if provider_kel.did().as_str() != self.provider.as_str() {
            return Err(AttestError::ProviderMismatch);
        }
        provider_kel
            .verify_message(&self.signing_bytes(), &self.signature)
            .map_err(|_| AttestError::BadProviderSignature)
    }
}

/// Inputs needed to issue a receipt. The provider signs only after the
/// engagement is independently reconciled against `ledger`.
#[derive(Debug)]
pub struct Tier0ReceiptContext<'a, L: CanonicalLedgerView> {
    pub engagement: &'a Engagement,
    pub grant: &'a EngagementGrant,
    pub ledger: &'a L,
    pub now_ms: u64,
    pub epoch_length_ms: u64,
    pub expiry_epoch: u64,
    pub reviewable_subject: ReviewSubjectCommitment,
}

pub fn issue_tier0_receipt(
    context: &Tier0ReceiptContext<'_, impl CanonicalLedgerView>,
    provider: &Controller,
) -> Result<EngagementCompletionReceiptV1> {
    context.grant.check_wellformed()?;
    validate_provider_relationship(context.engagement, context.grant, &provider.did())?;
    let completed_at_ms = completed_at_ms(context.engagement)?;
    ensure_grant_active_at_completion(context.grant, completed_at_ms)?;
    let completed_at_epoch = validate_epoch_window(context, completed_at_ms, context.now_ms)?;
    ensure_canonical_completion(context.engagement, context.ledger, context.now_ms)?;

    let settlement_digest = claim_digest(&context.engagement.escrow_claim);
    let engagement_id = engagement_id(context.engagement);
    let mut receipt = EngagementCompletionReceiptV1 {
        engagement_id,
        terms_object_id: context.engagement.terms.clone(),
        provider_declaration_id: context.grant.declaration.clone(),
        provider: provider.did(),
        holder_commitment: HolderCommitment::from_bytes(context.grant.holder_commitment),
        completed_at_epoch,
        completion_state_hash: completion_state_hash(
            engagement_id,
            completed_at_ms,
            settlement_digest,
        ),
        settlement_reference: SettlementReference::CanonicalClaimDigest(settlement_digest),
        reviewable_subject: context.reviewable_subject,
        expiry_epoch: context.expiry_epoch,
        signature: Vec::new(),
    };
    receipt.signature = provider.sign_message(&receipt.signing_bytes());
    Ok(receipt)
}

pub fn verify_tier0_receipt(
    receipt: &EngagementCompletionReceiptV1,
    context: &Tier0ReceiptContext<'_, impl CanonicalLedgerView>,
    provider_kel: &Kel,
) -> Result<ReceiptId> {
    context.grant.check_wellformed()?;
    validate_provider_relationship(context.engagement, context.grant, &receipt.provider)?;
    receipt.verify_provider_signature(provider_kel)?;

    let completed_at_ms = completed_at_ms(context.engagement)?;
    ensure_grant_active_at_completion(context.grant, completed_at_ms)?;
    let completed_at_epoch = validate_epoch_window(context, completed_at_ms, context.now_ms)?;
    ensure_canonical_completion(context.engagement, context.ledger, context.now_ms)?;

    if receipt.completed_at_epoch != completed_at_epoch {
        return Err(AttestError::CompletionStateMismatch);
    }
    if receipt.expiry_epoch != context.expiry_epoch {
        return Err(AttestError::InvalidEpochWindow);
    }
    if receipt.reviewable_subject != context.reviewable_subject {
        return Err(AttestError::ReviewSubjectMismatch);
    }
    if receipt.terms_object_id != context.engagement.terms {
        return Err(AttestError::TermsMismatch);
    }
    if receipt.provider_declaration_id != context.grant.declaration {
        return Err(AttestError::DeclarationMismatch);
    }
    if receipt.holder_commitment.as_bytes() != &context.grant.holder_commitment {
        return Err(AttestError::HolderCommitmentMismatch);
    }

    let expected_engagement_id = engagement_id(context.engagement);
    if receipt.engagement_id != expected_engagement_id {
        return Err(AttestError::EngagementMismatch);
    }
    let settlement_digest = claim_digest(&context.engagement.escrow_claim);
    if receipt.settlement_reference != SettlementReference::CanonicalClaimDigest(settlement_digest)
    {
        return Err(AttestError::SettlementReferenceMismatch);
    }
    if receipt.completion_state_hash
        != completion_state_hash(expected_engagement_id, completed_at_ms, settlement_digest)
    {
        return Err(AttestError::CompletionStateMismatch);
    }
    receipt.id()
}

pub fn engagement_id(engagement: &Engagement) -> [u8; 32] {
    let mut writer = Writer::new();
    writer.raw(ENGAGEMENT_ID_DOMAIN);
    writer.bytes(engagement.terms.as_str().as_bytes());
    writer.bytes(engagement.payer.as_str().as_bytes());
    writer.bytes(engagement.performer.as_str().as_bytes());
    writer.raw(&claim_digest(&engagement.escrow_claim));
    writer.u64(engagement.deadline_ms);
    HashAlgorithm::Blake3.digest(&writer.finish())
}

fn completion_state_hash(
    engagement_id: [u8; 32],
    completed_at_ms: u64,
    settlement_digest: [u8; 32],
) -> [u8; 32] {
    let mut writer = Writer::new();
    writer.raw(COMPLETION_STATE_DOMAIN);
    writer.raw(&engagement_id);
    writer.u8(1); // Completed
    writer.u64(completed_at_ms);
    writer.raw(&settlement_digest);
    HashAlgorithm::Blake3.digest(&writer.finish())
}

fn completed_at_ms(engagement: &Engagement) -> Result<u64> {
    match engagement.state {
        EngagementState::Completed { at_ms } => Ok(at_ms),
        _ => Err(AttestError::EngagementNotCanonicallyComplete),
    }
}

fn validate_provider_relationship(
    engagement: &Engagement,
    grant: &EngagementGrant,
    provider: &Did,
) -> Result<()> {
    if engagement.performer.as_str() != provider.as_str()
        || grant.provider.as_str() != provider.as_str()
    {
        return Err(AttestError::ProviderMismatch);
    }
    Ok(())
}

fn ensure_grant_active_at_completion(grant: &EngagementGrant, completed_at_ms: u64) -> Result<()> {
    if grant.is_active_at(completed_at_ms) {
        Ok(())
    } else {
        Err(AttestError::GrantInactiveAtCompletion)
    }
}

fn validate_epoch_window(
    context: &Tier0ReceiptContext<'_, impl CanonicalLedgerView>,
    completed_at_ms: u64,
    now_ms: u64,
) -> Result<u64> {
    if context.epoch_length_ms == 0 {
        return Err(AttestError::InvalidEpochLength);
    }
    if completed_at_ms > now_ms {
        return Err(AttestError::ReceiptNotYetComplete);
    }
    let completed_epoch = completed_at_ms / context.epoch_length_ms;
    let current_epoch = now_ms / context.epoch_length_ms;
    if context.expiry_epoch <= completed_epoch {
        return Err(AttestError::InvalidEpochWindow);
    }
    if current_epoch >= context.expiry_epoch {
        return Err(AttestError::ReceiptExpired);
    }
    Ok(completed_epoch)
}

fn ensure_canonical_completion(
    engagement: &Engagement,
    ledger: &impl CanonicalLedgerView,
    now_ms: u64,
) -> Result<()> {
    let status = canonical_completion_status(engagement, ledger, now_ms)?;
    if status == CanonicalCompletionStatus::CanonicallyCompleted {
        Ok(())
    } else {
        Err(AttestError::EngagementNotCanonicallyComplete)
    }
}

fn encode_signatures(writer: &mut Writer, signatures: &[IndexedSig]) {
    writer.u32(signatures.len() as u32);
    for signature in signatures {
        writer.u32(signature.index);
        writer.u8(signature.signature.suite().tag());
        writer.bytes(&signature.signature.to_bytes());
    }
}

fn decode_signatures(reader: &mut Reader<'_>) -> Result<Vec<IndexedSig>> {
    let count = reader.u32()? as usize;
    if count > MAX_SIGNATURES {
        return Err(AttestError::LimitExceeded);
    }
    let mut signatures = Vec::with_capacity(count);
    for _ in 0..count {
        let index = reader.u32()?;
        let suite = SignatureSuite::from_tag(reader.u8()?)?;
        let bytes = reader.bytes_limited(MAX_SIGNATURE_BYTES)?;
        let signature = Signature::from_suite_bytes(suite, &bytes)?;
        signatures.push(IndexedSig { index, signature });
    }
    if !did_mini::signatures_are_canonical(&signatures) {
        return Err(AttestError::NoncanonicalSignatureOrder);
    }
    Ok(signatures)
}

fn parse_did(bytes: Vec<u8>) -> Result<Did> {
    let value = String::from_utf8(bytes).map_err(|_| AttestError::InvalidDid)?;
    Did::parse(&value).map_err(|_| AttestError::InvalidDid)
}

fn parse_object_id(bytes: Vec<u8>) -> Result<ObjectId> {
    let value = String::from_utf8(bytes).map_err(|_| AttestError::InvalidObjectId)?;
    ObjectId::parse(&value).map_err(|_| AttestError::InvalidObjectId)
}

pub(crate) fn encode_receipt_id(writer: &mut Writer, receipt_id: &ReceiptId) {
    writer.bytes(receipt_id.as_str().as_bytes());
}

pub(crate) fn decode_receipt_id(reader: &mut Reader<'_>) -> Result<ReceiptId> {
    ReceiptId::parse(&reader.bytes_limited(MAX_OBJECT_ID_BYTES)?)
}

pub(crate) fn encode_indexed_signatures(writer: &mut Writer, signatures: &[IndexedSig]) {
    encode_signatures(writer, signatures);
}

pub(crate) fn decode_indexed_signatures(reader: &mut Reader<'_>) -> Result<Vec<IndexedSig>> {
    decode_signatures(reader)
}

pub(crate) fn parse_review_did(bytes: Vec<u8>) -> Result<Did> {
    parse_did(bytes)
}

pub(crate) const REVIEW_MAX_DID_BYTES: usize = MAX_DID_BYTES;

#[cfg(test)]
mod signature_codec_tests {
    use super::*;
    use mini_crypto::SigningKey;

    /// A `did-mini` identity may hold up to `did_mini::MAX_KEYS` keys and
    /// signs with every one of them, so a receipt from a large threshold
    /// identity carries more than sixteen signatures.
    ///
    /// This crate previously capped its decoder at sixteen. That is the
    /// dangerous direction for a limit to be wrong in: such an identity could
    /// issue a receipt, verify it in memory, encode it — and then fail to
    /// decode its own bytes, which reads as corruption and is really a limit
    /// mismatch between two crates. Regression test for the alignment.
    #[test]
    fn a_full_size_threshold_identitys_signatures_survive_a_round_trip() {
        const _: () = assert!(MAX_SIGNATURES >= did_mini::MAX_KEYS);
        assert_eq!(MAX_SIGNATURES, did_mini::MAX_SIGNATURES);

        let message = b"receipt signing bytes";
        let signatures: Vec<IndexedSig> = (0..did_mini::MAX_KEYS)
            .map(|index| IndexedSig {
                index: index as u32,
                signature: SigningKey::from_seed(&[index as u8; 32]).sign(message),
            })
            .collect();
        assert!(
            signatures.len() > 16,
            "the old cap must actually be exceeded"
        );

        let mut writer = Writer::new();
        encode_signatures(&mut writer, &signatures);
        let bytes = writer.finish();

        let mut reader = Reader::new(&bytes);
        let decoded = decode_signatures(&mut reader).unwrap();
        assert!(reader.finish().is_ok());
        assert_eq!(decoded, signatures);
    }

    /// One logical receipt must have exactly one encoding. Receipt ids are
    /// derived from the receipt's own bytes, so an unsorted or repeated
    /// signature list would give the same receipt a second identity -- and a
    /// dedup index keyed on that id would let the duplicate through.
    #[test]
    fn an_unsorted_or_repeated_signature_list_is_refused() {
        let message = b"receipt signing bytes";
        let sig = |index: u32| IndexedSig {
            index,
            signature: SigningKey::from_seed(&[index as u8; 32]).sign(message),
        };

        for list in [
            vec![sig(1), sig(0)],
            vec![sig(0), sig(0)],
            vec![sig(0), sig(2), sig(1)],
        ] {
            let mut writer = Writer::new();
            encode_signatures(&mut writer, &list);
            let bytes = writer.finish();
            assert_eq!(
                decode_signatures(&mut Reader::new(&bytes)),
                Err(AttestError::NoncanonicalSignatureOrder)
            );

            // And the fix is available to any caller assembling signatures from
            // several devices, rather than left as a trap.
            let fixed = did_mini::canonicalize_signatures(list);
            let mut writer = Writer::new();
            encode_signatures(&mut writer, &fixed);
            let bytes = writer.finish();
            assert_eq!(decode_signatures(&mut Reader::new(&bytes)).unwrap(), fixed);
        }
    }

    #[test]
    fn one_past_the_cap_is_still_refused_before_allocating() {
        let mut writer = Writer::new();
        writer.u32(MAX_SIGNATURES as u32 + 1);
        let bytes = writer.finish();
        assert_eq!(
            decode_signatures(&mut Reader::new(&bytes)),
            Err(AttestError::LimitExceeded)
        );
    }
}
