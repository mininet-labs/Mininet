use mini_crypto::SigningKey;
use mini_economy::{
    credit::{CreditError, CreditPolicy, HumanShareCredit},
    Amount,
};
use mini_settlement::credit_permit::{
    sign_credit_iou, sign_credit_permit, verify_credit_iou, verify_credit_permit, CreditIouTerms,
    CreditPermitTerms, VerifiedCreditIou, VerifiedCreditPermit,
};

const SUBJECT: [u8; 32] = [1; 32];
const NETWORK: [u8; 32] = [2; 32];
const REVISION: [u8; 32] = [3; 32];
fn policy() -> CreditPolicy {
    CreditPolicy {
        network_id: NETWORK,
        revision: REVISION,
        max_debt_per_human: 100,
        repayment_ppm: 500_000,
        max_permit_height_span: 100,
        max_accounts: 10,
        max_permits: 10,
        max_ious: 100,
    }
}
fn book() -> HumanShareCredit {
    let mut b = HumanShareCredit::new(policy()).unwrap();
    b.enroll(SUBJECT, 100).unwrap();
    b
}
fn permit(serial: u8, limit: u64, issued: u64) -> VerifiedCreditPermit {
    let issuer = SigningKey::from_seed(&[8; 32]);
    let borrower = SigningKey::from_seed(&[9; 32]);
    let signed = sign_credit_permit(
        &issuer,
        CreditPermitTerms {
            network_id: NETWORK,
            policy_revision: REVISION,
            serial: [serial; 32],
            borrower_key: borrower.verifying_key().to_bytes().try_into().unwrap(),
            limit_micro: limit,
            issued_height: issued,
            valid_through_height: issued + 100,
        },
        100,
    )
    .unwrap();
    verify_credit_permit(&signed, &issuer.verifying_key(), &NETWORK, 100).unwrap()
}
fn iou(permit: &VerifiedCreditPermit, seq: u64, amount: u64, payee: u8) -> VerifiedCreditIou {
    let signed = sign_credit_iou(
        &SigningKey::from_seed(&[9; 32]),
        permit,
        CreditIouTerms {
            permit_id: *permit.id(),
            sequence: seq,
            payee_commitment: [payee; 32],
            amount_micro: amount,
        },
    )
    .unwrap();
    verify_credit_iou(&signed, permit, permit.terms().issued_height).unwrap()
}

#[test]
fn empty_account_borrows_then_future_share_pays_recipients_without_minting() {
    let mut b = book();
    let p = permit(1, 100, 10);
    b.register_permit(SUBJECT, p.clone(), 10).unwrap();
    let a = iou(&p, 1, 60, 4);
    let c = iou(&p, 2, 30, 5);
    assert!(b.record_iou(&a, 11).unwrap());
    assert!(b.record_iou(&c, 11).unwrap());
    assert_eq!(b.debt_micro(&SUBJECT), Some(90));
    let (split, applied) = b
        .release_share(&SUBJECT, 0, Amount::from_micro(100))
        .unwrap();
    assert!(applied);
    assert_eq!(split.repayments.len(), 1);
    assert_eq!(split.repayments[0].iou_digest, *a.digest());
    assert_eq!(split.repayments[0].amount_micro, 50);
    assert_eq!(split.recipient_available.as_micro(), 50);
    let (next, _) = b
        .release_share(&SUBJECT, 1, Amount::from_micro(100))
        .unwrap();
    assert_eq!(
        next.repayments
            .iter()
            .map(|p| p.amount_micro)
            .collect::<Vec<_>>(),
        vec![10, 30]
    );
    assert_eq!(next.recipient_available.as_micro(), 60);
    assert_eq!(b.debt_micro(&SUBJECT), Some(0));
    assert_eq!(
        b.permit_remaining(p.id()),
        Some(10),
        "repayment does not recharge an old signature"
    );
}

#[test]
fn copied_coupon_redeems_once_and_altered_coupon_conflicts() {
    let mut b = book();
    let p = permit(1, 100, 10);
    b.register_permit(SUBJECT, p.clone(), 10).unwrap();
    let a = iou(&p, 0, 60, 4);
    assert!(b.record_iou(&a, 11).unwrap());
    let snapshot = b.clone();
    assert!(!b.record_iou(&a, 200).unwrap());
    assert_eq!(b, snapshot);
    assert_eq!(
        b.record_iou(&iou(&p, 0, 60, 5), 11),
        Err(CreditError::IouConflict)
    );
    assert_eq!(b, snapshot);
    assert_eq!(
        b.record_iou(&iou(&p, 1, 50, 5), 11),
        Err(CreditError::LimitExceeded)
    );
    assert_eq!(b, snapshot);
}

#[test]
fn renewal_and_multiple_devices_cannot_reset_unsubmitted_allowance_or_debt() {
    let mut b = book();
    let p = permit(1, 100, 10);
    b.register_permit(SUBJECT, p.clone(), 10).unwrap();
    assert_eq!(b.new_allowance_capacity(&SUBJECT).unwrap(), 0);
    assert_eq!(
        b.register_permit(SUBJECT, permit(2, 1, 11), 11),
        Err(CreditError::LimitExceeded)
    );
    b.record_iou(&iou(&p, 0, 60, 4), 11).unwrap();
    assert_eq!(b.new_allowance_capacity(&SUBJECT).unwrap(), 0);
    b.expire_permit(p.id(), 111).unwrap();
    assert_eq!(b.new_allowance_capacity(&SUBJECT).unwrap(), 40);
    assert_eq!(
        b.register_permit(SUBJECT, permit(3, 41, 111), 111),
        Err(CreditError::LimitExceeded)
    );
    b.register_permit(SUBJECT, permit(4, 40, 111), 111).unwrap();
    assert_eq!(b.debt_micro(&SUBJECT), Some(60));
    assert_eq!(b.enroll(SUBJECT, 100), Err(CreditError::LimitExceeded));
}

#[test]
fn expiration_is_canonical_and_does_not_forgive_existing_debt() {
    let mut b = book();
    let p = permit(1, 100, 10);
    b.register_permit(SUBJECT, p.clone(), 10).unwrap();
    b.record_iou(&iou(&p, 0, 20, 4), 110).unwrap();
    assert_eq!(b.expire_permit(p.id(), 110), Err(CreditError::NotExpired));
    b.expire_permit(p.id(), 111).unwrap();
    assert_eq!(
        b.record_iou(&iou(&p, 1, 20, 4), 111),
        Err(CreditError::OutsideWindow)
    );
    assert_eq!(b.debt_micro(&SUBJECT), Some(20));
}

#[test]
fn share_replay_and_reordered_epochs_cannot_pay_twice() {
    let mut b = book();
    let p = permit(1, 100, 10);
    b.register_permit(SUBJECT, p.clone(), 10).unwrap();
    b.record_iou(&iou(&p, 0, 80, 4), 11).unwrap();
    let (first, applied) = b
        .release_share(&SUBJECT, 3, Amount::from_micro(40))
        .unwrap();
    assert!(applied);
    let snapshot = b.clone();
    assert_eq!(
        b.release_share(&SUBJECT, 3, Amount::from_micro(40))
            .unwrap(),
        (first, false)
    );
    assert_eq!(b, snapshot);
    assert_eq!(
        b.release_share(&SUBJECT, 3, Amount::from_micro(41)),
        Err(CreditError::ReplayMismatch)
    );
    assert_eq!(
        b.release_share(&SUBJECT, 2, Amount::from_micro(40)),
        Err(CreditError::InvalidEpoch)
    );
    assert_eq!(b, snapshot);
}

#[test]
fn tiny_share_releases_accumulate_fractional_repayment_without_starvation() {
    let mut b = book();
    let p = permit(1, 100, 10);
    b.register_permit(SUBJECT, p.clone(), 10).unwrap();
    b.record_iou(&iou(&p, 0, 80, 4), 11).unwrap();
    let mut total = 0;
    for epoch in 0..100 {
        let (split, _) = b
            .release_share(&SUBJECT, epoch, Amount::from_micro(1))
            .unwrap();
        total += split.repayments.iter().map(|p| p.amount_micro).sum::<u64>();
        assert_eq!(
            split.recipient_available.as_micro()
                + split
                    .repayments
                    .iter()
                    .map(|p| u128::from(p.amount_micro))
                    .sum::<u128>(),
            1
        );
    }
    assert_eq!(total, 50);
    assert_eq!(b.debt_micro(&SUBJECT), Some(30));
}

#[test]
fn maximum_share_amount_does_not_overflow_or_take_more_than_debt() {
    let mut b = book();
    let p = permit(1, 100, 10);
    b.register_permit(SUBJECT, p.clone(), 10).unwrap();
    b.record_iou(&iou(&p, 0, 80, 4), 11).unwrap();
    let (split, _) = b
        .release_share(&SUBJECT, 0, Amount::from_micro(u128::MAX))
        .unwrap();
    assert_eq!(split.recipient_available.as_micro(), u128::MAX - 80);
    assert_eq!(split.repayments[0].amount_micro, 80);
}

#[test]
fn exact_permit_replay_is_inert_and_budget_is_not_shared_between_people() {
    let mut b = book();
    let p = permit(1, 100, 10);
    b.register_permit(SUBJECT, p.clone(), 10).unwrap();
    let snapshot = b.clone();
    assert!(!b.register_permit(SUBJECT, p.clone(), 10).unwrap());
    assert_eq!(b, snapshot);
    b.enroll([2; 32], 100).unwrap();
    assert_eq!(
        b.register_permit([2; 32], p, 10),
        Err(CreditError::PermitConflict)
    );
}

#[test]
fn policy_capacity_and_network_fail_closed() {
    let mut zero = policy();
    zero.repayment_ppm = 0;
    assert!(HumanShareCredit::new(zero).is_err());
    let mut other = policy();
    other.network_id = [99; 32];
    let mut b = HumanShareCredit::new(other).unwrap();
    b.enroll(SUBJECT, 100).unwrap();
    assert_eq!(
        b.register_permit(SUBJECT, permit(1, 100, 10), 10),
        Err(CreditError::InvalidTerms)
    );
    let mut limited = policy();
    limited.max_ious = 1;
    let mut b = HumanShareCredit::new(limited).unwrap();
    b.enroll(SUBJECT, 100).unwrap();
    let p = permit(1, 100, 10);
    b.register_permit(SUBJECT, p.clone(), 10).unwrap();
    b.record_iou(&iou(&p, 0, 1, 4), 11).unwrap();
    let snapshot = b.clone();
    assert_eq!(
        b.record_iou(&iou(&p, 1, 1, 4), 11),
        Err(CreditError::LimitExceeded)
    );
    assert_eq!(b, snapshot);
}
