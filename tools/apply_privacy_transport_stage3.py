#!/usr/bin/env python3
"""Close replay-eviction and unbounded-validity gaps in PR #292.

Temporary branch-local helper. The push verifier deletes it in the same tested
commit that carries the permanent source and documentation changes.
"""

from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old!r}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


def replace_count(path: str, old: str, new: str, expected: int) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != expected:
        raise SystemExit(f"{path}: expected {expected} matches, found {count}: {old!r}")
    target.write_text(text.replace(old, new), encoding="utf-8")


# Transport claim/advertisement replay entries remain until signed expiry.
replace_once(
    "crates/mini-transport-security/src/auth.rs",
    "        replay.check_and_record(self.replay_id(channel_binding))?;\n",
    """        replay.check_and_record(
            self.replay_id(channel_binding),
            self.expires_at_ms,
            now_ms,
        )?;
""",
)
replace_once(
    "crates/mini-transport-security/src/advertisement.rs",
    "        replay.check_and_record(self.replay_id())?;\n",
    "        replay.check_and_record(self.replay_id(), self.expires_at_ms, now_ms)?;\n",
)

# Onion cache: no eviction of still-valid tokens. Every hop and destination
# envelope gains an issued/expiry window with a hard maximum.
replace_once(
    "crates/mini-relay/src/onion.rs",
    "use std::collections::{HashSet, VecDeque};\n",
    "use std::collections::{HashMap, HashSet};\n",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """pub const MAX_ONION_REPLAY_ENTRIES: usize = 65_536;
pub const SMALL_ONION_PAYLOAD_BYTES: usize = 4 * 1024;
""",
    """pub const MAX_ONION_REPLAY_ENTRIES: usize = 65_536;
pub const MAX_ONION_LIFETIME_MS: u64 = 10 * 60 * 1000;
pub const MAX_ONION_CLOCK_SKEW_MS: u64 = 30 * 1000;
pub const SMALL_ONION_PAYLOAD_BYTES: usize = 4 * 1024;
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """#[derive(Debug, Clone)]
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
""",
    """#[derive(Debug, Clone)]
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
        self.seen.retain(|_, expires_at_ms| *expires_at_ms >= now_ms);
    }

    /// Record a per-hop or destination token until its authenticated expiry.
    /// A full cache fails closed: no still-valid token is evicted to make a
    /// replay acceptable again.
    pub fn check_and_record(
        &mut self,
        token: [u8; 32],
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<()> {
        if expires_at_ms < now_ms {
            return Err(RelayError::OnionExpired);
        }
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
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """    destination_key: AgreementPublicKey,
    plaintext: &[u8],
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
    """    destination_key: AgreementPublicKey,
    plaintext: &[u8],
    issued_at_ms: u64,
    expires_at_ms: u64,
) -> Result<OnionPacket> {
    validate_route(hops)?;
    validate_onion_window(issued_at_ms, expires_at_ms, issued_at_ms)?;

    let destination = DestinationEnvelope::seal(
        destination_connection_id,
        size_class,
        destination_key,
        plaintext,
        issued_at_ms,
        expires_at_ms,
    )?;
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """        let hop_plaintext =
            encode_hop_plaintext(hop.role, expires_at_ms, replay_token, &hop.next_hop, &inner)?;
""",
    """        let hop_plaintext = encode_hop_plaintext(
            hop.role,
            issued_at_ms,
            expires_at_ms,
            replay_token,
            &hop.next_hop,
            &inner,
        )?;
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """        if now_ms > decoded.expires_at_ms {
            return Err(RelayError::OnionExpired);
        }
        replay.check_and_record(decoded.replay_token)?;
""",
    """        validate_onion_window(decoded.issued_at_ms, decoded.expires_at_ms, now_ms)?;
        replay.check_and_record(decoded.replay_token, decoded.expires_at_ms, now_ms)?;
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """pub fn open_onion_destination(
    opaque_destination_envelope: &[u8],
    destination_secret: &AgreementSecretKey,
) -> Result<Vec<u8>> {
    DestinationEnvelope::from_bytes(opaque_destination_envelope)?.open(destination_secret)
}
""",
    """pub fn open_onion_destination(
    opaque_destination_envelope: &[u8],
    destination_secret: &AgreementSecretKey,
    now_ms: u64,
    replay: &mut OnionReplayCache,
) -> Result<Vec<u8>> {
    DestinationEnvelope::from_bytes(opaque_destination_envelope)?
        .open(destination_secret, now_ms, replay)
}
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """struct DestinationEnvelope {
    connection_id: ConnectionId,
    size_class: PayloadSizeClass,
    ephemeral_key: AgreementPublicKey,
    nonce: AeadNonce,
    ciphertext: Vec<u8>,
}
""",
    """struct DestinationEnvelope {
    connection_id: ConnectionId,
    size_class: PayloadSizeClass,
    issued_at_ms: u64,
    expires_at_ms: u64,
    replay_token: [u8; 32],
    ephemeral_key: AgreementPublicKey,
    nonce: AeadNonce,
    ciphertext: Vec<u8>,
}
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """        destination_key: AgreementPublicKey,
        plaintext: &[u8],
    ) -> Result<Self> {
        let frame = encode_fixed_payload(size_class, plaintext)?;
        let ephemeral_secret = AgreementSecretKey::generate()?;
        let ephemeral_key = ephemeral_secret.public_key();
        let shared = ephemeral_secret.agree(&destination_key)?;
        let nonce = AeadNonce::generate()?;
        let aad = destination_aad(connection_id, size_class, ephemeral_key, nonce);
        let key = derive_key(DESTINATION_KEY_DOMAIN, &shared.to_bytes(), &aad)?;
        let ciphertext = key.encrypt(&nonce, &frame, &aad)?;
        Ok(Self {
            connection_id,
            size_class,
            ephemeral_key,
            nonce,
            ciphertext,
        })
    }

    fn open(&self, destination_secret: &AgreementSecretKey) -> Result<Vec<u8>> {
        let shared = destination_secret.agree(&self.ephemeral_key)?;
        let aad = destination_aad(
            self.connection_id,
            self.size_class,
            self.ephemeral_key,
            self.nonce,
        );
        let key = derive_key(DESTINATION_KEY_DOMAIN, &shared.to_bytes(), &aad)?;
        let frame = key.decrypt(&self.nonce, &self.ciphertext, &aad)?;
        decode_fixed_payload(self.size_class, &frame)
    }
""",
    """        destination_key: AgreementPublicKey,
        plaintext: &[u8],
        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> Result<Self> {
        validate_onion_window(issued_at_ms, expires_at_ms, issued_at_ms)?;
        let frame = encode_fixed_payload(size_class, plaintext)?;
        let ephemeral_secret = AgreementSecretKey::generate()?;
        let ephemeral_key = ephemeral_secret.public_key();
        let shared = ephemeral_secret.agree(&destination_key)?;
        let nonce = AeadNonce::generate()?;
        let replay_token = random_32()?;
        let aad = destination_aad(
            connection_id,
            size_class,
            issued_at_ms,
            expires_at_ms,
            replay_token,
            ephemeral_key,
            nonce,
        );
        let key = derive_key(DESTINATION_KEY_DOMAIN, &shared.to_bytes(), &aad)?;
        let ciphertext = key.encrypt(&nonce, &frame, &aad)?;
        Ok(Self {
            connection_id,
            size_class,
            issued_at_ms,
            expires_at_ms,
            replay_token,
            ephemeral_key,
            nonce,
            ciphertext,
        })
    }

    fn open(
        &self,
        destination_secret: &AgreementSecretKey,
        now_ms: u64,
        replay: &mut OnionReplayCache,
    ) -> Result<Vec<u8>> {
        validate_onion_window(self.issued_at_ms, self.expires_at_ms, now_ms)?;
        let shared = destination_secret.agree(&self.ephemeral_key)?;
        let aad = destination_aad(
            self.connection_id,
            self.size_class,
            self.issued_at_ms,
            self.expires_at_ms,
            self.replay_token,
            self.ephemeral_key,
            self.nonce,
        );
        let key = derive_key(DESTINATION_KEY_DOMAIN, &shared.to_bytes(), &aad)?;
        let frame = key.decrypt(&self.nonce, &self.ciphertext, &aad)?;
        let plaintext = decode_fixed_payload(self.size_class, &frame)?;
        replay.check_and_record(self.replay_token, self.expires_at_ms, now_ms)?;
        Ok(plaintext)
    }
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """        writer.raw(&self.connection_id.to_bytes());
        writer.u8(size_class_tag(self.size_class));
        writer.u8(self.ephemeral_key.suite().tag());
""",
    """        writer.raw(&self.connection_id.to_bytes());
        writer.u8(size_class_tag(self.size_class));
        writer.u64(self.issued_at_ms);
        writer.u64(self.expires_at_ms);
        writer.raw(&self.replay_token);
        writer.u8(self.ephemeral_key.suite().tag());
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """        let size_class = size_class_from_tag(reader.u8()?)?;
        let agreement_suite = KeyAgreementSuite::from_tag(reader.u8()?)?;
""",
    """        let size_class = size_class_from_tag(reader.u8()?)?;
        let issued_at_ms = reader.u64()?;
        let expires_at_ms = reader.u64()?;
        let replay_token: [u8; 32] = reader
            .raw(32)?
            .try_into()
            .map_err(|_| RelayError::Truncated)?;
        let agreement_suite = KeyAgreementSuite::from_tag(reader.u8()?)?;
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """        let envelope = Self {
            connection_id,
            size_class,
            ephemeral_key,
            nonce,
            ciphertext,
        };
""",
    """        let envelope = Self {
            connection_id,
            size_class,
            issued_at_ms,
            expires_at_ms,
            replay_token,
            ephemeral_key,
            nonce,
            ciphertext,
        };
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """struct HopPlaintext {
    role: RelayRole,
    expires_at_ms: u64,
""",
    """struct HopPlaintext {
    role: RelayRole,
    issued_at_ms: u64,
    expires_at_ms: u64,
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """fn encode_hop_plaintext(
    role: RelayRole,
    expires_at_ms: u64,
""",
    """fn encode_hop_plaintext(
    role: RelayRole,
    issued_at_ms: u64,
    expires_at_ms: u64,
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """    writer.u8(role.tag());
    writer.u64(expires_at_ms);
""",
    """    writer.u8(role.tag());
    writer.u64(issued_at_ms);
    writer.u64(expires_at_ms);
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """    let role = RelayRole::from_tag(reader.u8()?)?;
    let expires_at_ms = reader.u64()?;
""",
    """    let role = RelayRole::from_tag(reader.u8()?)?;
    let issued_at_ms = reader.u64()?;
    let expires_at_ms = reader.u64()?;
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """    Ok(HopPlaintext {
        role,
        expires_at_ms,
""",
    """    Ok(HopPlaintext {
        role,
        issued_at_ms,
        expires_at_ms,
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """fn validate_route(hops: &[OnionHop]) -> Result<()> {
""",
    """fn validate_onion_window(issued_at_ms: u64, expires_at_ms: u64, now_ms: u64) -> Result<()> {
    let lifetime = expires_at_ms
        .checked_sub(issued_at_ms)
        .ok_or(RelayError::InvalidOnionRoute)?;
    if lifetime == 0 || lifetime > MAX_ONION_LIFETIME_MS {
        return Err(RelayError::OnionLifetimeTooLong);
    }
    if issued_at_ms > now_ms.saturating_add(MAX_ONION_CLOCK_SKEW_MS) {
        return Err(RelayError::OnionNotYetValid);
    }
    if now_ms > expires_at_ms {
        return Err(RelayError::OnionExpired);
    }
    Ok(())
}

fn validate_route(hops: &[OnionHop]) -> Result<()> {
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """fn destination_aad(
    connection_id: ConnectionId,
    size_class: PayloadSizeClass,
    ephemeral_key: AgreementPublicKey,
    nonce: AeadNonce,
) -> Vec<u8> {
""",
    """fn destination_aad(
    connection_id: ConnectionId,
    size_class: PayloadSizeClass,
    issued_at_ms: u64,
    expires_at_ms: u64,
    replay_token: [u8; 32],
    ephemeral_key: AgreementPublicKey,
    nonce: AeadNonce,
) -> Vec<u8> {
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """    writer.raw(&connection_id.to_bytes());
    writer.u8(size_class_tag(size_class));
    writer.u8(ephemeral_key.suite().tag());
""",
    """    writer.raw(&connection_id.to_bytes());
    writer.u8(size_class_tag(size_class));
    writer.u64(issued_at_ms);
    writer.u64(expires_at_ms);
    writer.raw(&replay_token);
    writer.u8(ephemeral_key.suite().tag());
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """    let destination = 1usize
        .checked_add(16 + 1 + 1 + 32 + 12 + 4)
""",
    """    let destination = 1usize
        .checked_add(16 + 1 + 8 + 8 + 32 + 1 + 32 + 12 + 4)
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    "    let hop_plaintext_overhead: usize = 1 + 8 + 32 + 4 + NEXT_HOP_PAD_BYTES + 4;\n",
    "    let hop_plaintext_overhead: usize = 1 + 8 + 8 + 32 + 4 + NEXT_HOP_PAD_BYTES + 4;\n",
)

# Public error surface and exports.
replace_once(
    "crates/mini-relay/src/error.rs",
    """    /// The onion packet's signed/encrypted validity window ended.
    OnionExpired,
    /// A per-hop replay token was already processed.
""",
    """    /// The onion packet's authenticated validity window has not started.
    OnionNotYetValid,
    /// The onion validity window is empty or exceeds the hard maximum.
    OnionLifetimeTooLong,
    /// The onion packet's authenticated validity window ended.
    OnionExpired,
    /// A per-hop or destination replay token was already processed.
""",
)
replace_once(
    "crates/mini-relay/src/error.rs",
    """            RelayError::WrongOnionHop => write!(f, "onion packet belongs to another hop"),
            RelayError::OnionExpired => write!(f, "onion packet has expired"),
""",
    """            RelayError::WrongOnionHop => write!(f, "onion packet belongs to another hop"),
            RelayError::OnionNotYetValid => write!(f, "onion packet is not valid yet"),
            RelayError::OnionLifetimeTooLong => {
                write!(f, "onion validity window exceeds the hard maximum")
            }
            RelayError::OnionExpired => write!(f, "onion packet has expired"),
""",
)
replace_once(
    "crates/mini-relay/src/lib.rs",
    """    MAX_ONION_NEXT_HOP_BYTES, MAX_ONION_REPLAY_ENTRIES, MEDIUM_ONION_PAYLOAD_BYTES,
    ONION_HOP_COUNT, ONION_VERSION, SMALL_ONION_PAYLOAD_BYTES,
""",
    """    MAX_ONION_CLOCK_SKEW_MS, MAX_ONION_LIFETIME_MS, MAX_ONION_NEXT_HOP_BYTES,
    MAX_ONION_REPLAY_ENTRIES, MEDIUM_ONION_PAYLOAD_BYTES, ONION_HOP_COUNT,
    ONION_VERSION, SMALL_ONION_PAYLOAD_BYTES,
""",
)

# Update every build call in unit tests and the real TCP test with issued_at.
replace_count(
    "crates/mini-relay/src/onion.rs",
    """            10_000,
        )""",
    """            1_000,
            10_000,
        )""",
    6,
)
replace_once(
    "crates/mini-relay/tests/onion_tcp.rs",
    """        b"private over three real sockets",
        10_000,
    )
""",
    """        b"private over three real sockets",
        1_000,
        10_000,
    )
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """        let opened = open_onion_destination(&destination_envelope.unwrap(), &destination).unwrap();
        assert_eq!(opened, b"private application payload");
""",
    """        let destination_envelope = destination_envelope.unwrap();
        let mut destination_replay = OnionReplayCache::new(8).unwrap();
        let opened = open_onion_destination(
            &destination_envelope,
            &destination,
            5_000,
            &mut destination_replay,
        )
        .unwrap();
        assert_eq!(opened, b"private application payload");
        assert_eq!(
            open_onion_destination(
                &destination_envelope,
                &destination,
                5_000,
                &mut destination_replay,
            ),
            Err(RelayError::OnionReplay)
        );
""",
)
replace_once(
    "crates/mini-relay/tests/onion_tcp.rs",
    """        open_onion_destination(&opaque, &destination_secret).unwrap()
""",
    """        let mut replay = OnionReplayCache::new(32).unwrap();
        open_onion_destination(&opaque, &destination_secret, 5_000, &mut replay).unwrap()
""",
)

# Add one focused unit test for maximum lifetime, future windows, and
# fail-closed replay-cache capacity.
replace_once(
    "crates/mini-relay/src/onion.rs",
    """    #[test]
    fn packet_round_trip_is_canonical_and_bounded() {
""",
    """    #[test]
    fn validity_windows_and_replay_capacity_fail_closed() {
        let (hops, secrets) = route();
        let destination = AgreementSecretKey::from_seed(&[9; 32]);
        assert_eq!(
            build_onion(
                ConnectionId::from_bytes([7; 16]),
                PayloadSizeClass::Small,
                &hops,
                destination.public_key(),
                b"payload",
                1_000,
                1_000 + MAX_ONION_LIFETIME_MS + 1,
            ),
            Err(RelayError::OnionLifetimeTooLong)
        );

        let future = build_onion(
            ConnectionId::from_bytes([7; 16]),
            PayloadSizeClass::Small,
            &hops,
            destination.public_key(),
            b"payload",
            100_000,
            101_000,
        )
        .unwrap();
        let mut future_replay = OnionReplayCache::new(8).unwrap();
        assert_eq!(
            future.peel(&secrets[0], 1_000, &mut future_replay),
            Err(RelayError::OnionNotYetValid)
        );

        let first = build_onion(
            ConnectionId::from_bytes([8; 16]),
            PayloadSizeClass::Small,
            &hops,
            destination.public_key(),
            b"first",
            1_000,
            10_000,
        )
        .unwrap();
        let second = build_onion(
            ConnectionId::from_bytes([9; 16]),
            PayloadSizeClass::Small,
            &hops,
            destination.public_key(),
            b"second",
            1_000,
            10_000,
        )
        .unwrap();
        let mut one_slot = OnionReplayCache::new(1).unwrap();
        first.peel(&secrets[0], 5_000, &mut one_slot).unwrap();
        assert_eq!(
            second.peel(&secrets[0], 5_000, &mut one_slot),
            Err(RelayError::LimitExceeded)
        );
        assert_eq!(one_slot.len(), 1);
    }

    #[test]
    fn packet_round_trip_is_canonical_and_bounded() {
""",
)

# Truth-sync the strengthened semantics and correct the destination-id wording.
replace_once(
    "docs/DECISION_LOG.md",
    "Focused formatting, 64 unit tests, three\nreal-socket tests plus one discovery/session integration test, and strict Clippy",
    "Focused formatting, 66 unit tests, three\nreal-socket tests plus one discovery/session integration test, and strict Clippy",
)
replace_once(
    "docs/THREAT_MODEL.md",
    "Every relay layer has an independent random public connection id; the destination id exists only inside destination encryption.",
    "Every relay layer has an independent random public connection id; the destination-envelope id is hidden from earlier relays by the delivery layer and differs from the delivery hop's public id.",
)
replace_once(
    "docs/THREAT_MODEL.md",
    "**Closed for explicit circuit ids.** Timing/volume correlation remains open.",
    "**Closed for one shared clear circuit id.** The delivery relay sees the distinct destination-envelope id; timing, packet length, and volume correlation remain open.",
)
replace_once(
    "docs/planning/privacy-transport-security.md",
    "per-hop expiry/replay checks, fixed-size destination payloads, and destination-only decryption.",
    "hard-capped issued/expiry windows, fail-closed per-hop and destination replay caches, fixed-size destination payloads, and destination-only decryption.",
)
replace_once(
    "docs/STATUS.md",
    "padded opaque routing tokens, per-hop expiry/replay checks, fixed-size\n  destination-encrypted payloads",
    "padded opaque routing tokens, hard-capped validity windows, fail-closed\n  per-hop and destination replay checks, fixed-size destination-encrypted payloads",
)
replace_once(
    "crates/mini-transport-security/README.md",
    "bounded validity windows, and a\n  bounded replay cache.",
    "bounded validity windows, and a replay cache that retains every accepted id until signed expiry and fails closed at capacity.",
)

print("applied PR #292 stage-three replay and lifetime hardening")
