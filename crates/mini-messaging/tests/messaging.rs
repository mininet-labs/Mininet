use did_mini::{Capabilities, Controller};
use mini_crypto::{AeadKey, AeadSuite};
use mini_messaging::{scan, send, ConversationSecret, MessageDraft, MessageKind, ReceiptState};
use mini_objects::{ObjectBuilder, ObjectType, OpaqueRoute, Payload};
use mini_store::{MemoryBackend, Store};

fn identity(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

fn secret(route: u8, key: u8) -> ConversationSecret {
    ConversationSecret::established(
        OpaqueRoute::from_bytes([route; 32]),
        AeadKey::from_suite_bytes(AeadSuite::DEFAULT, &[key; 32]).unwrap(),
    )
}

#[test]
fn text_reply_attachment_and_receipt_round_trip() {
    let (alice, alice_device) = identity(10);
    let (bob, bob_device) = identity(30);
    let conversation = secret(5, 7);
    let mut store = Store::new(MemoryBackend::new());
    let attachment = ObjectBuilder::new(ObjectType::MEDIA_MANIFEST)
        .payload(Payload::Public(b"manifest".to_vec()))
        .sign(&alice.did(), &alice_device)
        .unwrap();

    let first = send(
        &mut store,
        &conversation,
        alice.did(),
        &alice_device,
        2_000,
        1,
        MessageDraft::text("hello Bob"),
    )
    .unwrap();
    let mut reply = MessageDraft::text("hello Alice");
    reply.reply_to = Some(first.clone());
    reply.attachments.push(attachment.id().clone());
    let second = send(
        &mut store,
        &conversation,
        bob.did(),
        &bob_device,
        1_000,
        1,
        reply,
    )
    .unwrap();
    send(
        &mut store,
        &conversation,
        alice.did(),
        &alice_device,
        3_000,
        2,
        MessageDraft::receipt(second.clone(), ReceiptState::Read),
    )
    .unwrap();

    let result = scan(&store, &conversation).unwrap();
    assert!(result.rejected.is_empty());
    assert_eq!(result.messages.len(), 3);
    assert_eq!(result.messages[0].body, "hello Alice");
    assert_eq!(result.messages[0].reply_to, Some(first));
    assert_eq!(
        result.messages[0].attachments,
        vec![attachment.id().clone()]
    );
    assert_eq!(result.messages[1].body, "hello Bob");
    assert_eq!(
        result.messages[2].kind,
        MessageKind::Receipt(ReceiptState::Read)
    );
    assert_eq!(result.messages[2].receipt_for, Some(second));
    result.messages[0]
        .verify_signature(&bob_device.kel())
        .unwrap();
}

#[test]
fn wrong_key_cannot_read_and_plaintext_is_absent_from_outer_bytes() {
    let (alice, alice_device) = identity(10);
    let conversation = secret(5, 7);
    let wrong = secret(5, 8);
    let mut store = Store::new(MemoryBackend::new());
    let message_id = send(
        &mut store,
        &conversation,
        alice.did(),
        &alice_device,
        1,
        1,
        MessageDraft::text("never store this in plaintext"),
    )
    .unwrap();

    let outer = store.get_private(&message_id).unwrap().to_bytes();
    assert!(!outer
        .windows(b"never store this in plaintext".len())
        .any(|window| window == b"never store this in plaintext"));
    let result = scan(&store, &wrong).unwrap();
    assert!(result.messages.is_empty());
    assert_eq!(result.rejected, vec![message_id]);
}

#[test]
fn malformed_drafts_are_rejected_before_storage() {
    let (alice, alice_device) = identity(10);
    let conversation = secret(5, 7);
    let mut store = Store::new(MemoryBackend::new());
    let invalid = MessageDraft::text("");

    assert!(send(
        &mut store,
        &conversation,
        alice.did(),
        &alice_device,
        1,
        1,
        invalid,
    )
    .is_err());
    assert!(store
        .private_by_route(&conversation.route())
        .unwrap()
        .is_empty());
}
