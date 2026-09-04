//! Amount disclosure (roadmap R6), and the thing it must never be allowed
//! to imply.
//!
//! A view key makes an account's income *enumerable*. It does not make it
//! *addable*, because it does not open a Pedersen commitment. R6 closes
//! that — and the interesting tests here are not "an opening verifies",
//! which is arithmetic, but the ones that stop a partial disclosure from
//! reading like a complete one.

mod support;

use mini_private_payment::{
    audit_amounts, verify, verify_disclosure, AcknowledgedAmountDisclosure,
    AcknowledgedIrreversibleDisclosure, AmountDisclosure, PrivatePaymentError, ViewKeyDisclosure,
};
use mini_value::StealthKeypair;
use support::{payment_to, recipient, NETWORK};

fn acknowledged() -> AcknowledgedAmountDisclosure {
    AcknowledgedAmountDisclosure::new(AcknowledgedAmountDisclosure::REQUIRED_ACKNOWLEDGEMENT)
        .expect("the exact phrase")
}

fn disclose(account: &StealthKeypair) -> ViewKeyDisclosure {
    ViewKeyDisclosure::create(
        account.spend_public_bytes().to_vec(),
        account.view_public_bytes().to_vec(),
        account.view_secret_bytes().to_vec(),
        b"treasury disbursements, D-0073".to_vec(),
        1_700_000_000_000,
        &AcknowledgedIrreversibleDisclosure::new(
            AcknowledgedIrreversibleDisclosure::REQUIRED_ACKNOWLEDGEMENT,
        )
        .expect("the exact phrase"),
    )
}

/// Receive a payment and produce the opening for it, the way a treasury
/// would: scan with the view key, read the note, publish the opening.
fn received(
    account: &StealthKeypair,
    amount: u64,
    purpose: &[u8],
) -> (mini_private_payment::VerifiedPrivateClaim, AmountDisclosure) {
    let (claim, _) = payment_to(account, amount, purpose);
    let verified = verify(&claim, &NETWORK).unwrap();
    let found = mini_private_payment::scan_one(
        &account.view_secret_bytes(),
        &account.spend_public_bytes(),
        &verified,
    )
    .unwrap()
    .pop()
    .expect("addressed here");
    let opening =
        AmountDisclosure::create(&verified, found.output_index, &found.note, &acknowledged())
            .unwrap();
    (verified, opening)
}

#[test]
fn an_opening_proves_the_amount_against_the_commitment() {
    let treasury = recipient();
    let (claim, opening) = received(&treasury, 4_000, b"grant:mini-forge");

    assert_eq!(opening.open_against(&claim).unwrap(), 4_000);
    assert_eq!(opening.claimed_amount_micro(), 4_000);
    assert_eq!(opening.claim_digest(), claim.transcript_digest());
}

#[test]
fn a_tampered_amount_does_not_open_its_commitment() {
    // Pedersen commitments are binding: there is no second (v, b) pair a
    // discloser could substitute. Asserted rather than argued, by editing
    // the wire bytes -- the one place a value could be changed after the
    // constructor refused to build a wrong one.
    let treasury = recipient();
    let (claim, opening) = received(&treasury, 4_000, b"grant");

    let mut bytes = opening.encode();
    let len = bytes.len();
    // The amount sits immediately before the 32-byte blinding factor.
    bytes[len - 33] ^= 0x01;
    let forged = AmountDisclosure::decode(&bytes).unwrap();

    assert_ne!(forged.claimed_amount_micro(), 4_000);
    assert_eq!(
        forged.open_against(&claim),
        Err(PrivatePaymentError::DisclosedAmountMismatch)
    );
}

#[test]
fn an_opening_cannot_be_moved_to_another_claim_or_another_output() {
    let treasury = recipient();
    let (first, opening) = received(&treasury, 4_000, b"a");
    let (second, _) = received(&treasury, 9_000, b"b");

    assert!(opening.open_against(&first).is_ok());
    assert_eq!(
        opening.open_against(&second),
        Err(PrivatePaymentError::DisclosedAmountMismatch),
        "an opening names the claim it belongs to"
    );
}

#[test]
fn an_opening_that_would_not_verify_is_refused_at_creation() {
    // Publishing a broken opening would put an official-looking number into
    // the world and make every auditor discover independently that it is
    // wrong. Caught where it is cheap instead.
    let treasury = recipient();
    let (claim, _) = payment_to(&treasury, 1_000, b"x");
    let verified = verify(&claim, &NETWORK).unwrap();
    let found = mini_private_payment::scan_one(
        &treasury.view_secret_bytes(),
        &treasury.spend_public_bytes(),
        &verified,
    )
    .unwrap()
    .pop()
    .unwrap();

    let mut lying = found.note.clone();
    lying.amount_micro = 999_999;
    assert_eq!(
        AmountDisclosure::create(&verified, found.output_index, &lying, &acknowledged()),
        Err(PrivatePaymentError::DisclosedAmountMismatch)
    );

    // An out-of-range output index is refused the same way.
    assert_eq!(
        AmountDisclosure::create(&verified, 99, &found.note, &acknowledged()),
        Err(PrivatePaymentError::DisclosedAmountMismatch)
    );
}

#[test]
fn a_fully_opened_account_audits_to_a_real_total() {
    let treasury = recipient();
    let (a, open_a) = received(&treasury, 4_000, b"grant:mini-forge");
    let (b, open_b) = received(&treasury, 9_000, b"grant:mini-net");
    let claims = [a, b];

    let disclosure = verify_disclosure(&disclose(&treasury)).unwrap();
    let audited = audit_amounts(&disclosure, claims.iter(), &[open_a, open_b]);

    assert!(audited.is_complete());
    assert_eq!(audited.opened.len(), 2);
    assert_eq!(audited.opened_total_micro(), 13_000);
    assert_eq!(audited.unopened, 0);
    assert_eq!(audited.unmatched, 0);
}

#[test]
fn withholding_one_opening_is_visible_rather_than_silent() {
    // **The property R6 exists for.** A discloser chooses what to open, and
    // no cryptography can force the choice. What can be guaranteed is that
    // the auditor learns the payment *count* from the view key rather than
    // from the discloser's cooperation -- so a missing opening is a hole
    // with a number on it, not a payment nobody knew to ask about.
    let treasury = recipient();
    let (a, open_a) = received(&treasury, 4_000, b"the flattering one");
    let (b, _open_b) = received(&treasury, 250_000, b"the one they would rather not show");
    let claims = [a, b];

    let disclosure = verify_disclosure(&disclose(&treasury)).unwrap();
    let audited = audit_amounts(&disclosure, claims.iter(), &[open_a]);

    assert_eq!(audited.opened_total_micro(), 4_000);
    assert_eq!(audited.unopened, 1);
    assert!(
        !audited.is_complete(),
        "4000 must never be reportable as this account's income"
    );
}

#[test]
fn a_broken_opening_counts_as_unopened_not_as_absent() {
    // "Withheld" and "published something that does not verify" are the
    // same fact to an auditor: the amount was not established. Splitting
    // them would invite reading the first as innocent.
    let treasury = recipient();
    let (a, opening) = received(&treasury, 4_000, b"grant");

    let mut bytes = opening.encode();
    let len = bytes.len();
    bytes[len - 1] ^= 0x01; // corrupt the blinding factor
    let broken = AmountDisclosure::decode(&bytes).unwrap();

    let claims = [a];
    let disclosure = verify_disclosure(&disclose(&treasury)).unwrap();
    let audited = audit_amounts(&disclosure, claims.iter(), &[broken]);

    assert_eq!(audited.opened.len(), 0);
    assert_eq!(audited.unopened, 1);
    assert!(!audited.is_complete());
}

#[test]
fn openings_for_someone_elses_claims_are_counted_apart_and_change_nothing() {
    let treasury = recipient();
    let bystander = recipient();
    let (mine, open_mine) = received(&treasury, 1_000, b"mine");
    let (_theirs, open_theirs) = received(&bystander, 50_000, b"not mine");
    let claims = [mine];

    let disclosure = verify_disclosure(&disclose(&treasury)).unwrap();
    let audited = audit_amounts(&disclosure, claims.iter(), &[open_mine, open_theirs]);

    assert!(audited.is_complete());
    assert_eq!(audited.opened_total_micro(), 1_000);
    assert_eq!(audited.unmatched, 1, "and it is not silently ignored");
}

#[test]
fn an_audit_of_amounts_still_says_nothing_about_spending() {
    // The asymmetric limit, unchanged by R6 and worth pinning again now
    // that sums exist and could be mistaken for a balance. A view key sees
    // income. An account that received 1000 and spent it looks exactly like
    // one that received 1000 and still holds it.
    let treasury = recipient();
    let vendor = recipient();
    let (income, open_income) = received(&treasury, 1_000, b"funding");
    let (outgoing, _) = received(&vendor, 1_000, b"treasury pays a vendor");
    let claims = [income, outgoing];

    let disclosure = verify_disclosure(&disclose(&treasury)).unwrap();
    let audited = audit_amounts(&disclosure, claims.iter(), &[open_income]);

    assert!(audited.is_complete());
    assert_eq!(
        audited.opened_total_micro(),
        1_000,
        "income only -- this is not a balance and must never be called one"
    );
}

#[test]
fn opening_an_amount_does_not_let_anyone_spend_the_output() {
    // The blinding factor is half of what spends an output; the one-time
    // secret key is the other half, and it never travels in a memo. A
    // published opening is therefore not a published spending capability.
    let treasury = recipient();
    let (claim, opening) = received(&treasury, 7_777, b"disbursement");
    assert_eq!(opening.open_against(&claim).unwrap(), 7_777);

    // Debug shows the amount, which is the published point of the object,
    // and redacts the blinding factor, which is opening material and has no
    // business arriving in a log through an incidental `{:?}`.
    let rendered = format!("{opening:?}");
    assert!(
        rendered.contains("7777"),
        "the disclosed amount is not a secret"
    );
    assert!(
        rendered.contains("<redacted>"),
        "the blinding stays out of logs"
    );
}

#[test]
fn a_disclosure_round_trips_and_is_domain_separated() {
    let treasury = recipient();
    let (_claim, opening) = received(&treasury, 4_000, b"grant");
    let bytes = opening.encode();

    assert!(bytes.starts_with(mini_private_payment::AMOUNT_DISCLOSURE_DOMAIN));
    assert_eq!(AmountDisclosure::decode(&bytes).unwrap(), opening);
    assert_eq!(
        opening.digest(),
        mini_crypto::HashAlgorithm::Blake3.digest(&bytes)
    );

    // Truncated, extended, and wrong-domain bytes are all refused.
    assert!(AmountDisclosure::decode(&bytes[..bytes.len() - 1]).is_err());
    let mut extended = bytes.clone();
    extended.push(0);
    assert!(AmountDisclosure::decode(&extended).is_err());
    let mut wrong_domain = bytes;
    wrong_domain[0] ^= 0xff;
    assert!(AmountDisclosure::decode(&wrong_domain).is_err());
}

#[test]
fn the_amount_domain_cannot_collide_with_the_view_key_domain() {
    // Two domain-separated disclosure objects in one crate: neither may be
    // a prefix of the other, or a decoder could be walked from one into the
    // other by a caller who controls the bytes.
    use mini_private_payment::{AMOUNT_DISCLOSURE_DOMAIN, DISCLOSURE_DOMAIN};
    assert!(!AMOUNT_DISCLOSURE_DOMAIN.starts_with(DISCLOSURE_DOMAIN));
    assert!(!DISCLOSURE_DOMAIN.starts_with(AMOUNT_DISCLOSURE_DOMAIN));
}
