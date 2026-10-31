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

fn register(core: &mut Core<MemoryVault>, epoch: [u8; 32]) -> [u8; 32] {
    match core.handle(Command::BetaRegisterAccount { epoch }).unwrap() {
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
    core.handle(Command::BetaTransfer {
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
        .handle(Command::BetaRegisterAccount { epoch: EPOCH })
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
