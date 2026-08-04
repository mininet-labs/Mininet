//! Three-hop layered onion transport for `PrivacyTier::Relayed`.
//!
//! This is a compact Mininet onion format, not Sphinx and not a mixnet. Every
//! route has exactly `Entry -> Rendezvous -> Delivery`; each hop uses an
//! independent ephemeral X25519 agreement and ChaCha20-Poly1305 layer. The
//! delivery hop receives only a destination-encrypted fixed-size payload.
//!
//! Therefore no relay receives application plaintext or both endpoint
//! identities. Timing/volume/intersection resistance remains the externally
//! reviewed Mixed-tier Sphinx/Loopix executor's job.

use std::collections::{HashSet, VecDeque};

use mini_crypto::{
    random_32, AeadKey, AeadNonce, AeadSuite, AgreementPublicKey, AgreementSecretKey, KdfSuite,
    KeyAgreementSuite,
};
use mini_transport_policy::PayloadSizeClass;

use crate::codec::{Reader, Writer};
use crate::connection::ConnectionId;
use crate::error::{RelayError, Result};
use crate::role::RelayRole;

pub const ONION_VERSION: u8 = 1;
pub const ONION_HOP_COUNT: usize = 3;
pub const MAX_ONION_NEXT_HOP_BYTES: usize = 256;
pub const MAX_ONION_REPLAY_ENTRIES: usize = 65_536;
pub const SMALL_ONION_PAYLOAD_BYTES: usize = 4 * 1024;
pub const MEDIUM_ONION_PAYLOAD_BYTES: usize = 64 * 1024;
pub const LARGE_ONION_PAYLOAD_BYTES: usize = 1024 * 1024;

const HOP_KEY_DOMAIN: &[u8] = b"mini-relay/onion-hop-key/v1";
const DESTINATION_KEY_DOMAIN: &[u8] = b"mini-relay/onion-destination-key/v1";
const NEXT_HOP_PAD_BYTES: usize = MAX_ONION_NEXT_HOP_BYTES;
const AEAD_TAG_BYTES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnionHop {
    pub role: RelayRole,
    pub routing_key: AgreementPublicKey,
    /// Opaque token interpreted only by this relay to reach the next hop.
    pub next_hop: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnionPacket {
    pub connection_id: ConnectionId,
    pub size_class: PayloadSizeClass,
    pub hop_index: u8,
    ephemeral_key: AgreementPublicKey,
    nonce: AeadNonce,
    ciphertext: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnionForward {
    Next(OnionPacket),
    /// Destination envelope still encrypted to the destination routing key.
    Destination(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeeledOnion {
    pub role: RelayRole,
    pub connection_id: ConnectionId,
    pub size_class: PayloadSizeClass,
    pub next_hop: Vec<u8>,
    pub forward: OnionForward,
}

#[derive(Debug, Clone)]
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

/// Build one fixed-role onion circuit. `destination_key` belongs to the final
/// endpoint, not the delivery relay. Every hop key should come from a verified,
/// signed peer advertisement. `destination_connection_id` is visible only in
/// the destination-encrypted envelope; each relay layer receives an independent
/// random public connection id so observers cannot correlate hops by one shared
/// cleartext circuit identifier.
pub fn build_onion(
    destination_connection_id: ConnectionId,
    size_class: PayloadSizeClass,
    hops: &[OnionHop],
    destination_key: AgreementPublicKey,
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
    let mut inner = destination.to_bytes()?;
    let mut public_connection_ids = HashSet::with_capacity(ONION_HOP_COUNT + 1);
    public_connection_ids.insert(destination_connection_id);

    for (index, hop) in hops.iter().enumerate().rev() {
        let connection_id = ConnectionId::generate()?;
        if !public_connection_ids.insert(connection_id) {
            return Err(RelayError::InvalidOnionRoute);
        }
        let ephemeral_secret = AgreementSecretKey::generate()?;
        let ephemeral_key = ephemeral_secret.public_key();
        let shared = ephemeral_secret.agree(&hop.routing_key)?;
        let nonce = AeadNonce::generate()?;
        let replay_token = random_32()?;
        let hop_plaintext =
            encode_hop_plaintext(hop.role, expires_at_ms, replay_token, &hop.next_hop, &inner)?;
        let hop_index = u8::try_from(index).map_err(|_| RelayError::InvalidOnionRoute)?;
        let aad = hop_aad(connection_id, size_class, hop_index, ephemeral_key, nonce);
        let key = derive_key(HOP_KEY_DOMAIN, &shared.to_bytes(), &aad)?;
        let ciphertext = key.encrypt(&nonce, &hop_plaintext, &aad)?;
        let packet = OnionPacket {
            connection_id,
            size_class,
            hop_index,
            ephemeral_key,
            nonce,
            ciphertext,
        };
        inner = packet.to_bytes()?;
    }
    OnionPacket::from_bytes(&inner)
}

impl OnionPacket {
    pub fn peel(
        &self,
        relay_secret: &AgreementSecretKey,
        now_ms: u64,
        replay: &mut OnionReplayCache,
    ) -> Result<PeeledOnion> {
        if self.hop_index as usize >= ONION_HOP_COUNT {
            return Err(RelayError::WrongOnionHop);
        }
        let shared = relay_secret.agree(&self.ephemeral_key)?;
        let aad = hop_aad(
            self.connection_id,
            self.size_class,
            self.hop_index,
            self.ephemeral_key,
            self.nonce,
        );
        let key = derive_key(HOP_KEY_DOMAIN, &shared.to_bytes(), &aad)?;
        let plaintext = key.decrypt(&self.nonce, &self.ciphertext, &aad)?;
        let decoded = decode_hop_plaintext(&plaintext)?;
        let expected_role = route_role(self.hop_index)?;
        if decoded.role != expected_role {
            return Err(RelayError::WrongOnionHop);
        }
        if now_ms > decoded.expires_at_ms {
            return Err(RelayError::OnionExpired);
        }
        replay.check_and_record(decoded.replay_token)?;

        let forward = if self.hop_index as usize + 1 < ONION_HOP_COUNT {
            let next = OnionPacket::from_bytes(&decoded.inner)?;
            if next.connection_id == self.connection_id
                || next.size_class != self.size_class
                || next.hop_index != self.hop_index + 1
            {
                return Err(RelayError::WrongOnionHop);
            }
            OnionForward::Next(next)
        } else {
            // Validate the destination envelope's public binding now, but do
            // not decrypt it: the delivery relay does not hold that key.
            let destination = DestinationEnvelope::from_bytes(&decoded.inner)?;
            if destination.connection_id == self.connection_id
                || destination.size_class != self.size_class
            {
                return Err(RelayError::OnionDestinationMismatch);
            }
            OnionForward::Destination(decoded.inner)
        };

        Ok(PeeledOnion {
            role: decoded.role,
            connection_id: self.connection_id,
            size_class: self.size_class,
            next_hop: decoded.next_hop,
            forward,
        })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut writer = Writer::new();
        writer.u8(ONION_VERSION);
        writer.raw(&self.connection_id.to_bytes());
        writer.u8(size_class_tag(self.size_class));
        writer.u8(self.hop_index);
        writer.u8(ONION_HOP_COUNT as u8);
        writer.u8(self.ephemeral_key.suite().tag());
        writer.raw(&self.ephemeral_key.to_bytes());
        writer.raw(self.nonce.as_bytes());
        writer.bytes(&self.ciphertext);
        let bytes = writer.into_bytes();
        if bytes.len() > mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES {
            return Err(RelayError::LimitExceeded);
        }
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES {
            return Err(RelayError::LimitExceeded);
        }
        let mut reader = Reader::new(bytes);
        if reader.u8()? != ONION_VERSION {
            return Err(RelayError::UnsupportedOnionVersion);
        }
        let connection_id = ConnectionId::from_bytes(
            reader
                .raw(16)?
                .try_into()
                .map_err(|_| RelayError::Truncated)?,
        );
        let size_class = size_class_from_tag(reader.u8()?)?;
        let hop_index = reader.u8()?;
        if reader.u8()? as usize != ONION_HOP_COUNT || hop_index as usize >= ONION_HOP_COUNT {
            return Err(RelayError::InvalidOnionRoute);
        }
        let agreement_suite = KeyAgreementSuite::from_tag(reader.u8()?)?;
        let ephemeral_key = AgreementPublicKey::from_suite_bytes(
            agreement_suite,
            reader.raw(agreement_suite.public_key_len())?,
        )?;
        let nonce = AeadNonce::from_bytes(reader.raw(AeadSuite::DEFAULT.nonce_len())?)?;
        let ciphertext =
            reader.bytes_limited(max_onion_ciphertext_bytes(size_class, hop_index)?)?;
        if !reader.finished() {
            return Err(RelayError::TrailingBytes);
        }
        let packet = Self {
            connection_id,
            size_class,
            hop_index,
            ephemeral_key,
            nonce,
            ciphertext,
        };
        if packet.to_bytes()?.as_slice() != bytes {
            return Err(RelayError::InvalidOnionRoute);
        }
        Ok(packet)
    }
}

/// Open the destination-only envelope after the delivery relay forwards it.
pub fn open_onion_destination(
    opaque_destination_envelope: &[u8],
    destination_secret: &AgreementSecretKey,
) -> Result<Vec<u8>> {
    DestinationEnvelope::from_bytes(opaque_destination_envelope)?.open(destination_secret)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DestinationEnvelope {
    connection_id: ConnectionId,
    size_class: PayloadSizeClass,
    ephemeral_key: AgreementPublicKey,
    nonce: AeadNonce,
    ciphertext: Vec<u8>,
}

impl DestinationEnvelope {
    fn seal(
        connection_id: ConnectionId,
        size_class: PayloadSizeClass,
        destination_key: AgreementPublicKey,
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

    fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut writer = Writer::new();
        writer.u8(ONION_VERSION);
        writer.raw(&self.connection_id.to_bytes());
        writer.u8(size_class_tag(self.size_class));
        writer.u8(self.ephemeral_key.suite().tag());
        writer.raw(&self.ephemeral_key.to_bytes());
        writer.raw(self.nonce.as_bytes());
        writer.bytes(&self.ciphertext);
        Ok(writer.into_bytes())
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        if reader.u8()? != ONION_VERSION {
            return Err(RelayError::UnsupportedOnionVersion);
        }
        let connection_id = ConnectionId::from_bytes(
            reader
                .raw(16)?
                .try_into()
                .map_err(|_| RelayError::Truncated)?,
        );
        let size_class = size_class_from_tag(reader.u8()?)?;
        let agreement_suite = KeyAgreementSuite::from_tag(reader.u8()?)?;
        let ephemeral_key = AgreementPublicKey::from_suite_bytes(
            agreement_suite,
            reader.raw(agreement_suite.public_key_len())?,
        )?;
        let nonce = AeadNonce::from_bytes(reader.raw(AeadSuite::DEFAULT.nonce_len())?)?;
        let expected = fixed_payload_bytes(size_class)
            .checked_add(AEAD_TAG_BYTES)
            .ok_or(RelayError::LimitExceeded)?;
        let ciphertext = reader.bytes_limited(expected)?;
        if ciphertext.len() != expected || !reader.finished() {
            return Err(RelayError::InvalidOnionRoute);
        }
        let envelope = Self {
            connection_id,
            size_class,
            ephemeral_key,
            nonce,
            ciphertext,
        };
        if envelope.to_bytes()?.as_slice() != bytes {
            return Err(RelayError::InvalidOnionRoute);
        }
        Ok(envelope)
    }
}

#[derive(Debug)]
struct HopPlaintext {
    role: RelayRole,
    expires_at_ms: u64,
    replay_token: [u8; 32],
    next_hop: Vec<u8>,
    inner: Vec<u8>,
}

fn encode_hop_plaintext(
    role: RelayRole,
    expires_at_ms: u64,
    replay_token: [u8; 32],
    next_hop: &[u8],
    inner: &[u8],
) -> Result<Vec<u8>> {
    if next_hop.is_empty() || next_hop.len() > MAX_ONION_NEXT_HOP_BYTES {
        return Err(RelayError::InvalidOnionRoute);
    }
    let mut writer = Writer::new();
    writer.u8(role.tag());
    writer.u64(expires_at_ms);
    writer.raw(&replay_token);
    writer.u32(u32::try_from(next_hop.len()).map_err(|_| RelayError::LimitExceeded)?);
    writer.raw(next_hop);
    writer.raw(&vec![0u8; NEXT_HOP_PAD_BYTES - next_hop.len()]);
    writer.bytes(inner);
    Ok(writer.into_bytes())
}

fn decode_hop_plaintext(bytes: &[u8]) -> Result<HopPlaintext> {
    let mut reader = Reader::new(bytes);
    let role = RelayRole::from_tag(reader.u8()?)?;
    let expires_at_ms = reader.u64()?;
    let replay_token: [u8; 32] = reader
        .raw(32)?
        .try_into()
        .map_err(|_| RelayError::Truncated)?;
    let next_hop_len = reader.u32()? as usize;
    if next_hop_len == 0 || next_hop_len > MAX_ONION_NEXT_HOP_BYTES {
        return Err(RelayError::InvalidOnionRoute);
    }
    let padded = reader.raw(NEXT_HOP_PAD_BYTES)?;
    if padded[next_hop_len..].iter().any(|byte| *byte != 0) {
        return Err(RelayError::InvalidOnionRoute);
    }
    let next_hop = padded[..next_hop_len].to_vec();
    let inner = reader.bytes_limited(mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES)?;
    if !reader.finished() {
        return Err(RelayError::TrailingBytes);
    }
    Ok(HopPlaintext {
        role,
        expires_at_ms,
        replay_token,
        next_hop,
        inner,
    })
}

fn validate_route(hops: &[OnionHop]) -> Result<()> {
    if hops.len() != ONION_HOP_COUNT {
        return Err(RelayError::InvalidOnionRoute);
    }
    let expected = [RelayRole::Entry, RelayRole::Rendezvous, RelayRole::Delivery];
    let mut keys = HashSet::new();
    for (hop, expected_role) in hops.iter().zip(expected) {
        if hop.role != expected_role
            || hop.next_hop.is_empty()
            || hop.next_hop.len() > MAX_ONION_NEXT_HOP_BYTES
            || !keys.insert(hop.routing_key)
        {
            return Err(RelayError::InvalidOnionRoute);
        }
    }
    Ok(())
}

fn route_role(hop_index: u8) -> Result<RelayRole> {
    match hop_index {
        0 => Ok(RelayRole::Entry),
        1 => Ok(RelayRole::Rendezvous),
        2 => Ok(RelayRole::Delivery),
        _ => Err(RelayError::WrongOnionHop),
    }
}

fn derive_key(domain: &[u8], shared: &[u8; 32], aad: &[u8]) -> Result<AeadKey> {
    let mut info = Vec::with_capacity(domain.len() + aad.len());
    info.extend_from_slice(domain);
    info.extend_from_slice(aad);
    Ok(KdfSuite::DEFAULT.derive_aead_key(Some(domain), shared, &info, AeadSuite::DEFAULT)?)
}

fn hop_aad(
    connection_id: ConnectionId,
    size_class: PayloadSizeClass,
    hop_index: u8,
    ephemeral_key: AgreementPublicKey,
    nonce: AeadNonce,
) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.raw(HOP_KEY_DOMAIN);
    writer.u8(ONION_VERSION);
    writer.raw(&connection_id.to_bytes());
    writer.u8(size_class_tag(size_class));
    writer.u8(hop_index);
    writer.u8(ONION_HOP_COUNT as u8);
    writer.u8(ephemeral_key.suite().tag());
    writer.raw(&ephemeral_key.to_bytes());
    writer.raw(nonce.as_bytes());
    writer.into_bytes()
}

fn destination_aad(
    connection_id: ConnectionId,
    size_class: PayloadSizeClass,
    ephemeral_key: AgreementPublicKey,
    nonce: AeadNonce,
) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.raw(DESTINATION_KEY_DOMAIN);
    writer.u8(ONION_VERSION);
    writer.raw(&connection_id.to_bytes());
    writer.u8(size_class_tag(size_class));
    writer.u8(ephemeral_key.suite().tag());
    writer.raw(&ephemeral_key.to_bytes());
    writer.raw(nonce.as_bytes());
    writer.into_bytes()
}

fn encode_fixed_payload(size_class: PayloadSizeClass, plaintext: &[u8]) -> Result<Vec<u8>> {
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

fn fixed_payload_bytes(size_class: PayloadSizeClass) -> usize {
    match size_class {
        PayloadSizeClass::Small => SMALL_ONION_PAYLOAD_BYTES,
        PayloadSizeClass::Medium => MEDIUM_ONION_PAYLOAD_BYTES,
        PayloadSizeClass::Large => LARGE_ONION_PAYLOAD_BYTES,
    }
}

fn max_onion_ciphertext_bytes(size_class: PayloadSizeClass, hop_index: u8) -> Result<usize> {
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

fn size_class_tag(size_class: PayloadSizeClass) -> u8 {
    match size_class {
        PayloadSizeClass::Small => 1,
        PayloadSizeClass::Medium => 2,
        PayloadSizeClass::Large => 3,
    }
}

fn size_class_from_tag(tag: u8) -> Result<PayloadSizeClass> {
    match tag {
        1 => Ok(PayloadSizeClass::Small),
        2 => Ok(PayloadSizeClass::Medium),
        3 => Ok(PayloadSizeClass::Large),
        _ => Err(RelayError::BadSizeClass),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn three_relays_peel_only_their_layer_and_destination_opens_plaintext() {
        let (hops, secrets) = route();
        let destination = AgreementSecretKey::from_seed(&[9; 32]);
        let connection_id = ConnectionId::from_bytes([7; 16]);
        let mut packet = build_onion(
            connection_id,
            PayloadSizeClass::Small,
            &hops,
            destination.public_key(),
            b"private application payload",
            10_000,
        )
        .unwrap();

        let mut destination_envelope = None;
        let mut public_connection_ids = HashSet::new();
        for (index, secret) in secrets.iter().enumerate() {
            assert!(public_connection_ids.insert(packet.connection_id));
            let mut replay = OnionReplayCache::new(8).unwrap();
            let peeled = packet.peel(secret, 5_000, &mut replay).unwrap();
            assert_eq!(peeled.role, hops[index].role);
            assert_eq!(peeled.next_hop, hops[index].next_hop);
            match peeled.forward {
                OnionForward::Next(next) => packet = next,
                OnionForward::Destination(bytes) => destination_envelope = Some(bytes),
            }
        }
        assert_eq!(public_connection_ids.len(), ONION_HOP_COUNT);
        let opened = open_onion_destination(&destination_envelope.unwrap(), &destination).unwrap();
        assert_eq!(opened, b"private application payload");
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
            10_000,
        )
        .unwrap();
        let wrong = AgreementSecretKey::from_seed(&[44; 32]);
        let mut cache = OnionReplayCache::new(8).unwrap();
        assert!(packet.peel(&wrong, 5_000, &mut cache).is_err());
        assert_eq!(
            packet.peel(&secrets[0], 10_001, &mut cache),
            Err(RelayError::OnionExpired)
        );
        let peeled = packet.peel(&secrets[0], 5_000, &mut cache).unwrap();
        assert_eq!(
            packet.peel(&secrets[0], 5_000, &mut cache),
            Err(RelayError::OnionReplay)
        );
        let mut tampered = match peeled.forward {
            OnionForward::Next(next) => next,
            OnionForward::Destination(_) => panic!("entry cannot be final"),
        };
        tampered.connection_id = ConnectionId::from_bytes([8; 16]);
        let mut next_cache = OnionReplayCache::new(8).unwrap();
        assert!(tampered.peel(&secrets[1], 5_000, &mut next_cache).is_err());
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
                10_000,
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
                10_000,
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
                10_000,
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
            10_000,
        )
        .unwrap();
        let bytes = packet.to_bytes().unwrap();
        assert_eq!(OnionPacket::from_bytes(&bytes).unwrap(), packet);
        for cut in 0..bytes.len() {
            assert!(OnionPacket::from_bytes(&bytes[..cut]).is_err());
        }
    }
}
