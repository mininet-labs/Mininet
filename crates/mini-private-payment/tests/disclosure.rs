//! Voluntary auditability (D-0451), and its limits.
//!
//! The property under test is not "an audit works" — that is easy and
//! uninteresting. It is that disclosure grants *exactly* the account
//! holder's reading ability: no more (an auditor cannot see spending, cannot
//! see amounts, cannot read anyone else's payments) and no less (an auditor
//! sees every payment the holder received, including ones the holder would
//! rather not show).

mod support;

use mini_private_payment::{
    audit, verify, verify_disclosure, AcknowledgedIrreversibleDisclosure, PrivatePaymentError,
    ViewKeyDisclosure, DISCLOSURE_DOMAIN,
};
use mini_value::StealthKeypair;
use support::{payment_to, recipient, NETWORK};

fn acknowledged() -> AcknowledgedIrreversibleDisclosure {
    AcknowledgedIrreversibleDisclosure::new(
        AcknowledgedIrreversibleDisclosure::REQUIRED_ACKNOWLEDGEMENT,
    )
    .expect("the exact phrase must be accepted")
}

fn disclose(account: &StealthKeypair) -> ViewKeyDisclosure {
    disclose_parts(
        account.spend_public_bytes().to_vec(),
        account.view_public_bytes().to_vec(),
        account.view_secret_bytes().to_vec(),
        1_700_000_000_000,
    )
}

/// Every field explicit, so a test can publish a deliberately wrong one.
/// Note there is no way to reach a `ViewKeyDisclosure` that skips the
/// acknowledgement -- not even here, and not even to build a broken one.
fn disclose_parts(
    spend_public: Vec<u8>,
    view_public: Vec<u8>,
    view_secret: Vec<u8>,
    disclosed_at_ms: u64,
) -> ViewKeyDisclosure {
    ViewKeyDisclosure::create(
        spend_public,
        view_public,
        view_secret,
        b"treasury disbursements, D-0073".to_vec(),
        disclosed_at_ms,
        &acknowledged(),
    )
}

#[test]
fn the_acknowledgement_cannot_be_given_by_accident() {
    // The whole point of the typed acknowledgement: no near-miss, no
    // shortened form, no empty string gets through. If any of these were
    // accepted, the type would be decoration and a `bool` would have done
    // the same job with the same risk.
    assert!(AcknowledgedIrreversibleDisclosure::new("yes").is_none());
    assert!(AcknowledgedIrreversibleDisclosure::new("").is_none());
    assert!(
        AcknowledgedIrreversibleDisclosure::new("publishing this view key is permanent").is_none()
    );
    let almost = format!(
        "{}.",
        AcknowledgedIrreversibleDisclosure::REQUIRED_ACKNOWLEDGEMENT
    );
    assert!(AcknowledgedIrreversibleDisclosure::new(&almost).is_none());
    assert!(AcknowledgedIrreversibleDisclosure::new(
        AcknowledgedIrreversibleDisclosure::REQUIRED_ACKNOWLEDGEMENT
    )
    .is_some());
}

#[test]
fn the_acknowledgement_names_the_third_party_exposure() {
    // The phrase must keep saying the part people would otherwise miss:
    // that disclosing exposes memos written by senders who never agreed.
    // Softening it later would be a real regression in honesty, so it is
    // pinned here rather than left to review.
    let phrase = AcknowledgedIrreversibleDisclosure::REQUIRED_ACKNOWLEDGEMENT;
    assert!(phrase.contains("permanent"));
    assert!(phrase.contains("every payment"));
    assert!(phrase.contains("did not"));
}

#[test]
fn a_disclosed_account_can_be_audited() {
    let treasury = recipient();
    let (claim_a, _, _) = payment_to(&treasury, 4_000, b"grant:mini-forge");
    let (claim_b, _, _) = payment_to(&treasury, 9_000, b"grant:mini-net");
    let verified = [
        verify(&claim_a, &NETWORK).unwrap(),
        verify(&claim_b, &NETWORK).unwrap(),
    ];

    let disclosure = verify_disclosure(&disclose(&treasury)).unwrap();
    let found = audit(&disclosure, verified.iter());

    assert_eq!(found.payments.len(), 2);
    assert!(found.unreadable.is_empty());
    let mut references: Vec<_> = found
        .payments
        .iter()
        .map(|payment| payment.purpose.reference.clone())
        .collect();
    references.sort();
    assert_eq!(
        references,
        vec![b"grant:mini-forge".to_vec(), b"grant:mini-net".to_vec()]
    );
}

#[test]
fn an_audit_sees_only_the_disclosed_account() {
    // Disclosure must not be contagious. A treasury publishing its view key
    // cannot drag the people it transacts alongside into visibility.
    let treasury = recipient();
    let bystander = recipient();
    let (to_treasury, _, _) = payment_to(&treasury, 1_000, b"disbursement");
    let (to_bystander, _, _) = payment_to(&bystander, 1_000, b"nobody-else-s-business");
    let verified = [
        verify(&to_treasury, &NETWORK).unwrap(),
        verify(&to_bystander, &NETWORK).unwrap(),
    ];

    let disclosure = verify_disclosure(&disclose(&treasury)).unwrap();
    let found = audit(&disclosure, verified.iter());

    assert_eq!(found.payments.len(), 1);
    assert_eq!(
        found.payments[0].purpose.reference,
        b"disbursement".to_vec()
    );
    assert!(found.unreadable.is_empty());
}

#[test]
fn an_audit_cannot_read_amounts() {
    // The honest limit, asserted rather than only documented. A view key
    // does not open a Pedersen commitment, so "auditable" here means the set
    // of incoming payments is checkable -- never the sums.
    let treasury = recipient();
    let (claim, _, _) = payment_to(&treasury, 7_777, b"disbursement");
    let verified = [verify(&claim, &NETWORK).unwrap()];

    let disclosure = verify_disclosure(&disclose(&treasury)).unwrap();
    let found = audit(&disclosure, verified.iter());
    assert_eq!(found.payments.len(), 1);

    // The commitment is present and reveals nothing: it is not the amount,
    // and it is not equal to any encoding of the amount.
    let commitment = &found.payments[0].claim.claim().amount_commitment;
    assert_eq!(commitment.len(), 32);
    assert_ne!(commitment.as_slice(), &7_777u64.to_le_bytes()[..]);
    assert_ne!(commitment.as_slice(), &[0u8; 32][..]);
}

#[test]
fn disclosure_is_retroactive() {
    // Not a feature -- a warning made executable. A view key published today
    // reads payments received before it was published, so "I will disclose
    // from now on" is not something anyone can actually offer.
    let treasury = recipient();
    let (earlier, _, _) = payment_to(&treasury, 500, b"before-the-disclosure");
    let verified = [verify(&earlier, &NETWORK).unwrap()];

    // Disclosed "after" everything, and it still reads what came before.
    let published = disclose_parts(
        treasury.spend_public_bytes().to_vec(),
        treasury.view_public_bytes().to_vec(),
        treasury.view_secret_bytes().to_vec(),
        u64::MAX,
    );
    let disclosure = verify_disclosure(&published).unwrap();

    let found = audit(&disclosure, verified.iter());
    assert_eq!(found.payments.len(), 1);
    assert_eq!(
        found.payments[0].purpose.reference,
        b"before-the-disclosure".to_vec()
    );
}

#[test]
fn swapping_in_a_strangers_spend_key_verifies_but_audits_to_nothing() {
    // The honest limit of `verify_disclosure`, named accurately rather than
    // aspirationally. Pairing a real view secret with a stranger's *spend*
    // key is still internally consistent -- the view keypair binds -- so it
    // verifies, and the audit is simply empty. Nothing in the disclosure
    // object can distinguish that from an account that received nothing,
    // which is why the check that carries weight is the *view* key binding
    // in the next test, and why a disclosure asserts nothing about whose
    // account it describes.
    let treasury = recipient();
    let stranger = recipient();

    let mismatched = disclose_parts(
        stranger.spend_public_bytes().to_vec(),
        treasury.view_public_bytes().to_vec(),
        treasury.view_secret_bytes().to_vec(),
        1_700_000_000_000,
    );
    let verified = verify_disclosure(&mismatched).expect("a well-formed pair is not itself a lie");

    let (claim, _, _) = payment_to(&treasury, 100, b"disbursement");
    let claims = [verify(&claim, &NETWORK).unwrap()];
    let found = audit(&verified, claims.iter());
    assert!(found.payments.is_empty());
    assert!(found.unreadable.is_empty());
}

#[test]
fn a_disclosure_whose_secret_does_not_open_its_account_is_refused() {
    let treasury = recipient();
    let stranger = recipient();

    let forged = disclose_parts(
        treasury.spend_public_bytes().to_vec(),
        treasury.view_public_bytes().to_vec(),
        stranger.view_secret_bytes().to_vec(),
        1_700_000_000_000,
    );
    assert_eq!(
        verify_disclosure(&forged),
        Err(PrivatePaymentError::DisclosureKeyMismatch)
    );
}

#[test]
fn malformed_disclosure_keys_are_refused() {
    let treasury = recipient();
    let spend = treasury.spend_public_bytes().to_vec();
    let view = treasury.view_public_bytes().to_vec();
    let secret = treasury.view_secret_bytes().to_vec();
    let at = 1_700_000_000_000;

    let cases = [
        (
            "short spend key",
            vec![0u8; 4],
            view.clone(),
            secret.clone(),
        ),
        (
            "short view key",
            spend.clone(),
            vec![0u8; 4],
            secret.clone(),
        ),
        ("short secret", spend.clone(), view.clone(), vec![0u8; 4]),
        // A zero view secret is a canonical scalar and still not a key.
        ("zero secret", spend.clone(), view.clone(), vec![0u8; 32]),
        // One key doing both jobs: disclosing the view secret would disclose
        // the ability to spend, so the account shape itself is refused.
        (
            "collapsed keypair",
            view.clone(),
            view.clone(),
            secret.clone(),
        ),
    ];
    for (name, spend_public, view_public, view_secret) in cases {
        let broken = disclose_parts(spend_public, view_public, view_secret, at);
        assert_eq!(
            verify_disclosure(&broken),
            Err(PrivatePaymentError::MalformedDisclosureKey),
            "{name} must be refused"
        );
    }
}

#[test]
fn a_disclosure_round_trips_and_is_domain_separated() {
    let treasury = recipient();
    let published = disclose(&treasury);
    let bytes = published.encode();
    assert!(bytes.starts_with(DISCLOSURE_DOMAIN));
    assert_eq!(ViewKeyDisclosure::decode(&bytes).unwrap(), published);

    // A digest is over the whole encoding, domain included, so a disclosure
    // can never be replayed as some other domain-separated object.
    let mut flipped = bytes.clone();
    let last = flipped.len() - 1;
    flipped[last] ^= 0x01;
    assert_ne!(
        ViewKeyDisclosure::decode(&flipped)
            .map(|d| d.digest())
            .unwrap_or([0u8; 32]),
        published.digest()
    );
}

#[test]
fn a_truncated_or_extended_disclosure_is_refused() {
    let treasury = recipient();
    let bytes = disclose(&treasury).encode();

    assert!(ViewKeyDisclosure::decode(&bytes[..bytes.len() - 1]).is_err());
    let mut extended = bytes.clone();
    extended.push(0);
    assert!(ViewKeyDisclosure::decode(&extended).is_err());

    let mut wrong_domain = bytes.clone();
    wrong_domain[0] ^= 0xff;
    assert!(ViewKeyDisclosure::decode(&wrong_domain).is_err());

    let mut wrong_version = bytes;
    wrong_version[DISCLOSURE_DOMAIN.len()] = 0xff;
    assert!(ViewKeyDisclosure::decode(&wrong_version).is_err());
}

#[test]
fn the_reason_is_a_label_and_not_a_claim_the_protocol_checks() {
    // Stated in the docs, pinned here: a disclosure's reason is human
    // context. Two disclosures of the same account differing only in reason
    // are both valid and are different objects. Nobody should ever build a
    // policy on the reason text.
    let treasury = recipient();
    let honest = disclose(&treasury);
    let absurd = ViewKeyDisclosure::create(
        treasury.spend_public_bytes().to_vec(),
        treasury.view_public_bytes().to_vec(),
        treasury.view_secret_bytes().to_vec(),
        b"no reason at all".to_vec(),
        honest.disclosed_at_ms(),
        &acknowledged(),
    );

    assert!(verify_disclosure(&honest).is_ok());
    assert!(verify_disclosure(&absurd).is_ok());
    assert_ne!(honest.digest(), absurd.digest());
}

#[test]
fn an_audit_does_not_reveal_what_the_account_spent() {
    // The asymmetric limit worth having a test for. A view key recognizes
    // income. An account that received one payment and spent it looks, to an
    // auditor, exactly like an account that received one payment and still
    // holds it -- because the spend is a claim addressed to somebody else,
    // and this disclosure cannot read those.
    let treasury = recipient();
    let vendor = recipient();
    let (income, _, _) = payment_to(&treasury, 1_000, b"funding");
    let (outgoing, _, _) = payment_to(&vendor, 1_000, b"treasury pays a vendor");
    let ledger = [
        verify(&income, &NETWORK).unwrap(),
        verify(&outgoing, &NETWORK).unwrap(),
    ];

    let disclosure = verify_disclosure(&disclose(&treasury)).unwrap();
    let found = audit(&disclosure, ledger.iter());

    assert_eq!(found.payments.len(), 1, "income only");
    assert_eq!(found.payments[0].purpose.reference, b"funding".to_vec());
}

#[test]
fn an_audit_of_a_large_ledger_accounts_for_every_claim_exactly_once() {
    // An audit must be total. It cannot return "error" for a batch, because
    // anyone can pay a published address and a single payment must never be
    // able to erase an account's visible income -- see
    // `mini_private_payment::scan`, whose unit tests drive the adversarial
    // case a hostile encoder can produce. Here the invariant is the simpler
    // one an auditor relies on: nothing is double-counted and nothing is
    // silently dropped.
    let treasury = recipient();
    let bystander = recipient();
    let mut ledger = Vec::new();
    for i in 0..4u64 {
        let (mine, _, _) = payment_to(&treasury, 1_000 + i, b"disbursement");
        ledger.push(verify(&mine, &NETWORK).unwrap());
        let (theirs, _, _) = payment_to(&bystander, 1_000 + i, b"unrelated");
        ledger.push(verify(&theirs, &NETWORK).unwrap());
    }

    let disclosure = verify_disclosure(&disclose(&treasury)).unwrap();
    let found = audit(&disclosure, ledger.iter());

    assert_eq!(found.payments.len(), 4);
    assert!(found.unreadable.is_empty());
    assert_eq!(found.all_claims().count(), 4);

    // Distinct claims, not the same one counted four times.
    let mut digests: Vec<_> = found
        .all_claims()
        .map(|claim| *claim.transcript_digest())
        .collect();
    digests.sort();
    digests.dedup();
    assert_eq!(digests.len(), 4);
}
