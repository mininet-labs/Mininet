//! V3 wallet scanning and amount disclosure (research P1-1 / P1-2; T11-T13 cost
//! context, T12 no remote oracle, T29 view-key scope).

use mini_private_payment::{
    audit_amounts_v3, build_v3, quote_fee_micro, recognizes_v3, scan_one_v3, scan_v3, verify_v3,
    AcknowledgedAmountDisclosure, AcknowledgedIrreversibleDisclosure, AmountDisclosureV3,
    InMemoryOutputSet, OutputSet, PaymentPurpose, PaymentRequestV3, RecipientV3, SpendableOutputV3,
    VerifiedPrivateClaimV3, ViewKeyDisclosure, RING_SIZE_V3,
};
use mini_value::{pedersen_commitment_v2, StealthKeypair};

const NETWORK: [u8; 32] = [0x5a; 32];
const FEE_POLICY: [u8; 32] = [0x11; 32];

struct Ledger {
    outputs: InMemoryOutputSet,
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

fn funded(value: u64) -> (Ledger, SpendableOutputV3) {
    let mut outputs = InMemoryOutputSet::new();
    let fill = |outputs: &mut InMemoryOutputSet, count: usize| {
        for _ in 0..count {
            let key = StealthKeypair::generate().unwrap();
            let blinding = mini_value::random_scalar_bytes().unwrap();
            let c = pedersen_commitment_v2(1_000, &blinding).unwrap();
            outputs.push(key.spend_public_bytes().to_vec(), c.to_vec());
        }
    };
    fill(&mut outputs, RING_SIZE_V3 * 4);
    let key = StealthKeypair::generate().unwrap();
    let blinding = mini_value::random_scalar_bytes().unwrap();
    let c = pedersen_commitment_v2(value, &blinding).unwrap();
    outputs.push(key.spend_public_bytes().to_vec(), c.to_vec());
    let set_index = outputs.len() - 1;
    fill(&mut outputs, 4);
    (
        Ledger { outputs },
        SpendableOutputV3 {
            set_index,
            one_time_secret: key.spend_secret_bytes(),
            value_micro: value,
            blinding,
        },
    )
}

fn pay(to: &StealthKeypair, amount: u64, purpose: &[u8]) -> VerifiedPrivateClaimV3 {
    let fee = quote_fee_micro(&FEE_POLICY, 1, 1);
    let (ledger, spend) = funded(amount + fee);
    let (claim, _) = build_v3(
        &PaymentRequestV3 {
            network_id: NETWORK,
            fee_policy_id: FEE_POLICY,
            spends: vec![spend],
            recipients: vec![RecipientV3 {
                spend_public: to.spend_public_bytes().to_vec(),
                view_public: to.view_public_bytes().to_vec(),
                amount_micro: amount,
                purpose: PaymentPurpose::new(purpose.to_vec()),
            }],
        },
        &ledger,
    )
    .unwrap();
    verify_v3(&claim, &NETWORK).unwrap()
}

fn ack() -> AcknowledgedAmountDisclosure {
    AcknowledgedAmountDisclosure::new(AcknowledgedAmountDisclosure::REQUIRED_ACKNOWLEDGEMENT)
        .unwrap()
}

#[test]
fn a_wallet_recognizes_only_its_own_payments_locally() {
    let me = StealthKeypair::generate().unwrap();
    let stranger = StealthKeypair::generate().unwrap();
    let claim = pay(&me, 5_000, b"invoice-7");
    assert!(recognizes_v3(
        &me.view_secret_bytes(),
        &me.spend_public_bytes(),
        &claim
    ));
    assert!(!recognizes_v3(
        &stranger.view_secret_bytes(),
        &stranger.spend_public_bytes(),
        &claim
    ));
    let found = scan_one_v3(&me.view_secret_bytes(), &me.spend_public_bytes(), &claim).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].note.amount_micro, 5_000);
    assert_eq!(found[0].note.purpose.reference, b"invoice-7");
    assert!(scan_one_v3(
        &stranger.view_secret_bytes(),
        &stranger.spend_public_bytes(),
        &claim
    )
    .unwrap()
    .is_empty());
}

#[test]
fn scanning_a_mixed_batch_returns_only_this_wallets_payments_and_never_fails() {
    let me = StealthKeypair::generate().unwrap();
    let other = StealthKeypair::generate().unwrap();
    let mine_a = pay(&me, 2_000, b"a");
    let mine_b = pay(&me, 3_000, b"b");
    let theirs = pay(&other, 9_000, b"theirs");
    let outcome = scan_v3(
        &me.view_secret_bytes(),
        &me.spend_public_bytes(),
        [&mine_a, &theirs, &mine_b],
    );
    let mut amounts: Vec<u64> = outcome
        .payments
        .iter()
        .map(|p| p.note.amount_micro)
        .collect();
    amounts.sort_unstable();
    assert_eq!(amounts, vec![2_000, 3_000]);
    assert!(outcome.unreadable.is_empty());
}

#[test]
fn an_opening_proves_exactly_the_committed_amount_and_nothing_else() {
    let me = StealthKeypair::generate().unwrap();
    let claim = pay(&me, 7_000, b"salary");
    let found = scan_one_v3(&me.view_secret_bytes(), &me.spend_public_bytes(), &claim).unwrap();
    let opening =
        AmountDisclosureV3::create(&claim, found[0].output_index, &found[0].note, &ack()).unwrap();
    assert_eq!(opening.open_against(&claim).unwrap(), 7_000);

    // Round trip and strict decode.
    let bytes = opening.encode();
    assert_eq!(AmountDisclosureV3::decode(&bytes).unwrap(), opening);
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(AmountDisclosureV3::decode(&trailing).is_err());
    assert!(!format!("{opening:?}").contains(&format!("{:?}", found[0].note.blinding)));

    // A different claim, or a tampered amount, never verifies.
    let other = pay(&me, 7_000, b"other");
    assert!(opening.open_against(&other).is_err());
    let mut tampered = bytes;
    let len = tampered.len();
    tampered[len - 33] ^= 1; // flips a bit of the amount
    let tampered = AmountDisclosureV3::decode(&tampered).unwrap();
    assert!(tampered.open_against(&claim).is_err());
}

#[test]
fn an_audit_never_reports_a_partial_total_as_income() {
    let me = StealthKeypair::generate().unwrap();
    let first = pay(&me, 1_000, b"one");
    let second = pay(&me, 2_000, b"two");
    let disclosure = mini_private_payment::verify_disclosure(&ViewKeyDisclosure::create(
        me.spend_public_bytes().to_vec(),
        me.view_public_bytes().to_vec(),
        me.view_secret_bytes().to_vec(),
        b"audit".to_vec(),
        1,
        &AcknowledgedIrreversibleDisclosure::new(
            AcknowledgedIrreversibleDisclosure::REQUIRED_ACKNOWLEDGEMENT,
        )
        .unwrap(),
    ))
    .unwrap();
    let open = |claim: &VerifiedPrivateClaimV3| {
        let found = scan_one_v3(&me.view_secret_bytes(), &me.spend_public_bytes(), claim).unwrap();
        AmountDisclosureV3::create(claim, found[0].output_index, &found[0].note, &ack()).unwrap()
    };
    let only_first = audit_amounts_v3(&disclosure, [&first, &second], &[open(&first)]);
    assert_eq!(only_first.opened_total_micro(), 1_000);
    assert_eq!(only_first.unopened, 1);
    assert!(!only_first.is_complete());

    let both = audit_amounts_v3(
        &disclosure,
        [&first, &second],
        &[open(&first), open(&second)],
    );
    assert_eq!(both.opened_total_micro(), 3_000);
    assert!(both.is_complete());

    // An opening for a claim outside the audited set is counted as unmatched.
    let stray = audit_amounts_v3(&disclosure, [&first], &[open(&first), open(&second)]);
    assert_eq!(stray.unmatched, 1);
}

#[test]
fn a_view_key_recognizes_but_cannot_spend_or_open_amounts() {
    // T29: the view secret alone derives no spend scalar and no opening.
    let me = StealthKeypair::generate().unwrap();
    let claim = pay(&me, 4_000, b"scope");
    let output = &claim.claim().outputs[0];
    assert!(mini_value::derive_spend_scalar_v3(
        mini_private_payment::SUITE_ID_V3,
        &NETWORK,
        &me.view_secret_bytes(),
        &me.view_secret_bytes(), // the view secret standing in for a spend secret
        &mini_value::StealthOutput {
            tx_public_key: output.tx_public_key.clone(),
            one_time_address: output.one_time_address.clone(),
        },
    )
    .is_none());
}
