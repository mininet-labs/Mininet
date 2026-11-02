use super::*;
use did_mini::Controller;
use mini_app_protocol::{Command, Reply};
use mini_objects::{ObjectBuilder, ObjectType, Payload};
use mini_store::CacheTier;

const DAY: u64 = 86_400_000;

fn other(seed: u8) -> Controller {
    Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap()
}

fn foreign_post(author: &Controller, seq: u64) -> mini_objects::Object {
    ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(1_000 + seq)
        .sequence(seq)
        .payload(Payload::Public(format!("remote post {seq}").into_bytes()))
        .sign(&author.did(), author)
        .unwrap()
}

fn unlocked(tag: &str) -> (Core<MemoryVault>, PathBuf) {
    let root = temp_root(tag);
    let mut core = Core::open_inner(root.clone(), MemoryVault::seeded(), false).unwrap();
    core.handle(Command::UnlockIdentity).unwrap();
    (core, root)
}

fn owner(core: &Core<MemoryVault>) -> Did {
    core.human.clone().unwrap()
}

/// Insert a foreign post and backdate its first-seen time.
fn seed(core: &mut Core<MemoryVault>, author: &Controller, seq: u64, age_days: u64) -> ObjectId {
    let object = foreign_post(author, seq);
    core.store.insert(&object).unwrap();
    let seen = now_ms() - age_days * DAY;
    core.store.note_first_seen(object.id(), seen).unwrap();
    object.id().clone()
}

#[test]
fn expired_foreign_cache_is_evicted_and_everything_else_is_kept() {
    let (mut core, root) = unlocked("cache-evict");
    let remote = other(60);
    let old = seed(&mut core, &remote, 1, 45);
    let fresh = seed(&mut core, &remote, 2, 3);
    let pinned = seed(&mut core, &remote, 3, 400);
    core.store
        .set_cache_tier(&pinned, CacheTier::PinnedByOwner)
        .unwrap();
    // The owner's own post is never touched, however old it looks.
    let own = ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(5)
        .sequence(1)
        .payload(Payload::Public(b"mine".to_vec()))
        .sign(&owner(&core), &core.identity.as_ref().unwrap().device)
        .unwrap();
    core.store.insert(&own).unwrap();
    core.store.note_first_seen(own.id(), 1).unwrap();
    // A remote object of a type outside the allow-list is never evicted.
    let remote_head = ObjectBuilder::new(ObjectType::PROFILE)
        .timestamp_ms(9)
        .sequence(9)
        .payload(Payload::Public(b"profile".to_vec()))
        .sign(&remote.did(), &remote)
        .unwrap();
    core.store.insert(&remote_head).unwrap();
    core.store.note_first_seen(remote_head.id(), 1).unwrap();

    match core.handle(Command::StorageStatus).unwrap() {
        Reply::Storage { .. } => {}
        _ => panic!("unexpected reply"),
    }
    assert!(
        !core.store.contains(&old).unwrap(),
        "30-day default evicts 45-day-old cache"
    );
    for kept in [&fresh, &pinned, own.id(), remote_head.id()] {
        assert!(core.store.contains(kept).unwrap());
    }
    assert!(core.cache_freed_bytes > 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn retention_is_chosen_by_the_owner_and_never_means_never() {
    let (mut core, root) = unlocked("cache-retention");
    let remote = other(61);
    let old = seed(&mut core, &remote, 1, 400);

    // Never: nothing is deleted automatically, however old.
    let Reply::CacheRetention { value, .. } = core
        .handle(Command::CacheRetentionSet { value: 3 })
        .unwrap()
    else {
        panic!("unexpected reply")
    };
    assert_eq!(value, 3);
    core.handle(Command::StorageStatus).unwrap();
    assert!(core.store.contains(&old).unwrap());

    // Long (180 days) still evicts a 400-day-old object.
    core.handle(Command::CacheRetentionSet { value: 2 })
        .unwrap();
    core.handle(Command::StorageStatus).unwrap();
    assert!(!core.store.contains(&old).unwrap());

    // The choice survives a restart; invalid values are refused.
    drop(core);
    let reopened = Core::open_inner(root.clone(), MemoryVault::seeded(), false).unwrap();
    assert_eq!(reopened.cache_retention.to_u8(), 2);
    let mut reopened = reopened;
    assert!(reopened
        .handle(Command::CacheRetentionSet { value: 9 })
        .is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_retained_reply_keeps_its_parent() {
    let (mut core, root) = unlocked("cache-parent");
    let remote = other(62);
    let parent = foreign_post(&remote, 1);
    core.store.insert(&parent).unwrap();
    core.store
        .note_first_seen(parent.id(), now_ms() - 90 * DAY)
        .unwrap();
    // A fresh reply (recently seen, so retained) links to the old parent.
    let reply = ObjectBuilder::new(ObjectType::COMMENT)
        .timestamp_ms(2_000)
        .sequence(2)
        .link("parent", parent.id().clone())
        .payload(Payload::Public(b"reply".to_vec()))
        .sign(&remote.did(), &remote)
        .unwrap();
    core.store.insert(&reply).unwrap();
    core.handle(Command::StorageStatus).unwrap();
    assert!(core.store.contains(parent.id()).unwrap());
    assert!(core.store.contains(reply.id()).unwrap());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn space_pressure_shortens_retention_but_never_overrides_never() {
    use cache_policy::Retention;
    use mini_durable::space::SpaceLevel;
    assert_eq!(
        Retention::Standard.effective_ms(SpaceLevel::Ok),
        Some(30 * DAY)
    );
    assert_eq!(
        Retention::Standard.effective_ms(SpaceLevel::Low),
        Some(7 * DAY)
    );
    assert_eq!(
        Retention::Long.effective_ms(SpaceLevel::Critical),
        Some(DAY)
    );
    assert_eq!(
        Retention::Short.effective_ms(SpaceLevel::Low),
        Some(7 * DAY)
    );
    for level in [SpaceLevel::Ok, SpaceLevel::Low, SpaceLevel::Critical] {
        assert_eq!(Retention::Never.effective_ms(level), None);
    }
}
