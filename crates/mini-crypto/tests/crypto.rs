//! Integration tests for `mini-crypto`.
//!
//! These run fully deterministically (fixed seeds), so they are reproducible
//! across machines and double as executable documentation of the frozen
//! invariants this crate enforces.

use mini_crypto::encoding::{self, BASE16, BASE58BTC};
use mini_crypto::hash::{HashAlgorithm, FORBIDDEN_SHA1_CODE};
use mini_crypto::{CryptoError, Multihash, Signature, SignatureSuite, SigningKey, VerifyingKey};

// Fixed seeds keep the whole suite deterministic and reproducible.
const SEED_A: [u8; 32] = [7u8; 32];
const SEED_B: [u8; 32] = [9u8; 32];

// ---- signing / verifying ----

#[test]
fn sign_and_verify_roundtrip() {
    let sk = SigningKey::from_seed(&SEED_A);
    let vk = sk.verifying_key();
    let msg = b"two strangers meet and verify each other in person";
    let sig = sk.sign(msg);
    assert!(vk.verify(msg, &sig).is_ok());
}

#[test]
fn verify_rejects_tampered_message() {
    let sk = SigningKey::from_seed(&SEED_A);
    let vk = sk.verifying_key();
    let sig = sk.sign(b"original");
    assert_eq!(vk.verify(b"tampered", &sig), Err(CryptoError::BadSignature));
}

#[test]
fn verify_rejects_other_keys_signature() {
    let sk_a = SigningKey::from_seed(&SEED_A);
    let sk_b = SigningKey::from_seed(&SEED_B);
    let msg = b"presence attestation";
    let sig_b = sk_b.sign(msg);
    assert_eq!(
        sk_a.verifying_key().verify(msg, &sig_b),
        Err(CryptoError::BadSignature)
    );
}

#[test]
fn deterministic_key_derivation() {
    // Same seed must yield the same public key on every platform.
    let vk1 = SigningKey::from_seed(&SEED_A).verifying_key();
    let vk2 = SigningKey::from_seed(&SEED_A).verifying_key();
    assert_eq!(vk1.to_bytes(), vk2.to_bytes());
}

// ---- crypto-agility (the suite tag travels with the data) ----

#[test]
fn keys_and_signatures_carry_their_suite() {
    let sk = SigningKey::from_seed(&SEED_A);
    assert_eq!(sk.suite(), SignatureSuite::Ed25519);
    assert_eq!(sk.verifying_key().suite(), SignatureSuite::Ed25519);
    assert_eq!(sk.sign(b"x").suite(), SignatureSuite::Ed25519);
    assert_eq!(SignatureSuite::DEFAULT, SignatureSuite::Ed25519);
}

#[test]
fn suite_tag_roundtrip() {
    let suite = SignatureSuite::Ed25519;
    assert_eq!(SignatureSuite::from_tag(suite.tag()).unwrap(), suite);
    assert_eq!(
        SignatureSuite::from_tag(0xff),
        Err(CryptoError::UnknownSuite(0xff))
    );
}

#[test]
fn verifying_key_byte_roundtrip() {
    let vk = SigningKey::from_seed(&SEED_A).verifying_key();
    let bytes = vk.to_bytes();
    let vk2 = VerifyingKey::from_suite_bytes(SignatureSuite::Ed25519, &bytes).unwrap();
    assert_eq!(vk, vk2);
}

#[test]
fn signature_byte_roundtrip() {
    let sk = SigningKey::from_seed(&SEED_A);
    let sig = sk.sign(b"msg");
    let sig2 = Signature::from_suite_bytes(SignatureSuite::Ed25519, &sig.to_bytes()).unwrap();
    assert_eq!(sig, sig2);
    assert!(sk.verifying_key().verify(b"msg", &sig2).is_ok());
}

#[test]
fn bad_length_public_key_is_rejected() {
    assert_eq!(
        VerifyingKey::from_suite_bytes(SignatureSuite::Ed25519, &[0u8; 8]),
        Err(CryptoError::BadLength {
            expected: 32,
            got: 8
        })
    );
}

// ---- secret hygiene (SPEC-01 G1) ----

#[test]
fn signing_key_debug_does_not_leak_secret() {
    let sk = SigningKey::from_seed(&SEED_A);
    let dbg = format!("{sk:?}");
    assert!(dbg.contains("redacted"));
    let seed_hex: String = SEED_A.iter().map(|b| format!("{b:02x}")).collect();
    assert!(!dbg.contains(&seed_hex));
}

// ---- hashing / multihash (strong hash, never SHA-1) ----

#[test]
fn blake3_is_default_hash() {
    assert_eq!(mini_crypto::DEFAULT_HASH, HashAlgorithm::Blake3);
}

#[test]
fn multihash_roundtrip_blake3() {
    let mh = Multihash::of(HashAlgorithm::Blake3, b"hello mininet");
    let decoded = Multihash::from_bytes(&mh.to_bytes()).unwrap();
    assert_eq!(decoded, mh);
    assert_eq!(decoded.algorithm(), HashAlgorithm::Blake3);
    assert_eq!(decoded.digest().len(), 32);
}

#[test]
fn multihash_roundtrip_sha256() {
    let mh = Multihash::of(HashAlgorithm::Sha256, b"git interop");
    let decoded = Multihash::from_bytes(&mh.to_bytes()).unwrap();
    assert_eq!(decoded, mh);
}

#[test]
fn multihash_rejects_sha1_code() {
    // Forge a multihash claiming SHA-1 (code 0x11, length 20) and ensure the
    // decoder refuses it. This is the structural form of the frozen no-SHA-1 rule.
    let mut forged = Vec::new();
    forged.push(FORBIDDEN_SHA1_CODE as u8); // 0x11 — single varint byte
    forged.push(20u8); // claimed digest length
    forged.extend_from_slice(&[0u8; 20]);
    assert_eq!(
        Multihash::from_bytes(&forged),
        Err(CryptoError::UnknownOrForbiddenHashCode(FORBIDDEN_SHA1_CODE))
    );
}


#[test]
fn multihash_rejects_short_digest_even_when_length_field_matches() {
    // The length prefix is not enough: a supported strong hash must have its
    // canonical digest length. A forged BLAKE3 multihash with a 4-byte digest is
    // not a valid content address, even if the internal length field is honest.
    let mut forged = Vec::new();
    forged.push(HashAlgorithm::Blake3.multihash_code() as u8);
    forged.push(4u8);
    forged.extend_from_slice(&[0u8; 4]);
    assert_eq!(
        Multihash::from_bytes(&forged),
        Err(CryptoError::BadLength {
            expected: 32,
            got: 4
        })
    );
}


#[test]
fn multihash_rejects_non_canonical_varint_encoding() {
    // BLAKE3 code 0x1e encoded as an overlong varint: 0x9e 0x00.
    // It decodes numerically but is not the canonical multihash byte form.
    let digest = [0u8; 32];
    let mut forged = Vec::new();
    forged.extend_from_slice(&[0x9e, 0x00]);
    forged.push(32u8);
    forged.extend_from_slice(&digest);
    assert_eq!(Multihash::from_bytes(&forged), Err(CryptoError::BadEncoding));
}

#[test]
fn multihash_rejects_unknown_code() {
    let mut forged = Vec::new();
    forged.push(0x55u8); // some unknown single-byte code
    forged.push(4u8);
    forged.extend_from_slice(&[1, 2, 3, 4]);
    assert_eq!(
        Multihash::from_bytes(&forged),
        Err(CryptoError::UnknownOrForbiddenHashCode(0x55))
    );
}

#[test]
fn multihash_rejects_length_mismatch() {
    let mut forged = Vec::new();
    forged.push(HashAlgorithm::Blake3.multihash_code() as u8);
    forged.push(32u8); // claims 32 bytes...
    forged.extend_from_slice(&[0u8; 4]); // ...but only 4 present
    assert_eq!(
        Multihash::from_bytes(&forged),
        Err(CryptoError::BadLength {
            expected: 32,
            got: 4
        })
    );
}

// ---- multibase ----

#[test]
fn multibase_base58_roundtrip() {
    let data = b"\x00\x01\x02\xff\xfe identifier bytes";
    let s = encoding::encode(BASE58BTC, data).unwrap();
    assert!(s.starts_with('z'));
    assert_eq!(encoding::decode(&s).unwrap(), data);
}

#[test]
fn multibase_hex_roundtrip() {
    let data = b"\xde\xad\xbe\xef";
    let s = encoding::encode(BASE16, data).unwrap();
    assert_eq!(s, "fdeadbeef");
    assert_eq!(encoding::decode(&s).unwrap(), data);
}

#[test]
fn multibase_rejects_unknown_prefix() {
    assert_eq!(
        encoding::decode("Qsomething"),
        Err(CryptoError::UnsupportedMultibase('Q'))
    );
}

// ---- key agreement / AEAD / HKDF (bearer-session primitives) ----

#[test]
fn x25519_agreement_is_symmetric_and_suite_tagged() {
    let alice = mini_crypto::AgreementSecretKey::from_seed(&[11u8; 32]);
    let bob = mini_crypto::AgreementSecretKey::from_seed(&[12u8; 32]);

    let alice_public = alice.public_key();
    let bob_public = bob.public_key();
    assert_eq!(alice_public.suite(), mini_crypto::KeyAgreementSuite::X25519);
    assert_eq!(bob_public.suite(), mini_crypto::KeyAgreementSuite::X25519);

    let s1 = alice.agree(&bob_public).unwrap();
    let s2 = bob.agree(&alice_public).unwrap();
    assert_eq!(s1, s2);
    assert_ne!(s1.as_bytes(), &[0u8; 32]);
}

#[test]
fn x25519_rejects_all_zero_shared_secret() {
    let alice = mini_crypto::AgreementSecretKey::from_seed(&[13u8; 32]);
    let malicious = mini_crypto::AgreementPublicKey::from_suite_bytes(
        mini_crypto::KeyAgreementSuite::X25519,
        &[0u8; 32],
    )
    .unwrap();

    assert_eq!(alice.agree(&malicious), Err(CryptoError::InvalidPublicKey));
}

#[test]
fn key_agreement_public_key_roundtrip() {
    let alice = mini_crypto::AgreementSecretKey::from_seed(&[14u8; 32]);
    let public = alice.public_key();
    let decoded = mini_crypto::AgreementPublicKey::from_suite_bytes(
        mini_crypto::KeyAgreementSuite::X25519,
        &public.to_bytes(),
    )
    .unwrap();
    assert_eq!(decoded, public);
    assert_eq!(
        mini_crypto::KeyAgreementSuite::from_tag(0xff),
        Err(CryptoError::UnknownKeyAgreementSuite(0xff))
    );
}

#[test]
fn hkdf_derives_deterministic_aead_key_from_shared_secret() {
    let alice = mini_crypto::AgreementSecretKey::from_seed(&[15u8; 32]);
    let bob = mini_crypto::AgreementSecretKey::from_seed(&[16u8; 32]);
    let s1 = alice.agree(&bob.public_key()).unwrap();
    let s2 = bob.agree(&alice.public_key()).unwrap();

    let salt = b"MINI/BT0 salt";
    let info = b"MINI/BT0 handshake traffic key";
    let k1 = mini_crypto::KdfSuite::HkdfSha256
        .derive_aead_key_from_shared(
            Some(salt),
            &s1,
            info,
            mini_crypto::AeadSuite::ChaCha20Poly1305,
        )
        .unwrap();
    let k2 = mini_crypto::KdfSuite::HkdfSha256
        .derive_aead_key_from_shared(
            Some(salt),
            &s2,
            info,
            mini_crypto::AeadSuite::ChaCha20Poly1305,
        )
        .unwrap();
    assert_eq!(k1, k2);
    assert_eq!(k1.suite(), mini_crypto::AeadSuite::ChaCha20Poly1305);
}

#[test]
fn chacha20poly1305_aead_roundtrip_and_authentication() {
    let key = mini_crypto::AeadKey::from_suite_bytes(
        mini_crypto::AeadSuite::ChaCha20Poly1305,
        &[17u8; 32],
    )
    .unwrap();
    let nonce = mini_crypto::AeadNonce::from_bytes(&[18u8; 12]).unwrap();
    let aad = b"MINI/BT0 frame header";
    let plaintext = b"identity KEL chunk";

    let ciphertext = key.encrypt(&nonce, plaintext, aad).unwrap();
    assert_ne!(ciphertext, plaintext);
    assert_eq!(key.decrypt(&nonce, &ciphertext, aad).unwrap(), plaintext.to_vec());
    assert_eq!(
        key.decrypt(&nonce, &ciphertext, b"wrong aad"),
        Err(CryptoError::Aead)
    );
}


#[test]
fn hkdf_rejects_oversized_output_before_allocating() {
    assert_eq!(
        mini_crypto::KdfSuite::HkdfSha256.derive_bytes(
            Some(b"salt"),
            b"input",
            b"info",
            255 * 32 + 1,
        ),
        Err(CryptoError::BadLength {
            expected: 255 * 32,
            got: 255 * 32 + 1,
        })
    );
}

#[test]
fn aead_and_kdf_suite_tags_reject_unknown_values() {
    assert_eq!(
        mini_crypto::AeadSuite::from_tag(0xfe),
        Err(CryptoError::UnknownAeadSuite(0xfe))
    );
    assert_eq!(
        mini_crypto::KdfSuite::from_tag(0xfd),
        Err(CryptoError::UnknownKdfSuite(0xfd))
    );
}

#[test]
fn secret_debug_impls_redact_key_material() {
    let dh = mini_crypto::AgreementSecretKey::from_seed(&[19u8; 32]);
    let shared = dh.agree(&dh.public_key()).unwrap();
    let aead = mini_crypto::AeadKey::from_suite_bytes(
        mini_crypto::AeadSuite::ChaCha20Poly1305,
        &[20u8; 32],
    )
    .unwrap();

    assert!(format!("{dh:?}").contains("redacted"));
    assert!(format!("{shared:?}").contains("redacted"));
    assert!(format!("{aead:?}").contains("redacted"));
}
