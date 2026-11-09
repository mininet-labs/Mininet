//! Adversarial tests for the private participation-claim path (#337).

use did_mini::Controller;
use mini_beta::{
    build_claim_redemption, build_redeemed_participation_grant, create_campaign,
    create_claim_redemption, create_contribution_receipt, verify_claim_redemption, BetaAccountId,
    BetaEpochId, BetaError, BetaMiniLedger, BetaMiniPolicy, ClaimKey, ContributionKind,
    BETA_CLAIM_REDEMPTION_TYPE,
};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{MemoryBackend, Store};

fn signer(seed: u8) -> Controller {
    Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap()
}

struct World {
    store: Store<MemoryBackend>,
    operator: Controller,
    campaign: ObjectId,
    epoch: BetaEpochId,
}

fn world(epoch_byte: u8) -> World {
    let operator = signer(1);
    let mut store = Store::new(MemoryBackend::new());
    let target = ObjectBuilder::new(ObjectType::Custom("mini/claim-test-target".to_string()))
        .payload(Payload::Public(b"target".to_vec()))
        .sign(&operator.did(), &operator)
        .unwrap();
    store.insert(&target).unwrap();
    let epoch = BetaEpochId::new([epoch_byte; 32]).unwrap();
    let campaign = create_campaign(
        &mut store,
        &operator.did(),
        &operator,
        target.id(),
        epoch,
        "claim campaign",
        "exercise private claims",
        &["claims".to_string()],
        1,
        100,
        1_000,
        1,
        1,
    )
    .unwrap();
    World {
        store,
        operator,
        campaign: campaign.id().clone(),
        epoch,
    }
}

fn accept(w: &mut World, source: &[u8], key: &ClaimKey, seq: u64) -> Object {
    let src = ObjectBuilder::new(ObjectType::Custom("mini/claim-test-source".to_string()))
        .payload(Payload::Public(source.to_vec()))
        .sign(&w.operator.did(), &w.operator)
        .unwrap();
    w.store.insert(&src).unwrap();
    create_contribution_receipt(
        &mut w.store,
        &w.operator.did(),
        &w.operator,
        src.id(),
        Some(&w.campaign),
        key.claim_tag().unwrap(),
        ContributionKind::Testing,
        "accepted work",
        &["evidence:accepted".to_string()],
        seq,
        seq,
    )
    .unwrap()
}

fn account(byte: u8) -> BetaAccountId {
    BetaAccountId::new([byte; 32]).unwrap()
}

#[test]
fn the_holder_of_the_claim_key_redeems_to_an_account_and_gets_paid_once() {
    let mut w = world(7);
    let key = ClaimKey::generate().unwrap();
    let receipt = accept(&mut w, b"one", &key, 10);
    // The redemption is authored by a throwaway identity, not the operator.
    let throwaway = signer(99);
    let redemption = create_claim_redemption(
        &mut w.store,
        &throwaway.did(),
        &throwaway,
        receipt.id(),
        account(5),
        &key,
        11,
        1,
    )
    .unwrap();
    let verified = verify_claim_redemption(&w.store, redemption.id()).unwrap();
    assert_eq!(verified.account, account(5));
    assert_eq!(verified.epoch, w.epoch);

    let grant = build_redeemed_participation_grant(
        &w.store,
        &w.operator.did(),
        &w.operator,
        redemption.id(),
        500,
        "redeemed participation",
        12,
        12,
    )
    .unwrap();
    w.store.insert(&grant).unwrap();
    let mut ledger = BetaMiniLedger::new(w.epoch, BetaMiniPolicy::open_beta_default()).unwrap();
    ledger.apply_grant(&w.store, grant.id()).unwrap();
    assert_eq!(ledger.balance(&account(5)), 500);

    // The same work cannot be paid twice through a second redemption.
    let other = create_claim_redemption(
        &mut w.store,
        &throwaway.did(),
        &throwaway,
        receipt.id(),
        account(6),
        &key,
        13,
        2,
    )
    .unwrap();
    let again = build_redeemed_participation_grant(
        &w.store,
        &w.operator.did(),
        &w.operator,
        other.id(),
        500,
        "second attempt",
        14,
        14,
    )
    .unwrap();
    w.store.insert(&again).unwrap();
    assert!(matches!(
        ledger.apply_grant(&w.store, again.id()),
        Err(BetaError::DuplicateGrant)
    ));
    assert_eq!(ledger.balance(&account(6)), 0);
}

#[test]
fn someone_without_the_claim_key_cannot_redeem() {
    let mut w = world(7);
    let key = ClaimKey::generate().unwrap();
    let receipt = accept(&mut w, b"two", &key, 10);
    let thief_key = ClaimKey::generate().unwrap();
    let thief = signer(98);
    let result = build_claim_redemption(
        &w.store,
        &thief.did(),
        &thief,
        receipt.id(),
        account(9),
        &thief_key,
        11,
        1,
    );
    assert!(matches!(result, Err(BetaError::InvalidClaimProof)));
}

#[test]
fn a_copied_redemption_cannot_be_redirected_to_another_account() {
    let mut w = world(7);
    let key = ClaimKey::generate().unwrap();
    let receipt = accept(&mut w, b"three", &key, 10);
    let owner = signer(97);
    let honest = create_claim_redemption(
        &mut w.store,
        &owner.did(),
        &owner,
        receipt.id(),
        account(5),
        &key,
        11,
        1,
    )
    .unwrap();

    // An observer re-signs the same payload bytes but swaps the account in the
    // payload: the claim-key signature no longer covers it.
    let Payload::Public(bytes) = &honest.payload else {
        panic!("public payload expected")
    };
    let mut forged = bytes.clone();
    forged[1 + 32..1 + 64].copy_from_slice(&[6u8; 32]);
    let observer = signer(96);
    let copy = ObjectBuilder::new(ObjectType::Custom(BETA_CLAIM_REDEMPTION_TYPE.to_string()))
        .timestamp_ms(12)
        .sequence(1)
        .payload(Payload::Public(forged))
        .link("contribution", receipt.id().clone())
        .sign(&observer.did(), &observer)
        .unwrap();
    w.store.insert(&copy).unwrap();
    assert!(matches!(
        verify_claim_redemption(&w.store, copy.id()),
        Err(BetaError::InvalidClaimProof)
    ));
    // The honest one is untouched.
    verify_claim_redemption(&w.store, honest.id()).unwrap();
}

#[test]
fn a_redemption_for_another_contribution_is_rejected_in_a_grant() {
    let mut w = world(7);
    let key_a = ClaimKey::generate().unwrap();
    let key_b = ClaimKey::generate().unwrap();
    let a = accept(&mut w, b"a", &key_a, 10);
    let b = accept(&mut w, b"b", &key_b, 11);
    let who = signer(95);
    let red_a = create_claim_redemption(
        &mut w.store,
        &who.did(),
        &who,
        a.id(),
        account(5),
        &key_a,
        12,
        1,
    )
    .unwrap();
    // A grant citing contribution B but redemption A (hand-built).
    let mut payload = vec![1u8];
    payload.extend_from_slice(w.epoch.as_bytes());
    payload.extend_from_slice(account(5).as_bytes());
    for text in ["participation", "mismatch"] {
        payload.extend_from_slice(&(text.len() as u32).to_be_bytes());
        payload.extend_from_slice(text.as_bytes());
        if text == "participation" {
            payload.extend_from_slice(&500u64.to_be_bytes());
        }
    }
    let grant = ObjectBuilder::new(ObjectType::Custom(mini_beta::BETA_GRANT_TYPE.to_string()))
        .timestamp_ms(13)
        .sequence(13)
        .payload(Payload::Public(payload))
        .link("campaign", w.campaign.clone())
        .link("contribution", b.id().clone())
        .link("redemption", red_a.id().clone())
        .sign(&w.operator.did(), &w.operator)
        .unwrap();
    w.store.insert(&grant).unwrap();
    let mut ledger = BetaMiniLedger::new(w.epoch, BetaMiniPolicy::open_beta_default()).unwrap();
    assert!(matches!(
        ledger.apply_grant(&w.store, grant.id()),
        Err(BetaError::InvalidClaimProof)
    ));
    assert_eq!(ledger.balance(&account(5)), 0);
}

#[test]
fn a_redemption_does_not_cross_epochs() {
    let mut w = world(7);
    let key = ClaimKey::generate().unwrap();
    let receipt = accept(&mut w, b"epoch", &key, 10);
    let who = signer(94);
    let redemption = create_claim_redemption(
        &mut w.store,
        &who.did(),
        &who,
        receipt.id(),
        account(5),
        &key,
        11,
        1,
    )
    .unwrap();
    let grant = build_redeemed_participation_grant(
        &w.store,
        &w.operator.did(),
        &w.operator,
        redemption.id(),
        100,
        "epoch bound",
        12,
        12,
    )
    .unwrap();
    w.store.insert(&grant).unwrap();
    let mut next = BetaMiniLedger::new(
        BetaEpochId::new([8; 32]).unwrap(),
        BetaMiniPolicy::open_beta_default(),
    )
    .unwrap();
    assert!(matches!(
        next.apply_grant(&w.store, grant.id()),
        Err(BetaError::WrongEpoch)
    ));
}

#[test]
fn a_claim_key_secret_round_trips_and_a_wrong_secret_proves_nothing() {
    let key = ClaimKey::generate().unwrap();
    let again = ClaimKey::from_secret(&key.secret());
    assert_eq!(key.claim_tag().unwrap(), again.claim_tag().unwrap());
    let other = ClaimKey::generate().unwrap();
    assert_ne!(key.claim_tag().unwrap(), other.claim_tag().unwrap());
    // Debug never prints key material.
    assert_eq!(format!("{key:?}"), "ClaimKey { .. }");
}

#[test]
fn a_legacy_hash_tag_receipt_cannot_be_redeemed() {
    let mut w = world(7);
    // A receipt whose tag is not an Ed25519 key we hold (the old random tag).
    let src = ObjectBuilder::new(ObjectType::Custom("mini/claim-test-source".to_string()))
        .payload(Payload::Public(b"legacy".to_vec()))
        .sign(&w.operator.did(), &w.operator)
        .unwrap();
    w.store.insert(&src).unwrap();
    let legacy = create_contribution_receipt(
        &mut w.store,
        &w.operator.did(),
        &w.operator,
        src.id(),
        Some(&w.campaign),
        mini_beta::ClaimTag::new([3; 32]).unwrap(),
        ContributionKind::Testing,
        "legacy",
        &["evidence:accepted".to_string()],
        20,
        20,
    )
    .unwrap();
    let key = ClaimKey::generate().unwrap();
    let who = signer(93);
    assert!(matches!(
        build_claim_redemption(
            &w.store,
            &who.did(),
            &who,
            legacy.id(),
            account(5),
            &key,
            21,
            1
        ),
        Err(BetaError::InvalidClaimProof)
    ));
}

#[test]
fn a_participation_grant_without_a_redemption_is_refused_everywhere() {
    use mini_beta::{create_grant_authorization, GrantClass};
    let mut w = world(7);
    let key = ClaimKey::generate().unwrap();
    let receipt = accept(&mut w, b"no-redemption", &key, 10);

    // The builders refuse to make one.
    let built = create_grant_authorization(
        &mut w.store,
        &w.operator.did(),
        &w.operator,
        &w.campaign,
        Some(receipt.id()),
        w.epoch,
        account(5),
        GrantClass::Participation,
        100,
        "typed account",
        12,
        12,
    );
    assert!(matches!(built, Err(BetaError::InvalidClaimProof)));

    // A hand-built object with no redemption link fails to parse, so it can
    // never reach the ledger.
    let mut payload = vec![1u8];
    payload.extend_from_slice(w.epoch.as_bytes());
    payload.extend_from_slice(account(5).as_bytes());
    for text in ["participation", "typed"] {
        payload.extend_from_slice(&(text.len() as u32).to_be_bytes());
        payload.extend_from_slice(text.as_bytes());
        if text == "participation" {
            payload.extend_from_slice(&100u64.to_be_bytes());
        }
    }
    let forged = ObjectBuilder::new(ObjectType::Custom(mini_beta::BETA_GRANT_TYPE.to_string()))
        .timestamp_ms(13)
        .sequence(13)
        .payload(Payload::Public(payload))
        .link("campaign", w.campaign.clone())
        .link("contribution", receipt.id().clone())
        .sign(&w.operator.did(), &w.operator)
        .unwrap();
    w.store.insert(&forged).unwrap();
    let mut ledger = BetaMiniLedger::new(w.epoch, BetaMiniPolicy::open_beta_default()).unwrap();
    assert!(ledger.apply_grant(&w.store, forged.id()).is_err());
    assert_eq!(ledger.balance(&account(5)), 0);
}

#[test]
fn a_small_order_claim_key_is_refused_and_cannot_be_redeemed_without_a_secret() {
    use mini_beta::{validate_claim_tag, ClaimTag};
    // The compressed Ed25519 identity point (y = 1) is a small-order key.
    let mut identity = [0u8; 32];
    identity[0] = 1;
    let weak = ClaimTag::new(identity).unwrap();
    assert!(matches!(
        validate_claim_tag(&weak),
        Err(BetaError::InvalidClaimProof)
    ));

    // Even if an operator recorded such a tag, an identity-R, zero-s signature
    // (valid under non-strict verification for any message) does not redeem.
    let mut w = world(7);
    let src = ObjectBuilder::new(ObjectType::Custom("mini/claim-test-source".to_string()))
        .payload(Payload::Public(b"weak".to_vec()))
        .sign(&w.operator.did(), &w.operator)
        .unwrap();
    w.store.insert(&src).unwrap();
    let receipt = create_contribution_receipt(
        &mut w.store,
        &w.operator.did(),
        &w.operator,
        src.id(),
        Some(&w.campaign),
        weak,
        ContributionKind::Testing,
        "weak tag",
        &["evidence:accepted".to_string()],
        30,
        30,
    )
    .unwrap();
    let mut payload = vec![1u8];
    payload.extend_from_slice(w.epoch.as_bytes());
    payload.extend_from_slice(account(5).as_bytes());
    let mut sig = [0u8; 64];
    sig[0] = 1; // R = identity, s = 0
    payload.extend_from_slice(&sig);
    let forged = ObjectBuilder::new(ObjectType::Custom(BETA_CLAIM_REDEMPTION_TYPE.to_string()))
        .timestamp_ms(31)
        .sequence(31)
        .payload(Payload::Public(payload))
        .link("contribution", receipt.id().clone())
        .sign(&signer(9).did(), &signer(9))
        .unwrap();
    w.store.insert(&forged).unwrap();
    assert!(matches!(
        verify_claim_redemption(&w.store, forged.id()),
        Err(BetaError::InvalidClaimProof)
    ));
}
