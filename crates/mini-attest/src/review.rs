//! Pairwise-pseudonym-signed Tier-0 reviews and local duplicate protection.

use std::collections::HashSet;

use did_mini::{Controller, Did, IndexedSig, Kel};
use mini_crypto::HashAlgorithm;
use mini_engagement::Engagement;
use mini_provider::EngagementGrant;
use mini_settlement::CanonicalLedgerView;

use crate::codec::{Reader, Writer};
use crate::receipt::{
    decode_indexed_signatures, decode_receipt_id, encode_indexed_signatures, encode_receipt_id,
    parse_review_did, verify_tier0_receipt, REVIEW_MAX_DID_BYTES,
};
use crate::{
    AttestError, EngagementCompletionReceiptV1, EngagementHolderToken, ReceiptId, Result,
    ReviewSubjectCommitment, Tier0ReceiptContext,
};

pub const REVIEW_VERSION: u8 = 1;
pub const MAX_REVIEW_BODY_BYTES: usize = 64 * 1024;

const REVIEW_SIGNING_DOMAIN: &[u8] = b"mininet/mini-attest/tier0-review/v1";
const REVIEW_PAYLOAD_DOMAIN: &[u8] = b"mininet/mini-attest/review-payload/v1";
const REVIEW_REGISTRY_KEY_DOMAIN: &[u8] = b"mininet/mini-attest/tier0-review-registry-key/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssuranceTier {
    LinkableTier0,
}

impl AssuranceTier {
    pub const fn api_label(self) -> &'static str {
        match self {
            AssuranceTier::LinkableTier0 => "LINKABLE_TIER_0",
        }
    }

    fn tag(self) -> u8 {
        match self {
            AssuranceTier::LinkableTier0 => 0,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            0 => Ok(Self::LinkableTier0),
            _ => Err(AttestError::UnsupportedAssuranceTier),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Recommend,
    Neutral,
    DoNotRecommend,
}

impl Verdict {
    fn tag(self) -> u8 {
        match self {
            Verdict::Recommend => 1,
            Verdict::Neutral => 2,
            Verdict::DoNotRecommend => 3,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            1 => Ok(Self::Recommend),
            2 => Ok(Self::Neutral),
            3 => Ok(Self::DoNotRecommend),
            _ => Err(AttestError::ReviewPayloadMismatch),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewPayload {
    pub verdict: Verdict,
    body: Vec<u8>,
}

impl ReviewPayload {
    pub fn new(verdict: Verdict, body: Vec<u8>) -> Result<Self> {
        if body.len() > MAX_REVIEW_BODY_BYTES {
            return Err(AttestError::LimitExceeded);
        }
        Ok(Self { verdict, body })
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    fn encode(&self, writer: &mut Writer) {
        writer.u8(self.verdict.tag());
        writer.bytes(&self.body);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self> {
        let verdict = Verdict::from_tag(reader.u8()?)?;
        let body = reader.bytes_limited(MAX_REVIEW_BODY_BYTES)?;
        Self::new(verdict, body)
    }

    fn hash(&self) -> [u8; 32] {
        let mut writer = Writer::new();
        writer.raw(REVIEW_PAYLOAD_DOMAIN);
        self.encode(&mut writer);
        HashAlgorithm::Blake3.digest(&writer.finish())
    }
}

/// The public review object. `reviewer` must be the grant's pairwise subject,
/// never a human-root DID. The holder token is intentionally absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedReviewV1 {
    pub tier: AssuranceTier,
    pub receipt_id: ReceiptId,
    pub reviewer: Did,
    pub review_subject: ReviewSubjectCommitment,
    pub payload: ReviewPayload,
    pub payload_hash: [u8; 32],
    pub created_at_epoch: u64,
    signature: Vec<IndexedSig>,
}

impl SignedReviewV1 {
    fn signing_bytes(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.raw(REVIEW_SIGNING_DOMAIN);
        self.encode_fields(&mut writer);
        writer.finish()
    }

    fn encode_fields(&self, writer: &mut Writer) {
        writer.u8(REVIEW_VERSION);
        writer.u8(self.tier.tag());
        encode_receipt_id(writer, &self.receipt_id);
        writer.bytes(self.reviewer.as_str().as_bytes());
        writer.raw(self.review_subject.as_bytes());
        self.payload.encode(writer);
        writer.raw(&self.payload_hash);
        writer.u64(self.created_at_epoch);
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        self.encode_fields(&mut writer);
        encode_indexed_signatures(&mut writer, &self.signature);
        writer.finish()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        if reader.u8()? != REVIEW_VERSION {
            return Err(AttestError::UnsupportedReviewVersion);
        }
        let tier = AssuranceTier::from_tag(reader.u8()?)?;
        let receipt_id = decode_receipt_id(&mut reader)?;
        let reviewer = parse_review_did(reader.bytes_limited(REVIEW_MAX_DID_BYTES)?)?;
        let review_subject = ReviewSubjectCommitment::from_bytes(reader.raw_array::<32>()?);
        let payload = ReviewPayload::decode(&mut reader)?;
        let payload_hash = reader.raw_array::<32>()?;
        let created_at_epoch = reader.u64()?;
        let signature = decode_indexed_signatures(&mut reader)?;
        reader.finish()?;
        if signature.is_empty() {
            return Err(AttestError::BadReviewerSignature);
        }
        let review = Self {
            tier,
            receipt_id,
            reviewer,
            review_subject,
            payload,
            payload_hash,
            created_at_epoch,
            signature,
        };
        if review.payload_hash != review.payload.hash() {
            return Err(AttestError::ReviewPayloadMismatch);
        }
        Ok(review)
    }
}

pub fn issue_tier0_review(
    receipt: &EngagementCompletionReceiptV1,
    grant: &EngagementGrant,
    holder_token: &EngagementHolderToken,
    reviewer: &Controller,
    payload: ReviewPayload,
    created_at_epoch: u64,
) -> Result<SignedReviewV1> {
    grant.check_wellformed()?;
    if reviewer.did().as_str() != grant.subject.as_str() {
        return Err(AttestError::ReviewerMismatch);
    }
    if receipt.provider.as_str() != grant.provider.as_str() {
        return Err(AttestError::ProviderMismatch);
    }
    if receipt.provider_declaration_id != grant.declaration {
        return Err(AttestError::DeclarationMismatch);
    }
    holder_token.verify(
        receipt.holder_commitment,
        &grant.subject,
        &grant.provider,
        &grant.declaration,
    )?;
    if created_at_epoch < receipt.completed_at_epoch || created_at_epoch >= receipt.expiry_epoch {
        return Err(AttestError::InvalidEpochWindow);
    }

    let mut review = SignedReviewV1 {
        tier: AssuranceTier::LinkableTier0,
        receipt_id: receipt.id()?,
        reviewer: reviewer.did(),
        review_subject: receipt.reviewable_subject,
        payload_hash: payload.hash(),
        payload,
        created_at_epoch,
        signature: Vec::new(),
    };
    review.signature = reviewer.sign_message(&review.signing_bytes());
    Ok(review)
}

/// Everything a verifier needs to check the public receipt against canonical
/// settlement and the review against its pairwise holder.
#[derive(Debug)]
pub struct Tier0ReviewVerification<'a, L: CanonicalLedgerView> {
    pub receipt: &'a EngagementCompletionReceiptV1,
    pub engagement: &'a Engagement,
    pub grant: &'a EngagementGrant,
    pub ledger: &'a L,
    pub provider_kel: &'a Kel,
    pub reviewer_kel: &'a Kel,
    pub holder_token: &'a EngagementHolderToken,
    pub now_ms: u64,
    pub epoch_length_ms: u64,
    pub expiry_epoch: u64,
    pub reviewable_subject: ReviewSubjectCommitment,
}

pub trait ReviewRegistry {
    /// Atomically records `key` if absent. Returns `true` only for the first
    /// observation. Implementations decide their own local/policy scope; this
    /// trait is not a canonical network registry.
    fn check_and_record(&mut self, key: [u8; 32]) -> bool;
}

#[derive(Debug, Default)]
pub struct InMemoryReviewRegistry {
    keys: HashSet<[u8; 32]>,
}

impl InMemoryReviewRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

impl ReviewRegistry for InMemoryReviewRegistry {
    fn check_and_record(&mut self, key: [u8; 32]) -> bool {
        self.keys.insert(key)
    }
}

pub fn verify_and_record_tier0_review(
    review: &SignedReviewV1,
    verification: &Tier0ReviewVerification<'_, impl CanonicalLedgerView>,
    registry: &mut impl ReviewRegistry,
) -> Result<()> {
    if review.tier != AssuranceTier::LinkableTier0 {
        return Err(AttestError::UnsupportedAssuranceTier);
    }

    let receipt_context = Tier0ReceiptContext {
        engagement: verification.engagement,
        grant: verification.grant,
        ledger: verification.ledger,
        now_ms: verification.now_ms,
        epoch_length_ms: verification.epoch_length_ms,
        expiry_epoch: verification.expiry_epoch,
        reviewable_subject: verification.reviewable_subject,
    };
    let receipt_id = verify_tier0_receipt(
        verification.receipt,
        &receipt_context,
        verification.provider_kel,
    )?;
    if review.receipt_id != receipt_id {
        return Err(AttestError::InvalidReceiptId);
    }
    if review.review_subject != verification.receipt.reviewable_subject
        || review.review_subject != verification.reviewable_subject
    {
        return Err(AttestError::ReviewSubjectMismatch);
    }
    if review.reviewer.as_str() != verification.grant.subject.as_str()
        || verification.reviewer_kel.did().as_str() != review.reviewer.as_str()
    {
        return Err(AttestError::ReviewerMismatch);
    }
    verification.holder_token.verify(
        verification.receipt.holder_commitment,
        &verification.grant.subject,
        &verification.grant.provider,
        &verification.grant.declaration,
    )?;
    if review.created_at_epoch < verification.receipt.completed_at_epoch
        || review.created_at_epoch >= verification.receipt.expiry_epoch
    {
        return Err(AttestError::InvalidEpochWindow);
    }
    if review.payload_hash != review.payload.hash() {
        return Err(AttestError::ReviewPayloadMismatch);
    }
    verification
        .reviewer_kel
        .verify_message(&review.signing_bytes(), &review.signature)
        .map_err(|_| AttestError::BadReviewerSignature)?;

    let key = review_registry_key(&review.receipt_id, review.review_subject);
    if !registry.check_and_record(key) {
        return Err(AttestError::DuplicateReview);
    }
    Ok(())
}

fn review_registry_key(receipt_id: &ReceiptId, subject: ReviewSubjectCommitment) -> [u8; 32] {
    let mut writer = Writer::new();
    writer.raw(REVIEW_REGISTRY_KEY_DOMAIN);
    writer.bytes(receipt_id.as_str().as_bytes());
    writer.raw(subject.as_bytes());
    HashAlgorithm::Blake3.digest(&writer.finish())
}
