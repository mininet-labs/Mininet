#!/usr/bin/env python3
"""Apply the first compiler/privacy hardening pass for PR #292.

Temporary branch-local helper. The verification job deletes this file in the
same tested commit that carries its source changes.
"""

from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old!r}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


replace_once(
    "crates/mini-relay/src/onion.rs",
    """/// Build one fixed-role onion circuit. `destination_key` belongs to the final
/// endpoint, not the delivery relay. Every hop key should come from a verified,
/// signed peer advertisement.
pub fn build_onion(
    connection_id: ConnectionId,
""",
    """/// Build one fixed-role onion circuit. `destination_key` belongs to the final
/// endpoint, not the delivery relay. Every hop key should come from a verified,
/// signed peer advertisement. `destination_connection_id` is visible only in
/// the destination-encrypted envelope; each relay layer receives an independent
/// random public connection id so observers cannot correlate hops by one shared
/// cleartext circuit identifier.
pub fn build_onion(
    destination_connection_id: ConnectionId,
""",
)

replace_once(
    "crates/mini-relay/src/onion.rs",
    """    let destination = DestinationEnvelope::seal(
        connection_id,
        size_class,
        destination_key,
        plaintext,
    )?;
    let mut inner = destination.to_bytes()?;

    for (index, hop) in hops.iter().enumerate().rev() {
        let ephemeral_secret = AgreementSecretKey::generate()?;
""",
    """    let destination = DestinationEnvelope::seal(
        destination_connection_id,
        size_class,
        destination_key,
        plaintext,
    )?;
    let mut inner = destination.to_bytes()?;
    let mut public_connection_ids = HashSet::with_capacity(ONION_HOP_COUNT + 1);
    public_connection_ids.insert(destination_connection_id);

    for (index, hop) in hops.iter().enumerate().rev() {
        let connection_id = ConnectionId::generate()?;
        if !public_connection_ids.insert(connection_id) {
            return Err(RelayError::InvalidOnionRoute);
        }
        let ephemeral_secret = AgreementSecretKey::generate()?;
""",
)

replace_once(
    "crates/mini-relay/src/onion.rs",
    """            if next.connection_id != self.connection_id
                || next.size_class != self.size_class
                || next.hop_index != self.hop_index + 1
""",
    """            if next.connection_id == self.connection_id
                || next.size_class != self.size_class
                || next.hop_index != self.hop_index + 1
""",
)

replace_once(
    "crates/mini-relay/src/onion.rs",
    """            if destination.connection_id != self.connection_id
                || destination.size_class != self.size_class
""",
    """            if destination.connection_id == self.connection_id
                || destination.size_class != self.size_class
""",
)

replace_once(
    "crates/mini-relay/src/onion.rs",
    """    let public_header = 1 + 16 + 1 + 1 + 1 + 1 + 32 + 12 + 4;
    let hop_plaintext_overhead = 1 + 8 + 32 + 4 + NEXT_HOP_PAD_BYTES + 4;
""",
    """    let public_header: usize = 1 + 16 + 1 + 1 + 1 + 1 + 32 + 12 + 4;
    let hop_plaintext_overhead: usize = 1 + 8 + 32 + 4 + NEXT_HOP_PAD_BYTES + 4;
""",
)

replace_once(
    "crates/mini-relay/src/onion.rs",
    """        let mut destination_envelope = None;
        for (index, secret) in secrets.iter().enumerate() {
            let mut replay = OnionReplayCache::new(8).unwrap();
            let peeled = packet.peel(secret, 5_000, &mut replay).unwrap();
""",
    """        let mut destination_envelope = None;
        let mut public_connection_ids = HashSet::new();
        for (index, secret) in secrets.iter().enumerate() {
            assert!(public_connection_ids.insert(packet.connection_id));
            let mut replay = OnionReplayCache::new(8).unwrap();
            let peeled = packet.peel(secret, 5_000, &mut replay).unwrap();
""",
)

replace_once(
    "crates/mini-relay/src/onion.rs",
    """        let opened = open_onion_destination(&destination_envelope.unwrap(), &destination).unwrap();
        assert_eq!(opened, b\"private application payload\");
""",
    """        assert_eq!(public_connection_ids.len(), ONION_HOP_COUNT);
        let opened = open_onion_destination(&destination_envelope.unwrap(), &destination).unwrap();
        assert_eq!(opened, b\"private application payload\");
""",
)

# Do not spend redundant dial slots on the same signed X25519 routing key under
# several endpoint ids. Prefix diversity remains an additional, separate cap.
replace_once(
    "crates/mini-transport-security/src/selection.rs",
    """    let mut endpoints = HashSet::new();
    let mut prefix_counts: HashMap<NetworkPrefix, usize> = HashMap::new();
""",
    """    let mut endpoints = HashSet::new();
    let mut routing_keys = HashSet::new();
    let mut prefix_counts: HashMap<NetworkPrefix, usize> = HashMap::new();
""",
)

replace_once(
    "crates/mini-transport-security/src/selection.rs",
    """        if !endpoints.insert(record.endpoint_id()) {
            continue;
        }
        let prefix = NetworkPrefix::from_ip(record.address().ip());
""",
    """        if !endpoints.insert(record.endpoint_id())
            || !routing_keys.insert(record.routing_key())
        {
            continue;
        }
        let prefix = NetworkPrefix::from_ip(record.address().ip());
""",
)

print("applied PR #292 stage-one compiler and privacy hardening")
