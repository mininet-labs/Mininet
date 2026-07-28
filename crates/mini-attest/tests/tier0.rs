use did_mini::{Controller, Did, Kel};
use mini_attest::{
    issue_tier0_receipt, issue_tier0_review, verify_and_record_tier0_review, verify_tier0_receipt,
    AssuranceTier, AttestError, EngagementCompletionReceiptV1, EngagementHolderToken,
    InMemoryReviewRegistry, ReviewPayload, ReviewSubjectCommitment, SignedReviewV1,
    Tier0ReceiptContext, Tier0ReviewVerification, Verdict, MAX_REVIEW_BODY_BYTES,
};
use mini_crypto::SigningKey;
use mini_engagement::{accept, complete, Engagement};
use mini_objects::{ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_provider::{EngagementGrant, Permit};
use mini_settlement::{claim_digest, sign_claim, InMemoryLedgerView};

const EPOCH_LENGTH_MS: u64 = 100;
const EXPIRY_EPOCH: u64 = 20;
const NOW_MS: u64 = 800;

fn controller(seed: u8) -> Controller {
    Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap()
}

fn object_id(seed: u8, payload: &[u8]) -> ObjectId {
    let author = controller(seed);
    ObjectBuilder::new(ObjectType::Custom("mini-attest/test".to_string()))
        .payload(Payload::Public(payload.to_vec()))
        .sign(&author.did(), &author)
        .unwrap()
        .id()
        .clone()
}

struct Fixture {
    provider: Controller,
    reviewer: Controller,
    provider_kel: Kel,
    reviewer_kel: Kel,
    hidden_human_root: Did,
    holder_token: EngagementHolderToken,
    engagement: Engagement,
    grant: EngagementGrant,
    ledger: InMemoryLedgerView,
    subject: ReviewSubjectCommitment,
}

impl Fixture {
    fn new() -> Self {
        let provider = controller(10);
        let reviewer = controller(20);
        let hidden_human_root = controller(30).did();
        let terms = object_id(40, b"private engagement terms that must not be copied");
        let declaration = object_id(50, b"provider declaration");
        let holder_token = EngagementHolderToken::from_bytes([0x66; 32]);
        let holder_commitment = holder_token.commit(&reviewer.did(), &provider.did(), &declaration);

        let payer_key = SigningKey::from_seed(&[0x77; 32]);
        let claim = sign_claim(
            &payer_key,
            b"private-payee-address",
            987_654_321,
            4,
            10_000,
            b"private-chain-head",
            0,
        )
        .unwrap();
        let payer = controller(60).did();
        let engagement = Engagement::offer(terms, payer, provider.did(), claim, 10_000);
        let engagement = accept(engagement, provider.did(), 500).unwrap();
        let engagement = complete(engagement, 700).unwrap();

        let grant = EngagementGrant {
            subject: reviewer.did(),
            provider: provider.did(),
            declaration,
            permits: vec![Permit::ReadAttestation {
                kind: mini_provider::AttestationKind::FullText,
            }],
            not_before_ms: 100,
            not_after_ms: 5_000,
            holder_commitment: *holder_commitment.as_bytes(),
        };
        let mut ledger = InMemoryLedgerView::new();
        ledger.finalize(
            &engagement.escrow_claim.payer,
            engagement.escrow_claim.sequence,
            claim_digest(&engagement.escrow_claim),
        );

        Self {
            provider_kel: provider.kel(),
            reviewer_kel: reviewer.kel(),
            provider,
            reviewer,
            hidden_human_root,
            holder_token,
            engagement,
            grant,
            ledger,
            subject: ReviewSubjectCommitment::derive(b"provider/product/42").unwrap(),
        }
    }

    fn receipt_context(&self) -> Tier0ReceiptContext<'_, InMemoryLedgerView> {
        Tier0ReceiptContext {
            engagement: &self.engagement,
            grant: &self.grant,
            ledger: &self.ledger,
            now_ms: NOW_MS,
            epoch_length_ms: EPOCH_LENGTH_MS,
            expiry_epoch: EXPIRY_EPOCH,
            reviewable_subject: self.subject,
        }
    }

    fn receipt(&self) -> EngagementCompletionReceiptV1 {
        issue_tier0_receipt(&self.receipt_context(), &self.provider).unwrap()
    }

    fn review(&self, receipt: &EngagementCompletionReceiptV1) -> SignedReviewV1 {
        issue_tier0_review(
            receipt,
            &self.grant,
            &self.holder_token,
            &self.reviewer,
            ReviewPayload::new(Verdict::Recommend, b"delivered as promised".to_vec()).unwrap(),
            8,
        )
        .unwrap()
    }

    fn verification<'a>(
        &'a self,
        receipt: &'a EngagementCompletionReceiptV1,
    ) -> Tier0ReviewVerification<'a, InMemoryLedgerView> {
        Tier0ReviewVerification {
            receipt,
            engagement: &self.engagement,
            grant: &self.grant,
            ledger: &self.ledger,
            provider_kel: &self.provider_kel,
            reviewer_kel: &self.reviewer_kel,
            holder_token: &self.holder_token,
            now_ms: NOW_MS,
            epoch_length_ms: EPOCH_LENGTH_MS,
            expiry_epoch: EXPIRY_EPOCH,
            reviewable_subject: self.subject,
        }
    }
}

#[test]
fn happy_path_round_trips_and_records_once() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let receipt_bytes = receipt.to_bytes();
    let decoded_receipt = EngagementCompletionReceiptV1::from_bytes(&receipt_bytes).unwrap();
    assert_eq!(decoded_receipt, receipt);
    assert_eq!(decoded_receipt.id().unwrap(), receipt.id().unwrap());

    let review = fixture.review(&receipt);
    let review_bytes = review.to_bytes();
    let decoded_review = SignedReviewV1::from_bytes(&review_bytes).unwrap();
    assert_eq!(decoded_review, review);
    assert_eq!(
        decoded_review.tier.api_label(),
        AssuranceTier::LinkableTier0.api_label()
    );

    let mut registry = InMemoryReviewRegistry::new();
    verify_and_record_tier0_review(
        &decoded_review,
        &fixture.verification(&decoded_receipt),
        &mut registry,
    )
    .unwrap();
    assert_eq!(registry.len(), 1);
}

#[test]
fn public_receipt_and_review_exclude_private_payment_and_human_root_material() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let review = fixture.review(&receipt);
    let public = [receipt.to_bytes(), review.to_bytes()].concat();

    for forbidden in [
        fixture.engagement.escrow_claim.payer.as_slice(),
        fixture.engagement.escrow_claim.payee.as_slice(),
        b"private-chain-head".as_slice(),
        fixture.hidden_human_root.as_str().as_bytes(),
        b"private engagement terms that must not be copied".as_slice(),
    ] {
        assert!(
            !public
                .windows(forbidden.len())
                .any(|window| window == forbidden),
            "public artifact leaked forbidden bytes"
        );
    }
    assert!(
        !String::from_utf8_lossy(&public).contains("987654321"),
        "public artifact leaked the decimal payment amount"
    );
}

#[test]
fn receipt_cannot_issue_until_canonical_completion() {
    let fixture = Fixture::new();
    let empty_ledger = InMemoryLedgerView::new();
    let context = Tier0ReceiptContext {
        ledger: &empty_ledger,
        ..fixture.receipt_context()
    };
    assert_eq!(
        issue_tier0_receipt(&context, &fixture.provider),
        Err(AttestError::EngagementNotCanonicallyComplete)
    );
}

#[test]
fn conflicting_canonical_claim_cannot_issue_a_receipt() {
    let fixture = Fixture::new();
    let other_key = SigningKey::from_seed(&[0x77; 32]);
    let conflicting = sign_claim(
        &other_key,
        b"different-payee",
        1,
        fixture.engagement.escrow_claim.sequence,
        10_000,
        b"private-chain-head",
        0,
    )
    .unwrap();
    let mut conflict_ledger = InMemoryLedgerView::new();
    conflict_ledger.finalize(
        &conflicting.payer,
        conflicting.sequence,
        claim_digest(&conflicting),
    );
    let context = Tier0ReceiptContext {
        ledger: &conflict_ledger,
        ..fixture.receipt_context()
    };
    assert_eq!(
        issue_tier0_receipt(&context, &fixture.provider),
        Err(AttestError::EngagementNotCanonicallyComplete)
    );
}

#[test]
fn provider_cannot_issue_for_someone_elses_engagement() {
    let fixture = Fixture::new();
    let impostor = controller(90);
    assert_eq!(
        issue_tier0_receipt(&fixture.receipt_context(), &impostor),
        Err(AttestError::ProviderMismatch)
    );
}

#[test]
fn grant_must_have_been_active_when_the_engagement_completed() {
    let mut fixture = Fixture::new();
    fixture.grant.not_after_ms = 700;
    assert_eq!(
        issue_tier0_receipt(&fixture.receipt_context(), &fixture.provider),
        Err(AttestError::GrantInactiveAtCompletion)
    );
}

#[test]
fn wrong_holder_token_is_rejected() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let wrong = EngagementHolderToken::from_bytes([0x99; 32]);
    assert_eq!(
        issue_tier0_review(
            &receipt,
            &fixture.grant,
            &wrong,
            &fixture.reviewer,
            ReviewPayload::new(Verdict::Neutral, vec![]).unwrap(),
            8,
        ),
        Err(AttestError::HolderCommitmentMismatch)
    );
}

#[test]
fn a_different_pairwise_pseudonym_cannot_sign_the_review() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let outsider = controller(91);
    assert_eq!(
        issue_tier0_review(
            &receipt,
            &fixture.grant,
            &fixture.holder_token,
            &outsider,
            ReviewPayload::new(Verdict::Neutral, vec![]).unwrap(),
            8,
        ),
        Err(AttestError::ReviewerMismatch)
    );
}

#[test]
fn duplicate_review_is_rejected_after_full_verification() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let review = fixture.review(&receipt);
    let mut registry = InMemoryReviewRegistry::new();
    let verification = fixture.verification(&receipt);
    verify_and_record_tier0_review(&review, &verification, &mut registry).unwrap();
    assert_eq!(
        verify_and_record_tier0_review(&review, &verification, &mut registry),
        Err(AttestError::DuplicateReview)
    );
    assert_eq!(registry.len(), 1);
}

#[test]
fn expired_receipt_is_rejected_without_recording() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let review = fixture.review(&receipt);
    let verification = Tier0ReviewVerification {
        now_ms: EXPIRY_EPOCH * EPOCH_LENGTH_MS,
        ..fixture.verification(&receipt)
    };
    let mut registry = InMemoryReviewRegistry::new();
    assert_eq!(
        verify_and_record_tier0_review(&review, &verification, &mut registry),
        Err(AttestError::ReceiptExpired)
    );
    assert!(registry.is_empty());
}

#[test]
fn failed_verification_does_not_consume_the_duplicate_key() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let review = fixture.review(&receipt);
    let empty_ledger = InMemoryLedgerView::new();
    let bad_verification = Tier0ReviewVerification {
        ledger: &empty_ledger,
        ..fixture.verification(&receipt)
    };
    let mut registry = InMemoryReviewRegistry::new();
    assert_eq!(
        verify_and_record_tier0_review(&review, &bad_verification, &mut registry),
        Err(AttestError::EngagementNotCanonicallyComplete)
    );
    assert!(registry.is_empty());

    verify_and_record_tier0_review(&review, &fixture.verification(&receipt), &mut registry)
        .unwrap();
    assert_eq!(registry.len(), 1);
}

#[test]
fn tampered_receipt_signature_is_rejected() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let mut bytes = receipt.to_bytes();
    *bytes.last_mut().unwrap() ^= 0x01;
    let tampered = EngagementCompletionReceiptV1::from_bytes(&bytes).unwrap();
    assert_eq!(
        verify_tier0_receipt(&tampered, &fixture.receipt_context(), &fixture.provider_kel),
        Err(AttestError::BadProviderSignature)
    );
}

#[test]
fn tampered_review_payload_is_rejected_during_decode() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let review = fixture.review(&receipt);
    let mut bytes = review.to_bytes();
    let needle = b"delivered as promised";
    let position = bytes
        .windows(needle.len())
        .position(|window| window == needle)
        .unwrap();
    bytes[position] ^= 0x01;
    assert_eq!(
        SignedReviewV1::from_bytes(&bytes),
        Err(AttestError::ReviewPayloadMismatch)
    );
}

#[test]
fn trailing_and_truncated_wire_data_are_rejected() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let mut trailing = receipt.to_bytes();
    trailing.push(0);
    assert_eq!(
        EngagementCompletionReceiptV1::from_bytes(&trailing),
        Err(AttestError::TrailingBytes)
    );

    let review = fixture.review(&receipt);
    let mut truncated = review.to_bytes();
    truncated.truncate(truncated.len() - 5);
    assert_eq!(
        SignedReviewV1::from_bytes(&truncated),
        Err(AttestError::Truncated)
    );
}

#[test]
fn oversized_review_body_is_rejected_before_signing() {
    assert_eq!(
        ReviewPayload::new(Verdict::Recommend, vec![0; MAX_REVIEW_BODY_BYTES + 1]),
        Err(AttestError::LimitExceeded)
    );
}

#[test]
fn receipt_id_changes_if_signed_receipt_bytes_change() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    let original_id = receipt.id().unwrap();
    let mut bytes = receipt.to_bytes();
    *bytes.last_mut().unwrap() ^= 0x01;
    let changed = EngagementCompletionReceiptV1::from_bytes(&bytes).unwrap();
    assert_ne!(original_id, changed.id().unwrap());
}
