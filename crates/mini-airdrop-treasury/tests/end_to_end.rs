//! End-to-end proof that `mini-airdrop` and `mini-airdrop-treasury`
//! actually compose: a real snapshot, a real signed claim verified
//! against a real `did-mini` KEL, a real on-disk `FileClaimedRegistry`,
//! and real treasury-signer KEL-verified approvals -- all the way to a
//! `TreasuryApprovedPayout`.
//!
//! This is deliberately still not a full airdrop: nothing here submits a
//! `mini_settlement::PaymentClaim` or moves value. See `mini-airdrop-
//! treasury`'s own crate docs and D-0356 for exactly where the composed
//! flow currently stops and why.

use did_mini::Controller;
use mini_airdrop::{
    message_to_sign, verify_and_resolve_claim, AllocationEntry, ClaimRequest, ClaimedRegistry,
    FileClaimedRegistry, SnapshotBuilder,
};
use mini_airdrop_treasury::{payout_message, verify_payout_approvals};
use mini_treasury::TreasurySignerSet;

fn temp_registry_path(name: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mini-airdrop-e2e-{}-{}-{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    p
}

#[test]
fn a_full_snapshot_to_treasury_approval_flow_succeeds() {
    // -- Campaign setup: one eligible claimant. --
    let claimant = Controller::incept_single().unwrap();
    let mut builder = SnapshotBuilder::new(b"e2e-campaign".to_vec()).unwrap();
    builder
        .insert(AllocationEntry {
            identity_root: claimant.did(),
            amount_micro: 5_000,
            human_status: None,
            reason: "e2e test allocation".to_string(),
        })
        .unwrap();
    let snapshot = builder.build();

    // -- Claimant signs and submits a claim request. --
    let request = ClaimRequest {
        campaign_id: b"e2e-campaign".to_vec(),
        identity_root: claimant.did(),
        recipient: b"claimant-payee-address".to_vec(),
        sequence: 0,
    };
    let sigs = claimant.sign_message(&message_to_sign(&request));

    let registry_path = temp_registry_path("full-flow");
    let mut registry = FileClaimedRegistry::open(&registry_path).unwrap();

    let outcome = verify_and_resolve_claim(
        &snapshot,
        &request,
        &sigs,
        &claimant.kel(),
        &mut registry,
        1_000,
    )
    .unwrap();
    assert_eq!(outcome.amount_micro, 5_000);
    assert_eq!(outcome.recipient, b"claimant-payee-address");

    // A second claim attempt for the same identity root is rejected --
    // even against a freshly reopened registry over the same file.
    let reopened = FileClaimedRegistry::open(&registry_path).unwrap();
    assert!(reopened.already_claimed(&claimant.did()));

    // -- Treasury signer set approves the resolved outcome. --
    let s1 = Controller::incept_single().unwrap();
    let s2 = Controller::incept_single().unwrap();
    let s3 = Controller::incept_single().unwrap();
    let signer_set = TreasurySignerSet::new(vec![s1.did(), s2.did(), s3.did()], 2).unwrap();

    let payout_msg = payout_message(b"e2e-campaign", &outcome);
    let sigs1 = s1.sign_message(&payout_msg);
    let sigs2 = s2.sign_message(&payout_msg);
    let kel1 = s1.kel();
    let kel2 = s2.kel();
    let candidates = vec![(&kel1, sigs1.as_slice()), (&kel2, sigs2.as_slice())];

    let approved =
        verify_payout_approvals(b"e2e-campaign", &outcome, &signer_set, &candidates).unwrap();
    assert_eq!(approved.outcome, outcome);
    assert_eq!(approved.approving_signers.len(), 2);
    assert!(approved.approving_signers.contains(&s1.did()));
    assert!(approved.approving_signers.contains(&s2.did()));
    // The third signer never approved and never counts.
    assert!(!approved.approving_signers.contains(&s3.did()));

    let _ = std::fs::remove_file(&registry_path);
}

#[test]
fn an_ineligible_identity_never_reaches_treasury_approval() {
    // A claimant with no snapshot entry is rejected at the claim-
    // verification stage; there is nothing for a treasury signer set to
    // even be asked to approve.
    let outsider = Controller::incept_single().unwrap();
    let someone_else = Controller::incept_single().unwrap();

    let mut builder = SnapshotBuilder::new(b"e2e-campaign".to_vec()).unwrap();
    builder
        .insert(AllocationEntry {
            identity_root: someone_else.did(),
            amount_micro: 1_000,
            human_status: None,
            reason: "not the outsider".to_string(),
        })
        .unwrap();
    let snapshot = builder.build();

    let request = ClaimRequest {
        campaign_id: b"e2e-campaign".to_vec(),
        identity_root: outsider.did(),
        recipient: b"payee".to_vec(),
        sequence: 0,
    };
    let sigs = outsider.sign_message(&message_to_sign(&request));

    let registry_path = temp_registry_path("ineligible");
    let mut registry = FileClaimedRegistry::open(&registry_path).unwrap();

    let result = verify_and_resolve_claim(
        &snapshot,
        &request,
        &sigs,
        &outsider.kel(),
        &mut registry,
        0,
    );
    assert!(result.is_err());
    // Nothing was ever marked claimed -- a caller retrying with a
    // corrected request (or a legitimately different identity) is not
    // blocked by this failed attempt.
    assert!(!registry.already_claimed(&outsider.did()));

    let _ = std::fs::remove_file(&registry_path);
}
