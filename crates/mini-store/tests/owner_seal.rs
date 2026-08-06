//! Adversarial coverage for the `owner_seal` sealed-box primitive (D-0434).
//! See `docs/design/cold-storage-and-owner-only-encryption.md`.

use mini_objects::MAX_PAYLOAD_BYTES;
use mini_store::{open_as_owner, seal_for_owner, OwnerSealingKey, MAX_OWNER_SEAL_PLAINTEXT_BYTES};

#[test]
fn round_trip_recovers_the_exact_plaintext() {
    let owner = OwnerSealingKey::generate().unwrap();
    let plaintext = b"the whole of the moon".to_vec();
    let aad = b"object-id-binds-here";

    let sealed = seal_for_owner(&owner.public_key(), &plaintext, aad).unwrap();
    let opened = open_as_owner(&owner, &sealed, aad).unwrap();

    assert_eq!(opened, plaintext);
}

#[test]
fn wrong_owner_key_fails_to_open() {
    let owner = OwnerSealingKey::generate().unwrap();
    let attacker = OwnerSealingKey::generate().unwrap();
    let sealed = seal_for_owner(&owner.public_key(), b"secret", b"aad").unwrap();

    assert!(open_as_owner(&attacker, &sealed, b"aad").is_err());
}

#[test]
fn tampered_ciphertext_fails_closed() {
    let owner = OwnerSealingKey::generate().unwrap();
    let mut sealed = seal_for_owner(&owner.public_key(), b"secret", b"aad").unwrap();
    let last = sealed.len() - 1;
    sealed[last] ^= 0xff;

    assert!(open_as_owner(&owner, &sealed, b"aad").is_err());
}

#[test]
fn tampered_aad_fails_closed() {
    let owner = OwnerSealingKey::generate().unwrap();
    let sealed = seal_for_owner(&owner.public_key(), b"secret", b"aad").unwrap();

    assert!(open_as_owner(&owner, &sealed, b"different-aad").is_err());
}

#[test]
fn tampered_ephemeral_public_key_fails_closed() {
    let owner = OwnerSealingKey::generate().unwrap();
    let mut sealed = seal_for_owner(&owner.public_key(), b"secret", b"aad").unwrap();
    // Flip a byte inside the leading 32-byte ephemeral public key.
    sealed[0] ^= 0xff;

    assert!(open_as_owner(&owner, &sealed, b"aad").is_err());
}

#[test]
fn truncated_input_is_rejected() {
    let owner = OwnerSealingKey::generate().unwrap();
    let sealed = seal_for_owner(&owner.public_key(), b"secret", b"aad").unwrap();

    for len in [0, 1, 32, 44, sealed.len() - 1] {
        assert!(open_as_owner(&owner, &sealed[..len], b"aad").is_err());
    }
}

#[test]
fn oversized_input_is_rejected_before_any_allocation() {
    let owner = OwnerSealingKey::generate().unwrap();
    // Far larger than any legal sealed-box length; must fail fast, not by
    // attempting to allocate/process the whole thing.
    let bogus = vec![0u8; 16 * 1024 * 1024];

    assert!(open_as_owner(&owner, &bogus, b"aad").is_err());
}

#[test]
fn oversized_plaintext_is_rejected_before_sealing() {
    let owner = OwnerSealingKey::generate().unwrap();
    let huge = vec![0u8; MAX_OWNER_SEAL_PLAINTEXT_BYTES + 1];

    assert!(seal_for_owner(&owner.public_key(), &huge, b"aad").is_err());
}

#[test]
fn largest_accepted_plaintext_still_fits_the_object_payload_ceiling() {
    let owner = OwnerSealingKey::generate().unwrap();
    let plaintext = vec![0u8; MAX_OWNER_SEAL_PLAINTEXT_BYTES];

    let sealed = seal_for_owner(&owner.public_key(), &plaintext, b"aad").unwrap();
    assert_eq!(sealed.len(), MAX_PAYLOAD_BYTES);
}

#[test]
fn two_seals_of_the_same_plaintext_produce_different_ciphertext() {
    let owner = OwnerSealingKey::generate().unwrap();
    let plaintext = b"repeated content".to_vec();

    let sealed1 = seal_for_owner(&owner.public_key(), &plaintext, b"aad").unwrap();
    let sealed2 = seal_for_owner(&owner.public_key(), &plaintext, b"aad").unwrap();

    assert_ne!(sealed1, sealed2);
    // But both still open to the same plaintext.
    assert_eq!(open_as_owner(&owner, &sealed1, b"aad").unwrap(), plaintext);
    assert_eq!(open_as_owner(&owner, &sealed2, b"aad").unwrap(), plaintext);
}

#[test]
fn empty_plaintext_round_trips() {
    let owner = OwnerSealingKey::generate().unwrap();
    let sealed = seal_for_owner(&owner.public_key(), b"", b"aad").unwrap();
    let opened = open_as_owner(&owner, &sealed, b"aad").unwrap();
    assert!(opened.is_empty());
}

#[test]
fn from_seed_is_deterministic_and_public_key_matches() {
    let seed = [42u8; 32];
    let a = OwnerSealingKey::from_seed(&seed);
    let b = OwnerSealingKey::from_seed(&seed);
    assert_eq!(a.public_key(), b.public_key());
}
