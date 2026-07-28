//! Android-facing bridge for the signed LAN/QR pairing protocol.
//!
//! Kotlin receives only a bounded QR string and verified public contact data.
//! Root and delegated-device signing keys remain inside [`crate::RootCore`].
//! A successful exchange creates an ordinary signed `mini-social` follow
//! object on each participant. The objects are retained for later sync; this
//! module does not claim that global synchronization exists yet.

use std::net::{IpAddr, SocketAddr, TcpListener};
use std::time::Duration;

use did_mini::Did;
use mini_social::{
    create_pairing_acceptance, create_pairing_offer, receive_pairing_acceptance,
    send_pairing_acceptance, set_follow, verify_pairing_acceptance, verify_pairing_offer,
    MAX_PAIRING_OFFER_WINDOW_MS, PAIRING_NONCE_BYTES,
};
use mini_store::{MemoryBackend, Store};

use crate::{PersistReader, RootCore, RootError};

const QR_PREFIX: &str = "mini:pair:v1:";
// A QR symbol has a much tighter real capacity than the protocol's generic
// 16 KiB KEL ceiling. Keep the signed binary under 2 KiB so base64url plus
// the scheme prefix remains renderable by a high-density QR symbol.
const MAX_QR_PAYLOAD_BYTES: usize = 2 * 1024;
const MAX_CONTACTS: usize = 256;
const MAX_FOLLOW_OBJECTS: usize = 512;
const MAX_FOLLOW_OBJECT_BYTES: usize = 1 << 20;
const MAX_TIMEOUT_MS: u64 = 30_000;
const MAX_DISPLAY_NAME_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingContact {
    pub did: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingOfferView {
    pub qr_payload: String,
    pub expires_at_ms: u64,
    pub listen_port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingError {
    NoRoot,
    NoDevice,
    InvalidAddress,
    InvalidQr,
    Expired,
    Replayed,
    AlreadyListening,
    NoPendingOffer,
    Capacity,
    InvalidTimeout,
    Protocol(String),
    Io(String),
}

impl core::fmt::Display for PairingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoRoot => f.write_str("create or restore an identity before pairing"),
            Self::NoDevice => f.write_str("no delegated device is available for pairing"),
            Self::InvalidAddress => f.write_str("invalid LAN address"),
            Self::InvalidQr => f.write_str("invalid Mininet pairing QR payload"),
            Self::Expired => f.write_str("pairing offer expired"),
            Self::Replayed => f.write_str("pairing offer was already used"),
            Self::AlreadyListening => f.write_str("a pairing offer is already active"),
            Self::NoPendingOffer => f.write_str("no pairing offer is awaiting acceptance"),
            Self::Capacity => f.write_str("pairing state capacity reached"),
            Self::InvalidTimeout => f.write_str("invalid pairing timeout"),
            Self::Protocol(message) => {
                write!(f, "pairing protocol rejected the exchange: {message}")
            }
            Self::Io(message) => write!(f, "LAN pairing failed: {message}"),
        }
    }
}

impl std::error::Error for PairingError {}

#[derive(Debug)]
pub(super) struct PendingOffer {
    nonce: [u8; PAIRING_NONCE_BYTES],
    expires_at_ms: u64,
    listener: TcpListener,
}

#[derive(Debug, Clone)]
struct ConsumedOffer {
    nonce: [u8; PAIRING_NONCE_BYTES],
    expires_at_ms: u64,
}

#[derive(Debug, Default)]
pub(super) struct PairingState {
    contacts: Vec<PairingContact>,
    follow_objects: Vec<Vec<u8>>,
    consumed: Vec<ConsumedOffer>,
    pending: Option<PendingOffer>,
    next_sequence: u64,
}

impl RootCore {
    /// Bind a foreground LAN listener and create the signed bytes rendered as
    /// a QR code. `advertised_ip` must be a concrete LAN address selected by
    /// the platform; wildcard, loopback, multicast, and unspecified addresses
    /// are rejected because a peer could not safely connect to them.
    pub fn begin_pairing_offer(
        &self,
        display_name: String,
        advertised_ip: String,
        now_ms: u64,
        ttl_ms: u64,
    ) -> Result<PairingOfferView, PairingError> {
        validate_name(&display_name)?;
        if ttl_ms == 0 || ttl_ms > MAX_PAIRING_OFFER_WINDOW_MS {
            return Err(PairingError::Expired);
        }
        let expires_at_ms = now_ms.checked_add(ttl_ms).ok_or(PairingError::Expired)?;
        let advertised_ip: IpAddr = advertised_ip
            .parse()
            .map_err(|_| PairingError::InvalidAddress)?;
        let test_loopback = cfg!(test) && advertised_ip.is_loopback();
        if advertised_ip.is_unspecified()
            || (advertised_ip.is_loopback() && !test_loopback)
            || advertised_ip.is_multicast()
            || (!test_loopback && !is_lan_address(advertised_ip))
        {
            return Err(PairingError::InvalidAddress);
        }

        let bind_ip = match advertised_ip {
            IpAddr::V4(_) => IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
            IpAddr::V6(_) => IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
        };
        let listener = TcpListener::bind(SocketAddr::new(bind_ip, 0))
            .map_err(|error| PairingError::Io(error.to_string()))?;
        let port = listener
            .local_addr()
            .map_err(|error| PairingError::Io(error.to_string()))?
            .port();

        let mut state = self.lock();
        sweep_consumed(&mut state.pairing, now_ms);
        ensure_capacity_for(&state.pairing, None)?;
        if state
            .pairing
            .pending
            .as_ref()
            .is_some_and(|pending| pending.expires_at_ms > now_ms)
        {
            return Err(PairingError::AlreadyListening);
        }
        let root = state.root.as_ref().ok_or(PairingError::NoRoot)?;
        let device = state.devices.first().ok_or(PairingError::NoDevice)?;
        let random =
            mini_crypto::random_32().map_err(|error| PairingError::Protocol(error.to_string()))?;
        let mut nonce = [0_u8; PAIRING_NONCE_BYTES];
        nonce.copy_from_slice(&random[..PAIRING_NONCE_BYTES]);
        let endpoint = SocketAddr::new(advertised_ip, port);
        let offer = create_pairing_offer(
            &root.kel(),
            device,
            &display_name,
            endpoint,
            nonce,
            now_ms,
            expires_at_ms,
        )
        .map_err(protocol_error)?;
        if offer.len() > MAX_QR_PAYLOAD_BYTES {
            return Err(PairingError::Capacity);
        }
        state.pairing.pending = Some(PendingOffer {
            nonce,
            expires_at_ms,
            listener,
        });
        Ok(PairingOfferView {
            qr_payload: encode_qr(&offer),
            expires_at_ms,
            listen_port: port,
        })
    }

    /// Verify a scanned offer, deliver a signed acceptance over its advertised
    /// LAN endpoint, and create this device's signed follow object.
    pub fn accept_pairing_offer(
        &self,
        qr_payload: String,
        display_name: String,
        now_ms: u64,
        connect_timeout_ms: u64,
    ) -> Result<PairingContact, PairingError> {
        validate_name(&display_name)?;
        let timeout = timeout(connect_timeout_ms)?;
        let offer_bytes = decode_qr(&qr_payload)?;
        let offer = verify_pairing_offer(&offer_bytes, now_ms).map_err(protocol_error)?;

        let contact = PairingContact {
            did: offer.offerer.as_str().to_string(),
            display_name: offer.display_name,
        };
        let mut state = self.lock();
        sweep_consumed(&mut state.pairing, now_ms);
        reject_consumed(&state.pairing, &offer.nonce)?;
        ensure_capacity_for(&state.pairing, Some(offer.offerer.as_str()))?;
        let next_sequence = next_sequence(&state.pairing)?;
        let (acceptance, follow_object) = {
            let root = state.root.as_ref().ok_or(PairingError::NoRoot)?;
            let device = state.devices.first().ok_or(PairingError::NoDevice)?;
            (
                create_pairing_acceptance(offer.nonce, &root.kel(), device, &display_name)
                    .map_err(protocol_error)?,
                build_follow_object(&root.did(), device, &offer.offerer, now_ms, next_sequence)?,
            )
        };
        // The RootCore lock deliberately remains held for this bounded
        // foreground send. That prevents two concurrent accept actions from
        // reserving the same author sequence or consuming one QR twice.
        send_pairing_acceptance(offer.endpoint, &acceptance, timeout, timeout)
            .map_err(protocol_error)?;
        record_consumed(&mut state.pairing, offer.nonce, offer.expires_at_ms, now_ms)?;
        record_follow_object(&mut state.pairing, next_sequence, follow_object)?;
        upsert_contact(&mut state.pairing, contact.clone())?;
        Ok(contact)
    }

    /// Wait for and authenticate the acceptance corresponding to the active
    /// offer, then create the offerer's signed follow object.
    pub fn finish_pairing_offer(
        &self,
        now_ms: u64,
        accept_timeout_ms: u64,
        read_timeout_ms: u64,
    ) -> Result<PairingContact, PairingError> {
        let accept_timeout = timeout(accept_timeout_ms)?;
        let read_timeout = timeout(read_timeout_ms)?;
        let (nonce, expires_at_ms, listener) = {
            let state = self.lock();
            let pending = state
                .pairing
                .pending
                .as_ref()
                .ok_or(PairingError::NoPendingOffer)?;
            if now_ms > pending.expires_at_ms {
                return Err(PairingError::Expired);
            }
            (
                pending.nonce,
                pending.expires_at_ms,
                pending
                    .listener
                    .try_clone()
                    .map_err(|error| PairingError::Io(error.to_string()))?,
            )
        };

        let bytes = receive_pairing_acceptance(&listener, accept_timeout, read_timeout)
            .map_err(protocol_error)?;
        let acceptance = verify_pairing_acceptance(&bytes, nonce).map_err(protocol_error)?;
        let contact = PairingContact {
            did: acceptance.acceptor.as_str().to_string(),
            display_name: acceptance.display_name,
        };

        let mut state = self.lock();
        let current = state
            .pairing
            .pending
            .as_ref()
            .ok_or(PairingError::NoPendingOffer)?;
        if current.nonce != nonce || current.expires_at_ms != expires_at_ms {
            return Err(PairingError::Replayed);
        }
        ensure_capacity_for(&state.pairing, Some(acceptance.acceptor.as_str()))?;
        let next_sequence = next_sequence(&state.pairing)?;
        let follow_object = {
            let root = state.root.as_ref().ok_or(PairingError::NoRoot)?.did();
            let device = state.devices.first().ok_or(PairingError::NoDevice)?;
            build_follow_object(&root, device, &acceptance.acceptor, now_ms, next_sequence)?
        };
        state.pairing.pending = None;
        record_consumed(&mut state.pairing, nonce, expires_at_ms, now_ms)?;
        record_follow_object(&mut state.pairing, next_sequence, follow_object)?;
        upsert_contact(&mut state.pairing, contact.clone())?;
        Ok(contact)
    }

    pub fn pairing_contacts(&self) -> Vec<PairingContact> {
        self.lock().pairing.contacts.clone()
    }

    /// Canonical signed follow objects created by completed pairings, ready
    /// for the later sync transport. No secret material is present.
    pub fn pairing_follow_objects(&self) -> Vec<Vec<u8>> {
        self.lock().pairing.follow_objects.clone()
    }
}

fn validate_name(name: &str) -> Result<(), PairingError> {
    if name.is_empty() || name.len() > MAX_DISPLAY_NAME_BYTES || name.chars().any(char::is_control)
    {
        Err(PairingError::InvalidQr)
    } else {
        Ok(())
    }
}

fn is_lan_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => address.is_private() || address.is_link_local(),
        IpAddr::V6(address) => {
            let octets = address.octets();
            // fc00::/7 unique-local or fe80::/10 link-local. Spell the
            // prefixes out to preserve the workspace's Rust 1.83 MSRV.
            (octets[0] & 0xfe) == 0xfc || (octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80)
        }
    }
}

fn timeout(ms: u64) -> Result<Duration, PairingError> {
    if ms == 0 || ms > MAX_TIMEOUT_MS {
        Err(PairingError::InvalidTimeout)
    } else {
        Ok(Duration::from_millis(ms))
    }
}

fn protocol_error(error: mini_social::SocialError) -> PairingError {
    match error {
        mini_social::SocialError::PairingExpired => PairingError::Expired,
        mini_social::SocialError::PairingReplayed => PairingError::Replayed,
        mini_social::SocialError::Io(message) => PairingError::Io(message),
        other => PairingError::Protocol(other.to_string()),
    }
}

fn sweep_consumed(state: &mut PairingState, now_ms: u64) {
    state.consumed.retain(|entry| entry.expires_at_ms > now_ms);
}

fn reject_consumed(
    state: &PairingState,
    nonce: &[u8; PAIRING_NONCE_BYTES],
) -> Result<(), PairingError> {
    if state.consumed.iter().any(|entry| &entry.nonce == nonce) {
        Err(PairingError::Replayed)
    } else {
        Ok(())
    }
}

fn ensure_capacity_for(
    state: &PairingState,
    contact_did: Option<&str>,
) -> Result<(), PairingError> {
    let adds_contact = contact_did.is_some_and(|did| {
        !state
            .contacts
            .iter()
            .any(|contact| contact.did.as_str() == did)
    });
    if state.consumed.len() >= MAX_CONTACTS
        || state.follow_objects.len() >= MAX_FOLLOW_OBJECTS
        || (adds_contact && state.contacts.len() >= MAX_CONTACTS)
    {
        Err(PairingError::Capacity)
    } else {
        Ok(())
    }
}

fn record_consumed(
    state: &mut PairingState,
    nonce: [u8; PAIRING_NONCE_BYTES],
    expires_at_ms: u64,
    now_ms: u64,
) -> Result<(), PairingError> {
    sweep_consumed(state, now_ms);
    reject_consumed(state, &nonce)?;
    if state.consumed.len() >= MAX_CONTACTS {
        return Err(PairingError::Capacity);
    }
    state.consumed.push(ConsumedOffer {
        nonce,
        expires_at_ms,
    });
    Ok(())
}

fn next_sequence(state: &PairingState) -> Result<u64, PairingError> {
    state
        .next_sequence
        .checked_add(1)
        .ok_or(PairingError::Capacity)
}

fn build_follow_object(
    follower: &Did,
    device: &did_mini::Controller,
    target: &Did,
    now_ms: u64,
    sequence: u64,
) -> Result<Vec<u8>, PairingError> {
    let mut store = Store::new(MemoryBackend::new());
    Ok(
        set_follow(&mut store, follower, device, target, true, now_ms, sequence)
            .map_err(protocol_error)?
            .to_bytes(),
    )
}

fn record_follow_object(
    state: &mut PairingState,
    sequence: u64,
    object: Vec<u8>,
) -> Result<(), PairingError> {
    if state.follow_objects.len() >= MAX_FOLLOW_OBJECTS {
        return Err(PairingError::Capacity);
    }
    state.next_sequence = sequence;
    state.follow_objects.push(object);
    Ok(())
}

fn upsert_contact(state: &mut PairingState, contact: PairingContact) -> Result<(), PairingError> {
    if let Some(existing) = state
        .contacts
        .iter_mut()
        .find(|item| item.did == contact.did)
    {
        *existing = contact;
        return Ok(());
    }
    if state.contacts.len() >= MAX_CONTACTS {
        return Err(PairingError::Capacity);
    }
    state.contacts.push(contact);
    state.contacts.sort_by(|left, right| {
        left.display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase())
            .then_with(|| left.did.cmp(&right.did))
    });
    Ok(())
}

pub(super) fn encode_pairing_state(out: &mut Vec<u8>, state: &PairingState) {
    out.extend_from_slice(&state.next_sequence.to_le_bytes());
    out.extend_from_slice(&(state.contacts.len() as u32).to_le_bytes());
    for contact in &state.contacts {
        put_bytes(out, contact.did.as_bytes());
        put_bytes(out, contact.display_name.as_bytes());
    }
    out.extend_from_slice(&(state.follow_objects.len() as u32).to_le_bytes());
    for object in &state.follow_objects {
        put_bytes(out, object);
    }
    out.extend_from_slice(&(state.consumed.len() as u32).to_le_bytes());
    for entry in &state.consumed {
        out.extend_from_slice(&entry.nonce);
        out.extend_from_slice(&entry.expires_at_ms.to_le_bytes());
    }
}

pub(super) fn decode_pairing_state(
    reader: &mut PersistReader<'_>,
) -> Result<PairingState, RootError> {
    let next_sequence = read_u64(reader)?;
    let contact_count = reader.u32()? as usize;
    if contact_count > MAX_CONTACTS {
        return Err(RootError::CorruptState);
    }
    let mut contacts = Vec::with_capacity(contact_count);
    for _ in 0..contact_count {
        let did = read_string(reader, 256)?;
        Did::parse(&did).map_err(|_| RootError::CorruptState)?;
        let display_name = read_string(reader, MAX_DISPLAY_NAME_BYTES)?;
        validate_name(&display_name).map_err(|_| RootError::CorruptState)?;
        contacts.push(PairingContact { did, display_name });
    }
    let object_count = reader.u32()? as usize;
    if object_count > MAX_FOLLOW_OBJECTS {
        return Err(RootError::CorruptState);
    }
    let mut follow_objects = Vec::with_capacity(object_count);
    for _ in 0..object_count {
        let len = reader.u32()? as usize;
        if len > MAX_FOLLOW_OBJECT_BYTES {
            return Err(RootError::CorruptState);
        }
        let bytes = reader.take(len)?.to_vec();
        let object =
            mini_objects::Object::from_bytes(&bytes).map_err(|_| RootError::CorruptState)?;
        if object.object_type != mini_objects::ObjectType::FOLLOW {
            return Err(RootError::CorruptState);
        }
        follow_objects.push(bytes);
    }
    let consumed_count = reader.u32()? as usize;
    if consumed_count > MAX_CONTACTS {
        return Err(RootError::CorruptState);
    }
    let mut consumed = Vec::with_capacity(consumed_count);
    for _ in 0..consumed_count {
        let mut nonce = [0_u8; PAIRING_NONCE_BYTES];
        nonce.copy_from_slice(reader.take(PAIRING_NONCE_BYTES)?);
        consumed.push(ConsumedOffer {
            nonce,
            expires_at_ms: read_u64(reader)?,
        });
    }
    Ok(PairingState {
        contacts,
        follow_objects,
        consumed,
        pending: None,
        next_sequence,
    })
}

fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

fn read_string(reader: &mut PersistReader<'_>, max: usize) -> Result<String, RootError> {
    let len = reader.u32()? as usize;
    if len > max {
        return Err(RootError::CorruptState);
    }
    String::from_utf8(reader.take(len)?.to_vec()).map_err(|_| RootError::CorruptState)
}

fn read_u64(reader: &mut PersistReader<'_>) -> Result<u64, RootError> {
    let bytes = reader.take(8)?;
    Ok(u64::from_le_bytes(
        bytes.try_into().map_err(|_| RootError::CorruptState)?,
    ))
}

fn encode_qr(bytes: &[u8]) -> String {
    format!("{QR_PREFIX}{}", base64url_encode(bytes))
}

fn decode_qr(text: &str) -> Result<Vec<u8>, PairingError> {
    let encoded = text
        .strip_prefix(QR_PREFIX)
        .ok_or(PairingError::InvalidQr)?;
    let bytes = base64url_decode(encoded)?;
    if bytes.is_empty() || bytes.len() > MAX_QR_PAYLOAD_BYTES {
        return Err(PairingError::InvalidQr);
    }
    Ok(bytes)
}

const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64url_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let value = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | chunk.get(2).copied().unwrap_or(0) as u32;
        out.push(BASE64URL[((value >> 18) & 63) as usize] as char);
        out.push(BASE64URL[((value >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(BASE64URL[((value >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(BASE64URL[(value & 63) as usize] as char);
        }
    }
    out
}

fn base64url_decode(text: &str) -> Result<Vec<u8>, PairingError> {
    if text.is_empty() || text.len() % 4 == 1 {
        return Err(PairingError::InvalidQr);
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3 + 2);
    let mut accumulator = 0_u32;
    let mut bits = 0_u8;
    for byte in text.bytes() {
        let value = BASE64URL
            .iter()
            .position(|candidate| *candidate == byte)
            .ok_or(PairingError::InvalidQr)? as u32;
        accumulator = (accumulator << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((accumulator >> bits) as u8);
            accumulator &= (1_u32 << bits).wrapping_sub(1);
        }
    }
    if accumulator != 0 {
        return Err(PairingError::InvalidQr);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StorageCipher, StorageCipherError};

    #[derive(Debug)]
    struct IdentityCipher;

    impl StorageCipher for IdentityCipher {
        fn encrypt(&self, plaintext: Vec<u8>) -> Result<Vec<u8>, StorageCipherError> {
            Ok(plaintext)
        }

        fn decrypt(&self, ciphertext: Vec<u8>) -> Result<Vec<u8>, StorageCipherError> {
            Ok(ciphertext)
        }
    }

    fn ready_core() -> RootCore {
        let core = RootCore::new();
        core.create_root().unwrap();
        core.create_device().unwrap();
        core
    }

    fn pair(
        offerer: &RootCore,
        acceptor: &RootCore,
        now_ms: u64,
    ) -> (PairingOfferView, PairingContact, PairingContact) {
        let offer = offerer
            .begin_pairing_offer("Alice".to_string(), "127.0.0.1".to_string(), now_ms, 30_000)
            .unwrap();
        let qr = offer.qr_payload.clone();
        std::thread::scope(|scope| {
            let receiver = scope.spawn(|| {
                offerer
                    .finish_pairing_offer(now_ms + 1, 5_000, 5_000)
                    .unwrap()
            });
            let accepted = acceptor
                .accept_pairing_offer(qr, "Bob".to_string(), now_ms + 1, 5_000)
                .unwrap();
            (offer, receiver.join().unwrap(), accepted)
        })
    }

    #[test]
    fn qr_encoding_round_trips_without_padding() {
        for bytes in [b"a".as_slice(), b"ab", b"abc", b"pairing payload"] {
            assert_eq!(decode_qr(&encode_qr(bytes)).unwrap(), bytes);
        }
    }

    #[test]
    fn malformed_qr_text_is_rejected() {
        for text in [
            "",
            "https://example.test",
            "mini:pair:v1:*",
            "mini:pair:v1:A",
        ] {
            assert_eq!(decode_qr(text), Err(PairingError::InvalidQr));
        }
    }

    #[test]
    fn two_cores_pair_over_real_tcp_and_each_create_a_signed_follow() {
        let alice = ready_core();
        let bob = ready_core();
        let alice_did = alice.root_did().unwrap();
        let bob_did = bob.root_did().unwrap();

        let (_, seen_by_alice, seen_by_bob) = pair(&alice, &bob, 1_000);

        assert_eq!(seen_by_alice.did, bob_did);
        assert_eq!(seen_by_alice.display_name, "Bob");
        assert_eq!(seen_by_bob.did, alice_did);
        assert_eq!(seen_by_bob.display_name, "Alice");
        assert_eq!(alice.pairing_contacts(), vec![seen_by_alice]);
        assert_eq!(bob.pairing_contacts(), vec![seen_by_bob]);
        for core in [&alice, &bob] {
            let objects = core.pairing_follow_objects();
            assert_eq!(objects.len(), 1);
            let object = mini_objects::Object::from_bytes(&objects[0]).unwrap();
            assert_eq!(object.object_type, mini_objects::ObjectType::FOLLOW);
        }
    }

    #[test]
    fn a_successfully_consumed_qr_is_rejected_before_a_second_network_send() {
        let alice = ready_core();
        let bob = ready_core();
        let (offer, _, _) = pair(&alice, &bob, 2_000);

        assert_eq!(
            bob.accept_pairing_offer(offer.qr_payload, "Bob".to_string(), 2_002, 1_000),
            Err(PairingError::Replayed)
        );
        assert_eq!(bob.pairing_follow_objects().len(), 1);
    }

    #[test]
    fn replay_memory_and_contacts_survive_encrypted_state_restore() {
        let alice = ready_core();
        let bob = ready_core();
        let (offer, _, contact) = pair(&alice, &bob, 3_000);
        let persisted = bob.persist_state(Box::new(IdentityCipher)).unwrap();
        let restored = RootCore::restore(persisted, Box::new(IdentityCipher)).unwrap();

        assert_eq!(restored.pairing_contacts(), vec![contact]);
        assert_eq!(restored.pairing_follow_objects().len(), 1);
        assert_eq!(
            restored.accept_pairing_offer(offer.qr_payload, "Bob".to_string(), 3_002, 1_000,),
            Err(PairingError::Replayed)
        );
    }

    #[test]
    fn expired_and_forged_qr_payloads_fail_closed_without_following() {
        let alice = ready_core();
        let bob = ready_core();
        let offer = alice
            .begin_pairing_offer("Alice".to_string(), "127.0.0.1".to_string(), 4_000, 100)
            .unwrap();
        assert_eq!(
            bob.accept_pairing_offer(offer.qr_payload.clone(), "Bob".to_string(), 4_101, 1_000,),
            Err(PairingError::Expired)
        );

        let mut forged = offer.qr_payload;
        let last = forged.pop().unwrap();
        forged.push(if last == 'A' { 'B' } else { 'A' });
        assert!(matches!(
            bob.accept_pairing_offer(forged, "Bob".to_string(), 4_050, 1_000),
            Err(PairingError::Protocol(_)) | Err(PairingError::InvalidQr)
        ));
        assert!(bob.pairing_contacts().is_empty());
        assert!(bob.pairing_follow_objects().is_empty());
    }

    #[test]
    fn unsafe_advertised_addresses_and_unbounded_timeouts_are_rejected() {
        let alice = ready_core();
        for address in ["0.0.0.0", "8.8.8.8", "224.0.0.1", "::"] {
            assert_eq!(
                alice.begin_pairing_offer("Alice".to_string(), address.to_string(), 5_000, 1_000,),
                Err(PairingError::InvalidAddress)
            );
        }
        assert_eq!(timeout(0), Err(PairingError::InvalidTimeout));
        assert_eq!(
            timeout(MAX_TIMEOUT_MS + 1),
            Err(PairingError::InvalidTimeout)
        );
    }
}
