//! Integration tests for the unified object envelope.

use did_mini::{Capabilities, Controller, Did};
use mini_objects::{
    verify_provenance, Object, ObjectBuilder, ObjectError, ObjectId, ObjectType, Payload,
    MAX_LINKS,
};

fn human_with_device(
    root_c: u8,
    caps: Capabilities,
) -> (Controller, Controller) {
    let mut root =
        Controller::incept_single_from_seeds(&[root_c; 32], &[root_c + 1; 32]).unwrap();
    let device = Controller::incept_device_single_from_seeds(
        &root.did(),
        &[root_c + 2; 32],
        &[root_c + 3; 32],
    )
    .unwrap();
    root.delegate_device(&device.did(), caps).unwrap();
    (root, device)
}

fn post(human: &Did, device: &Controller, text: &[u8], seq: u64) -> Object {
    ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(1_000)
        .sequence(seq)
        .payload(Payload::Public(text.to_vec()))
        .sign(human, device)
        .unwrap()
}

#[test]
fn object_id_is_deterministic_and_content_addressed() {
    let (root, device) = human_with_device(10, Capabilities::primary());
    let a = post(&root.did(), &device, b"hello mininet", 1);
    let b = post(&root.did(), &device, b"hello mininet", 1);
    // Same author, same content, same signature inputs -> same id (Ed25519 is
    // deterministic), and the id verifies against the bytes.
    assert_eq!(a.id(), b.id());
    let decoded = Object::from_bytes(&a.to_bytes()).unwrap();
    assert_eq!(decoded.id(), a.id());
    decoded.verify_integrity(a.id()).unwrap();

    // Different content -> different id.
    let c = post(&root.did(), &device, b"different", 1);
    assert_ne!(a.id(), c.id());
}

#[test]
fn round_trip_preserves_every_field() {
    let (root, device) = human_with_device(10, Capabilities::primary());
    let target = post(&root.did(), &device, b"root post", 1);
    let obj = ObjectBuilder::new(ObjectType::COMMENT)
        .timestamp_ms(2_000)
        .sequence(2)
        .link("re", target.id().clone())
        .payload(Payload::Public(b"a reply".to_vec()))
        .sign(&root.did(), &device)
        .unwrap();

    let back = Object::from_bytes(&obj.to_bytes()).unwrap();
    assert_eq!(back, obj);
    assert_eq!(back.links[0].rel, "re");
    assert_eq!(back.links[0].target, *target.id());
}

#[test]
fn tampered_bytes_change_the_id_and_fail_integrity() {
    let (root, device) = human_with_device(10, Capabilities::primary());
    let obj = post(&root.did(), &device, b"original", 1);
    let mut bytes = obj.to_bytes();
    // Flip one payload byte.
    let n = bytes.len();
    bytes[n - 40] ^= 0x01;
    match Object::from_bytes(&bytes) {
        // Either it no longer parses...
        Err(_) => {}
        // ...or it parses to a DIFFERENT id, so the claimed id fails integrity.
        Ok(t) => {
            assert_ne!(t.id(), obj.id());
            assert_eq!(
                t.verify_integrity(obj.id()),
                Err(ObjectError::IdMismatch)
            );
        }
    }
}

#[test]
fn signature_verifies_and_wrong_device_is_rejected() {
    let (root, device) = human_with_device(10, Capabilities::primary());
    let (_other_root, other_device) = human_with_device(50, Capabilities::primary());
    let obj = post(&root.did(), &device, b"signed", 1);

    obj.verify_signature(&device.kel()).unwrap();
    assert!(obj.verify_signature(&other_device.kel()).is_err());
}

#[test]
fn provenance_requires_real_delegation_and_capability() {
    let (root, device) = human_with_device(10, Capabilities::primary());
    let obj = post(&root.did(), &device, b"provenance", 1);

    // Full chain: device signed, device is root's unrevoked delegate with POST.
    let caps = verify_provenance(&obj, &root.kel(), &device.kel()).unwrap();
    assert!(caps.contains(Capabilities::POST));

    // A different human's KEL cannot claim the object.
    let (other_root, _od) = human_with_device(50, Capabilities::primary());
    assert!(verify_provenance(&obj, &other_root.kel(), &device.kel()).is_err());

    // Revocation kills provenance for new verifications.
    let mut root2 = root;
    root2.revoke_device(&device.did()).unwrap();
    assert!(verify_provenance(&obj, &root2.kel(), &device.kel()).is_err());
}

#[test]
fn post_capability_is_required_for_content_types() {
    // Device delegated with SIGN only: may author a RELEASE (forge type, SIGN)
    // but not a POST (content type).
    let (root, device) = human_with_device(10, Capabilities::SIGN);

    let release = ObjectBuilder::new(ObjectType::RELEASE)
        .payload(Payload::Public(b"release manifest".to_vec()))
        .sign(&root.did(), &device)
        .unwrap();
    verify_provenance(&release, &root.kel(), &device.kel()).unwrap();

    let content = post(&root.did(), &device, b"should not pass", 1);
    assert_eq!(
        verify_provenance(&content, &root.kel(), &device.kel()),
        Err(ObjectError::MissingCapability)
    );
}

#[test]
fn one_envelope_composes_across_surfaces() {
    // The SPEC-09 composability claim: a forum comment links a forge commit and
    // embeds a media manifest — one model, no per-surface formats.
    let (root, device) = human_with_device(10, Capabilities::primary());
    let commit = ObjectBuilder::new(ObjectType::COMMIT)
        .payload(Payload::Public(b"commit bytes".to_vec()))
        .sign(&root.did(), &device)
        .unwrap();
    let media = ObjectBuilder::new(ObjectType::MEDIA_MANIFEST)
        .payload(Payload::Public(b"chunk manifest".to_vec()))
        .sign(&root.did(), &device)
        .unwrap();
    let comment = ObjectBuilder::new(ObjectType::COMMENT)
        .link("embed", media.id().clone())
        .link("re", commit.id().clone())
        .payload(Payload::Public(b"review with demo video".to_vec()))
        .sign(&root.did(), &device)
        .unwrap();

    let back = Object::from_bytes(&comment.to_bytes()).unwrap();
    assert_eq!(back.links.len(), 2);
    verify_provenance(&back, &root.kel(), &device.kel()).unwrap();
}

#[test]
fn custom_types_and_encrypted_payloads_round_trip() {
    let (root, device) = human_with_device(10, Capabilities::primary());
    let obj = ObjectBuilder::new(ObjectType::Custom("chess/move".to_string()))
        .payload(Payload::Encrypted(vec![9, 9, 9]))
        .sign(&root.did(), &device)
        .unwrap();
    let back = Object::from_bytes(&obj.to_bytes()).unwrap();
    assert_eq!(back.object_type, ObjectType::Custom("chess/move".to_string()));
    assert!(matches!(back.payload, Payload::Encrypted(ref b) if b == &vec![9, 9, 9]));
    // Encryption hides content; the signature still proves authorship.
    back.verify_signature(&device.kel()).unwrap();
}

#[test]
fn decode_limits_reject_hostile_objects() {
    let (root, device) = human_with_device(10, Capabilities::primary());
    // Builder refuses oversized link sets before signing.
    let target = post(&root.did(), &device, b"t", 1);
    let mut b = ObjectBuilder::new(ObjectType::POST);
    for _ in 0..(MAX_LINKS + 1) {
        b = b.link("re", target.id().clone());
    }
    assert_eq!(
        b.payload(Payload::Public(vec![1]))
            .sign(&root.did(), &device)
            .map(|_| ()),
        Err(ObjectError::LimitExceeded)
    );

    // A malformed id string inside a link is rejected on decode.
    assert!(ObjectId::parse("not-a-valid-id").is_err());
}
