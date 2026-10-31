use super::*;
use did_mini::Controller;
use mini_app_protocol::{BetaBalancesView, Command, ErrorCode, Reply};
use mini_beta::{
    create_campaign, create_grant_authorization, BetaAccountId, BetaEpochId, GrantClass,
};
use mini_beta_exec::{create_account_registration, parse_account_registration_object};
use mini_beta_grants::{create_grant_approval, create_grant_policy};
use mini_objects::{ObjectBuilder, Payload};

const EPOCH: [u8; 32] = [7; 32];

fn signer(seed: u8) -> Controller {
    Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap()
}

fn unlocked(tag: &str) -> (Core<MemoryVault>, PathBuf) {
    let root = temp_root(tag);
    let mut core = Core::open_inner(root.clone(), MemoryVault::seeded(), false).unwrap();
    core.handle(Command::UnlockIdentity).unwrap();
    (core, root)
}

fn op() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!("beta-test-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

fn register(core: &mut Core<MemoryVault>, epoch: [u8; 32]) -> [u8; 32] {
    match core
        .handle(Command::BetaRegisterAccount {
            operation_id: op(),
            epoch,
        })
        .unwrap()
    {
        Reply::BetaAccount { account, .. } => account,
        _ => panic!("unexpected reply"),
    }
}

fn balances(core: &mut Core<MemoryVault>, epoch: [u8; 32], campaign: &str) -> BetaBalancesView {
    match core
        .handle(Command::BetaBalances {
            epoch,
            campaigns: vec![campaign.to_string()],
        })
        .unwrap()
    {
        Reply::BetaBalances(view) => view,
        _ => panic!("unexpected reply"),
    }
}

fn amount_of(view: &BetaBalancesView, account: [u8; 32]) -> u64 {
    view.accounts
        .iter()
        .find(|entry| entry.account == account)
        .map(|entry| entry.amount_micro)
        .expect("account listed")
}

/// Campaign + 2-of-3 policy + a 100 testing grant for `account`, approved by
/// two members, all written straight into the core's store by library calls.
fn fund(core: &mut Core<MemoryVault>, account: [u8; 32]) -> String {
    let author = signer(10);
    let members = [signer(20), signer(30), signer(40)];
    let epoch = BetaEpochId::new(EPOCH).unwrap();
    let store = &mut core.store;
    let target = ObjectBuilder::new(ObjectType::Custom("mininet.beta-wallet/test".into()))
        .timestamp_ms(1)
        .sequence(1)
        .payload(Payload::Public(b"service-fixture".to_vec()))
        .sign(&author.did(), &author)
        .unwrap();
    store.insert(&target).unwrap();
    let campaign = create_campaign(
        store,
        &author.did(),
        &author,
        target.id(),
        epoch,
        "service beta",
        "exercise the wallet",
        &["wallet".to_string()],
        100,
        10_000,
        1_000,
        100,
        2,
    )
    .unwrap();
    let dids = members.iter().map(Controller::did).collect::<Vec<_>>();
    let policy = create_grant_policy(
        store,
        &author.did(),
        &author,
        campaign.id(),
        &dids,
        2,
        2,
        100,
        &[25, 50, 100],
        200,
        9_000,
        150,
        3,
    )
    .unwrap();
    let grant = create_grant_authorization(
        store,
        &author.did(),
        &author,
        campaign.id(),
        None,
        epoch,
        BetaAccountId::new(account).unwrap(),
        GrantClass::Testing,
        100,
        "testing balance",
        500,
        4,
    )
    .unwrap();
    for (index, member) in members.iter().take(2).enumerate() {
        create_grant_approval(
            store,
            &member.did(),
            member,
            policy.id(),
            grant.id(),
            600 + index as u64,
            1,
        )
        .unwrap();
    }
    campaign.id().as_str().to_string()
}

fn transfer(
    core: &mut Core<MemoryVault>,
    campaign: &str,
    from: [u8; 32],
    to: [u8; 32],
    amount: u64,
) -> Result<Reply, ServiceError> {
    transfer_op(core, &op(), campaign, from, to, amount)
}

fn transfer_op(
    core: &mut Core<MemoryVault>,
    operation_id: &str,
    campaign: &str,
    from: [u8; 32],
    to: [u8; 32],
    amount: u64,
) -> Result<Reply, ServiceError> {
    core.handle(Command::BetaTransfer {
        operation_id: operation_id.to_string(),
        epoch: EPOCH,
        campaigns: vec![campaign.to_string()],
        from,
        to,
        amount_micro: amount,
        memo: "test".into(),
    })
}

#[test]
fn register_returns_fresh_accounts() {
    let (mut core, root) = unlocked("beta-register");
    let a = register(&mut core, EPOCH);
    let b = register(&mut core, EPOCH);
    assert_ne!(a, b);
    fs::remove_dir_all(root).unwrap();
}

fn foreign_account(core: &mut Core<MemoryVault>, epoch: [u8; 32]) -> [u8; 32] {
    let foreign = signer(90);
    let registration = create_account_registration(
        &mut core.store,
        &foreign.did(),
        &foreign,
        BetaEpochId::new(epoch).unwrap(),
        mini_crypto::random_32().unwrap(),
        10,
        1,
    )
    .unwrap();
    *parse_account_registration_object(&registration)
        .unwrap()
        .account
        .as_bytes()
}

#[test]
fn unfunded_balance_is_zero_and_lists_only_own_accounts() {
    let (mut core, root) = unlocked("beta-unfunded");
    let own = register(&mut core, EPOCH);
    let foreign = foreign_account(&mut core, EPOCH);
    // The campaign funds the foreign account only.
    let campaign = fund(&mut core, foreign);
    let view = balances(&mut core, EPOCH, &campaign);
    assert_eq!(view.accounts.len(), 1);
    assert_eq!(amount_of(&view, own), 0);
    assert_eq!(view.total_issued_micro, 100);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn funded_account_shows_balance_and_transfer_moves_value_with_change() {
    let (mut core, root) = unlocked("beta-funded");
    let alice = register(&mut core, EPOCH);
    let bob = foreign_account(&mut core, EPOCH);
    let campaign = fund(&mut core, alice);
    let view = balances(&mut core, EPOCH, &campaign);
    assert_eq!(amount_of(&view, alice), 100);
    assert_eq!(view.conflicts, 0);

    let reply = transfer(&mut core, &campaign, alice, bob, 30).unwrap();
    match reply {
        Reply::BetaTransfer {
            amount_micro,
            change_micro,
            ..
        } => assert_eq!((amount_micro, change_micro), (30, 70)),
        _ => panic!("unexpected reply"),
    }
    let after = balances(&mut core, EPOCH, &campaign);
    assert_eq!(amount_of(&after, alice), 70);
    assert_eq!(after.total_issued_micro, 100);
    // Exact spend with no change.
    match transfer(&mut core, &campaign, alice, bob, 70).unwrap() {
        Reply::BetaTransfer { change_micro, .. } => assert_eq!(change_micro, 0),
        _ => panic!("unexpected reply"),
    }
    let end = balances(&mut core, EPOCH, &campaign);
    assert_eq!(amount_of(&end, alice), 0);
    assert_eq!(end.total_issued_micro, 100);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn bad_transfers_are_refused_and_change_nothing() {
    let (mut core, root) = unlocked("beta-refused");
    let alice = register(&mut core, EPOCH);
    let bob = foreign_account(&mut core, EPOCH);
    let campaign = fund(&mut core, alice);
    let before = balances(&mut core, EPOCH, &campaign);
    let count = |core: &Core<MemoryVault>| {
        core.store
            .by_type(&ObjectType::Custom(
                mini_beta_exec::BETA_TRANSFER_TYPE.to_string(),
            ))
            .unwrap()
            .len()
    };

    let insufficient = transfer(&mut core, &campaign, alice, bob, 101).unwrap_err();
    assert_eq!(insufficient.code, ErrorCode::BadRequest);
    let unowned = transfer(&mut core, &campaign, bob, alice, 1).unwrap_err();
    assert_eq!(unowned.code, ErrorCode::BadRequest);
    let unregistered = transfer(&mut core, &campaign, alice, [5; 32], 1).unwrap_err();
    assert_eq!(unregistered.code, ErrorCode::BadRequest);
    assert_eq!(count(&core), 0);

    core.handle(Command::LockIdentity).unwrap();
    let locked = transfer(&mut core, &campaign, alice, bob, 1).unwrap_err();
    assert_eq!(locked.code, ErrorCode::IdentityLocked);
    let locked_register = core
        .handle(Command::BetaRegisterAccount {
            operation_id: op(),
            epoch: EPOCH,
        })
        .unwrap_err();
    assert_eq!(locked_register.code, ErrorCode::IdentityLocked);
    assert_eq!(count(&core), 0);
    assert_eq!(balances(&mut core, EPOCH, &campaign), before);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_different_epoch_carries_nothing() {
    let (mut core, root) = unlocked("beta-epoch");
    let alice = register(&mut core, EPOCH);
    let campaign = fund(&mut core, alice);
    let other = [8; 32];
    let fresh = register(&mut core, other);
    let view = balances(&mut core, other, &campaign);
    assert_eq!(view.accounts.len(), 1);
    assert_eq!(amount_of(&view, fresh), 0);
    assert_eq!(view.total_issued_micro, 0);
    fs::remove_dir_all(root).unwrap();
}

fn count_type(core: &Core<MemoryVault>, kind: &str) -> usize {
    core.store
        .by_type(&ObjectType::Custom(kind.to_string()))
        .unwrap()
        .len()
}

#[test]
fn register_retry_with_the_same_operation_id_returns_the_same_account() {
    let (mut core, root) = unlocked("beta-register-retry");
    let id = "beta-retry-register";
    let first = core
        .handle(Command::BetaRegisterAccount {
            operation_id: id.into(),
            epoch: EPOCH,
        })
        .unwrap();
    let again = core
        .handle(Command::BetaRegisterAccount {
            operation_id: id.into(),
            epoch: EPOCH,
        })
        .unwrap();
    assert_eq!(first, again);
    assert_eq!(count_type(&core, mini_beta_exec::BETA_ACCOUNT_TYPE), 1);
    // Same id for different content is refused, never silently replayed.
    let other_epoch = core
        .handle(Command::BetaRegisterAccount {
            operation_id: id.into(),
            epoch: [8; 32],
        })
        .unwrap_err();
    assert_eq!(other_epoch.code, ErrorCode::BadRequest);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn transfer_retry_never_signs_a_second_transfer() {
    let (mut core, root) = unlocked("beta-transfer-retry");
    let alice = register(&mut core, EPOCH);
    let bob = foreign_account(&mut core, EPOCH);
    let campaign = fund(&mut core, alice);
    let first = transfer_op(&mut core, "beta-retry-transfer", &campaign, alice, bob, 30).unwrap();
    let again = transfer_op(&mut core, "beta-retry-transfer", &campaign, alice, bob, 30).unwrap();
    assert_eq!(first, again);
    assert_eq!(count_type(&core, mini_beta_exec::BETA_TRANSFER_TYPE), 1);
    assert_eq!(amount_of(&balances(&mut core, EPOCH, &campaign), alice), 70);
    // Reusing the id for a different amount is an error, not a second spend.
    let changed =
        transfer_op(&mut core, "beta-retry-transfer", &campaign, alice, bob, 31).unwrap_err();
    assert_eq!(changed.code, ErrorCode::BadRequest);
    assert_eq!(count_type(&core, mini_beta_exec::BETA_TRANSFER_TYPE), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_journaled_but_uncommitted_registration_is_recovered_not_duplicated() {
    let root = temp_root("beta-crash");
    let mut core = Core::open_inner(root.clone(), MemoryVault::seeded(), false).unwrap();
    core.handle(Command::UnlockIdentity).unwrap();
    let epoch = BetaEpochId::new(EPOCH).unwrap();
    let id = "beta-crash-register";
    // Crash window: the signed registration is journaled but never committed.
    let nonce = mini_crypto::random_32().unwrap();
    let slot = beta_wallet::derive_slot_for_test(&mut core, epoch, nonce);
    let owner = slot.identity.root.did();
    let object = mini_beta_exec::build_account_registration(
        &owner,
        &slot.identity.device,
        epoch,
        slot.nonce,
        10,
        1,
    )
    .unwrap();
    beta_journal::write_pending(
        &core.beta_ops,
        id,
        &beta_journal::Record {
            kind: beta_journal::KIND_REGISTER,
            digest: beta_wallet::input_digest(b"register", &[epoch.as_bytes()]),
            account: *slot.account.as_bytes(),
            amount: 0,
            change: 0,
            object: object.to_bytes(),
        },
    )
    .unwrap();
    assert_eq!(count_type(&core, mini_beta_exec::BETA_ACCOUNT_TYPE), 0);
    drop(core);
    // Reopening commits the journaled object; the retry then replays it.
    let mut core = Core::open_inner(root.clone(), MemoryVault::seeded(), false).unwrap();
    assert_eq!(count_type(&core, mini_beta_exec::BETA_ACCOUNT_TYPE), 1);
    core.handle(Command::UnlockIdentity).unwrap();
    let reply = core
        .handle(Command::BetaRegisterAccount {
            operation_id: id.into(),
            epoch: EPOCH,
        })
        .unwrap();
    match reply {
        Reply::BetaAccount { account, .. } => assert_eq!(account, *slot.account.as_bytes()),
        _ => panic!("unexpected reply"),
    }
    assert_eq!(count_type(&core, mini_beta_exec::BETA_ACCOUNT_TYPE), 1);
    // The recovered account is owned by this profile.
    let owned = core.beta_own_slots(epoch).unwrap();
    assert_eq!(owned.len(), 1);
    assert_eq!(owned[0].account, slot.account);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn two_devices_restored_from_one_root_never_collide() {
    let (mut one, root_one) = unlocked("beta-device-one");
    let (mut two, root_two) = unlocked("beta-device-two");
    let a = register(&mut one, EPOCH);
    let b = register(&mut two, EPOCH);
    assert_ne!(a, b, "random nonce per registration, so accounts differ");
    // Registrations from both devices land in one store with no conflict.
    let objects: Vec<_> = two
        .store
        .by_type(&ObjectType::Custom(
            mini_beta_exec::BETA_ACCOUNT_TYPE.to_string(),
        ))
        .unwrap()
        .into_iter()
        .map(|id| two.store.get(&id).unwrap())
        .collect();
    for object in objects {
        one.store.insert(&object).unwrap();
    }
    let campaign = fund(&mut one, a);
    let snapshot = mini_beta_exec::resolve_snapshot(
        &one.store,
        BetaEpochId::new(EPOCH).unwrap(),
        mini_beta::BetaMiniPolicy::open_beta_default(),
        &mini_beta_exec::EpochIssuancePolicy::new(
            BetaEpochId::new(EPOCH).unwrap(),
            &[ObjectId::parse(&campaign).unwrap()],
        ),
    )
    .unwrap();
    assert!(snapshot.account_conflicts.is_empty());
    fs::remove_dir_all(root_one).unwrap();
    fs::remove_dir_all(root_two).unwrap();
}

#[test]
fn derived_identities_publish_kel_carriers_for_verified_sync() {
    let (mut core, root) = unlocked("beta-kels");
    let account = register(&mut core, EPOCH);
    let primary = core.human.clone().unwrap();
    let carriers = core
        .store
        .by_type(&ObjectType::Custom(mini_sync::KEL_CARRIER.to_string()))
        .unwrap();
    assert_eq!(carriers.len(), 2, "derived root and device KEL carriers");
    for id in carriers {
        assert_ne!(core.store.get(&id).unwrap().author_human, primary);
    }
    // A peer that has only the carriers and the registration can verify them.
    let mut cache = mini_sync::KelCache::new();
    cache.hydrate_from_store(&core.store).unwrap();
    for id in core
        .store
        .by_type(&ObjectType::Custom(
            mini_beta_exec::BETA_ACCOUNT_TYPE.to_string(),
        ))
        .unwrap()
    {
        let object = core.store.get(&id).unwrap();
        assert!(matches!(
            mini_sync::Ingest::check(&mut cache, &object),
            mini_sync::IngestOutcome::Accepted
        ));
    }
    let _ = account;
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn beta_objects_are_not_signed_by_the_primary_identity() {
    let (mut core, root) = unlocked("beta-isolation");
    let primary = core.human.clone().unwrap();
    let alice = register(&mut core, EPOCH);
    let bob = foreign_account(&mut core, EPOCH);
    let campaign = fund(&mut core, alice);
    transfer(&mut core, &campaign, alice, bob, 10).unwrap();
    let mut authors = Vec::new();
    for kind in [
        mini_beta_exec::BETA_ACCOUNT_TYPE,
        mini_beta_exec::BETA_TRANSFER_TYPE,
    ] {
        for id in core
            .store
            .by_type(&ObjectType::Custom(kind.to_string()))
            .unwrap()
        {
            let object = core.store.get(&id).unwrap();
            assert_ne!(object.author_human, primary, "primary DID must not appear");
            authors.push(object.author_human);
        }
    }
    assert!(authors.len() >= 3);
    // Two accounts of the same identity do not share an author either.
    let second = register(&mut core, EPOCH);
    assert_ne!(alice, second);
    let view = balances(&mut core, EPOCH, &campaign);
    assert_eq!(view.accounts.len(), 2);
    fs::remove_dir_all(root).unwrap();
}
