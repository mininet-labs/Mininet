//! Run: cargo run -p mini-economy --example offline_credit
//! Synthetic local demonstration: no real network, money, or production policy.
use mini_crypto::{AeadKey, AeadSuite, SigningKey};
use mini_economy::{
    credit::{CreditPolicy, HumanShareCredit},
    Amount,
};
use mini_settlement::{credit_journal::FileCreditJournal, credit_permit::*};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const UNIT: u64 = 1_000_000;
    let issuer = SigningKey::from_seed(&[1; 32]); // Public demo seeds only.
    let borrower = SigningKey::from_seed(&[2; 32]);
    let subject = [3; 32];
    let network = [4; 32];
    let revision = [5; 32];
    let mut book = HumanShareCredit::new(CreditPolicy {
        network_id: network,
        revision,
        max_debt_per_human: 100 * UNIT,
        repayment_ppm: 1_000_000,
        max_permit_height_span: 100,
        max_accounts: 1,
        max_permits: 2,
        max_ious: 10,
    })?;
    book.enroll(subject, 100 * UNIT)?;
    let terms = CreditPermitTerms {
        network_id: network,
        policy_revision: revision,
        serial: [6; 32],
        borrower_key: borrower.verifying_key().to_bytes().try_into().unwrap(),
        limit_micro: 100 * UNIT,
        issued_height: 10,
        valid_through_height: 110,
    };
    let signed = sign_credit_permit(&issuer, terms.clone(), 100)?;
    let permit = verify_credit_permit(&signed, &issuer.verifying_key(), &network, 100)?;
    assert!(book.register_permit(subject, permit.clone(), 10)?);
    let key = AeadKey::generate(AeadSuite::DEFAULT)?;
    let directory = std::env::temp_dir().join(format!(
        "mini-credit-demo-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let mut journal = FileCreditJournal::create(&directory, permit.clone(), key.clone())?;
    for (amount, payee) in [(30, [7; 32]), (20, [8; 32]), (50, [9; 32])] {
        sign_next_credit_iou(&borrower, &permit, amount * UNIT, payee, &mut journal)?;
    }
    drop(journal);
    let mut journal = FileCreditJournal::open(&directory, permit.clone(), key)?;
    assert_eq!(
        sign_next_credit_iou(&borrower, &permit, UNIT, [7; 32], &mut journal),
        Err(CreditSignatureError::AllowanceExhausted)
    );
    println!(
        "Offline sender: 30 + 20 + 50 MINI signed; restart preserves usage; another 1 refused."
    );
    for coupon in journal.outbox()? {
        let verified = verify_credit_iou(&coupon, &permit, 11)?;
        assert!(book.record_iou(&verified, 11)?);
        assert!(!book.record_iou(&verified, 11)?);
    }
    assert_eq!(book.debt_micro(&subject), Some(100 * UNIT));
    assert_eq!(book.new_allowance_capacity(&subject)?, 0);
    println!("Reconnect: 100 MINI of IOUs recorded; copied submissions have no additional effect.");
    for (epoch, amount) in [(1, 60), (2, 40)] {
        let released = Amount::from_micro(u128::from(amount * UNIT));
        let (split, applied) = book.release_share(&subject, epoch, released)?;
        assert!(applied);
        assert_eq!(
            split.repayments.iter().map(|p| p.amount_micro).sum::<u64>(),
            amount * UNIT
        );
        assert!(!book.release_share(&subject, epoch, released)?.1);
    }
    assert_eq!(book.debt_micro(&subject), Some(0));
    assert_eq!(book.permit_remaining(permit.id()), Some(0));
    println!("Future releases: 60 then 40 MINI repay IOUs; old authorization remains exhausted.");
    let renewed = sign_credit_permit(
        &issuer,
        CreditPermitTerms {
            serial: [10; 32],
            issued_height: 12,
            valid_through_height: 112,
            ..terms
        },
        100,
    )?;
    let renewed = verify_credit_permit(&renewed, &issuer.verifying_key(), &network, 100)?;
    assert!(book.register_permit(subject, renewed.clone(), 12)?);
    assert_eq!(book.permit_remaining(renewed.id()), Some(100 * UNIT));
    println!("Online renewal: a distinct authorization now grants 100 MINI of capacity.");
    drop(journal);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
