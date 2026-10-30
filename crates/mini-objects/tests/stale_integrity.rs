use did_mini::Controller;
use mini_objects::{Object, ObjectBuilder, ObjectError, ObjectType, Payload};

#[test]
fn mutated_fields_cannot_keep_the_signed_content_id() {
    let device = Controller::incept_single().unwrap();
    let original = ObjectBuilder::new(ObjectType::POST)
        .payload(Payload::Public(b"original".to_vec()))
        .sign(&device.did(), &device)
        .unwrap();
    let claimed = original.id().clone();
    original.verify_integrity(&claimed).unwrap();

    let mut payload = original.clone();
    payload.payload = Payload::Public(b"modified".to_vec());
    let mut timestamp = original.clone();
    timestamp.timestamp_ms += 1;
    let mut object_type = original.clone();
    object_type.object_type = ObjectType::COMMENT;
    for modified in [payload, timestamp, object_type] {
        assert_eq!(
            modified.verify_integrity(&claimed),
            Err(ObjectError::IdMismatch)
        );
        let decoded = Object::from_bytes(&modified.to_bytes()).unwrap();
        assert_ne!(decoded.id(), &claimed);
        assert_eq!(
            modified.verify_integrity(decoded.id()),
            Err(ObjectError::IdMismatch)
        );
        decoded.verify_integrity(decoded.id()).unwrap();
        assert!(decoded.verify_signature(&device.kel()).is_err());
    }
}
