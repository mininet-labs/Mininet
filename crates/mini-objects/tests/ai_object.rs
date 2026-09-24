//! Tests for the AI-disclosure envelope (roadmap #63, Phase 8.5): an
//! AI-generated/AI-mediated object must never be mistaken for, or
//! substitutable into, the human-authored/human-governance object path.

use did_mini::{Capabilities, Controller, Did};
use mini_objects::{
    verify_ai_provenance, verify_provenance, AiObject, AiObjectBuilder, AiOrigin, AiProvenance,
    Object, ObjectBuilder, ObjectError, ObjectType, Payload,
};

fn human_with_device(root_c: u8, caps: Capabilities) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[root_c; 32], &[root_c + 1; 32]).unwrap();
    let device = Controller::incept_device_single_from_seeds(
        &root.did(),
        &[root_c + 2; 32],
        &[root_c + 3; 32],
    )
    .unwrap();
    root.delegate_device(&device.did(), caps).unwrap();
    (root, device)
}

fn provenance() -> AiProvenance {
    AiProvenance {
        system_id: "mini-forge-review-assistant/0.1.0".to_string(),
        model_id: "test:fixture-model".to_string(),
        produced_at_ms: 42,
    }
}

fn ai_object(human: &Did, device: &Controller, origin: AiOrigin) -> AiObject {
    AiObjectBuilder::new(origin, provenance())
        .payload(Payload::Public(b"a generated summary".to_vec()))
        .sign(human, device)
        .unwrap()
}

// --- (a) structurally distinguishable at the type level ---------------

#[test]
fn ai_object_round_trips_and_carries_a_render_ready_disclosure_label() {
    let (root, device) = human_with_device(20, Capabilities::AI_DISCLOSE);
    let obj = ai_object(&root.did(), &device, AiOrigin::GeneratedContent);
    let back = AiObject::from_bytes(&obj.to_bytes()).unwrap();
    assert_eq!(back, obj);
    assert_eq!(
        obj.origin.disclosure_label(),
        "AI-generated content — not human-authored"
    );
}

#[test]
fn render_disclosure_is_non_empty_and_names_the_producing_system() {
    let (root, device) = human_with_device(29, Capabilities::AI_DISCLOSE);
    let obj = ai_object(&root.did(), &device, AiOrigin::GeneratedContent);
    let rendered = obj.render_disclosure();
    assert!(!rendered.is_empty());
    assert!(rendered.contains("AI-generated content"));
    assert!(rendered.contains("mini-forge-review-assistant/0.1.0"));
}

#[test]
fn ai_envelope_bytes_are_rejected_by_the_human_object_decoder_and_vice_versa() {
    let (root, device) = human_with_device(21, Capabilities::AI_DISCLOSE);
    let ai_obj = ai_object(&root.did(), &device, AiOrigin::MediatedDecision);

    // AI bytes fed to the human-object decoder must not decode into a
    // plausible-looking human Object -- they must fail outright.
    let err = Object::from_bytes(&ai_obj.to_bytes()).unwrap_err();
    assert!(matches!(
        err,
        ObjectError::BadObject | ObjectError::Truncated | ObjectError::LimitExceeded
    ));

    // And the reverse: a genuine human post's bytes must not decode as an
    // AI-disclosure object.
    let (human_root, human_device) = human_with_device(22, Capabilities::primary());
    let human_post = ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(1_000)
        .sequence(1)
        .payload(Payload::Public(b"a real human post".to_vec()))
        .sign(&human_root.did(), &human_device)
        .unwrap();
    let err = AiObject::from_bytes(&human_post.to_bytes()).unwrap_err();
    assert_eq!(err, ObjectError::WrongEnvelopeKind);
}

// The compile-time proof that an `AiObject` cannot be passed anywhere an
// `Object` (the human-authored/human-governance envelope) is required lives
// as a `compile_fail` doctest on `mini_objects::ai_object`'s module docs
// (rustdoc only collects doctests from the library target, not from this
// integration-test binary) -- run with `cargo test -p mini-objects --doc`.

// --- (b) cannot be signed/attested via the human governance/attestation path

#[test]
fn a_device_holding_only_human_capabilities_cannot_author_an_ai_object() {
    // A device delegated the full "primary" human bundle (SIGN/PAY/POST/
    // ATTEST/VOTE) but never AI_DISCLOSE must not be able to produce an
    // AI-disclosure object that verifies -- POST (content) and VOTE/ATTEST
    // (governance/co-presence) must not silently double as AI disclosure
    // authority.
    let (root, device) = human_with_device(23, Capabilities::primary());
    let obj = ai_object(&root.did(), &device, AiOrigin::GeneratedContent);
    let root_kel = root.kel();
    let device_kel = device.kel();
    let err = verify_ai_provenance(&obj, &root_kel, &device_kel).unwrap_err();
    assert_eq!(err, ObjectError::MissingCapability);
}

#[test]
fn a_device_holding_only_ai_disclose_cannot_author_human_content_or_vote() {
    // Symmetric direction: AI_DISCLOSE must not imply POST or VOTE either.
    let (root, device) = human_with_device(24, Capabilities::AI_DISCLOSE);
    let human_post = ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(1_000)
        .sequence(1)
        .payload(Payload::Public(b"attempted human post".to_vec()))
        .sign(&root.did(), &device)
        .unwrap();
    let root_kel = root.kel();
    let device_kel = device.kel();
    let err = verify_provenance(&human_post, &root_kel, &device_kel).unwrap_err();
    assert_eq!(err, ObjectError::MissingCapability);
}

#[test]
fn ai_disclose_capability_authors_a_verifiable_ai_object() {
    let (root, device) = human_with_device(25, Capabilities::AI_DISCLOSE);
    let obj = ai_object(&root.did(), &device, AiOrigin::MediatedDecision);
    let root_kel = root.kel();
    let device_kel = device.kel();
    let caps = verify_ai_provenance(&obj, &root_kel, &device_kel).unwrap();
    assert!(caps.contains(Capabilities::AI_DISCLOSE));
}

// --- (c) mandatory provenance metadata ---------------------------------

#[test]
fn empty_system_id_is_rejected_before_signing() {
    let (root, device) = human_with_device(26, Capabilities::AI_DISCLOSE);
    let bad = AiProvenance {
        system_id: String::new(),
        model_id: "test:fixture-model".to_string(),
        produced_at_ms: 1,
    };
    let err = AiObjectBuilder::new(AiOrigin::GeneratedContent, bad)
        .payload(Payload::Public(b"x".to_vec()))
        .sign(&root.did(), &device)
        .unwrap_err();
    assert_eq!(err, ObjectError::MissingAiProvenance);
}

#[test]
fn empty_model_id_is_rejected_before_signing() {
    let (root, device) = human_with_device(27, Capabilities::AI_DISCLOSE);
    let bad = AiProvenance {
        system_id: "mini-forge-review-assistant/0.1.0".to_string(),
        model_id: String::new(),
        produced_at_ms: 1,
    };
    let err = AiObjectBuilder::new(AiOrigin::GeneratedContent, bad)
        .payload(Payload::Public(b"x".to_vec()))
        .sign(&root.did(), &device)
        .unwrap_err();
    assert_eq!(err, ObjectError::MissingAiProvenance);
}

#[test]
fn decoded_ai_object_preserves_mandatory_provenance_fields() {
    let (root, device) = human_with_device(28, Capabilities::AI_DISCLOSE);
    let obj = ai_object(&root.did(), &device, AiOrigin::GeneratedContent);
    let back = AiObject::from_bytes(&obj.to_bytes()).unwrap();
    assert_eq!(back.provenance, obj.provenance);
    assert_eq!(
        back.provenance.system_id,
        "mini-forge-review-assistant/0.1.0"
    );
    assert_eq!(back.provenance.model_id, "test:fixture-model");
}
