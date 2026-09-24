use mini_crypto::SigningKey;
use mini_settlement::credit_permit::*;

fn signed() -> (SigningKey, SigningKey, SignedCreditPermit) {
    let issuer = SigningKey::from_seed(&[1; 32]);
    let borrower = SigningKey::from_seed(&[2; 32]);
    let permit = sign_credit_permit(
        &issuer,
        CreditPermitTerms {
            network_id: [3; 32],
            policy_revision: [4; 32],
            serial: [5; 32],
            borrower_key: borrower.verifying_key().to_bytes().try_into().unwrap(),
            limit_micro: 100,
            issued_height: 10,
            valid_through_height: 20,
        },
        10,
    )
    .unwrap();
    (issuer, borrower, permit)
}

#[test]
fn offline_coupon_binds_person_permit_amount_and_recipient() {
    let (issuer, borrower, signed) = signed();
    let permit = verify_credit_permit(&signed, &issuer.verifying_key(), &[3; 32], 10).unwrap();
    let iou = sign_credit_iou(
        &borrower,
        &permit,
        CreditIouTerms {
            permit_id: *permit.id(),
            sequence: 0,
            payee_commitment: [6; 32],
            amount_micro: 50,
        },
    )
    .unwrap();
    let verified = verify_credit_iou(&iou, &permit, 15).unwrap();
    assert_eq!(
        SignedCreditIou::decode(&iou.encode().unwrap()).unwrap(),
        iou
    );
    assert_eq!(
        SignedCreditPermit::decode(&signed.encode().unwrap()).unwrap(),
        signed
    );
    for bytes in [iou.encode().unwrap(), signed.encode().unwrap()] {
        for len in 0..bytes.len() {
            assert!(SignedCreditIou::decode(&bytes[..len]).is_err());
            assert!(SignedCreditPermit::decode(&bytes[..len]).is_err());
        }
        let mut extra = bytes;
        extra.push(0);
        assert!(SignedCreditIou::decode(&extra).is_err());
        assert!(SignedCreditPermit::decode(&extra).is_err());
    }
    assert_eq!(verified.terms().amount_micro, 50);
    for n in 0..3 {
        let mut bad = iou.clone();
        match n {
            0 => bad.terms.amount_micro += 1,
            1 => bad.terms.payee_commitment = [7; 32],
            _ => bad.terms.sequence += 1,
        }
        assert_eq!(
            verify_credit_iou(&bad, &permit, 15),
            Err(CreditSignatureError::BadSignature)
        );
    }
    assert!(sign_credit_iou(&issuer, &permit, iou.terms.clone()).is_err());
    assert!(verify_credit_iou(&iou, &permit, 9).is_err());
    assert!(verify_credit_iou(&iou, &permit, 21).is_err());
    assert!(verify_credit_iou(&iou, &permit, 20).is_ok());
    assert_eq!(format!("{iou:?}"), "SignedCreditIou(<private>)");
    assert_eq!(format!("{permit:?}"), "VerifiedCreditPermit(<private>)");
}

#[test]
fn online_signature_cannot_be_forged_rebound_or_used_on_another_network() {
    let (issuer, _, signed) = signed();
    assert_eq!(
        verify_credit_permit(&signed, &issuer.verifying_key(), &[9; 32], 10),
        Err(CreditSignatureError::WrongNetwork)
    );
    assert!(verify_credit_permit(
        &signed,
        &SigningKey::from_seed(&[8; 32]).verifying_key(),
        &[3; 32],
        10
    )
    .is_err());
    for n in 0..6 {
        let mut forged = signed.clone();
        match n {
            0 => forged.terms.limit_micro += 1,
            1 => forged.terms.serial = [8; 32],
            2 => forged.terms.policy_revision = [8; 32],
            3 => forged.terms.issued_height += 1,
            4 => forged.terms.valid_through_height -= 1,
            _ => {
                forged.terms.borrower_key = SigningKey::from_seed(&[8; 32])
                    .verifying_key()
                    .to_bytes()
                    .try_into()
                    .unwrap()
            }
        }
        assert_eq!(
            verify_credit_permit(&forged, &issuer.verifying_key(), &[3; 32], 10),
            Err(CreditSignatureError::BadSignature)
        );
    }
}

#[test]
fn refreshed_signature_does_not_authorize_an_old_coupon() {
    let (issuer, borrower, signed) = signed();
    let permit = verify_credit_permit(&signed, &issuer.verifying_key(), &[3; 32], 10).unwrap();
    let iou = sign_credit_iou(
        &borrower,
        &permit,
        CreditIouTerms {
            permit_id: *permit.id(),
            sequence: 0,
            payee_commitment: [6; 32],
            amount_micro: 50,
        },
    )
    .unwrap();
    let mut terms = signed.terms;
    terms.serial = [7; 32];
    let renewed = sign_credit_permit(&issuer, terms, 10).unwrap();
    let renewed = verify_credit_permit(&renewed, &issuer.verifying_key(), &[3; 32], 10).unwrap();
    assert_eq!(
        verify_credit_iou(&iou, &renewed, 15),
        Err(CreditSignatureError::WrongPermit)
    );
    let mut excessive = iou.terms;
    excessive.amount_micro = 101;
    assert!(sign_credit_iou(&borrower, &permit, excessive).is_err());
}

#[derive(Default)]
struct Journal {
    used: CreditUse,
    saved: Vec<SignedCreditIou>,
    fail: bool,
}
impl CreditUseJournal for Journal {
    fn load(&self, _: &[u8; 32]) -> Result<CreditUse, CreditSignatureError> {
        Ok(self.used)
    }
    fn compare_and_record(
        &mut self,
        prior: CreditUse,
        next: CreditUse,
        coupon: &SignedCreditIou,
    ) -> Result<bool, CreditSignatureError> {
        if self.fail || self.used != prior {
            return Ok(false);
        }
        self.used = next;
        self.saved.push(coupon.clone());
        Ok(true)
    }
}

#[test]
fn honest_wallet_will_not_sign_beyond_allowance_or_return_unjournaled_coupon() {
    let (issuer, borrower, signed) = signed();
    let permit = verify_credit_permit(&signed, &issuer.verifying_key(), &[3; 32], 10).unwrap();
    let mut journal = Journal::default();
    let first = sign_next_credit_iou(&borrower, &permit, 60, [6; 32], &mut journal).unwrap();
    assert_eq!(first.terms.sequence, 0);
    assert_eq!(journal.saved.len(), 1);
    assert_eq!(
        sign_next_credit_iou(&borrower, &permit, 41, [6; 32], &mut journal),
        Err(CreditSignatureError::AllowanceExhausted)
    );
    journal.fail = true;
    assert_eq!(
        sign_next_credit_iou(&borrower, &permit, 40, [6; 32], &mut journal),
        Err(CreditSignatureError::JournalFailure)
    );
    assert_eq!(journal.used.signed_micro, 60);
    assert_eq!(journal.saved.len(), 1);
    journal.fail = false;
    let second = sign_next_credit_iou(&borrower, &permit, 40, [6; 32], &mut journal).unwrap();
    assert_eq!(second.terms.sequence, 1);
    assert_eq!(journal.used.signed_micro, 100);
    assert_eq!(
        sign_next_credit_iou(&borrower, &permit, 1, [6; 32], &mut journal),
        Err(CreditSignatureError::AllowanceExhausted)
    );
}

#[test]
fn durable_sender_budget_survives_restart_and_refuses_tampering() {
    use mini_crypto::{AeadKey, AeadSuite};
    use mini_settlement::credit_journal::FileCreditJournal;
    let (issuer, borrower, signed) = signed();
    let permit = verify_credit_permit(&signed, &issuer.verifying_key(), &[3; 32], 10).unwrap();
    let key = AeadKey::generate(AeadSuite::DEFAULT).unwrap();
    let dir = std::env::temp_dir().join(format!(
        "mini-credit-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut first = FileCreditJournal::create(&dir, permit.clone(), key.clone()).unwrap();
    let mut second = FileCreditJournal::open(&dir, permit.clone(), key.clone()).unwrap();
    let stale = second.load(permit.id()).unwrap();
    let a = sign_next_credit_iou(&borrower, &permit, 30, [6; 32], &mut first).unwrap();
    assert!(!second
        .compare_and_record(
            stale,
            CreditUse {
                signed_micro: 30,
                next_sequence: 1
            },
            &a
        )
        .unwrap());
    let b = sign_next_credit_iou(&borrower, &permit, 20, [7; 32], &mut second).unwrap();
    drop(first);
    drop(second);
    let mut recovered = FileCreditJournal::open(&dir, permit.clone(), key.clone()).unwrap();
    assert_eq!(recovered.outbox().unwrap(), vec![a, b]);
    sign_next_credit_iou(&borrower, &permit, 50, [8; 32], &mut recovered).unwrap();
    assert_eq!(
        sign_next_credit_iou(&borrower, &permit, 1, [8; 32], &mut recovered),
        Err(CreditSignatureError::AllowanceExhausted)
    );
    assert!(FileCreditJournal::create(&dir, permit.clone(), key.clone()).is_err());
    assert!(FileCreditJournal::open(
        &dir,
        permit.clone(),
        AeadKey::generate(AeadSuite::DEFAULT).unwrap()
    )
    .is_err());
    std::fs::write(dir.join("credit.enc"), [0; 32]).unwrap();
    assert!(FileCreditJournal::open(&dir, permit.clone(), key.clone()).is_err());
    assert!(sign_next_credit_iou(&borrower, &permit, 1, [8; 32], &mut recovered).is_err());
    std::fs::remove_file(dir.join("credit.enc")).unwrap();
    assert!(FileCreditJournal::open(&dir, permit.clone(), key.clone()).is_err());
    assert!(FileCreditJournal::create(&dir, permit, key).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
