//! Integration tests for the bearer transport and encrypted channel.
//!
//! Deterministic and offline. They establish an anonymous channel, exchange
//! authenticated traffic both ways, and confirm the rejection paths a hostile
//! Bluetooth peer would probe.

use mini_bearer::{
    encode_frame, pair, Bearer, BearerError, Channel, FrameReader, Initiator,
    Responder, MAX_CHANNEL_CIPHERTEXT_BYTES, MAX_CHANNEL_PLAINTEXT_BYTES,
    MAX_FRAME_BYTES, MAX_STREAM_BUFFER_BYTES, PROTOCOL_VERSION,
};

/// Drive a full handshake in-process and return both established channels.
fn establish() -> (Channel, Channel) {
    let (initiator, hello1) = Initiator::start().unwrap();
    let (responder_channel, hello2) = Responder::respond(&hello1).unwrap();
    let initiator_channel = initiator.finish(&hello2).unwrap();
    (initiator_channel, responder_channel)
}

#[test]
fn handshake_agrees_on_binding_and_keys() {
    let (mut a, mut b) = establish();
    // Both ends derive the same channel binding.
    assert_eq!(a.channel_binding(), b.channel_binding());

    // Initiator -> responder.
    let ct = a.seal(b"identity KEL chunk", b"hdr").unwrap();
    assert_ne!(ct, b"identity KEL chunk".to_vec());
    assert_eq!(b.open(&ct, b"hdr").unwrap(), b"identity KEL chunk".to_vec());

    // Responder -> initiator (independent direction/key).
    let ct2 = b.seal(b"ack", b"").unwrap();
    assert_eq!(a.open(&ct2, b"").unwrap(), b"ack".to_vec());
}

#[test]
fn messages_decrypt_in_order() {
    let (mut a, mut b) = establish();
    let c0 = a.seal(b"m0", b"").unwrap();
    let c1 = a.seal(b"m1", b"").unwrap();
    let c2 = a.seal(b"m2", b"").unwrap();
    assert_eq!(b.open(&c0, b"").unwrap(), b"m0".to_vec());
    assert_eq!(b.open(&c1, b"").unwrap(), b"m1".to_vec());
    assert_eq!(b.open(&c2, b"").unwrap(), b"m2".to_vec());
}

#[test]
fn tampered_ciphertext_is_rejected() {
    let (mut a, mut b) = establish();
    let mut ct = a.seal(b"secret", b"").unwrap();
    let last = ct.len() - 1;
    ct[last] ^= 0x01;
    assert!(matches!(b.open(&ct, b""), Err(BearerError::Crypto(_))));
}

#[test]
fn wrong_associated_data_is_rejected() {
    let (mut a, mut b) = establish();
    let ct = a.seal(b"bound to header", b"frame-1").unwrap();
    assert!(matches!(b.open(&ct, b"frame-2"), Err(BearerError::Crypto(_))));
}

#[test]
fn malformed_handshakes_are_rejected() {
    // Too short.
    assert!(matches!(Responder::respond(&[1, 1, 2, 3]), Err(BearerError::BadHandshake)));

    let (_initiator, good) = Initiator::start().unwrap();

    // Correct length, wrong version.
    let mut bad_version = good.clone();
    bad_version[0] = PROTOCOL_VERSION.wrapping_add(9);
    assert!(matches!(
        Responder::respond(&bad_version),
        Err(BearerError::UnsupportedVersion(_))
    ));

    // Correct length + version, unknown key-agreement suite.
    let mut bad_ka = good.clone();
    bad_ka[1] = 0xff;
    assert!(matches!(Responder::respond(&bad_ka), Err(BearerError::Crypto(_))));

    // Correct length + version, unknown KDF suite.
    let mut bad_kdf = good.clone();
    bad_kdf[2] = 0xfe;
    assert!(matches!(Responder::respond(&bad_kdf), Err(BearerError::Crypto(_))));

    // Correct length + version, unknown AEAD suite.
    let mut bad_aead = good;
    bad_aead[3] = 0xfd;
    assert!(matches!(Responder::respond(&bad_aead), Err(BearerError::Crypto(_))));
}


#[test]
fn all_zero_ephemeral_public_key_is_rejected() {
    // X25519 small-order points force an all-zero shared secret. The crypto layer
    // rejects that result, and the channel must surface the rejection during the
    // handshake instead of deriving known traffic keys.
    let (_initiator, mut hello) = Initiator::start().unwrap();
    hello[4..].fill(0);
    assert!(matches!(Responder::respond(&hello), Err(BearerError::Crypto(_))));
}

#[test]
fn channel_rejects_oversized_frames_before_crypto() {
    let (mut a, mut b) = establish();

    let too_large_plaintext = vec![0u8; MAX_CHANNEL_PLAINTEXT_BYTES + 1];
    assert!(matches!(
        a.seal(&too_large_plaintext, b""),
        Err(BearerError::FrameTooLarge { .. })
    ));

    let too_large_ciphertext = vec![0u8; MAX_CHANNEL_CIPHERTEXT_BYTES + 1];
    assert!(matches!(
        b.open(&too_large_ciphertext, b""),
        Err(BearerError::FrameTooLarge { .. })
    ));
}


#[test]
fn distinct_sessions_have_distinct_bindings() {
    let (a1, _b1) = establish();
    let (a2, _b2) = establish();
    // Fresh ephemeral keys each session -> different binding with overwhelming
    // probability.
    assert_ne!(a1.channel_binding(), a2.channel_binding());
}

#[test]
fn channel_runs_over_the_in_process_bearer() {
    let (mut left, mut right) = pair();

    // Handshake carried as frames over the bearer.
    let (initiator, hello1) = Initiator::start().unwrap();
    left.send(&hello1).unwrap();
    let got1 = right.recv().unwrap();

    let (mut right_channel, hello2) = Responder::respond(&got1).unwrap();
    right.send(&hello2).unwrap();
    let got2 = left.recv().unwrap();
    let mut left_channel = initiator.finish(&got2).unwrap();

    // Encrypted payload over the same bearer.
    let ct = left_channel.seal(b"presence nonce", b"ctx").unwrap();
    left.send(&ct).unwrap();
    let received = right.recv().unwrap();
    assert_eq!(right_channel.open(&received, b"ctx").unwrap(), b"presence nonce".to_vec());
}

#[test]
fn framing_roundtrips_and_reassembles_partial_chunks() {
    let f1 = encode_frame(b"alpha").unwrap();
    let f2 = encode_frame(b"bravo").unwrap();

    let mut reader = FrameReader::new();
    // Feed a byte at a time to prove reassembly across arbitrary boundaries.
    let stream: Vec<u8> = f1.iter().chain(f2.iter()).copied().collect();
    let mut frames = Vec::new();
    for byte in stream {
        reader.push(&[byte]).unwrap();
        while let Some(frame) = reader.next_frame().unwrap() {
            frames.push(frame);
        }
    }
    assert_eq!(frames, vec![b"alpha".to_vec(), b"bravo".to_vec()]);
}

#[test]
fn framing_reader_rejects_unbounded_buffer_growth() {
    let mut reader = FrameReader::new();
    let oversized = vec![0u8; MAX_STREAM_BUFFER_BYTES + 1];
    assert!(matches!(
        reader.push(&oversized),
        Err(BearerError::FrameTooLarge { .. })
    ));
}

#[test]
fn in_process_bearer_rejects_oversized_frames() {
    let (mut left, _right) = pair();
    let oversized = vec![0u8; MAX_FRAME_BYTES + 1];
    assert!(matches!(
        left.send(&oversized),
        Err(BearerError::FrameTooLarge { .. })
    ));
}

#[test]
fn closed_bearer_reports_closed() {
    let (mut left, right) = pair();
    drop(right);
    // With the peer gone, sends fail closed.
    assert_eq!(left.send(b"x"), Err(BearerError::Closed));
}
