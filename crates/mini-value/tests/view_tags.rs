use mini_value::{
    derive_output_with_secret, derive_tagged_output, recognizes_tagged_output,
    recover_shared_secret, MininetStealthAddress, StealthAddressScheme, StealthKeypair, ViewTag,
};

#[test]
fn tagged_scanning_preserves_legacy_recognition_and_spend_key_separation() {
    let owner = StealthKeypair::generate().unwrap();
    let (output, tag) =
        derive_tagged_output(&owner.spend_public_bytes(), &owner.view_public_bytes()).unwrap();
    let scan = |hint| {
        recognizes_tagged_output(
            &owner.view_secret_bytes(),
            &owner.spend_public_bytes(),
            &output,
            hint,
        )
    };
    assert!(scan(Some(tag)));
    assert!(scan(None));
    assert!(!scan(Some(ViewTag(tag.0.wrapping_add(1)))));
    assert!(MininetStealthAddress.recognizes(
        &owner.view_secret_bytes(),
        &owner.spend_public_bytes(),
        &output,
    ));
}

#[test]
fn sender_and_receiver_agree_on_domain_separated_hint() {
    let owner = StealthKeypair::generate().unwrap();
    let (output, sender) =
        derive_output_with_secret(&owner.spend_public_bytes(), &owner.view_public_bytes()).unwrap();
    let receiver =
        recover_shared_secret(&owner.view_secret_bytes(), &output.tx_public_key).unwrap();
    assert_eq!(sender.view_tag(), receiver.view_tag());
    let expected = blake3::derive_key("MINI_VALUE_STEALTH_VIEW_TAG_V1", sender.as_key_material());
    assert_eq!(sender.view_tag(), ViewTag(expected[0]));
}

#[test]
fn matching_hint_never_substitutes_for_address_verification() {
    let owner = StealthKeypair::generate().unwrap();
    let outsider = StealthKeypair::generate().unwrap();
    let (mut output, tag) =
        derive_tagged_output(&owner.spend_public_bytes(), &owner.view_public_bytes()).unwrap();
    output.one_time_address = outsider.spend_public_bytes().to_vec();
    assert!(!recognizes_tagged_output(
        &owner.view_secret_bytes(),
        &owner.spend_public_bytes(),
        &output,
        Some(tag),
    ));
    assert!(!recognizes_tagged_output(
        &[0; 32],
        &owner.spend_public_bytes(),
        &output,
        None,
    ));
    output.tx_public_key = vec![0; 32]; // identity is canonical but not an ephemeral key
    assert!(!recognizes_tagged_output(
        &owner.view_secret_bytes(),
        &owner.spend_public_bytes(),
        &output,
        None,
    ));
    assert!(derive_tagged_output(&[0; 32], &owner.view_public_bytes()).is_none());
}
