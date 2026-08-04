#!/usr/bin/env python3
"""Enforce exact onion ciphertext sizes for every declared size class and hop."""

from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:120]!r}")
    p.write_text(text.replace(old, new), encoding="utf-8")


replace_once(
    "crates/mini-relay/src/onion.rs",
    """        let nonce = AeadNonce::from_bytes(reader.raw(AeadSuite::DEFAULT.nonce_len())?)?;
        let ciphertext =
            reader.bytes_limited(max_onion_ciphertext_bytes(size_class, hop_index)?)?;
        if !reader.finished() {
            return Err(RelayError::TrailingBytes);
        }
""",
    """        let nonce = AeadNonce::from_bytes(reader.raw(AeadSuite::DEFAULT.nonce_len())?)?;
        let expected_ciphertext_bytes = onion_ciphertext_bytes(size_class, hop_index)?;
        let ciphertext = reader.bytes_limited(expected_ciphertext_bytes)?;
        if ciphertext.len() != expected_ciphertext_bytes {
            return Err(RelayError::InvalidOnionRoute);
        }
        if !reader.finished() {
            return Err(RelayError::TrailingBytes);
        }
""",
)

replace_once(
    "crates/mini-relay/src/onion.rs",
    """fn max_onion_ciphertext_bytes(size_class: PayloadSizeClass, hop_index: u8) -> Result<usize> {
    let destination = 1usize
        .checked_add(16 + 1 + 1 + 32 + 12 + 4)
        .and_then(|value| value.checked_add(fixed_payload_bytes(size_class) + AEAD_TAG_BYTES))
        .ok_or(RelayError::LimitExceeded)?;
    let public_header: usize = 1 + 16 + 1 + 1 + 1 + 1 + 32 + 12 + 4;
    let hop_plaintext_overhead: usize = 1 + 8 + 32 + 4 + NEXT_HOP_PAD_BYTES + 4;
    let remaining_layers = ONION_HOP_COUNT
        .checked_sub(hop_index as usize)
        .ok_or(RelayError::InvalidOnionRoute)?;
    let mut length = destination;
    for _ in 0..remaining_layers {
        length = public_header
            .checked_add(hop_plaintext_overhead)
            .and_then(|value| value.checked_add(length))
            .and_then(|value| value.checked_add(AEAD_TAG_BYTES))
            .ok_or(RelayError::LimitExceeded)?;
    }
    Ok(length)
}
""",
    """/// Exact encrypted-body length for one hop and payload class.
///
/// Size classes are a traffic-shape and allocation boundary, not a suggestion.
/// The previous decoder used the total packet length as a ciphertext maximum,
/// leaving one public-header worth of attacker-controlled slack per packet. A
/// malicious sender could therefore create canonical, oversized packets for a
/// declared class. Computing the exact body length and requiring equality keeps
/// every accepted packet on the same fixed-size profile as `build_onion`.
fn onion_ciphertext_bytes(size_class: PayloadSizeClass, hop_index: u8) -> Result<usize> {
    let destination = 1usize
        .checked_add(16 + 1 + 1 + 32 + 12 + 4)
        .and_then(|value| value.checked_add(fixed_payload_bytes(size_class) + AEAD_TAG_BYTES))
        .ok_or(RelayError::LimitExceeded)?;
    let public_header: usize = 1 + 16 + 1 + 1 + 1 + 1 + 32 + 12 + 4;
    let hop_plaintext_overhead: usize = 1 + 8 + 32 + 4 + NEXT_HOP_PAD_BYTES + 4;
    let remaining_layers = ONION_HOP_COUNT
        .checked_sub(hop_index as usize)
        .ok_or(RelayError::InvalidOnionRoute)?;
    let mut packet_bytes = destination;
    for _ in 0..remaining_layers {
        packet_bytes = public_header
            .checked_add(hop_plaintext_overhead)
            .and_then(|value| value.checked_add(packet_bytes))
            .and_then(|value| value.checked_add(AEAD_TAG_BYTES))
            .ok_or(RelayError::LimitExceeded)?;
    }
    packet_bytes
        .checked_sub(public_header)
        .ok_or(RelayError::InvalidOnionRoute)
}
""",
)

replace_once(
    "crates/mini-relay/src/onion.rs",
    """    fn packet_round_trip_is_canonical_and_bounded() {
        let (hops, _) = route();
        let destination = AgreementSecretKey::from_seed(&[9; 32]);
        let packet = build_onion(
            ConnectionId::from_bytes([7; 16]),
            PayloadSizeClass::Medium,
            &hops,
            destination.public_key(),
            b"payload",
            BUILD_NOW_MS,
            EXPIRES_AT_MS,
        )
        .unwrap();
        let bytes = packet.to_bytes().unwrap();
        assert_eq!(OnionPacket::from_bytes(&bytes).unwrap(), packet);
        for cut in 0..bytes.len() {
            assert!(OnionPacket::from_bytes(&bytes[..cut]).is_err());
        }
    }
""",
    """    fn packet_round_trip_is_canonical_and_bounded() {
        let (hops, _) = route();
        let destination = AgreementSecretKey::from_seed(&[9; 32]);
        let packet = build_onion(
            ConnectionId::from_bytes([7; 16]),
            PayloadSizeClass::Medium,
            &hops,
            destination.public_key(),
            b"payload",
            BUILD_NOW_MS,
            EXPIRES_AT_MS,
        )
        .unwrap();
        let bytes = packet.to_bytes().unwrap();
        assert_eq!(OnionPacket::from_bytes(&bytes).unwrap(), packet);
        for cut in 0..bytes.len() {
            assert!(OnionPacket::from_bytes(&bytes[..cut]).is_err());
        }
    }

    #[test]
    fn declared_size_class_rejects_shorter_or_longer_ciphertext() {
        let (hops, _) = route();
        let destination = AgreementSecretKey::from_seed(&[9; 32]);
        let packet = build_onion(
            ConnectionId::from_bytes([7; 16]),
            PayloadSizeClass::Small,
            &hops,
            destination.public_key(),
            b"payload",
            BUILD_NOW_MS,
            EXPIRES_AT_MS,
        )
        .unwrap();

        let mut shorter = packet.clone();
        shorter.ciphertext.pop().unwrap();
        assert_eq!(
            OnionPacket::from_bytes(&shorter.to_bytes().unwrap()),
            Err(RelayError::InvalidOnionRoute)
        );

        let mut longer = packet;
        longer.ciphertext.push(0);
        assert_eq!(
            OnionPacket::from_bytes(&longer.to_bytes().unwrap()),
            Err(RelayError::LimitExceeded)
        );
    }
""",
)

replace_once(
    "docs/planning/privacy-transport-runtime-convergence.md",
    """- `mini-relay` unit and real-socket onion tests pass after the onion-v2
  replay/lifetime upgrade, including destination replay, fail-closed capacity,
  expiry pruning, excessive-lifetime rejection, and malformed-state atomicity.
""",
    """- `mini-relay` unit and real-socket onion tests pass after the onion-v2
  replay/lifetime upgrade, including destination replay, fail-closed capacity,
  monotonic time under wall-clock rollback, expiry pruning, excessive-lifetime
  rejection, malformed-state atomicity, and exact ciphertext length for every
  declared payload size class and hop.
""",
)

replace_once(
    "docs/THREAT_MODEL.md",
    """| **Cross-hop clear identifier correlation** | Every relay layer has an independent random public connection id; the destination id exists only inside destination encryption. | **Closed for explicit circuit ids.** Timing/volume correlation remains open. |
""",
    """| **Cross-hop clear identifier correlation** | Every relay layer has an independent random public connection id; the destination id exists only inside destination encryption. | **Closed for explicit circuit ids.** Timing/volume correlation remains open. |
| **Declared onion size-class bypass** | Onion v2 derives the exact ciphertext length for each hop and payload class and rejects both shorter and longer canonical frames before decryption. | **Closed for packet framing.** Timing and coarse class choice remain visible by design. |
""",
)

print("PR 296 exact onion size enforcement applied")
