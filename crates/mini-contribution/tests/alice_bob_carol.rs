//! Vertical slice 1 (D-0417): Alice publishes, Bob seeds, Carol requests,
//! Bob delivers verified chunks, and Alice/Bob are both paid real,
//! finalized MINI -- funded entirely from Carol's own genesis balance, no
//! new issuance.
//!
//! Every step composes already-real primitives: `mini_media::publish_media`
//! for the content manifest, `mini_provider::ProviderDeclaration` for
//! discovery, `mini_engagement` for the escrowed offer/accept/complete
//! state machine, `mini_storage::verify_serve` for delivery evidence, and a
//! real `mini_chain`-finalized `mini_execution::LedgerChain` for
//! settlement -- the same end-to-end finality pattern
//! `mini-execution/tests/end_to_end.rs` already established. Local/shared-
//! store discovery only; no network transport, dispute/timeout coverage,
//! or CLI surface -- see `docs/design/contribution-and-settlement-
//! coordinator.md` for what remains out of scope.

use std::collections::BTreeMap;

use did_mini::{Capabilities, Controller, Did, Kel};
use mini_chain::{
    sign_vote, BlockHeader, QuorumCertificate, ValidatorOracle, ValidatorSet, VoteKind,
};
use mini_contribution::{bind_delivery_evidence, settle_completed_engagement, RewardSplit};
use mini_crypto::{HashAlgorithm, SigningKey};
use mini_engagement::{accept, complete, Engagement};
use mini_execution::{LedgerChain, SettlementBlockBody};
use mini_media::publish_media;
use mini_provider::{
    CustodyPosture, DeathDisposition, ExitTerms, FreezePowers, ProviderDeclaration, ServiceClass,
};
use mini_settlement::sign_claim_for_network;
use mini_storage::{
    verify_serve, FreshnessPolicy, InMemoryReplayGuard, ReceiptFields, ServeReceipt, VerifyContext,
    RECEIPT_VERSION,
};
use mini_store::{MemoryBackend, Store};

/// A root + single delegated device, both usable for signing.
fn identity(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

fn declaration(declarant: Did, role: &str, expires_at_ms: u64) -> ProviderDeclaration {
    ProviderDeclaration {
        declarant,
        service: ServiceClass::Other(role.to_string()),
        description: format!("content-delivery {role}"),
        jurisdictions: vec![],
        data_required: vec![],
        custody: CustodyPosture::NoneHeld,
        freeze_powers: FreezePowers {
            can_freeze_user: false,
            grounds: vec![],
            notifies_user: true,
        },
        death_disposition: DeathDisposition::NothingHeld,
        exit: ExitTerms {
            notice_required_ms: None,
            exit_fee_micromini: 0,
            retained_data: vec![],
        },
        expires_at_ms,
    }
}

fn account(key: &SigningKey) -> Vec<u8> {
    key.verifying_key().to_bytes().to_vec()
}

fn validator(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

#[derive(Default)]
struct Directory(BTreeMap<String, Kel>);
impl Directory {
    fn insert(&mut self, kel: Kel) {
        self.0.insert(kel.scid().to_string(), kel);
    }
}
impl ValidatorOracle for Directory {
    fn kel(&self, did: &Did) -> Option<&Kel> {
        self.0.get(did.scid())
    }
}

#[test]
fn alice_publishes_bob_seeds_carol_requests_and_both_are_paid_from_carols_balance() {
    // ---- Setup: three participants, a shared store, four consensus
    // validators (3-of-4 quorum), a funded Carol. ----
    let (alice_root, alice_device) = identity(10);
    let (bob_root, bob_device) = identity(20);
    let (carol_root, carol_device) = identity(30);

    let validators: Vec<(Controller, Controller)> =
        [100u8, 110, 120, 130].into_iter().map(validator).collect();
    let mut oracle = Directory::default();
    for (root, device) in &validators {
        oracle.insert(root.kel());
        oracle.insert(device.kel());
    }
    let validator_set =
        ValidatorSet::new(validators.iter().map(|(r, _)| r.did()).collect()).unwrap();

    let carol_key = SigningKey::from_seed(&[0x42; 32]);
    let bob_key = SigningKey::from_seed(&[0x43; 32]);
    let alice_key = SigningKey::from_seed(&[0x44; 32]);

    let carol_account = account(&carol_key);
    let bob_account = account(&bob_key);
    let alice_account = account(&alice_key);

    let genesis_micro = 10_000u64;
    let mut chain = LedgerChain::genesis_with_balances(
        mini_economy::Amount::from(genesis_micro),
        vec![(
            carol_account.clone(),
            mini_economy::Amount::from(genesis_micro),
        )],
    )
    .unwrap();

    let mut store = Store::new(MemoryBackend::new());

    // ---- 1. Publish: Alice publishes a content manifest. ----
    let bytes = b"the whole point of this slice is a real balance transfer".to_vec();
    let manifest = publish_media(
        &mut store,
        &alice_root.did(),
        &alice_device,
        "application/octet-stream",
        &bytes,
        1_000,
        1,
    )
    .unwrap();

    // ---- 2. Announce / 3. Seed: Alice and Bob each declare themselves,
    // as creator and seeder respectively, for this manifest. ----
    let alice_declaration = declaration(alice_root.did(), "creator", 100_000);
    let bob_declaration = declaration(bob_root.did(), "seeder", 100_000);
    assert!(alice_declaration.check_wellformed(2_000).is_ok());
    assert!(bob_declaration.check_wellformed(2_000).is_ok());

    // ---- 4. Discover: Carol picks Bob's declaration (local, no network --
    // the same honest limit `mini-provider`'s own docs already state). ----
    let chosen = &bob_declaration;
    assert_eq!(chosen.declarant, bob_root.did());

    // ---- 5. Request: Carol proposes an engagement naming Bob and the
    // manifest, with a real (not-yet-submitted) escrow claim. ----
    let price_micro = 1_001u64;
    let escrow_claim = sign_claim_for_network(
        &carol_key,
        &bob_account,
        price_micro,
        0,
        1_000_000,
        &mini_settlement::MININET_NETWORK_ID,
        b"genesis",
        1_500,
    )
    .unwrap();
    let engagement = Engagement::offer(
        manifest.id.clone(),
        carol_root.did(),
        bob_root.did(),
        escrow_claim,
        1_000_000,
    );

    // ---- 6. Accept. ----
    let engagement = accept(engagement, bob_root.did(), 1_600).unwrap();

    // ---- 7. Deliver: Bob serves the manifest's bytes to Carol. Both
    // already hold every chunk in the shared store (mini-sync's job of
    // actually moving bytes between separate stores is not exercised
    // here); Carol independently reassembles and re-verifies the digest. ----
    let assembled = mini_media::assemble(&store, &manifest).unwrap();
    assert_eq!(assembled, bytes);

    // ---- 8. Receipt: Bob (host) and Carol (witness) mutually sign a
    // storage-served receipt; verify_serve produces the real verdict. ----
    let receipt_fields = ReceiptFields {
        version: RECEIPT_VERSION,
        content_id: manifest.id.clone(),
        bytes: bytes.len() as u64,
        content_digest: HashAlgorithm::Blake3.digest(&assembled),
        host_device: bob_device.did(),
        witness_device: carol_device.did(),
        host_nonce: [7u8; 32],
        witness_nonce: [8u8; 32],
        at_ms: 1_700,
    };
    let receipt = ServeReceipt::new(
        receipt_fields.clone(),
        receipt_fields.sign(&bob_device),
        receipt_fields.sign(&carol_device),
    );
    let ctx = VerifyContext {
        host_root: &bob_root.kel(),
        witness_root: &carol_root.kel(),
        host_device: &bob_device.kel(),
        witness_device: &carol_device.kel(),
        policy: &FreshnessPolicy::default_policy(),
        now_ms: Some(1_700),
    };
    let mut replay = InMemoryReplayGuard::new();
    let verdict = verify_serve(&receipt, &ctx, &mut replay).unwrap();

    // ---- 9. Settle: bind the verdict to the engagement, mark it locally
    // complete, and build the split claims -- creator share to Alice,
    // seeder share to Bob, both signed by Carol. ----
    let evidence = bind_delivery_evidence(&engagement, verdict).unwrap();
    let engagement = complete(engagement, 1_800).unwrap();

    let split = RewardSplit::new(3_000, 7_000).unwrap(); // 30% creator, 70% seeder
    let claims = settle_completed_engagement(
        &engagement,
        &evidence,
        &carol_key,
        split,
        alice_account.clone(),
        bob_account.clone(),
        1, // start_sequence; 0 was escrow_claim's, never submitted
        1_000_000,
        b"genesis",
        1_800,
    )
    .unwrap();
    assert_eq!(
        claims.len(),
        2,
        "both shares are non-zero for this split/amount"
    );
    let creator_claim = claims
        .iter()
        .find(|c| c.payee == alice_account)
        .expect("creator share claim");
    let seeder_claim = claims
        .iter()
        .find(|c| c.payee == bob_account)
        .expect("seeder share claim");
    assert_eq!(creator_claim.amount_micro, 300); // 1_001 * 3_000 / 10_000
    assert_eq!(seeder_claim.amount_micro, 700); // 1_001 * 7_000 / 10_000
    assert_eq!(
        creator_claim.amount_micro + seeder_claim.amount_micro,
        1_000,
        "1 micro-MINI of the 1_001 total is the deliberately undistributed division remainder"
    );

    // Finalize both claims in one real, quorum-certified block -- the same
    // apply_block -> BlockHeader -> QuorumCertificate ->
    // apply_finalized_block path `mini-execution`'s own end-to-end tests use.
    let body = SettlementBlockBody::new(claims);
    let next_state = mini_execution::apply_block(chain.state(), &body).unwrap();
    let header = BlockHeader {
        height: 1,
        prev_hash: chain.tip_hash(),
        state_root: next_state.commitment(),
        timestamp_ms: 1,
        proposer: validators[0].0.did(),
    };
    let hash = header.hash();
    let qc = QuorumCertificate {
        height: 1,
        round: 0,
        block_hash: hash,
        votes: validators[..3]
            .iter()
            .map(|(root, device)| sign_vote(VoteKind::Precommit, 1, 0, hash, &root.did(), device))
            .collect(),
    };
    chain
        .apply_finalized_block(&header, &body, &qc, &validator_set, &oracle)
        .unwrap();

    // ---- 10. Reward: query real finalized balances. ----
    assert_eq!(
        chain.state().balance(&carol_account),
        mini_economy::Amount::from(genesis_micro - 1_000),
        "Carol paid exactly the sum of both shares; the 1 micro-MINI remainder stayed with her"
    );
    assert_eq!(
        chain.state().balance(&alice_account),
        mini_economy::Amount::from(300)
    );
    assert_eq!(
        chain.state().balance(&bob_account),
        mini_economy::Amount::from(700)
    );
    chain.state().verify_supply_conservation().unwrap();
}

#[test]
fn settlement_is_refused_before_the_engagement_locally_completes() {
    let bob_root = Controller::incept_single_from_seeds(&[20; 32], &[21; 32]).unwrap();
    let carol_root = Controller::incept_single_from_seeds(&[30; 32], &[31; 32]).unwrap();

    let alice_root = Controller::incept_single_from_seeds(&[10; 32], &[11; 32]).unwrap();
    let alice_device =
        Controller::incept_device_single_from_seeds(&alice_root.did(), &[12; 32], &[13; 32])
            .unwrap();
    let mut store = Store::new(MemoryBackend::new());
    let manifest = publish_media(
        &mut store,
        &alice_root.did(),
        &alice_device,
        "text/plain",
        b"hello",
        0,
        0,
    )
    .unwrap();

    let carol_key = SigningKey::from_seed(&[0x50; 32]);
    let bob_key = SigningKey::from_seed(&[0x51; 32]);
    let escrow_claim = sign_claim_for_network(
        &carol_key,
        &account(&bob_key),
        100,
        0,
        1_000_000,
        &mini_settlement::MININET_NETWORK_ID,
        b"genesis",
        0,
    )
    .unwrap();
    // Engagement stays `Offered` -- never accepted or completed.
    let engagement = Engagement::offer(
        manifest.id.clone(),
        carol_root.did(),
        bob_root.did(),
        escrow_claim,
        1_000_000,
    );

    let verdict = mini_storage::ServeVerdict {
        host_root: bob_root.did(),
        witness_root: carol_root.did(),
        content_id: manifest.id.clone(),
        bytes: 5,
        at_ms: 100,
    };
    let evidence = bind_delivery_evidence(&engagement, verdict).unwrap();
    let split = RewardSplit::new(3_000, 7_000).unwrap();
    let err = settle_completed_engagement(
        &engagement,
        &evidence,
        &carol_key,
        split,
        account(&SigningKey::from_seed(&[0x52; 32])),
        account(&bob_key),
        1,
        1_000_000,
        b"genesis",
        100,
    )
    .unwrap_err();
    assert_eq!(
        err,
        mini_contribution::ContributionError::EngagementNotCompleted
    );
}
