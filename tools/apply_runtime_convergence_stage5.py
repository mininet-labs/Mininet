#!/usr/bin/env python3
"""Apply final security hardening for PR #296.

Closes replay eviction and destination replay gaps in the merged onion format,
bounds onion validity and selection inputs, rechecks advertisement liveness at
runtime, and makes authenticated search-provider labels channel-scoped.
"""

from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    target = Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(text, encoding="utf-8")


def replace_exact(path: str, old: str, new: str, expected: int = 1) -> None:
    text = read(path)
    count = text.count(old)
    if count != expected:
        raise SystemExit(
            f"{path}: expected {expected} matches, found {count}: {old[:120]!r}"
        )
    write(path, text.replace(old, new))


def replace_from_marker(path: str, marker: str, replacement: str) -> None:
    text = read(path)
    count = text.count(marker)
    if count != 1:
        raise SystemExit(f"{path}: expected one marker, found {count}: {marker!r}")
    write(path, text[: text.index(marker)] + replacement)


relay_error = "crates/mini-relay/src/error.rs"
onion = "crates/mini-relay/src/onion.rs"
relay_lib = "crates/mini-relay/src/lib.rs"
onion_tcp = "crates/mini-relay/tests/onion_tcp.rs"
runtime = "crates/mini-transport-security/src/runtime.rs"
runtime_test = "crates/mini-transport-security/tests/runtime_tcp.rs"
verified_onion_test = "crates/mini-transport-security/tests/verified_onion_tcp.rs"
selection = "crates/mini-transport-security/src/selection.rs"
transport_lib = "crates/mini-transport-security/src/lib.rs"
query = "crates/mini-search-federation-net/src/query.rs"
query_test = "crates/mini-search-federation-net/tests/authenticated_query_over_tcp.rs"
planning = "docs/planning/privacy-transport-runtime-convergence.md"
decision = "docs/DECISION_LOG.md"
status = "docs/STATUS.md"
threat = "docs/THREAT_MODEL.md"
f6_design = "docs/design/f6-private-query-transport.md"
transport_readme = "crates/mini-transport-security/README.md"

# ---------------------------------------------------------------------------
# mini-relay: fail-closed validity-window replay state for every relay and the
# final destination. The destination plaintext format changes, so bump v1 -> v2.
# ---------------------------------------------------------------------------
replace_exact(
    relay_error,
    """    /// The onion packet's signed/encrypted validity window ended.
    OnionExpired,
    /// A per-hop replay token was already processed.
""",
    """    /// The onion packet's signed/encrypted validity window ended.
    OnionExpired,
    /// The onion packet asks a relay or destination to retain replay state for
    /// longer than the protocol's hard maximum.
    OnionLifetimeTooLong,
    /// A per-hop or destination replay token was already processed.
""",
)
replace_exact(
    relay_error,
    """            RelayError::OnionExpired => write!(f, "onion packet has expired"),
            RelayError::OnionReplay => write!(f, "onion hop replay detected"),
""",
    """            RelayError::OnionExpired => write!(f, "onion packet has expired"),
            RelayError::OnionLifetimeTooLong => {
                write!(f, "onion validity window exceeds the hard maximum")
            }
            RelayError::OnionReplay => write!(f, "onion relay/destination replay detected"),
""",
)

replace_exact(onion, "use std::collections::{HashSet, VecDeque};\n", "use std::collections::{HashMap, HashSet};\n")
replace_exact(onion, "pub const ONION_VERSION: u8 = 1;\n", "pub const ONION_VERSION: u8 = 2;\n")
replace_exact(
    onion,
    """pub const MAX_ONION_REPLAY_ENTRIES: usize = 65_536;
pub const SMALL_ONION_PAYLOAD_BYTES: usize = 4 * 1024;
""",
    """pub const MAX_ONION_REPLAY_ENTRIES: usize = 65_536;
/// Maximum remaining validity accepted when a relay or destination processes a
/// packet. This bounds replay-state retention even for adversarial senders.
pub const MAX_ONION_LIFETIME_MS: u64 = 30 * 60 * 1000;
pub const SMALL_ONION_PAYLOAD_BYTES: usize = 4 * 1024;
""",
)
replace_exact(
    onion,
    """const NEXT_HOP_PAD_BYTES: usize = MAX_ONION_NEXT_HOP_BYTES;
const AEAD_TAG_BYTES: usize = 16;
""",
    """const NEXT_HOP_PAD_BYTES: usize = MAX_ONION_NEXT_HOP_BYTES;
const AEAD_TAG_BYTES: usize = 16;
const DESTINATION_FRAME_OVERHEAD_BYTES: usize = 8 + 32 + 4;
""",
)

old_cache = """#[derive(Debug, Clone)]
pub struct OnionReplayCache {
    capacity: usize,
    seen: HashSet<[u8; 32]>,
    order: VecDeque<[u8; 32]>,
}

impl OnionReplayCache {
    pub fn new(capacity: usize) -> Result<Self> {
        if capacity == 0 || capacity > MAX_ONION_REPLAY_ENTRIES {
            return Err(RelayError::LimitExceeded);
        }
        Ok(Self {
            capacity,
            seen: HashSet::with_capacity(capacity.min(1024)),
            order: VecDeque::with_capacity(capacity.min(1024)),
        })
    }

    pub fn check_and_record(&mut self, token: [u8; 32]) -> Result<()> {
        if !self.seen.insert(token) {
            return Err(RelayError::OnionReplay);
        }
        self.order.push_back(token);
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.seen.remove(&oldest);
            }
        }
        Ok(())
    }
}
"""
new_cache = """/// Bounded replay state shared by relay hops and destination delivery.
///
/// Entries remain until the encrypted validity window ends. Capacity exhaustion
/// fails closed rather than evicting a still-valid token and silently accepting
/// its replay. A production relay/destination must persist equivalent state if
/// replay defense must survive process restart.
#[derive(Debug, Clone)]
pub struct OnionReplayCache {
    capacity: usize,
    seen: HashMap<[u8; 32], u64>,
}

impl OnionReplayCache {
    pub fn new(capacity: usize) -> Result<Self> {
        if capacity == 0 || capacity > MAX_ONION_REPLAY_ENTRIES {
            return Err(RelayError::LimitExceeded);
        }
        Ok(Self {
            capacity,
            seen: HashMap::with_capacity(capacity.min(1024)),
        })
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    pub fn prune_expired(&mut self, now_ms: u64) {
        self.seen
            .retain(|_, expires_at_ms| *expires_at_ms > now_ms);
    }

    pub fn check_and_record(
        &mut self,
        token: [u8; 32],
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<()> {
        validate_onion_window(now_ms, expires_at_ms)?;
        self.prune_expired(now_ms);
        if self.seen.contains_key(&token) {
            return Err(RelayError::OnionReplay);
        }
        if self.seen.len() >= self.capacity {
            return Err(RelayError::LimitExceeded);
        }
        self.seen.insert(token, expires_at_ms);
        Ok(())
    }
}
"""
replace_exact(onion, old_cache, new_cache)

replace_exact(
    onion,
    """    plaintext: &[u8],
    expires_at_ms: u64,
) -> Result<OnionPacket> {
    validate_route(hops)?;
    if expires_at_ms == 0 {
        return Err(RelayError::OnionExpired);
    }

    let destination = DestinationEnvelope::seal(
        destination_connection_id,
        size_class,
        destination_key,
        plaintext,
    )?;
""",
    """    plaintext: &[u8],
    now_ms: u64,
    expires_at_ms: u64,
) -> Result<OnionPacket> {
    validate_route(hops)?;
    validate_onion_window(now_ms, expires_at_ms)?;

    let destination = DestinationEnvelope::seal(
        destination_connection_id,
        size_class,
        destination_key,
        plaintext,
        expires_at_ms,
    )?;
""",
)
replace_exact(
    onion,
    """        if now_ms > decoded.expires_at_ms {
            return Err(RelayError::OnionExpired);
        }
        replay.check_and_record(decoded.replay_token)?;

        let forward = if self.hop_index as usize + 1 < ONION_HOP_COUNT {
""",
    """        validate_onion_window(now_ms, decoded.expires_at_ms)?;

        let forward = if self.hop_index as usize + 1 < ONION_HOP_COUNT {
""",
)
replace_exact(
    onion,
    """            OnionForward::Destination(decoded.inner)
        };

        Ok(PeeledOnion {
""",
    """            OnionForward::Destination(decoded.inner)
        };

        // Record only after the whole local layer and its next structure are
        // valid. Malformed inner packets must not consume replay capacity.
        replay.check_and_record(
            decoded.replay_token,
            decoded.expires_at_ms,
            now_ms,
        )?;

        Ok(PeeledOnion {
""",
)

old_destination_api = """/// Open the destination-only envelope after the delivery relay forwards it.
pub fn open_onion_destination(
    opaque_destination_envelope: &[u8],
    destination_secret: &AgreementSecretKey,
) -> Result<Vec<u8>> {
    DestinationEnvelope::from_bytes(opaque_destination_envelope)?.open(destination_secret)
}
"""
new_destination_api = """/// Open the destination-only envelope after the delivery relay forwards it.
/// Destination replay and expiry checks are mandatory: bypassing them would let
/// an observer replay a captured post-delivery envelope directly to the endpoint.
pub fn open_onion_destination(
    opaque_destination_envelope: &[u8],
    destination_secret: &AgreementSecretKey,
    now_ms: u64,
    replay: &mut OnionReplayCache,
) -> Result<Vec<u8>> {
    DestinationEnvelope::from_bytes(opaque_destination_envelope)?.open(
        destination_secret,
        now_ms,
        replay,
    )
}
"""
replace_exact(onion, old_destination_api, new_destination_api)
replace_exact(
    onion,
    """        destination_key: AgreementPublicKey,
        plaintext: &[u8],
    ) -> Result<Self> {
        let frame = encode_fixed_payload(size_class, plaintext)?;
""",
    """        destination_key: AgreementPublicKey,
        plaintext: &[u8],
        expires_at_ms: u64,
    ) -> Result<Self> {
        let replay_token = random_32()?;
        let frame = encode_fixed_payload(
            size_class,
            expires_at_ms,
            replay_token,
            plaintext,
        )?;
""",
)
replace_exact(
    onion,
    """    fn open(&self, destination_secret: &AgreementSecretKey) -> Result<Vec<u8>> {
        let shared = destination_secret.agree(&self.ephemeral_key)?;
""",
    """    fn open(
        &self,
        destination_secret: &AgreementSecretKey,
        now_ms: u64,
        replay: &mut OnionReplayCache,
    ) -> Result<Vec<u8>> {
        let shared = destination_secret.agree(&self.ephemeral_key)?;
""",
)
replace_exact(
    onion,
    """        let frame = key.decrypt(&self.nonce, &self.ciphertext, &aad)?;
        decode_fixed_payload(self.size_class, &frame)
    }
""",
    """        let frame = key.decrypt(&self.nonce, &self.ciphertext, &aad)?;
        let decoded = decode_fixed_payload(self.size_class, &frame)?;
        validate_onion_window(now_ms, decoded.expires_at_ms)?;
        replay.check_and_record(
            decoded.replay_token,
            decoded.expires_at_ms,
            now_ms,
        )?;
        Ok(decoded.plaintext)
    }
""",
)

old_fixed = """fn encode_fixed_payload(size_class: PayloadSizeClass, plaintext: &[u8]) -> Result<Vec<u8>> {
    let frame_size = fixed_payload_bytes(size_class);
    let capacity = frame_size.checked_sub(4).ok_or(RelayError::LimitExceeded)?;
    if plaintext.len() > capacity {
        return Err(RelayError::OnionPayloadTooLarge);
    }
    let mut frame = vec![0u8; frame_size];
    frame[..4].copy_from_slice(
        &u32::try_from(plaintext.len())
            .map_err(|_| RelayError::OnionPayloadTooLarge)?
            .to_be_bytes(),
    );
    frame[4..4 + plaintext.len()].copy_from_slice(plaintext);
    Ok(frame)
}

fn decode_fixed_payload(size_class: PayloadSizeClass, frame: &[u8]) -> Result<Vec<u8>> {
    if frame.len() != fixed_payload_bytes(size_class) || frame.len() < 4 {
        return Err(RelayError::InvalidOnionRoute);
    }
    let length = u32::from_be_bytes(
        frame[..4]
            .try_into()
            .map_err(|_| RelayError::InvalidOnionRoute)?,
    ) as usize;
    if length > frame.len() - 4 || frame[4 + length..].iter().any(|byte| *byte != 0) {
        return Err(RelayError::InvalidOnionRoute);
    }
    Ok(frame[4..4 + length].to_vec())
}
"""
new_fixed = """fn validate_onion_window(now_ms: u64, expires_at_ms: u64) -> Result<()> {
    let remaining = expires_at_ms
        .checked_sub(now_ms)
        .ok_or(RelayError::OnionExpired)?;
    if remaining == 0 {
        return Err(RelayError::OnionExpired);
    }
    if remaining > MAX_ONION_LIFETIME_MS {
        return Err(RelayError::OnionLifetimeTooLong);
    }
    Ok(())
}

fn encode_fixed_payload(
    size_class: PayloadSizeClass,
    expires_at_ms: u64,
    replay_token: [u8; 32],
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let frame_size = fixed_payload_bytes(size_class);
    let capacity = frame_size
        .checked_sub(DESTINATION_FRAME_OVERHEAD_BYTES)
        .ok_or(RelayError::LimitExceeded)?;
    if plaintext.len() > capacity {
        return Err(RelayError::OnionPayloadTooLarge);
    }
    let mut frame = vec![0u8; frame_size];
    frame[..8].copy_from_slice(&expires_at_ms.to_be_bytes());
    frame[8..40].copy_from_slice(&replay_token);
    frame[40..44].copy_from_slice(
        &u32::try_from(plaintext.len())
            .map_err(|_| RelayError::OnionPayloadTooLarge)?
            .to_be_bytes(),
    );
    frame[44..44 + plaintext.len()].copy_from_slice(plaintext);
    Ok(frame)
}

#[derive(Debug)]
struct DestinationPlaintext {
    expires_at_ms: u64,
    replay_token: [u8; 32],
    plaintext: Vec<u8>,
}

fn decode_fixed_payload(
    size_class: PayloadSizeClass,
    frame: &[u8],
) -> Result<DestinationPlaintext> {
    if frame.len() != fixed_payload_bytes(size_class)
        || frame.len() < DESTINATION_FRAME_OVERHEAD_BYTES
    {
        return Err(RelayError::InvalidOnionRoute);
    }
    let expires_at_ms = u64::from_be_bytes(
        frame[..8]
            .try_into()
            .map_err(|_| RelayError::InvalidOnionRoute)?,
    );
    let replay_token = frame[8..40]
        .try_into()
        .map_err(|_| RelayError::InvalidOnionRoute)?;
    let length = u32::from_be_bytes(
        frame[40..44]
            .try_into()
            .map_err(|_| RelayError::InvalidOnionRoute)?,
    ) as usize;
    if length > frame.len() - DESTINATION_FRAME_OVERHEAD_BYTES
        || frame[44 + length..].iter().any(|byte| *byte != 0)
    {
        return Err(RelayError::InvalidOnionRoute);
    }
    Ok(DestinationPlaintext {
        expires_at_ms,
        replay_token,
        plaintext: frame[44..44 + length].to_vec(),
    })
}
"""
replace_exact(onion, old_fixed, new_fixed)

new_onion_tests = r'''#[cfg(test)]
mod tests {
    use super::*;

    const BUILD_NOW_MS: u64 = 1_000;
    const PROCESS_NOW_MS: u64 = 5_000;
    const EXPIRES_AT_MS: u64 = 10_000;

    fn route() -> (Vec<OnionHop>, Vec<AgreementSecretKey>) {
        let secrets: Vec<_> = [1u8, 2, 3]
            .into_iter()
            .map(|seed| AgreementSecretKey::from_seed(&[seed; 32]))
            .collect();
        let hops = vec![
            OnionHop {
                role: RelayRole::Entry,
                routing_key: secrets[0].public_key(),
                next_hop: b"rendezvous-token".to_vec(),
            },
            OnionHop {
                role: RelayRole::Rendezvous,
                routing_key: secrets[1].public_key(),
                next_hop: b"delivery-token".to_vec(),
            },
            OnionHop {
                role: RelayRole::Delivery,
                routing_key: secrets[2].public_key(),
                next_hop: b"destination-mailbox".to_vec(),
            },
        ];
        (hops, secrets)
    }

    #[test]
    fn three_relays_and_destination_each_reject_replay() {
        let (hops, secrets) = route();
        let destination = AgreementSecretKey::from_seed(&[9; 32]);
        let connection_id = ConnectionId::from_bytes([7; 16]);
        let mut packet = build_onion(
            connection_id,
            PayloadSizeClass::Small,
            &hops,
            destination.public_key(),
            b"private application payload",
            BUILD_NOW_MS,
            EXPIRES_AT_MS,
        )
        .unwrap();

        let mut destination_envelope = None;
        let mut public_connection_ids = HashSet::new();
        for (index, secret) in secrets.iter().enumerate() {
            assert!(public_connection_ids.insert(packet.connection_id));
            let original = packet.clone();
            let mut replay = OnionReplayCache::new(8).unwrap();
            let peeled = packet
                .peel(secret, PROCESS_NOW_MS, &mut replay)
                .unwrap();
            assert_eq!(peeled.role, hops[index].role);
            assert_eq!(peeled.next_hop, hops[index].next_hop);
            assert_eq!(
                original.peel(secret, PROCESS_NOW_MS, &mut replay),
                Err(RelayError::OnionReplay)
            );
            match peeled.forward {
                OnionForward::Next(next) => packet = next,
                OnionForward::Destination(bytes) => destination_envelope = Some(bytes),
            }
        }
        assert_eq!(public_connection_ids.len(), ONION_HOP_COUNT);
        let destination_envelope = destination_envelope.unwrap();
        let mut destination_replay = OnionReplayCache::new(8).unwrap();
        let opened = open_onion_destination(
            &destination_envelope,
            &destination,
            PROCESS_NOW_MS,
            &mut destination_replay,
        )
        .unwrap();
        assert_eq!(opened, b"private application payload");
        assert_eq!(
            open_onion_destination(
                &destination_envelope,
                &destination,
                PROCESS_NOW_MS,
                &mut destination_replay,
            ),
            Err(RelayError::OnionReplay)
        );
    }

    #[test]
    fn wrong_relay_tampering_expiry_and_replay_fail_closed() {
        let (hops, secrets) = route();
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
        let wrong = AgreementSecretKey::from_seed(&[44; 32]);
        let mut cache = OnionReplayCache::new(8).unwrap();
        assert!(packet.peel(&wrong, PROCESS_NOW_MS, &mut cache).is_err());
        assert_eq!(
            packet.peel(&secrets[0], EXPIRES_AT_MS, &mut cache),
            Err(RelayError::OnionExpired)
        );
        let peeled = packet
            .peel(&secrets[0], PROCESS_NOW_MS, &mut cache)
            .unwrap();
        assert_eq!(
            packet.peel(&secrets[0], PROCESS_NOW_MS, &mut cache),
            Err(RelayError::OnionReplay)
        );
        let mut tampered = match peeled.forward {
            OnionForward::Next(next) => next,
            OnionForward::Destination(_) => panic!("entry cannot be final"),
        };
        tampered.connection_id = ConnectionId::from_bytes([8; 16]);
        let mut next_cache = OnionReplayCache::new(8).unwrap();
        assert!(tampered
            .peel(&secrets[1], PROCESS_NOW_MS, &mut next_cache)
            .is_err());
        assert!(next_cache.is_empty());
    }

    #[test]
    fn replay_capacity_fails_closed_until_entries_expire() {
        let mut cache = OnionReplayCache::new(2).unwrap();
        cache.check_and_record([1; 32], 2_000, 1_000).unwrap();
        cache.check_and_record([2; 32], 2_000, 1_000).unwrap();
        assert_eq!(
            cache.check_and_record([3; 32], 2_000, 1_000),
            Err(RelayError::LimitExceeded)
        );
        assert_eq!(cache.len(), 2);
        cache.check_and_record([3; 32], 3_000, 2_001).unwrap();
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn zero_or_excessive_remaining_lifetime_is_rejected() {
        let (hops, _) = route();
        let destination = AgreementSecretKey::from_seed(&[9; 32]);
        assert_eq!(
            build_onion(
                ConnectionId::from_bytes([7; 16]),
                PayloadSizeClass::Small,
                &hops,
                destination.public_key(),
                b"payload",
                BUILD_NOW_MS,
                BUILD_NOW_MS,
            ),
            Err(RelayError::OnionExpired)
        );
        assert_eq!(
            build_onion(
                ConnectionId::from_bytes([7; 16]),
                PayloadSizeClass::Small,
                &hops,
                destination.public_key(),
                b"payload",
                BUILD_NOW_MS,
                BUILD_NOW_MS + MAX_ONION_LIFETIME_MS + 1,
            ),
            Err(RelayError::OnionLifetimeTooLong)
        );
    }

    #[test]
    fn route_roles_keys_and_payload_size_are_strict() {
        let (mut hops, _) = route();
        let destination = AgreementSecretKey::from_seed(&[9; 32]);
        hops[1].role = RelayRole::Entry;
        assert_eq!(
            build_onion(
                ConnectionId::from_bytes([7; 16]),
                PayloadSizeClass::Small,
                &hops,
                destination.public_key(),
                b"payload",
                BUILD_NOW_MS,
                EXPIRES_AT_MS,
            ),
            Err(RelayError::InvalidOnionRoute)
        );
        let (mut hops, _) = route();
        hops[1].routing_key = hops[0].routing_key;
        assert_eq!(
            build_onion(
                ConnectionId::from_bytes([7; 16]),
                PayloadSizeClass::Small,
                &hops,
                destination.public_key(),
                b"payload",
                BUILD_NOW_MS,
                EXPIRES_AT_MS,
            ),
            Err(RelayError::InvalidOnionRoute)
        );
        let (hops, _) = route();
        let oversized = vec![0u8; SMALL_ONION_PAYLOAD_BYTES];
        assert_eq!(
            build_onion(
                ConnectionId::from_bytes([7; 16]),
                PayloadSizeClass::Small,
                &hops,
                destination.public_key(),
                &oversized,
                BUILD_NOW_MS,
                EXPIRES_AT_MS,
            ),
            Err(RelayError::OnionPayloadTooLarge)
        );
    }

    #[test]
    fn packet_round_trip_is_canonical_and_bounded() {
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
}
'''
replace_from_marker(onion, "#[cfg(test)]\nmod tests {", new_onion_tests)

replace_exact(
    relay_lib,
    """    PeeledOnion, LARGE_ONION_PAYLOAD_BYTES, MAX_ONION_NEXT_HOP_BYTES, MAX_ONION_REPLAY_ENTRIES,
    MEDIUM_ONION_PAYLOAD_BYTES, ONION_HOP_COUNT, ONION_VERSION, SMALL_ONION_PAYLOAD_BYTES,
""",
    """    PeeledOnion, LARGE_ONION_PAYLOAD_BYTES, MAX_ONION_LIFETIME_MS,
    MAX_ONION_NEXT_HOP_BYTES, MAX_ONION_REPLAY_ENTRIES, MEDIUM_ONION_PAYLOAD_BYTES,
    ONION_HOP_COUNT, ONION_VERSION, SMALL_ONION_PAYLOAD_BYTES,
""",
)

replace_exact(
    onion_tcp,
    """        let opaque = bearer.recv().unwrap();
        open_onion_destination(&opaque, &destination_secret).unwrap()
""",
    """        let opaque = bearer.recv().unwrap();
        let mut replay = OnionReplayCache::new(32).unwrap();
        open_onion_destination(&opaque, &destination_secret, 5_000, &mut replay).unwrap()
""",
)
replace_exact(
    onion_tcp,
    """        b"private over three real sockets",
        10_000,
""",
    """        b"private over three real sockets",
        1_000,
        10_000,
""",
)

# ---------------------------------------------------------------------------
# Runtime: recheck ad expiry at use time; reject mixed-network route sets; pass
# a bounded build time to onion v2.
# ---------------------------------------------------------------------------
replace_exact(
    runtime,
    """        PeerExpectation::Advertised {
            advertisement,
            root_kel,
            device_kel,
        } => claim.verify_advertised(
            advertisement,
""",
    """        PeerExpectation::Advertised {
            advertisement,
            root_kel,
            device_kel,
        } => {
            ensure_advertisement_live(advertisement, now_ms)?;
            claim.verify_advertised(
            advertisement,
""",
)
replace_exact(
    runtime,
    """            freshness,
            replay,
        ),
    }
}

/// Dial one signed endpoint over TCP, establish CH1, and authenticate the live
""",
    """            freshness,
            replay,
        )
        }
    }
}

fn ensure_advertisement_live(
    advertisement: &VerifiedPeerAdvertisement,
    now_ms: u64,
) -> Result<()> {
    if now_ms > advertisement.expires_at_ms() {
        return Err(TransportSecurityError::Expired);
    }
    Ok(())
}

/// Dial one signed endpoint over TCP, establish CH1, and authenticate the live
""",
)
replace_exact(
    runtime,
    """    let (bearer, channel) = establish_tcp_initiator(target.advertisement.address(), timeout_ms)?;
""",
    """    ensure_advertisement_live(target.advertisement, now_ms)?;
    let (bearer, channel) = establish_tcp_initiator(target.advertisement.address(), timeout_ms)?;
""",
)
replace_exact(
    runtime,
    """    let records: Vec<_> = targets
        .iter()
        .map(|target| target.advertisement.clone())
        .collect();
    let plan = diverse_dial_plan(&records, local_seed, policy)?;
""",
    """    if targets.len() > crate::MAX_SELECTION_CANDIDATES {
        return Err(TransportSecurityError::LimitExceeded);
    }
    let expected_network = targets
        .first()
        .map(|target| target.advertisement.network_id());
    let mut records = Vec::with_capacity(targets.len());
    for target in targets {
        if Some(target.advertisement.network_id()) != expected_network {
            return Err(TransportSecurityError::WrongNetwork);
        }
        if now_ms <= target.advertisement.expires_at_ms() {
            records.push(target.advertisement.clone());
        }
    }
    let plan = diverse_dial_plan(&records, local_seed, policy)?;
""",
)
replace_exact(
    runtime,
    """    plaintext: &[u8],
    expires_at_ms: u64,
) -> Result<OnionPacket> {
    for left in 0..relays.len() {
""",
    """    plaintext: &[u8],
    now_ms: u64,
    expires_at_ms: u64,
) -> Result<OnionPacket> {
    let expected_network = relays[0].advertisement.network_id();
    for relay in &relays {
        ensure_advertisement_live(relay.advertisement, now_ms)?;
        if relay.advertisement.network_id() != expected_network {
            return Err(TransportSecurityError::WrongNetwork);
        }
    }
    for left in 0..relays.len() {
""",
)
replace_exact(
    runtime,
    """        destination_key,
        plaintext,
        expires_at_ms,
""",
    """        destination_key,
        plaintext,
        now_ms,
        expires_at_ms,
""",
)

# Update runtime unit call sites and add expiry/network assertions.
replace_exact(runtime, "            b\"payload\",\n            10_000,\n", "            b\"payload\",\n            1_500,\n            10_000,\n", expected=2)
insert_marker = """    #[test]
    fn three_distinct_verified_endpoints_build_an_onion() {
"""
expiry_tests = r'''    #[test]
    fn verified_route_rechecks_expiry_and_network_at_use_time() {
        let a = verified(10, "10.0.0.1:9000");
        let b = verified(20, "10.0.1.1:9000");
        let c = verified(30, "10.0.2.1:9000");
        let destination = AgreementSecretKey::from_seed(&[99; 32]);
        assert_eq!(
            build_verified_onion_route(
                [
                    VerifiedRelay::new(&a, b"rendezvous"),
                    VerifiedRelay::new(&b, b"delivery"),
                    VerifiedRelay::new(&c, b"destination"),
                ],
                ConnectionId::from_bytes([1; 16]),
                PayloadSizeClass::Small,
                destination.public_key(),
                b"payload",
                2_001,
                10_000,
            ),
            Err(TransportSecurityError::Expired)
        );

        let mut foreign_root =
            Controller::incept_single_from_seeds(&[60; 32], &[61; 32]).unwrap();
        let foreign_device = Controller::incept_device_single_from_seeds(
            &foreign_root.did(),
            &[62; 32],
            &[63; 32],
        )
        .unwrap();
        foreign_root
            .delegate_device(&foreign_device.did(), Capabilities::primary())
            .unwrap();
        let foreign_routing = AgreementSecretKey::from_seed(&[64; 32]).public_key();
        let foreign = PeerAdvertisement::issue(
            [8; 32],
            &foreign_root.did(),
            &foreign_device,
            foreign_routing,
            "10.0.3.1:9000".parse().unwrap(),
            1_000,
            2_000,
        )
        .unwrap();
        let mut freshness = FreshnessPins::new();
        let mut replay = ReplayCache::new(8).unwrap();
        let foreign = foreign
            .verify(
                [8; 32],
                1_500,
                &foreign_root.kel(),
                &foreign_device.kel(),
                &mut freshness,
                &mut replay,
            )
            .unwrap();
        assert_eq!(
            build_verified_onion_route(
                [
                    VerifiedRelay::new(&a, b"rendezvous"),
                    VerifiedRelay::new(&b, b"delivery"),
                    VerifiedRelay::new(&foreign, b"destination"),
                ],
                ConnectionId::from_bytes([1; 16]),
                PayloadSizeClass::Small,
                destination.public_key(),
                b"payload",
                1_500,
                10_000,
            ),
            Err(TransportSecurityError::WrongNetwork)
        );
    }

'''
text = read(runtime)
if text.count(insert_marker) != 1:
    raise SystemExit("runtime test insert marker mismatch")
write(runtime, text.replace(insert_marker, expiry_tests + insert_marker, 1))

# Direct connection expiry is checked before touching the network.
expired_connection_test = r'''
#[test]
fn expired_advertisement_is_rejected_before_dial() {
    let client = Identity::new(10);
    let server = Identity::new(40);
    let (listener, address) = listener();
    let advertisement = verified_advertisement(&server, address);
    drop(listener);
    let mut freshness = FreshnessPins::new();
    let mut replay = ReplayCache::new(32).unwrap();
    let result = connect_authenticated_tcp(
        client.local(),
        TransportPurpose::PeerExchange,
        1_000,
        2_000,
        2_001,
        AuthenticatedDialTarget::new(
            &advertisement,
            &server.root.kel(),
            &server.device.kel(),
        ),
        5_000,
        &mut freshness,
        &mut replay,
    );
    assert_eq!(result.unwrap_err(), TransportSecurityError::Expired);
    assert!(replay.is_empty());
}
'''
write(runtime_test, read(runtime_test).rstrip() + expired_connection_test + "\n")

replace_exact(
    verified_onion_test,
    """        PLAINTEXT,
        10_000,
""",
    """        PLAINTEXT,
        1_000,
        10_000,
""",
)
replace_exact(
    verified_onion_test,
    """        assert!(!contains(&opaque, PLAINTEXT));
        open_onion_destination(&opaque, &destination_secret).unwrap()
""",
    """        assert!(!contains(&opaque, PLAINTEXT));
        let mut replay = OnionReplayCache::new(32).unwrap();
        open_onion_destination(&opaque, &destination_secret, 5_000, &mut replay).unwrap()
""",
)

# ---------------------------------------------------------------------------
# Bound selection input before allocation/sort.
# ---------------------------------------------------------------------------
replace_exact(
    selection,
    """pub const MAX_SELECTED_PEERS: usize = 64;
pub const MIN_DIAL_TIMEOUT_MS: u64 = 100;
""",
    """/// Maximum verified records accepted by one selection call before any
/// allocation/sort. Larger local pools must be sampled or processed in bounded
/// batches by the caller.
pub const MAX_SELECTION_CANDIDATES: usize = 1_024;
pub const MAX_SELECTED_PEERS: usize = 64;
pub const MIN_DIAL_TIMEOUT_MS: u64 = 100;
""",
)
replace_exact(
    selection,
    """    let policy = policy.validate()?;
    let mut candidates: Vec<_> = records
""",
    """    let policy = policy.validate()?;
    if records.len() > MAX_SELECTION_CANDIDATES {
        return Err(TransportSecurityError::LimitExceeded);
    }
    let mut candidates: Vec<_> = records
""",
)
selection_test_marker = """    #[test]
    fn local_seed_is_part_of_the_selection_score() {
"""
selection_bound_test = r'''    #[test]
    fn candidate_input_is_bounded_before_sorting() {
        let record = verified(10, "10.0.0.1:9000");
        let oversized = vec![record; MAX_SELECTION_CANDIDATES + 1];
        assert_eq!(
            diverse_dial_plan(&oversized, [1; 32], PeerSelectionPolicy::default()),
            Err(TransportSecurityError::LimitExceeded)
        );
    }

'''
text = read(selection)
if text.count(selection_test_marker) != 1:
    raise SystemExit("selection test marker mismatch")
write(selection, text.replace(selection_test_marker, selection_bound_test + selection_test_marker, 1))
replace_exact(
    transport_lib,
    """    diverse_dial_plan, DialAttempt, PeerSelectionPolicy, MAX_DIAL_TIMEOUT_MS, MAX_SELECTED_PEERS,
    MIN_DIAL_TIMEOUT_MS,
""",
    """    diverse_dial_plan, DialAttempt, PeerSelectionPolicy, MAX_DIAL_TIMEOUT_MS,
    MAX_SELECTED_PEERS, MAX_SELECTION_CANDIDATES, MIN_DIAL_TIMEOUT_MS,
""",
)

# ---------------------------------------------------------------------------
# Search provenance: derive from a sealed authenticated connection and exact
# channel binding, not from a publicly constructible AuthenticatedPeer value.
# ---------------------------------------------------------------------------
replace_exact(
    query,
    """use mini_transport_security::{
    AuthenticatedConnection, AuthenticatedPeer, TransportPurpose, TransportSecurityError,
};
""",
    """use mini_transport_security::{
    AuthenticatedConnection, TransportPurpose, TransportSecurityError,
};
""",
)
replace_exact(
    query,
    """/// Derive a rotating search-provider pseudonym from an authenticated transport
/// endpoint. The endpoint id already commits to the delegated device and current
/// X25519 routing key, so key rotation also rotates this provider label.
pub fn authenticated_provider_pseudonym(peer: &AuthenticatedPeer) -> ProviderPseudonym {
    let mut transcript = Vec::with_capacity(AUTHENTICATED_PROVIDER_DOMAIN.len() + 32);
    transcript.extend_from_slice(AUTHENTICATED_PROVIDER_DOMAIN);
    transcript.extend_from_slice(&peer.endpoint_id.to_bytes());
    ProviderPseudonym(Multihash::of(HashAlgorithm::Blake3, &transcript))
}
""",
    """/// Derive a channel-scoped provider pseudonym from a sealed authenticated
/// connection. Binding both the verified endpoint and exact CH1 transcript
/// prevents a caller from manufacturing provenance from a freely constructed
/// `AuthenticatedPeer`, avoids cross-session tracking, and stays stable for
/// repeated queries on this one connection.
pub fn authenticated_provider_pseudonym<B: Bearer>(
    connection: &AuthenticatedConnection<B>,
) -> ProviderPseudonym {
    let mut transcript = Vec::with_capacity(AUTHENTICATED_PROVIDER_DOMAIN.len() + 64);
    transcript.extend_from_slice(AUTHENTICATED_PROVIDER_DOMAIN);
    transcript.extend_from_slice(&connection.peer().endpoint_id.to_bytes());
    transcript.extend_from_slice(&connection.channel_binding());
    ProviderPseudonym(Multihash::of(HashAlgorithm::Blake3, &transcript))
}
""",
)
replace_exact(
    query,
    """        provider: authenticated_provider_pseudonym(connection.peer()),
""",
    """        provider: authenticated_provider_pseudonym(connection),
""",
)
replace_exact(
    query_test,
    """    let expected_provider = authenticated_provider_pseudonym(connection.peer());
""",
    """    let expected_provider = authenticated_provider_pseudonym(&connection);
""",
)

# ---------------------------------------------------------------------------
# Truth sync: close stale D-0377/D-0436 status and record exact remaining floors.
# ---------------------------------------------------------------------------
replace_exact(
    decision,
    """**Implementation status:** complete in draft PR #292. Permanent code covers
""",
    """**Implementation status:** merged through PR #292. Permanent code covers
""",
)
replace_exact(
    decision,
    """real-socket tests plus one discovery/session integration test, and strict Clippy passed before truth sync; exact-head
workspace/governance/reproducibility/Android workflows remain the merge floor.
""",
    """real-socket tests plus one discovery/session integration test, and strict
Clippy passed at merge. PR #296 subsequently adds the executable runtime seam,
rechecks advertisement liveness at use time, and upgrades onion replay handling;
see D-0437.
""",
)
replace_exact(
    decision,
    """**Required follow-up:** bind `remote_provider` to `mini-transport-security`'s
authenticated peer identity once that crate lands review, closing the
caller-assertion gap named above. `remote_query_many`-style multi-provider
""",
    """**Required follow-up:** D-0437/PR #296 closes the named `remote_provider`
caller-assertion gap with an optional channel-authenticated, sealed result path;
the anonymous legacy API intentionally retains caller-owned labeling.
`remote_query_many`-style multi-provider
""",
)
replace_exact(
    decision,
    """Pairwise/routing-key rotation intentionally rotates
the F6 provider label; privacy-preserving durable continuity is undesigned.
""",
    """Every authenticated F6 connection receives a channel-scoped provider label;
privacy-preserving durable continuity across sessions is intentionally undesigned.
""",
)
replace_exact(
    decision,
    """rejection before initiator disclosure; atomic freshness/replay state on failure;
bounded retry past an unreachable first hint; reuse of a `mini-bridge` channel;
authenticated search-provider provenance; and wrong-purpose rejection. Focused
""",
    """rejection before initiator disclosure; atomic freshness/replay state on failure;
bounded retry past an unreachable first hint; reuse of a `mini-bridge` channel;
validity-window, fail-closed relay and destination replay state; advertisement
expiry/network rechecks; bounded selection input; channel-scoped authenticated
search-provider provenance; and wrong-purpose rejection. Focused
""",
)
replace_exact(
    planning,
    """| Relay role separation at route build | **PARTIAL** | Three verified records must differ by endpoint id, routing key, visible root, and device. | One hidden operator can control several valid pairwise roots, devices, prefixes, or ASNs. |
| Authenticated F6 provider labeling | **PASS for the named API** | Typed `SearchQuery` proof, private `AuthenticatedQueryResults` fields, endpoint-derived provider pseudonym, and sealed merge path. | Anonymous/legacy APIs intentionally retain caller-owned labeling; provider identity does not prove result truth. |
""",
    """| Relay role separation at route build | **PARTIAL** | Three live, same-network verified records must differ by endpoint id, routing key, visible root, and device. | One hidden operator can control several valid pairwise roots, devices, prefixes, or ASNs. |
| Relay and destination replay defense | **PASS in-process** | Onion v2 encrypts expiry/replay tokens for every relay and destination; validity-window entries are never evicted while live, malformed inner packets do not consume state, and capacity fails closed. | Hosts must persist equivalent state across restart; authenticated packet floods can still exhaust bounded capacity and require rate/resource controls. |
| Authenticated F6 provider labeling | **PASS for the named API** | Typed `SearchQuery` proof, private `AuthenticatedQueryResults` fields, channel-scoped endpoint+CH1 provider pseudonym, and sealed merge path. | Anonymous/legacy APIs intentionally retain caller-owned labeling; provider identity does not prove result truth or continuity across sessions. |
""",
)
replace_exact(
    planning,
    """- Named F6 proves endpoint control, not index honesty. Privacy-preserving
  continuity across rotating provider endpoints remains undesigned.
""",
    """- Named F6 proves endpoint control on one exact channel, not index honesty.
  Provider labels intentionally rotate across channels; privacy-preserving
  durable continuity remains undesigned.
""",
)
replace_exact(
    planning,
    """- `mini-relay` unit and real-socket onion tests pass unchanged.
""",
    """- `mini-relay` unit and real-socket onion tests pass after the onion-v2
  replay/lifetime upgrade, including destination replay, fail-closed capacity,
  expiry pruning, excessive-lifetime rejection, and malformed-state atomicity.
""",
)
replace_exact(
    status,
    """  `SearchQuery` purpose, derives a rotating provider pseudonym from the endpoint
  proved on the response channel, and seals the authenticated merge input behind
""",
    """  `SearchQuery` purpose, derives a channel-scoped provider pseudonym from the
  endpoint and exact CH1 binding proved on the response channel, and seals the authenticated merge input behind
""",
)
replace_exact(
    status,
    """  bridge operations; real camouflage adapters; ISP-throttling resistance;
  global timing/volume/intersection protection; and privacy-preserving
""",
    """  bridge operations; real camouflage adapters; ISP-throttling resistance;
  crash-persistent relay/destination replay state and flood controls; global
  timing/volume/intersection protection; and privacy-preserving
""",
)
replace_exact(
    threat,
    """| **One visible endpoint assigned multiple onion roles** | `build_verified_onion_route` rejects endpoint-id, routing-key, visible-root, or device reuse before Entry/Rendezvous/Delivery construction. | **Partial.** One hidden operator can control several pairwise roots, devices, addresses, or ASNs. |
""",
    """| **One visible endpoint assigned multiple onion roles** | `build_verified_onion_route` rechecks live same-network advertisements and rejects endpoint-id, routing-key, visible-root, or device reuse before Entry/Rendezvous/Delivery construction. | **Partial.** One hidden operator can control several pairwise roots, devices, addresses, or ASNs. |
| **Relay-cache eviction re-enabling replay** | Onion v2 stores `(token, expiry)` through the encrypted validity window, prunes only expired entries, and fails closed at capacity. Recording occurs only after the whole local inner structure validates. | **Closed in-process.** Restart persistence and flood/rate controls remain host responsibilities. |
| **Post-delivery envelope replay** | Onion v2 puts a separate expiry and replay token inside destination encryption; `open_onion_destination` requires time and replay state. | **Closed in-process.** A destination that discards replay state on restart loses that guarantee. |
""",
)
replace_exact(
    threat,
    """| **Forged F6 provider label after authenticated query** | `SearchQuery`-purpose `AuthenticatedConnection`, endpoint-derived provider pseudonym, private `AuthenticatedQueryResults` fields, and `merge_authenticated_remote_results`. | **Closed for the named API.** Anonymous/legacy merge is intentionally caller-labeled; endpoint control does not prove result truth. |
""",
    """| **Forged F6 provider label after authenticated query** | `SearchQuery`-purpose `AuthenticatedConnection`, channel-scoped endpoint+CH1 provider pseudonym, private `AuthenticatedQueryResults` fields, and `merge_authenticated_remote_results`. | **Closed for the named API.** Anonymous/legacy merge is intentionally caller-labeled; endpoint control does not prove result truth or cross-session continuity. |
""",
)
replace_exact(
    f6_design,
    """- `authenticated_provider_pseudonym` domain-separates and hashes the verified
  `TransportEndpointId`. Because that endpoint id commits to the delegated device
  and current X25519 routing key, routing-key rotation also rotates the search
  provider label rather than creating a permanent global identifier.
""",
    """- `authenticated_provider_pseudonym` accepts the sealed connection, then
  domain-separates and hashes both its verified `TransportEndpointId` and exact
  CH1 binding. The label is stable for repeated queries on that connection but
  rotates across channels, preventing the named API from becoming a permanent
  cross-session tracking identifier.
""",
)
replace_exact(
    f6_design,
    """**Exact remaining failure:** endpoint-bound provenance proves who controlled one
transport endpoint for one session, not that the provider's index is honest or
independently operated. Pairwise/routing-key rotation intentionally changes the
provider label, so durable cross-rotation reputation requires a separate,
privacy-conscious continuity design. The anonymous legacy path can still be
""",
    """**Exact remaining failure:** endpoint-and-channel-bound provenance proves who
controlled one transport endpoint for one session, not that the provider's
index is honest or independently operated. Every new channel intentionally
changes the provider label, so durable reputation requires a separate,
privacy-conscious continuity design. The anonymous legacy path can still be
""",
)
replace_exact(
    transport_readme,
    """- `build_verified_onion_route` accepts three already-verified endpoints and
  rejects visible endpoint, routing-key, root, or device reuse before building
""",
    """- `build_verified_onion_route` accepts three live same-network verified endpoints and
  rejects visible endpoint, routing-key, root, or device reuse before building
""",
)
replace_exact(
    transport_readme,
    """- `diverse_dial_plan` is locally seeded, input-order-independent, duplicate-
  resistant, and capped per IPv4 `/24` or IPv6 `/48` prefix.
""",
    """- `diverse_dial_plan` is locally seeded, input-order-independent, duplicate-
  resistant, capped per IPv4 `/24` or IPv6 `/48` prefix, and rejects more than
  1,024 candidates before allocation/sort.
""",
)
replace_exact(
    transport_readme,
    """- The three-hop onion implementation lives in `mini-relay`; it protects payload
  confidentiality and separates endpoint knowledge, but is not Sphinx and does
  not defeat a global timing/volume observer.
""",
    """- The onion-v2 implementation in `mini-relay` protects payload confidentiality,
  separates endpoint knowledge, bounds remaining lifetime, and requires
  fail-closed relay/destination replay state. It is not Sphinx and does not
  defeat a global timing/volume observer; crash persistence and flood controls
  remain deployment responsibilities.
""",
)

print("stage 5 applied")
