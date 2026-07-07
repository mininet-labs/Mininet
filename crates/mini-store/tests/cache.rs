//! Cache-tier / seed-on-view tests (founder decision, 2026-07-07): watching
//! content can help seed it, but never against the user's wishes, never on a
//! metered/low-battery connection, and never for encrypted content.

use did_mini::{AvailabilityWindow, BaseDeviceRole, BatteryPolicy, Capabilities, Controller, Did};
use mini_objects::{Object, ObjectBuilder, ObjectType, Payload};
use mini_store::{CacheTier, MemoryBackend, Store, ViewConditions};

fn human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

fn public_post(human: &Did, device: &Controller, seq: u64) -> Object {
    ObjectBuilder::new(ObjectType::POST)
        .sequence(seq)
        .payload(Payload::Public(b"hello".to_vec()))
        .sign(human, device)
        .unwrap()
}

fn encrypted_post(human: &Did, device: &Controller, seq: u64) -> Object {
    ObjectBuilder::new(ObjectType::POST)
        .sequence(seq)
        .payload(Payload::Encrypted(b"ciphertext".to_vec()))
        .sign(human, device)
        .unwrap()
}

fn permissive_conditions() -> ViewConditions {
    ViewConditions {
        battery_percent: 100,
        on_battery: false,
        minute_of_day: 600,
        metered_connection: false,
        storage_budget_remaining: true,
    }
}

#[test]
fn unrecorded_objects_default_to_ephemeral_and_never_advertise() {
    let (h, d) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let post = public_post(&h.did(), &d, 1);
    store.insert(&post).unwrap();

    let tier = store.cache_tier(post.id()).unwrap();
    assert_eq!(tier, CacheTier::EphemeralCache);
    assert!(!tier.advertises());
}

#[test]
fn opening_public_content_promotes_to_seed_cache_when_policy_allows() {
    let (h, d) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let post = public_post(&h.did(), &d, 1);
    store.insert(&post).unwrap();

    let role = BaseDeviceRole::always_on_default();
    let tier = store
        .note_view(post.id(), &role, permissive_conditions())
        .unwrap();
    assert_eq!(tier, CacheTier::SeedCache);
    assert!(tier.advertises());
    assert_eq!(store.cache_tier(post.id()).unwrap(), CacheTier::SeedCache);
}

#[test]
fn opening_public_content_does_not_mutate_identity_or_object_bytes() {
    let (h, d) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let post = public_post(&h.did(), &d, 1);
    store.insert(&post).unwrap();

    let before = store.get(post.id()).unwrap();
    let role = BaseDeviceRole::always_on_default();
    // `note_view` takes no viewer identity at all — there is nothing for it
    // to mutate on the identity side, by construction.
    store
        .note_view(post.id(), &role, permissive_conditions())
        .unwrap();
    let after = store.get(post.id()).unwrap();

    assert_eq!(before, after);
    assert_eq!(after.author_human.as_str(), h.did().as_str());
    assert_eq!(after.author_device.as_str(), d.did().as_str());
}

#[test]
fn opening_encrypted_content_never_leaks_availability() {
    let (h, d) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let secret = encrypted_post(&h.did(), &d, 1);
    store.insert(&secret).unwrap();

    // Every policy knob is maximally permissive; the content is still never
    // promoted past PrivateOnly.
    let role = BaseDeviceRole::always_on_default();
    let tier = store
        .note_view(secret.id(), &role, permissive_conditions())
        .unwrap();
    assert_eq!(tier, CacheTier::PrivateOnly);
    assert!(!tier.advertises());

    // Viewing it again does not change that.
    let tier_again = store
        .note_view(secret.id(), &role, permissive_conditions())
        .unwrap();
    assert_eq!(tier_again, CacheTier::PrivateOnly);
}

#[test]
fn user_can_disable_seed_on_view() {
    let (h, d) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let post = public_post(&h.did(), &d, 1);
    store.insert(&post).unwrap();

    let mut role = BaseDeviceRole::always_on_default();
    role.seed_on_view_enabled = false;

    let tier = store
        .note_view(post.id(), &role, permissive_conditions())
        .unwrap();
    assert_eq!(tier, CacheTier::EphemeralCache);
    assert!(!tier.advertises());
}

#[test]
fn metered_connection_prevents_background_seeding() {
    let (h, d) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let post = public_post(&h.did(), &d, 1);
    store.insert(&post).unwrap();

    let role = BaseDeviceRole::always_on_default();
    let mut conditions = permissive_conditions();
    conditions.metered_connection = true;

    let tier = store.note_view(post.id(), &role, conditions).unwrap();
    assert_eq!(tier, CacheTier::EphemeralCache);
}

#[test]
fn low_battery_prevents_background_seeding() {
    let (h, d) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let post = public_post(&h.did(), &d, 1);
    store.insert(&post).unwrap();

    let mut role = BaseDeviceRole::battery_aware_default();
    role.battery_policy = BatteryPolicy::PauseBelowPercent(30);
    let mut conditions = permissive_conditions();
    conditions.on_battery = true;
    conditions.battery_percent = 5;

    let tier = store.note_view(post.id(), &role, conditions).unwrap();
    assert_eq!(tier, CacheTier::EphemeralCache);
}

#[test]
fn availability_window_prevents_background_seeding_outside_hours() {
    let (h, d) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let post = public_post(&h.did(), &d, 1);
    store.insert(&post).unwrap();

    let mut role = BaseDeviceRole::always_on_default();
    role.availability_window = AvailabilityWindow {
        start_minute: 480,
        end_minute: 1320,
    };
    let mut conditions = permissive_conditions();
    conditions.minute_of_day = 60; // 01:00, outside the window

    let tier = store.note_view(post.id(), &role, conditions).unwrap();
    assert_eq!(tier, CacheTier::EphemeralCache);
}

#[test]
fn storage_budget_exhaustion_prevents_background_seeding() {
    let (h, d) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let post = public_post(&h.did(), &d, 1);
    store.insert(&post).unwrap();

    let role = BaseDeviceRole::always_on_default();
    let mut conditions = permissive_conditions();
    conditions.storage_budget_remaining = false;

    let tier = store.note_view(post.id(), &role, conditions).unwrap();
    assert_eq!(tier, CacheTier::EphemeralCache);
}

#[test]
fn pinned_and_committed_tiers_are_never_downgraded_by_a_view() {
    let (h, d) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let post = public_post(&h.did(), &d, 1);
    store.insert(&post).unwrap();
    store
        .set_cache_tier(post.id(), CacheTier::PinnedByOwner)
        .unwrap();

    // Even with a hostile policy (seeding disabled), an explicit pin holds.
    let mut role = BaseDeviceRole::always_on_default();
    role.seed_on_view_enabled = false;
    let tier = store
        .note_view(post.id(), &role, permissive_conditions())
        .unwrap();
    assert_eq!(tier, CacheTier::PinnedByOwner);

    store
        .set_cache_tier(post.id(), CacheTier::CommittedStorage)
        .unwrap();
    let tier = store
        .note_view(post.id(), &role, permissive_conditions())
        .unwrap();
    assert_eq!(tier, CacheTier::CommittedStorage);
}
