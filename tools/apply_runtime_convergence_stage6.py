#!/usr/bin/env python3
"""Finalize onion-v2 domains, skew tolerance, and malformed-inner evidence."""

from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8")


def replace(path: str, old: str, new: str, expected: int = 1) -> None:
    text = read(path)
    count = text.count(old)
    if count != expected:
        raise SystemExit(f"{path}: expected {expected}, found {count}: {old[:100]!r}")
    write(path, text.replace(old, new))


onion = "crates/mini-relay/src/onion.rs"
relay_lib = "crates/mini-relay/src/lib.rs"
planning = "docs/planning/privacy-transport-runtime-convergence.md"
decision = "docs/DECISION_LOG.md"
threat = "docs/THREAT_MODEL.md"
readme = "crates/mini-transport-security/README.md"

replace(onion, 'const HOP_KEY_DOMAIN: &[u8] = b"mini-relay/onion-hop-key/v1";\n', 'const HOP_KEY_DOMAIN: &[u8] = b"mini-relay/onion-hop-key/v2";\n')
replace(onion, 'const DESTINATION_KEY_DOMAIN: &[u8] = b"mini-relay/onion-destination-key/v1";\n', 'const DESTINATION_KEY_DOMAIN: &[u8] = b"mini-relay/onion-destination-key/v2";\n')
replace(
    onion,
    """pub const MAX_ONION_LIFETIME_MS: u64 = 30 * 60 * 1000;
pub const SMALL_ONION_PAYLOAD_BYTES: usize = 4 * 1024;
""",
    """pub const MAX_ONION_LIFETIME_MS: u64 = 30 * 60 * 1000;
/// Clock disagreement tolerated when a relay compares the encrypted absolute
/// expiry against its local time. Retention remains bounded to lifetime + skew.
pub const MAX_ONION_CLOCK_SKEW_MS: u64 = 30 * 1000;
pub const SMALL_ONION_PAYLOAD_BYTES: usize = 4 * 1024;
""",
)
replace(
    onion,
    """    if remaining > MAX_ONION_LIFETIME_MS {
        return Err(RelayError::OnionLifetimeTooLong);
    }
""",
    """    let maximum = MAX_ONION_LIFETIME_MS
        .checked_add(MAX_ONION_CLOCK_SKEW_MS)
        .ok_or(RelayError::LimitExceeded)?;
    if remaining > maximum {
        return Err(RelayError::OnionLifetimeTooLong);
    }
""",
)
replace(
    onion,
    """                BUILD_NOW_MS + MAX_ONION_LIFETIME_MS + 1,
""",
    """                BUILD_NOW_MS + MAX_ONION_LIFETIME_MS + MAX_ONION_CLOCK_SKEW_MS + 1,
""",
)

marker = """    #[test]
    fn replay_capacity_fails_closed_until_entries_expire() {
"""
malformed_test = r'''    #[test]
    fn authenticated_but_malformed_inner_packet_does_not_consume_replay_state() {
        let relay_secret = AgreementSecretKey::from_seed(&[1; 32]);
        let ephemeral_secret = AgreementSecretKey::from_seed(&[2; 32]);
        let ephemeral_key = ephemeral_secret.public_key();
        let shared = ephemeral_secret
            .agree(&relay_secret.public_key())
            .unwrap();
        let nonce = AeadNonce::from_bytes(&[3; 12]).unwrap();
        let connection_id = ConnectionId::from_bytes([4; 16]);
        let aad = hop_aad(
            connection_id,
            PayloadSizeClass::Small,
            0,
            ephemeral_key,
            nonce,
        );
        let key = derive_key(HOP_KEY_DOMAIN, &shared.to_bytes(), &aad).unwrap();
        let plaintext = encode_hop_plaintext(
            RelayRole::Entry,
            EXPIRES_AT_MS,
            [5; 32],
            b"next-hop",
            b"not-a-canonical-inner-onion",
        )
        .unwrap();
        let packet = OnionPacket {
            connection_id,
            size_class: PayloadSizeClass::Small,
            hop_index: 0,
            ephemeral_key,
            nonce,
            ciphertext: key.encrypt(&nonce, &plaintext, &aad).unwrap(),
        };
        let mut replay = OnionReplayCache::new(1).unwrap();
        assert!(packet
            .peel(&relay_secret, PROCESS_NOW_MS, &mut replay)
            .is_err());
        assert!(replay.is_empty());
        replay
            .check_and_record([6; 32], EXPIRES_AT_MS, PROCESS_NOW_MS)
            .unwrap();
    }

'''
text = read(onion)
if text.count(marker) != 1:
    raise SystemExit("onion malformed test marker mismatch")
write(onion, text.replace(marker, malformed_test + marker, 1))

replace(
    relay_lib,
    """    PeeledOnion, LARGE_ONION_PAYLOAD_BYTES, MAX_ONION_LIFETIME_MS, MAX_ONION_NEXT_HOP_BYTES,
    MAX_ONION_REPLAY_ENTRIES, MEDIUM_ONION_PAYLOAD_BYTES, ONION_HOP_COUNT, ONION_VERSION,
""",
    """    PeeledOnion, LARGE_ONION_PAYLOAD_BYTES, MAX_ONION_CLOCK_SKEW_MS,
    MAX_ONION_LIFETIME_MS, MAX_ONION_NEXT_HOP_BYTES, MAX_ONION_REPLAY_ENTRIES,
    MEDIUM_ONION_PAYLOAD_BYTES, ONION_HOP_COUNT, ONION_VERSION,
""",
)
replace(
    planning,
    """| Relay and destination replay defense | **PASS in-process** | Onion v2 encrypts expiry/replay tokens for every relay and destination; validity-window entries are never evicted while live, malformed inner packets do not consume state, and capacity fails closed. | Hosts must persist equivalent state across restart; authenticated packet floods can still exhaust bounded capacity and require rate/resource controls. |
""",
    """| Relay and destination replay defense | **PASS in-process** | Onion v2 uses v2 key domains, encrypts expiry/replay tokens for every relay and destination, bounds lifetime with explicit clock-skew tolerance, never evicts live entries, records only after inner validation, and fails closed at capacity. | Hosts must persist equivalent state across restart; authenticated packet floods can still exhaust bounded capacity and require rate/resource controls. |
""",
)
replace(
    decision,
    """validity-window, fail-closed relay and destination replay state; advertisement
""",
    """onion-v2 domain separation, clock-skew-bounded validity, fail-closed relay and
 destination replay state; advertisement
""",
)
replace(
    threat,
    """| **Relay-cache eviction re-enabling replay** | Onion v2 stores `(token, expiry)` through the encrypted validity window, prunes only expired entries, and fails closed at capacity. Recording occurs only after the whole local inner structure validates. | **Closed in-process.** Restart persistence and flood/rate controls remain host responsibilities. |
""",
    """| **Relay-cache eviction re-enabling replay** | Onion v2 uses separate v2 cryptographic domains, stores `(token, expiry)` through a clock-skew-bounded encrypted window, prunes only expired entries, records only after the whole local inner structure validates, and fails closed at capacity. | **Closed in-process.** Restart persistence and flood/rate controls remain host responsibilities. |
""",
)
replace(
    readme,
    """  separates endpoint knowledge, bounds remaining lifetime, and requires
  fail-closed relay/destination replay state. It is not Sphinx and does not
""",
    """  separates endpoint knowledge, uses v2 cryptographic domains, bounds remaining
  lifetime with explicit clock-skew tolerance, and requires fail-closed
  relay/destination replay state. It is not Sphinx and does not
""",
)

print("stage 6 applied")
