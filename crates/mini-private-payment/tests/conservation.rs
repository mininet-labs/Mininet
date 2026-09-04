//! Value conservation: the property the shielded path did not have.
//!
//! Before this, a claim committed to an amount with nothing tying it to
//! what was spent. Every signature verified, every range proof passed, and
//! a payer could commit to any number they liked — the ledger hid amounts
//! *and* could not tell whether they added up. Hiding without conservation
//! is not privacy, it is an unaudited mint.
//!
//! These tests are about the equation, not the plumbing: what a spender can
//! and cannot get past a verifier.

mod support;

use mini_private_payment::{
    build, verify, OutputSet, PrivatePaymentError, MAX_INPUTS, MAX_OUTPUTS,
};
use support::{pay, payment_to, recipient, request_for, Ledger, NETWORK};

#[test]
fn a_balanced_payment_verifies() {
    let to = recipient();
    let (claim, _) = payment_to(&to, 1_000, b"balanced");
    assert!(verify(&claim, &NETWORK).is_ok());
}

#[test]
fn inputs_must_equal_outputs_plus_fee() {
    let to = recipient();
    let (ledger, spend) = Ledger::with_funds(1_000);
    // 900 out + 100 fee == 1000 in.
    let request = request_for(vec![spend], vec![pay(&to, 900, b"with fee")], 100);
    let (claim, _) = build(&request, &ledger).unwrap();
    let verified = verify(&claim, &NETWORK).unwrap();
    assert_eq!(verified.claim().fee_micro, 100);
}

#[test]
fn a_claim_that_pays_out_more_than_it_spends_cannot_be_built() {
    // Caught in integers, before any curve arithmetic, so the failure names
    // the actual problem instead of surfacing as an unexplained bad proof.
    let to = recipient();
    let (ledger, spend) = Ledger::with_funds(1_000);
    let request = request_for(vec![spend], vec![pay(&to, 5_000, b"minting")], 0);
    assert_eq!(
        build(&request, &ledger).unwrap_err(),
        PrivatePaymentError::UnbalancedAmounts
    );
}

#[test]
fn a_claim_that_pays_out_less_than_it_spends_cannot_be_built() {
    // Burning value is refused too. Not because burning is an attack, but
    // because a claim whose numbers do not add up is a claim no verifier
    // will accept, and silently building one wastes the payer's output.
    let to = recipient();
    let (ledger, spend) = Ledger::with_funds(1_000);
    let request = request_for(vec![spend], vec![pay(&to, 400, b"burning")], 0);
    assert_eq!(
        build(&request, &ledger).unwrap_err(),
        PrivatePaymentError::UnbalancedAmounts
    );
}

#[test]
fn forgetting_the_fee_unbalances_the_claim() {
    // The fee is not decoration: it has to come out of the inputs like any
    // other output, and a payer who declares one without funding it is
    // creating it from nothing.
    let to = recipient();
    let (ledger, spend) = Ledger::with_funds(1_000);
    let request = request_for(vec![spend], vec![pay(&to, 1_000, b"unfunded fee")], 50);
    assert_eq!(
        build(&request, &ledger).unwrap_err(),
        PrivatePaymentError::UnbalancedAmounts
    );
}

#[test]
fn a_tampered_fee_breaks_the_balance_at_verification() {
    // The published fee is what a verifier recomputes the fee commitment
    // from. Raising it after the fact -- to look like a more attractive
    // claim to include -- makes the sums stop cancelling.
    let to = recipient();
    let (ledger, spend) = Ledger::with_funds(1_000);
    let request = request_for(vec![spend], vec![pay(&to, 900, b"fee")], 100);
    let (mut claim, _) = build(&request, &ledger).unwrap();
    claim.fee_micro = 200;
    assert_eq!(
        verify(&claim, &NETWORK).unwrap_err(),
        PrivatePaymentError::UnbalancedAmounts
    );
}

#[test]
fn a_swapped_pseudo_commitment_breaks_conservation() {
    // The pseudo-commitment is what carries the input's value into the
    // balance sum. Substituting one from another claim -- even a perfectly
    // valid one -- must not let a spender claim value they did not spend.
    let to = recipient();
    let (claim_a, _) = payment_to(&to, 1_000, b"a");
    let (claim_b, _) = payment_to(&to, 5_000, b"b");
    let mut forged = claim_a.clone();
    forged.inputs[0].pseudo_commitment = claim_b.inputs[0].pseudo_commitment.clone();
    // Either the sums stop matching or the spend proof stops verifying --
    // both are refusals, and which one fires first is an ordering detail.
    let error = verify(&forged, &NETWORK).unwrap_err();
    assert!(
        matches!(
            error,
            PrivatePaymentError::UnbalancedAmounts | PrivatePaymentError::BadSpendProof
        ),
        "{error:?}"
    );
}

#[test]
fn change_is_just_another_output() {
    // A wallet spending 1000 to pay 300 sends itself 700. There is no
    // change field and no change flag: change is an output to yourself,
    // which is exactly what stops it being identifiable as change.
    let payee = recipient();
    let me = recipient();
    let (ledger, spend) = Ledger::with_funds(1_000);
    let request = request_for(
        vec![spend],
        vec![pay(&payee, 300, b"purchase"), pay(&me, 690, b"change")],
        10,
    );
    let (claim, _) = build(&request, &ledger).unwrap();
    let verified = verify(&claim, &NETWORK).unwrap();
    assert_eq!(verified.claim().outputs.len(), 2);

    // Nothing on the wire distinguishes the two: same shape, same sizes.
    let first = &verified.claim().outputs[0];
    let second = &verified.claim().outputs[1];
    assert_eq!(
        first.amount_commitment.len(),
        second.amount_commitment.len()
    );
    assert_eq!(first.memo.ciphertext.len(), second.memo.ciphertext.len());
    assert_eq!(
        first.range_proof.to_bytes().len(),
        second.range_proof.to_bytes().len()
    );
}

#[test]
fn several_inputs_fund_one_payment() {
    let to = recipient();
    let mut ledger = Ledger::new();
    ledger.fill(mini_private_payment::MIN_RING_SIZE * 4);
    let a = ledger.mint(400);
    let b = ledger.mint(350);
    let c = ledger.mint(250);
    let request = request_for(vec![a, b, c], vec![pay(&to, 1_000, b"consolidated")], 0);
    let (claim, _) = build(&request, &ledger).unwrap();
    let verified = verify(&claim, &NETWORK).unwrap();
    assert_eq!(verified.claim().inputs.len(), 3);
    assert_eq!(verified.key_images().count(), 3);
}

#[test]
fn one_claim_cannot_spend_the_same_output_twice() {
    // Every proof is individually valid and the balance sums, because the
    // spender simply counted one output twice. Only the repeated key image
    // catches it, which is why it is checked separately rather than being
    // assumed to fall out of the other checks.
    let to = recipient();
    let mut ledger = Ledger::new();
    ledger.fill(mini_private_payment::MIN_RING_SIZE * 4);
    let spend = ledger.mint(500);
    let twice = spend.clone();
    let request = request_for(vec![spend, twice], vec![pay(&to, 1_000, b"doubled")], 0);
    let (claim, _) = build(&request, &ledger).unwrap();
    assert_eq!(
        verify(&claim, &NETWORK).unwrap_err(),
        PrivatePaymentError::RepeatedKeyImage
    );
}

#[test]
fn a_recipient_can_spend_what_they_received() {
    // The end-to-end property that makes this a payment system rather than
    // a one-way ledger: the note carries the amount and blinding, so the
    // recipient can open their own commitment and spend it onward.
    let alice = recipient();
    let bob = recipient();

    let mut ledger = Ledger::new();
    ledger.fill(mini_private_payment::MIN_RING_SIZE * 4);
    let funding = ledger.mint(1_000);
    let request = request_for(vec![funding], vec![pay(&alice, 1_000, b"to alice")], 0);
    let (first, _) = build(&request, &ledger).unwrap();
    let verified = verify(&first, &NETWORK).unwrap();

    // Alice finds her output and reads what it is worth.
    let found = mini_private_payment::scan(
        &alice.view_secret_bytes(),
        &alice.spend_public_bytes(),
        [verified.clone()].iter(),
    );
    assert_eq!(found.payments.len(), 1);
    let received = &found.payments[0];
    assert_eq!(received.note.amount_micro, 1_000);

    // That output now exists on the ledger, so Alice can spend it to Bob.
    let output = &verified.claim().outputs[received.output_index];
    ledger.outputs.push(
        output.output.one_time_address.clone(),
        output.amount_commitment.clone(),
    );
    let alice_secret = mini_value::derive_spend_scalar(
        &alice.view_secret_bytes(),
        &alice.spend_secret_bytes(),
        &output.output,
    )
    .unwrap();

    let onward = request_for(
        vec![mini_private_payment::SpendableOutput {
            set_index: ledger.outputs.len() - 1,
            one_time_secret: alice_secret.to_bytes(),
            value_micro: received.note.amount_micro,
            blinding: received.note.blinding,
        }],
        vec![pay(&bob, 950, b"onward")],
        50,
    );
    let (second, _) = build(&onward, &ledger).unwrap();
    assert!(
        verify(&second, &NETWORK).is_ok(),
        "a received output must be spendable by whoever received it"
    );
}

#[test]
fn input_and_output_counts_are_bounded() {
    let to = recipient();
    let (ledger, spend) = Ledger::with_funds(1_000);
    let mut request = request_for(vec![spend], vec![pay(&to, 1_000, b"x")], 0);

    request.recipients = Vec::new();
    assert!(matches!(
        build(&request, &ledger).unwrap_err(),
        PrivatePaymentError::OutputCountOutOfRange { .. }
    ));

    // Verification cost is inputs x ring size, so both bounds matter, and
    // neither may be raised without deciding what that costs the weakest
    // honest device (Directive 11). Checked at compile time: a build that
    // moved either bound past what a verifier can be asked to do would not
    // link.
    const _: () = assert!(MAX_INPUTS > 0 && MAX_INPUTS <= 16);
    const _: () = assert!(MAX_OUTPUTS > 0 && MAX_OUTPUTS <= 16);
}

#[test]
fn a_claim_survives_a_wire_round_trip() {
    let to = recipient();
    let me = recipient();
    let (ledger, spend) = Ledger::with_funds(1_000);
    let request = request_for(
        vec![spend],
        vec![pay(&to, 600, b"payment"), pay(&me, 395, b"change")],
        5,
    );
    let (claim, _) = build(&request, &ledger).unwrap();
    let encoded = claim.encode();
    let decoded = mini_private_payment::PrivatePaymentClaim::decode(&encoded).unwrap();
    assert_eq!(decoded, claim);
    assert!(verify(&decoded, &NETWORK).is_ok());
}

#[test]
fn two_claims_that_overlap_on_a_single_input_conflict() {
    // The multi-input conflict that a naive reconciler misses. Claim A
    // spends {X, Y}; claim B spends {Z, Y}. They share exactly one output,
    // and not at the same position -- so a ledger asked about only the
    // first input of each would find no overlap at all and report the
    // double-spend as merely awaiting inclusion.
    //
    // M1 does not care which position the collision is at. One shared
    // output is one attempt to spend the same money twice.
    use mini_private_payment::{reconcile, InMemoryPrivateLedger, KeyImageSet, SpendOutcome};
    use mini_settlement::SettlementState;

    let to = recipient();
    let mut ledger = Ledger::new();
    ledger.fill(mini_private_payment::MIN_RING_SIZE * 4);
    let x = ledger.mint(300);
    let y = ledger.mint(700);
    let z = ledger.mint(400);

    let first = build(
        &request_for(vec![x, y.clone()], vec![pay(&to, 1_000, b"a")], 0),
        &ledger,
    )
    .unwrap()
    .0;
    let second = build(
        &request_for(vec![z, y], vec![pay(&to, 1_100, b"b")], 0),
        &ledger,
    )
    .unwrap()
    .0;
    let first = verify(&first, &NETWORK).unwrap();
    let second = verify(&second, &NETWORK).unwrap();

    // Exactly one key image in common, which is the whole point.
    let shared = first
        .key_images()
        .filter(|image| second.key_images().any(|other| other == *image))
        .count();
    assert_eq!(shared, 1);

    // The local nullifier set refuses the second outright, and refuses it
    // wholesale -- the non-overlapping input is not quietly recorded.
    let mut spent = KeyImageSet::new();
    assert_eq!(spent.observe(&first), SpendOutcome::Accepted);
    assert!(matches!(
        spent.observe(&second),
        SpendOutcome::Conflict { .. }
    ));
    assert_eq!(spent.len(), 2, "a refused claim adds nothing");

    // And canonical reconciliation reaches the same verdict, rather than
    // reporting the loser as pending because its *first* input is unspent.
    let mut canonical = InMemoryPrivateLedger::new();
    canonical.finalize(&first);
    assert_eq!(
        reconcile(&first, &canonical, 0).unwrap(),
        SettlementState::Finalized
    );
    assert_eq!(
        reconcile(&second, &canonical, 0).unwrap(),
        SettlementState::RejectedConflict
    );
}
