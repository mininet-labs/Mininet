use did_mini::Controller;
use mini_objects::{ObjectBuilder, ObjectError, ObjectType, Payload};
use mini_store::{MemoryBackend, Store, StoreError};

#[test]
fn stale_content_id_cannot_overwrite_blob_or_publish_indexes() {
    let device = Controller::incept_single().unwrap();
    let original = ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(10)
        .payload(Payload::Public(b"original".to_vec()))
        .sign(&device.did(), &device)
        .unwrap();
    let mut modified = original.clone();
    modified.payload = Payload::Public(b"modified".to_vec());
    modified.object_type = ObjectType::COMMENT;
    modified.timestamp_ms = 20;

    let mut store = Store::new(MemoryBackend::new());
    assert_eq!(
        store.insert(&modified),
        Err(StoreError::Object(ObjectError::IdMismatch))
    );
    assert!(!store.contains(original.id()).unwrap());
    assert!(store.all_ids().unwrap().is_empty());
    assert!(store.by_author(&device.did()).unwrap().is_empty());
    assert!(store.by_type(&ObjectType::COMMENT).unwrap().is_empty());
    assert!(store.since(0).unwrap().is_empty());

    store.insert(&original).unwrap();
    assert_eq!(
        store.insert(&modified),
        Err(StoreError::Object(ObjectError::IdMismatch))
    );
    assert_eq!(store.get(original.id()).unwrap(), original);
    assert_eq!(store.all_ids().unwrap(), vec![original.id().clone()]);
    assert!(store.by_type(&ObjectType::COMMENT).unwrap().is_empty());
    assert!(store.since(20).unwrap().is_empty());
}
