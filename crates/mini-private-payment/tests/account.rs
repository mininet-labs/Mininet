mod support;
use mini_private_payment::{
    account_snapshot, build, reconcile, scan_one, verify, AccountLedgerView, CreditStatus,
    OutputSet, PrivateLedgerView, PrivatePaymentError, SpendableOutput, VerifiedPrivateClaim,
};
use mini_settlement::SettlementState;
use std::collections::BTreeMap;
use support::{pay, recipient, request_for, Ledger, NETWORK};

#[derive(Default)]
struct Canonical {
    outputs: BTreeMap<Vec<u8>, Vec<u8>>,
    spent: BTreeMap<Vec<u8>, [u8; 32]>,
}
impl Canonical {
    fn backing(ledger: &Ledger) -> Self {
        Self {
            outputs: (0..ledger.len())
                .map(|i| (ledger.key_at(i).unwrap(), ledger.commitment_at(i).unwrap()))
                .collect(),
            spent: BTreeMap::new(),
        }
    }
    fn finalize(&mut self, c: &VerifiedPrivateClaim) {
        for k in c.key_images() {
            self.spent.insert(k.to_vec(), *c.transcript_digest());
        }
        for o in &c.claim().outputs {
            self.outputs.insert(
                o.output.one_time_address.clone(),
                o.amount_commitment.clone(),
            );
        }
    }
}
impl PrivateLedgerView for Canonical {
    fn finalized_claim(&self, k: &[u8]) -> Option<[u8; 32]> {
        self.spent.get(k).copied()
    }
}
impl AccountLedgerView for Canonical {
    fn output_commitment(&self, k: &[u8]) -> Option<Vec<u8>> {
        self.outputs.get(k).cloned()
    }
}

#[test]
fn offline_credit_is_pending_duplicate_safe_then_available_only_after_inclusion() {
    let bob = recipient();
    let (ledger, spend) = Ledger::with_funds(100);
    let (raw, _) = build(
        &request_for(vec![spend], vec![pay(&bob, 100, b"offline")], 0),
        &ledger,
    )
    .unwrap();
    let claim = verify(&raw, &NETWORK).unwrap();
    let mut chain = Canonical::backing(&ledger);
    let history = vec![claim.clone(), claim.clone()];
    let pending = account_snapshot(&NETWORK, &bob, &history, &[], &chain, 0).unwrap();
    assert_eq!(pending.pending_micro, 100);
    assert_eq!(pending.available_micro, 0);
    assert_eq!(pending.credits.len(), 1);
    chain.finalize(&claim);
    let confirmed = account_snapshot(&NETWORK, &bob, &history, &[], &chain, u64::MAX).unwrap();
    assert_eq!(confirmed.available_micro, 100);
    assert_eq!(confirmed.pending_micro, 0);
    assert_eq!(
        format!("{:?}", confirmed.credits[0]),
        "AccountCredit { status: Available, amount_known: true, .. }"
    );
}

#[test]
fn missing_input_backing_or_output_evidence_is_never_available() {
    let bob = recipient();
    let (ledger, spend) = Ledger::with_funds(100);
    let (raw, _) = build(
        &request_for(vec![spend], vec![pay(&bob, 100, b"x")], 0),
        &ledger,
    )
    .unwrap();
    let claim = verify(&raw, &NETWORK).unwrap();
    let mut chain = Canonical::default();
    let snapshot =
        account_snapshot(&NETWORK, &bob, std::slice::from_ref(&claim), &[], &chain, 0).unwrap();
    assert_eq!(snapshot.credits[0].status, CreditStatus::Unverified);
    for k in claim.key_images() {
        chain.spent.insert(k.to_vec(), *claim.transcript_digest());
    }
    let snapshot =
        account_snapshot(&NETWORK, &bob, std::slice::from_ref(&claim), &[], &chain, 0).unwrap();
    assert_eq!(snapshot.available_micro, 0);
    assert_eq!(snapshot.credits[0].status, CreditStatus::Unverified);
    assert!(account_snapshot(&[99; 32], &bob, &[claim], &[], &chain, 0).is_err());
}

#[test]
fn conflicting_offline_copies_cannot_both_turn_into_available_money() {
    let bob = recipient();
    let (ledger, spend) = Ledger::with_funds(100);
    let a = verify(
        &build(
            &request_for(vec![spend.clone()], vec![pay(&bob, 100, b"a")], 0),
            &ledger,
        )
        .unwrap()
        .0,
        &NETWORK,
    )
    .unwrap();
    let b = verify(
        &build(
            &request_for(vec![spend], vec![pay(&bob, 100, b"b")], 0),
            &ledger,
        )
        .unwrap()
        .0,
        &NETWORK,
    )
    .unwrap();
    let mut chain = Canonical::backing(&ledger);
    let history = vec![a.clone(), b];
    let before = account_snapshot(&NETWORK, &bob, &history, &[], &chain, 0).unwrap();
    assert_eq!(before.pending_micro, 0);
    assert_eq!(before.conflicting_pending_micro, 200);
    chain.finalize(&a);
    let after = account_snapshot(&NETWORK, &bob, &history, &[], &chain, 0).unwrap();
    assert_eq!(after.available_micro, 100);
    assert!(after
        .credits
        .iter()
        .any(|c| c.status == CreditStatus::Rejected));
}

#[test]
fn partial_multi_input_finality_is_an_error_not_money() {
    let bob = recipient();
    let (mut ledger, spend) = Ledger::with_funds(50);
    let second = ledger.mint(50);
    let claim = verify(
        &build(
            &request_for(vec![spend, second], vec![pay(&bob, 100, b"partial")], 0),
            &ledger,
        )
        .unwrap()
        .0,
        &NETWORK,
    )
    .unwrap();
    let mut chain = Canonical::backing(&ledger);
    chain.spent.insert(
        claim.key_images().next().unwrap().to_vec(),
        *claim.transcript_digest(),
    );
    assert_eq!(
        reconcile(&claim, &chain, u64::MAX),
        Err(PrivatePaymentError::IncompleteFinality)
    );
    let account = account_snapshot(
        &NETWORK,
        &bob,
        std::slice::from_ref(&claim),
        &[],
        &chain,
        u64::MAX,
    )
    .unwrap();
    assert_eq!(account.available_micro, 0);
    assert_eq!(account.credits[0].status, CreditStatus::Unverified);
    chain.finalize(&claim);
    assert_eq!(
        reconcile(&claim, &chain, u64::MAX).unwrap(),
        SettlementState::Finalized
    );
}

#[test]
fn outgoing_reservation_survives_local_expiry_and_final_spend_removes_available_value() {
    let bob = recipient();
    let carol = recipient();
    let (mut ledger, spend) = Ledger::with_funds(100);
    let claim = verify(
        &build(
            &request_for(vec![spend], vec![pay(&bob, 100, b"receive")], 0),
            &ledger,
        )
        .unwrap()
        .0,
        &NETWORK,
    )
    .unwrap();
    let mut chain = Canonical::backing(&ledger);
    chain.finalize(&claim);
    let output = &claim.claim().outputs[0];
    ledger.outputs.push(
        output.output.one_time_address.clone(),
        output.amount_commitment.clone(),
    );
    let note = scan_one(&bob.view_secret_bytes(), &bob.spend_public_bytes(), &claim)
        .unwrap()
        .remove(0)
        .note;
    let secret = mini_value::derive_spend_scalar(
        &bob.view_secret_bytes(),
        &bob.spend_secret_bytes(),
        &output.output,
    )
    .unwrap()
    .to_bytes();
    let request = request_for(
        vec![SpendableOutput {
            set_index: ledger.len() - 1,
            one_time_secret: secret,
            value_micro: note.amount_micro,
            blinding: note.blinding,
        }],
        vec![pay(&carol, 100, b"spend")],
        0,
    );
    let outgoing = verify(&build(&request, &ledger).unwrap().0, &NETWORK).unwrap();
    assert_eq!(
        mini_value::spend_key_image(&secret).unwrap().as_slice(),
        outgoing.key_images().next().unwrap()
    );
    for now in [0, u64::MAX] {
        let snapshot = account_snapshot(
            &NETWORK,
            &bob,
            std::slice::from_ref(&claim),
            std::slice::from_ref(&outgoing),
            &chain,
            now,
        )
        .unwrap();
        assert_eq!(snapshot.available_micro, 0);
        assert_eq!(snapshot.reserved_micro, 100);
    }
    chain.finalize(&outgoing);
    let snapshot =
        account_snapshot(&NETWORK, &bob, &[claim], &[outgoing], &chain, u64::MAX).unwrap();
    assert_eq!(snapshot.available_micro, 0);
    assert_eq!(snapshot.reserved_micro, 0);
    assert_eq!(snapshot.spent_micro, 100);
}
