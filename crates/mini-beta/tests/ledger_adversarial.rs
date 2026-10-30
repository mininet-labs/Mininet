use did_mini::Controller;
use mini_beta::{
    create_campaign, create_contribution_receipt, create_grant_authorization, BetaAccountId,
    BetaEpochId, BetaError, BetaMiniLedger, BetaMiniPolicy, ClaimTag, ContributionKind, GrantClass,
    BETA_GRANT_TYPE,
};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{MemoryBackend, Store};

fn signer(seed: u8) -> Controller {
    Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap()
}

fn target(store: &mut Store<MemoryBackend>, signer: &Controller, label: &[u8]) -> ObjectId {
    let object = ObjectBuilder::new(ObjectType::Custom(
        "mini/beta-adversarial-target".to_string(),
    ))
    .payload(Payload::Public(label.to_vec()))
    .sign(&signer.did(), signer)
    .unwrap();
    store.insert(&object).unwrap();
    object.id().clone()
}

fn campaign(
    store: &mut Store<MemoryBackend>,
    signer: &Controller,
    target: &ObjectId,
    epoch: BetaEpochId,
    cap: u64,
    sequence: u64,
) -> Object {
    create_campaign(
        store,
        &signer.did(),
        signer,
        target,
        epoch,
        "adversarial campaign",
        "exercise the apply-path security boundary",
        &["ledger".to_string()],
        1,
        100,
        cap,
        sequence,
        sequence,
    )
    .unwrap()
}

fn contribution(
    store: &mut Store<MemoryBackend>,
    signer: &Controller,
    source: &ObjectId,
    campaign_id: &ObjectId,
    tag: u8,
    sequence: u64,
) -> Object {
    create_contribution_receipt(
        store,
        &signer.did(),
        signer,
        source,
        Some(campaign_id),
        ClaimTag::new([tag; 32]).unwrap(),
        ContributionKind::Testing,
        "accepted adversarial contribution",
        &["evidence:accepted".to_string()],
        sequence,
        sequence,
    )
    .unwrap()
}

fn put_str(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
}

#[allow(clippy::too_many_arguments)]
fn raw_grant(
    store: &mut Store<MemoryBackend>,
    signer: &Controller,
    campaign_id: &ObjectId,
    contribution_id: Option<&ObjectId>,
    epoch: BetaEpochId,
    account: BetaAccountId,
    class: GrantClass,
    amount: u64,
    memo: &str,
    sequence: u64,
) -> Object {
    let mut payload = vec![1u8];
    payload.extend_from_slice(epoch.as_bytes());
    payload.extend_from_slice(account.as_bytes());
    put_str(&mut payload, class.as_str());
    payload.extend_from_slice(&amount.to_be_bytes());
    put_str(&mut payload, memo);

    let mut builder = ObjectBuilder::new(ObjectType::Custom(BETA_GRANT_TYPE.to_string()))
        .timestamp_ms(sequence)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .link("campaign", campaign_id.clone());
    if let Some(contribution_id) = contribution_id {
        builder = builder.link("contribution", contribution_id.clone());
    }
    let object = builder.sign(&signer.did(), signer).unwrap();
    store.insert(&object).unwrap();
    object
}

#[test]
fn one_contribution_cannot_back_two_distinct_participation_grants() {
    let signer = signer(10);
    let mut store = Store::new(MemoryBackend::new());
    let target = target(&mut store, &signer, b"same contribution twice");
    let epoch = BetaEpochId::new([1; 32]).unwrap();
    let campaign = campaign(&mut store, &signer, &target, epoch, 1_000, 1);
    let contribution = contribution(&mut store, &signer, &target, campaign.id(), 2, 2);
    let account = BetaAccountId::new([3; 32]).unwrap();

    let first = create_grant_authorization(
        &mut store,
        &signer.did(),
        &signer,
        campaign.id(),
        Some(contribution.id()),
        epoch,
        account,
        GrantClass::Participation,
        10,
        "first",
        3,
        3,
    )
    .unwrap();
    let second = create_grant_authorization(
        &mut store,
        &signer.did(),
        &signer,
        campaign.id(),
        Some(contribution.id()),
        epoch,
        account,
        GrantClass::Participation,
        10,
        "second distinct authorization",
        4,
        4,
    )
    .unwrap();
    assert_ne!(first.id(), second.id());

    let mut ledger = BetaMiniLedger::new(epoch, BetaMiniPolicy::open_beta_default()).unwrap();
    ledger.apply_grant(&store, first.id()).unwrap();
    assert!(matches!(
        ledger.apply_grant(&store, second.id()),
        Err(BetaError::DuplicateGrant)
    ));
    assert_eq!(ledger.balance(&account), 10);
}

#[test]
fn two_receipts_for_the_same_source_cannot_each_back_a_participation_grant() {
    // create_contribution_receipt is a bookkeeping record, not a uniqueness
    // authority -- nothing stops a record signer from creating two distinct
    // receipt objects for the identical accepted work (same `source_id`).
    // The ledger, not the intake path, is what must refuse to pay out twice
    // for one piece of work.
    let signer = signer(50);
    let mut store = Store::new(MemoryBackend::new());
    let target = target(&mut store, &signer, b"one piece of accepted work");
    let epoch = BetaEpochId::new([11; 32]).unwrap();
    let campaign = campaign(&mut store, &signer, &target, epoch, 1_000, 1);
    // Two independent receipts, same source, different claim tags/sequences.
    let receipt_a = contribution(&mut store, &signer, &target, campaign.id(), 21, 2);
    let receipt_b = contribution(&mut store, &signer, &target, campaign.id(), 22, 3);
    assert_ne!(receipt_a.id(), receipt_b.id());
    let account = BetaAccountId::new([12; 32]).unwrap();

    let first = create_grant_authorization(
        &mut store,
        &signer.did(),
        &signer,
        campaign.id(),
        Some(receipt_a.id()),
        epoch,
        account,
        GrantClass::Participation,
        10,
        "first receipt",
        4,
        4,
    )
    .unwrap();
    let second = create_grant_authorization(
        &mut store,
        &signer.did(),
        &signer,
        campaign.id(),
        Some(receipt_b.id()),
        epoch,
        account,
        GrantClass::Participation,
        10,
        "second receipt, same underlying work",
        5,
        5,
    )
    .unwrap();

    let mut ledger = BetaMiniLedger::new(epoch, BetaMiniPolicy::open_beta_default()).unwrap();
    ledger.apply_grant(&store, first.id()).unwrap();
    assert!(matches!(
        ledger.apply_grant(&store, second.id()),
        Err(BetaError::DuplicateGrant)
    ));
    assert_eq!(ledger.balance(&account), 10);
}

#[test]
fn participation_grant_rechecks_contribution_campaign_on_apply() {
    let signer = signer(20);
    let mut store = Store::new(MemoryBackend::new());
    let target = target(&mut store, &signer, b"cross campaign");
    let epoch = BetaEpochId::new([4; 32]).unwrap();
    let campaign_a = campaign(&mut store, &signer, &target, epoch, 1_000, 1);
    let campaign_b = campaign(&mut store, &signer, &target, epoch, 1_000, 2);
    let contribution = contribution(&mut store, &signer, &target, campaign_a.id(), 5, 3);
    let account = BetaAccountId::new([6; 32]).unwrap();

    let forged = raw_grant(
        &mut store,
        &signer,
        campaign_b.id(),
        Some(contribution.id()),
        epoch,
        account,
        GrantClass::Participation,
        10,
        "wrong campaign",
        4,
    );

    let mut ledger = BetaMiniLedger::new(epoch, BetaMiniPolicy::open_beta_default()).unwrap();
    assert!(matches!(
        ledger.apply_grant(&store, forged.id()),
        Err(BetaError::InvalidObject)
    ));
    assert_eq!(ledger.balance(&account), 0);
}

#[test]
fn synced_testing_grant_cannot_bypass_campaign_cap() {
    let signer = signer(30);
    let mut store = Store::new(MemoryBackend::new());
    let target = target(&mut store, &signer, b"campaign cap");
    let epoch = BetaEpochId::new([7; 32]).unwrap();
    let campaign = campaign(&mut store, &signer, &target, epoch, 500, 1);
    let account = BetaAccountId::new([8; 32]).unwrap();

    let forged = raw_grant(
        &mut store,
        &signer,
        campaign.id(),
        None,
        epoch,
        account,
        GrantClass::Testing,
        501,
        "above campaign cap but below global cap",
        2,
    );

    let mut ledger = BetaMiniLedger::new(epoch, BetaMiniPolicy::open_beta_default()).unwrap();
    assert!(matches!(
        ledger.apply_grant(&store, forged.id()),
        Err(BetaError::GrantLimitExceeded)
    ));
    assert_eq!(ledger.balance(&account), 0);
}

#[test]
fn self_transfer_still_requires_sufficient_balance() {
    let signer = signer(40);
    let mut store = Store::new(MemoryBackend::new());
    let target = target(&mut store, &signer, b"self transfer");
    let epoch = BetaEpochId::new([9; 32]).unwrap();
    let campaign = campaign(&mut store, &signer, &target, epoch, 100, 1);
    let account = BetaAccountId::new([10; 32]).unwrap();
    let grant = create_grant_authorization(
        &mut store,
        &signer.did(),
        &signer,
        campaign.id(),
        None,
        epoch,
        account,
        GrantClass::Testing,
        10,
        "fund account",
        2,
        2,
    )
    .unwrap();

    let mut ledger = BetaMiniLedger::new(epoch, BetaMiniPolicy::open_beta_default()).unwrap();
    ledger.apply_grant(&store, grant.id()).unwrap();
    assert!(matches!(
        ledger.transfer(&account, &account, 11),
        Err(BetaError::InsufficientBalance)
    ));
    ledger.transfer(&account, &account, 10).unwrap();
    assert_eq!(ledger.balance(&account), 10);
}
