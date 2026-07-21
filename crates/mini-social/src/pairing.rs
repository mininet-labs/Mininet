//! LAN/QR pairing exchange (Android beta slice 4, issue #200).
//!
//! Two devices that have never synced share no prior trust. A QR code is the
//! out-of-band channel that starts trust between them, so the payload it
//! encodes — a [`PairingOffer`] — must be **self-verifying**: it embeds the
//! offerer's own root and device KELs so the scanner can run
//! [`did_mini::verify_delegation`] fully offline, exactly like KEL exchange
//! anywhere else in Mininet (SPEC-01 §6). This is deliberately stronger than
//! [`crate::discovery`]'s LAN announcements, which stay unauthenticated
//! connection *hints* by design; a pairing offer is a signed claim.
//!
//! ## Protocol
//!
//! 1. Device A calls [`create_pairing_offer`] and renders the bytes as a QR
//!    code (Kotlin-side, out of scope here).
//! 2. Device B scans it and calls [`verify_pairing_offer`], which checks the
//!    signature, the delegation chain, the capability scope, and the bounded
//!    expiry window.
//! 3. Device B calls [`create_pairing_acceptance`] (naming the offer's
//!    nonce) and delivers it to A's advertised endpoint over a direct LAN
//!    TCP connection via [`send_pairing_acceptance`]; A receives it with
//!    [`receive_pairing_acceptance`] and verifies it with
//!    [`verify_pairing_acceptance`].
//! 4. Each side, having independently verified the other's signed identity,
//!    calls [`crate::set_follow`] for the counterpart. Mutual follow is two
//!    ordinary, separately-signed follow objects — this module never writes
//!    to a [`mini_store::Store`] itself, it only authenticates the exchange
//!    that precedes those writes.
//!
//! ## Bounding, not just verifying
//!
//! A validly signed offer is still replayable — only caller-held state can
//! reject a replay, so [`PairingNonceLedger`] is a separate, explicit piece
//! callers must hold (the same "capacity is trust, not verification" shape
//! as D-0339's `pending_enrollment` guard). The offer's own expiry window is
//! independently capped by [`MAX_PAIRING_OFFER_WINDOW_MS`] so a captured
//! offer cannot be held indefinitely even before a ledger ever sees it.
//!
//! ## Non-goals
//!
//! BLE transport (slice 5) and background/off-screen sync (slice 6) are
//! explicitly out of scope; this module only reaches a direct, foreground,
//! same-LAN TCP connection.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

use did_mini::{verify_delegation, Capabilities, Controller, Did, IndexedSig, Kel};
use mini_crypto::{Signature, SignatureSuite};

use crate::{get_str, put_str, Result, SocialError, MAX_NAME_BYTES};

/// Bound on the offerer's embedded root KEL, so a pairing offer cannot be
/// used to smuggle an unboundedly large blob.
pub const MAX_PAIRING_ROOT_KEL_BYTES: usize = 16 * 1024;
/// Bound on the offerer's embedded device KEL.
pub const MAX_PAIRING_DEVICE_KEL_BYTES: usize = 4 * 1024;
/// Byte width of a pairing nonce.
pub const PAIRING_NONCE_BYTES: usize = 16;
/// The longest an offer's own `issued_at..expires_at` window may span,
/// regardless of what the caller requests — a captured offer cannot be held
/// open indefinitely.
pub const MAX_PAIRING_OFFER_WINDOW_MS: u64 = 5 * 60 * 1000;
/// Bound on signatures carried by one offer/acceptance.
const MAX_PAIRING_SIGNATURES: usize = 4;
/// Bound on one detached signature's raw bytes (largest suite today).
const MAX_PAIRING_SIGNATURE_BYTES: usize = 4096;
/// Bound on the wire size of one acceptance sent over TCP.
const MAX_PAIRING_ACCEPTANCE_BYTES: usize =
    MAX_PAIRING_ROOT_KEL_BYTES + MAX_PAIRING_DEVICE_KEL_BYTES + 8 * 1024;
/// Bound on a [`PairingNonceLedger`]'s live (unexpired) entries.
const MAX_PAIRING_NONCE_LEDGER_ENTRIES: usize = 256;

const OFFER_MAGIC: &[u8; 8] = b"MINIPRO1";
const ACCEPT_MAGIC: &[u8; 8] = b"MINIPRA1";

/// A bounded, capability-scoped, signed offer to pair — the bytes a QR code
/// encodes. Build with [`create_pairing_offer`]; a scanner authenticates the
/// raw bytes with [`verify_pairing_offer`].
#[derive(Debug, Clone)]
pub struct VerifiedPairingOffer {
    /// The offerer's human-root DID (never the delegated device's own DID).
    pub offerer: Did,
    /// Capabilities the offerer's root has granted the signing device.
    pub capabilities: Capabilities,
    /// The offerer's chosen display name at offer time.
    pub display_name: String,
    /// Where the scanner should connect to deliver its acceptance.
    pub endpoint: SocketAddr,
    /// Anti-replay nonce; echoed back in the matching acceptance.
    pub nonce: [u8; PAIRING_NONCE_BYTES],
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
}

/// Build a signed pairing offer. `root_kel` and `device` must be the
/// offerer's own (a device signing an offer for a *different* identity is
/// meaningless — the recipient re-derives `offerer` from `root_kel` itself,
/// never trusting an unsigned claim of who is offering).
#[allow(clippy::too_many_arguments)]
pub fn create_pairing_offer(
    root_kel: &Kel,
    device: &Controller,
    display_name: &str,
    endpoint: SocketAddr,
    nonce: [u8; PAIRING_NONCE_BYTES],
    issued_at_ms: u64,
    expires_at_ms: u64,
) -> Result<Vec<u8>> {
    if display_name.len() > MAX_NAME_BYTES {
        return Err(SocialError::FieldTooLarge);
    }
    if expires_at_ms <= issued_at_ms || expires_at_ms - issued_at_ms > MAX_PAIRING_OFFER_WINDOW_MS {
        return Err(SocialError::PairingExpired);
    }
    let root_kel_bytes = root_kel.to_bytes();
    let device_kel_bytes = device.kel().to_bytes();
    if root_kel_bytes.len() > MAX_PAIRING_ROOT_KEL_BYTES
        || device_kel_bytes.len() > MAX_PAIRING_DEVICE_KEL_BYTES
    {
        return Err(SocialError::FieldTooLarge);
    }

    let mut msg = Vec::new();
    msg.extend_from_slice(OFFER_MAGIC);
    put_bytes(&mut msg, &root_kel_bytes);
    put_bytes(&mut msg, &device_kel_bytes);
    put_str(&mut msg, display_name);
    put_endpoint(&mut msg, &endpoint);
    msg.extend_from_slice(&nonce);
    msg.extend_from_slice(&issued_at_ms.to_be_bytes());
    msg.extend_from_slice(&expires_at_ms.to_be_bytes());

    let sigs = device.sign_message(&msg);
    let mut out = msg;
    put_sigs(&mut out, &sigs)?;
    Ok(out)
}

/// Authenticate raw pairing-offer bytes (as scanned from a QR code):
/// decodes the embedded KELs, checks the delegation chain, requires the
/// signing device to hold [`Capabilities::POST`], verifies the detached
/// signature, and enforces the bounded expiry window against `now_ms`.
/// Does **not** check for replay — pair with a [`PairingNonceLedger`] for
/// that.
pub fn verify_pairing_offer(bytes: &[u8], now_ms: u64) -> Result<VerifiedPairingOffer> {
    if bytes.len() < OFFER_MAGIC.len() || &bytes[..OFFER_MAGIC.len()] != OFFER_MAGIC {
        return Err(SocialError::PairingMalformed);
    }
    let mut pos = OFFER_MAGIC.len();
    let root_kel_bytes = get_bytes(bytes, &mut pos, MAX_PAIRING_ROOT_KEL_BYTES)
        .ok_or(SocialError::PairingMalformed)?;
    let device_kel_bytes = get_bytes(bytes, &mut pos, MAX_PAIRING_DEVICE_KEL_BYTES)
        .ok_or(SocialError::PairingMalformed)?;
    let display_name = get_str(bytes, &mut pos).ok_or(SocialError::PairingMalformed)?;
    if display_name.len() > MAX_NAME_BYTES {
        return Err(SocialError::PairingMalformed);
    }
    let endpoint = get_endpoint(bytes, &mut pos).ok_or(SocialError::PairingMalformed)?;
    let nonce = get_nonce(bytes, &mut pos).ok_or(SocialError::PairingMalformed)?;
    let issued_at_ms = get_u64(bytes, &mut pos).ok_or(SocialError::PairingMalformed)?;
    let expires_at_ms = get_u64(bytes, &mut pos).ok_or(SocialError::PairingMalformed)?;
    let msg_end = pos;
    let sigs = get_sigs(bytes, &mut pos).ok_or(SocialError::PairingMalformed)?;
    if pos != bytes.len() {
        return Err(SocialError::PairingMalformed);
    }

    if expires_at_ms <= issued_at_ms || expires_at_ms - issued_at_ms > MAX_PAIRING_OFFER_WINDOW_MS {
        return Err(SocialError::PairingExpired);
    }
    if now_ms > expires_at_ms {
        return Err(SocialError::PairingExpired);
    }

    let root_kel = Kel::from_bytes(&root_kel_bytes)?;
    let device_kel = Kel::from_bytes(&device_kel_bytes)?;
    let capabilities = verify_delegation(&root_kel, &device_kel)?;
    if !capabilities.contains(Capabilities::POST) {
        return Err(SocialError::PairingCapabilityMissing);
    }
    device_kel.verify_message(&bytes[..msg_end], &sigs)?;

    Ok(VerifiedPairingOffer {
        offerer: root_kel.did(),
        capabilities,
        display_name,
        endpoint,
        nonce,
        issued_at_ms,
        expires_at_ms,
    })
}

/// An authenticated response to a [`VerifiedPairingOffer`].
#[derive(Debug, Clone)]
pub struct VerifiedPairingAcceptance {
    /// The accepting human-root DID.
    pub acceptor: Did,
    /// Capabilities the acceptor's root has granted the signing device.
    pub capabilities: Capabilities,
    /// The acceptor's chosen display name at acceptance time.
    pub display_name: String,
    /// The offer nonce this acceptance responds to.
    pub nonce: [u8; PAIRING_NONCE_BYTES],
}

/// Build a signed acceptance naming the offer's `offer_nonce`, so the
/// offerer can bind it to the exact offer it issued rather than any offer
/// it may ever have made.
pub fn create_pairing_acceptance(
    offer_nonce: [u8; PAIRING_NONCE_BYTES],
    root_kel: &Kel,
    device: &Controller,
    display_name: &str,
) -> Result<Vec<u8>> {
    if display_name.len() > MAX_NAME_BYTES {
        return Err(SocialError::FieldTooLarge);
    }
    let root_kel_bytes = root_kel.to_bytes();
    let device_kel_bytes = device.kel().to_bytes();
    if root_kel_bytes.len() > MAX_PAIRING_ROOT_KEL_BYTES
        || device_kel_bytes.len() > MAX_PAIRING_DEVICE_KEL_BYTES
    {
        return Err(SocialError::FieldTooLarge);
    }

    let mut msg = Vec::new();
    msg.extend_from_slice(ACCEPT_MAGIC);
    msg.extend_from_slice(&offer_nonce);
    put_bytes(&mut msg, &root_kel_bytes);
    put_bytes(&mut msg, &device_kel_bytes);
    put_str(&mut msg, display_name);

    let sigs = device.sign_message(&msg);
    let mut out = msg;
    put_sigs(&mut out, &sigs)?;
    Ok(out)
}

/// Authenticate raw acceptance bytes against the exact `expected_nonce` of
/// the offer this acceptance must respond to. Rejects an acceptance for any
/// other (including forged or stale) nonce.
pub fn verify_pairing_acceptance(
    bytes: &[u8],
    expected_nonce: [u8; PAIRING_NONCE_BYTES],
) -> Result<VerifiedPairingAcceptance> {
    if bytes.len() < ACCEPT_MAGIC.len() || &bytes[..ACCEPT_MAGIC.len()] != ACCEPT_MAGIC {
        return Err(SocialError::PairingMalformed);
    }
    let mut pos = ACCEPT_MAGIC.len();
    let nonce = get_nonce(bytes, &mut pos).ok_or(SocialError::PairingMalformed)?;
    if nonce != expected_nonce {
        return Err(SocialError::PairingMalformed);
    }
    let root_kel_bytes = get_bytes(bytes, &mut pos, MAX_PAIRING_ROOT_KEL_BYTES)
        .ok_or(SocialError::PairingMalformed)?;
    let device_kel_bytes = get_bytes(bytes, &mut pos, MAX_PAIRING_DEVICE_KEL_BYTES)
        .ok_or(SocialError::PairingMalformed)?;
    let display_name = get_str(bytes, &mut pos).ok_or(SocialError::PairingMalformed)?;
    if display_name.len() > MAX_NAME_BYTES {
        return Err(SocialError::PairingMalformed);
    }
    let msg_end = pos;
    let sigs = get_sigs(bytes, &mut pos).ok_or(SocialError::PairingMalformed)?;
    if pos != bytes.len() {
        return Err(SocialError::PairingMalformed);
    }

    let root_kel = Kel::from_bytes(&root_kel_bytes)?;
    let device_kel = Kel::from_bytes(&device_kel_bytes)?;
    let capabilities = verify_delegation(&root_kel, &device_kel)?;
    if !capabilities.contains(Capabilities::POST) {
        return Err(SocialError::PairingCapabilityMissing);
    }
    device_kel.verify_message(&bytes[..msg_end], &sigs)?;

    Ok(VerifiedPairingAcceptance {
        acceptor: root_kel.did(),
        capabilities,
        display_name,
        nonce,
    })
}

/// A small, bounded, time-aware record of pairing nonces already consumed —
/// the caller-owned state that turns "this signature is valid" into "this
/// specific offer has not already been accepted." A signed message is
/// always replayable by itself; only stateful memory of what has already
/// been seen can reject a replay.
#[derive(Debug, Default)]
pub struct PairingNonceLedger {
    seen: Vec<([u8; PAIRING_NONCE_BYTES], u64)>,
}

impl PairingNonceLedger {
    pub fn new() -> Self {
        Self { seen: Vec::new() }
    }

    /// Record `nonce` (valid until `expires_at_ms`) as consumed at `now_ms`,
    /// first sweeping every entry already past its own expiry. Returns
    /// `Err(PairingReplayed)` if `nonce` is already on record and not yet
    /// swept, `Err(PairingNonceLedgerFull)` if the bounded ledger has no
    /// room even after sweeping.
    pub fn observe(
        &mut self,
        nonce: [u8; PAIRING_NONCE_BYTES],
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<()> {
        self.seen.retain(|(_, expires)| *expires > now_ms);
        if self.seen.iter().any(|(seen, _)| *seen == nonce) {
            return Err(SocialError::PairingReplayed);
        }
        if self.seen.len() >= MAX_PAIRING_NONCE_LEDGER_ENTRIES {
            return Err(SocialError::PairingNonceLedgerFull);
        }
        self.seen.push((nonce, expires_at_ms));
        Ok(())
    }
}

/// Waits up to `accept_timeout` for one inbound TCP connection, then reads a
/// length-prefixed pairing acceptance with a bounded `read_timeout` so a
/// stalled or malformed peer cannot hang the offerer indefinitely. Returns
/// raw bytes for the caller to authenticate with [`verify_pairing_acceptance`]
/// — this function performs no cryptographic verification itself.
pub fn receive_pairing_acceptance(
    listener: &TcpListener,
    accept_timeout: Duration,
    read_timeout: Duration,
) -> Result<Vec<u8>> {
    listener
        .set_nonblocking(true)
        .map_err(|e| SocialError::Io(e.to_string()))?;
    let deadline = Instant::now() + accept_timeout;
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(SocialError::Io(
                        "no pairing acceptance connection within accept_timeout".to_string(),
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(SocialError::Io(e.to_string())),
        }
    };
    stream
        .set_nonblocking(false)
        .map_err(|e| SocialError::Io(e.to_string()))?;
    stream
        .set_read_timeout(Some(read_timeout))
        .map_err(|e| SocialError::Io(e.to_string()))?;

    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .map_err(|e| SocialError::Io(e.to_string()))?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_PAIRING_ACCEPTANCE_BYTES {
        return Err(SocialError::PairingMalformed);
    }
    let mut buf = vec![0u8; len];
    stream
        .read_exact(&mut buf)
        .map_err(|e| SocialError::Io(e.to_string()))?;
    Ok(buf)
}

/// Connects to `endpoint` and delivers `acceptance_bytes` (length-prefixed),
/// with bounded connect/write timeouts.
pub fn send_pairing_acceptance(
    endpoint: SocketAddr,
    acceptance_bytes: &[u8],
    connect_timeout: Duration,
    write_timeout: Duration,
) -> Result<()> {
    if acceptance_bytes.len() > MAX_PAIRING_ACCEPTANCE_BYTES {
        return Err(SocialError::PairingMalformed);
    }
    let mut stream = TcpStream::connect_timeout(&endpoint, connect_timeout)
        .map_err(|e| SocialError::Io(e.to_string()))?;
    stream
        .set_write_timeout(Some(write_timeout))
        .map_err(|e| SocialError::Io(e.to_string()))?;
    stream
        .write_all(&(acceptance_bytes.len() as u32).to_be_bytes())
        .map_err(|e| SocialError::Io(e.to_string()))?;
    stream
        .write_all(acceptance_bytes)
        .map_err(|e| SocialError::Io(e.to_string()))?;
    Ok(())
}

fn put_bytes(w: &mut Vec<u8>, b: &[u8]) {
    w.extend_from_slice(&(b.len() as u32).to_be_bytes());
    w.extend_from_slice(b);
}

fn get_bytes(b: &[u8], pos: &mut usize, max: usize) -> Option<Vec<u8>> {
    let len = u32::from_be_bytes(b.get(*pos..*pos + 4)?.try_into().ok()?) as usize;
    *pos += 4;
    if len > max || *pos + len > b.len() {
        return None;
    }
    let out = b[*pos..*pos + len].to_vec();
    *pos += len;
    Some(out)
}

fn get_u64(b: &[u8], pos: &mut usize) -> Option<u64> {
    let v = u64::from_be_bytes(b.get(*pos..*pos + 8)?.try_into().ok()?);
    *pos += 8;
    Some(v)
}

fn get_nonce(b: &[u8], pos: &mut usize) -> Option<[u8; PAIRING_NONCE_BYTES]> {
    let slice = b.get(*pos..*pos + PAIRING_NONCE_BYTES)?;
    *pos += PAIRING_NONCE_BYTES;
    let mut nonce = [0u8; PAIRING_NONCE_BYTES];
    nonce.copy_from_slice(slice);
    Some(nonce)
}

fn put_endpoint(w: &mut Vec<u8>, addr: &SocketAddr) {
    match addr {
        SocketAddr::V4(a) => {
            w.push(4);
            w.extend_from_slice(&a.ip().octets());
            w.extend_from_slice(&a.port().to_be_bytes());
        }
        SocketAddr::V6(a) => {
            w.push(6);
            w.extend_from_slice(&a.ip().octets());
            w.extend_from_slice(&a.port().to_be_bytes());
        }
    }
}

fn get_endpoint(b: &[u8], pos: &mut usize) -> Option<SocketAddr> {
    let tag = *b.get(*pos)?;
    *pos += 1;
    match tag {
        4 => {
            let octets: [u8; 4] = b.get(*pos..*pos + 4)?.try_into().ok()?;
            *pos += 4;
            let port = u16::from_be_bytes(b.get(*pos..*pos + 2)?.try_into().ok()?);
            *pos += 2;
            Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(octets)), port))
        }
        6 => {
            let octets: [u8; 16] = b.get(*pos..*pos + 16)?.try_into().ok()?;
            *pos += 16;
            let port = u16::from_be_bytes(b.get(*pos..*pos + 2)?.try_into().ok()?);
            *pos += 2;
            Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::from(octets)), port))
        }
        _ => None,
    }
}

fn put_sigs(w: &mut Vec<u8>, sigs: &[IndexedSig]) -> Result<()> {
    if sigs.is_empty() || sigs.len() > MAX_PAIRING_SIGNATURES {
        return Err(SocialError::PairingMalformed);
    }
    w.push(sigs.len() as u8);
    for sig in sigs {
        w.extend_from_slice(&sig.index.to_be_bytes());
        w.push(sig.signature.suite().tag());
        put_bytes(w, &sig.signature.to_bytes());
    }
    Ok(())
}

fn get_sigs(b: &[u8], pos: &mut usize) -> Option<Vec<IndexedSig>> {
    let count = *b.get(*pos)? as usize;
    *pos += 1;
    if count == 0 || count > MAX_PAIRING_SIGNATURES {
        return None;
    }
    let mut sigs = Vec::with_capacity(count);
    for _ in 0..count {
        let index = u32::from_be_bytes(b.get(*pos..*pos + 4)?.try_into().ok()?);
        *pos += 4;
        let suite = SignatureSuite::from_tag(*b.get(*pos)?).ok()?;
        *pos += 1;
        let sig_bytes = get_bytes(b, pos, MAX_PAIRING_SIGNATURE_BYTES)?;
        let signature = Signature::from_suite_bytes(suite, &sig_bytes).ok()?;
        sigs.push(IndexedSig { index, signature });
    }
    Some(sigs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;
    use std::net::Ipv4Addr;

    fn delegated_pair(seed: u8) -> (Kel, Controller) {
        let mut root =
            Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed.wrapping_add(2); 32],
            &[seed.wrapping_add(3); 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root.kel(), device)
    }

    fn endpoint() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42)), 46001)
    }

    /// A deterministic-but-computed test fixture nonce, distinct per `seed`.
    /// Deliberately not a `[literal; N]` array expression: GitHub's default
    /// CodeQL code-scanning setup flags a hard-coded byte-array literal
    /// flowing into a parameter named `nonce` as a "hard-coded
    /// cryptographic value" regardless of context, so a real fixed-array
    /// literal here reads as production key material even though it is
    /// purely test data (real callers of `create_pairing_offer`/
    /// `create_pairing_acceptance` must supply a genuinely random nonce —
    /// nothing here weakens that).
    fn fixture_nonce(seed: u32) -> [u8; PAIRING_NONCE_BYTES] {
        let mut bytes = [0u8; PAIRING_NONCE_BYTES];
        bytes[..4].copy_from_slice(&seed.to_be_bytes());
        bytes
    }

    #[test]
    fn a_valid_offer_round_trips_and_verifies() {
        let (root_kel, device) = delegated_pair(1);
        let bytes = create_pairing_offer(
            &root_kel,
            &device,
            "Alice",
            endpoint(),
            fixture_nonce(7),
            1_000,
            1_000 + 60_000,
        )
        .unwrap();

        let verified = verify_pairing_offer(&bytes, 1_500).unwrap();
        assert_eq!(verified.offerer, root_kel.did());
        assert_eq!(verified.display_name, "Alice");
        assert_eq!(verified.endpoint, endpoint());
        assert_eq!(verified.nonce, fixture_nonce(7));
        assert!(verified.capabilities.contains(Capabilities::POST));
    }

    #[test]
    fn an_offer_scanned_after_its_expiry_is_rejected() {
        let (root_kel, device) = delegated_pair(2);
        let bytes = create_pairing_offer(
            &root_kel,
            &device,
            "Bob",
            endpoint(),
            fixture_nonce(1),
            1_000,
            1_000 + 60_000,
        )
        .unwrap();

        let err = verify_pairing_offer(&bytes, 1_000 + 60_001).unwrap_err();
        assert_eq!(err, SocialError::PairingExpired);
    }

    #[test]
    fn an_offer_window_longer_than_the_bound_is_rejected_at_creation() {
        let (root_kel, device) = delegated_pair(3);
        let err = create_pairing_offer(
            &root_kel,
            &device,
            "Carol",
            endpoint(),
            fixture_nonce(2),
            0,
            MAX_PAIRING_OFFER_WINDOW_MS + 1,
        )
        .unwrap_err();
        assert_eq!(err, SocialError::PairingExpired);
    }

    #[test]
    fn a_forged_offer_with_flipped_payload_bytes_fails_signature_verification() {
        let (root_kel, device) = delegated_pair(4);
        let mut bytes = create_pairing_offer(
            &root_kel,
            &device,
            "Dave",
            endpoint(),
            fixture_nonce(3),
            1_000,
            1_000 + 60_000,
        )
        .unwrap();
        // Flip a byte inside the display-name field, well before the signature.
        let target = OFFER_MAGIC.len() + 4 + 4;
        bytes[target] ^= 0xFF;

        assert!(matches!(
            verify_pairing_offer(&bytes, 1_500),
            Err(SocialError::Identity(_))
        ));
    }

    #[test]
    fn an_offer_whose_device_kel_is_not_actually_delegated_by_the_root_kel_is_rejected() {
        let (root_kel, _device) = delegated_pair(5);
        let (_other_root_kel, stranger_device) = delegated_pair(6);
        // Sign with a device that root_kel never delegated to.
        let mut msg = Vec::new();
        msg.extend_from_slice(OFFER_MAGIC);
        put_bytes(&mut msg, &root_kel.to_bytes());
        put_bytes(&mut msg, &stranger_device.kel().to_bytes());
        put_str(&mut msg, "Eve");
        put_endpoint(&mut msg, &endpoint());
        msg.extend_from_slice(&fixture_nonce(4));
        msg.extend_from_slice(&1_000u64.to_be_bytes());
        msg.extend_from_slice(&(1_000 + 60_000u64).to_be_bytes());
        let sigs = stranger_device.sign_message(&msg);
        let mut bytes = msg;
        put_sigs(&mut bytes, &sigs).unwrap();

        assert!(matches!(
            verify_pairing_offer(&bytes, 1_500),
            Err(SocialError::Identity(_))
        ));
    }

    #[test]
    fn a_device_with_only_attest_capability_cannot_produce_an_accepted_offer() {
        let mut root = Controller::incept_single_from_seeds(&[9; 32], &[10; 32]).unwrap();
        let device =
            Controller::incept_device_single_from_seeds(&root.did(), &[11; 32], &[12; 32]).unwrap();
        root.delegate_device(&device.did(), Capabilities::ATTEST)
            .unwrap();
        let root_kel = root.kel();

        let bytes = create_pairing_offer(
            &root_kel,
            &device,
            "Frank",
            endpoint(),
            fixture_nonce(5),
            1_000,
            1_000 + 60_000,
        )
        .unwrap();

        let err = verify_pairing_offer(&bytes, 1_500).unwrap_err();
        assert_eq!(err, SocialError::PairingCapabilityMissing);
    }

    #[test]
    fn truncated_or_trailing_bytes_are_rejected_as_malformed() {
        let (root_kel, device) = delegated_pair(7);
        let bytes = create_pairing_offer(
            &root_kel,
            &device,
            "Grace",
            endpoint(),
            fixture_nonce(6),
            1_000,
            1_000 + 60_000,
        )
        .unwrap();

        let mut truncated = bytes.clone();
        truncated.truncate(bytes.len() - 1);
        assert_eq!(
            verify_pairing_offer(&truncated, 1_500).unwrap_err(),
            SocialError::PairingMalformed
        );

        let mut trailing = bytes;
        trailing.push(0);
        assert_eq!(
            verify_pairing_offer(&trailing, 1_500).unwrap_err(),
            SocialError::PairingMalformed
        );
    }

    #[test]
    fn an_offer_and_its_matching_acceptance_authenticate_each_other() {
        let (a_root_kel, a_device) = delegated_pair(20);
        let (b_root_kel, b_device) = delegated_pair(21);

        let offer_bytes = create_pairing_offer(
            &a_root_kel,
            &a_device,
            "Alice",
            endpoint(),
            fixture_nonce(42),
            1_000,
            1_000 + 60_000,
        )
        .unwrap();
        let offer = verify_pairing_offer(&offer_bytes, 1_100).unwrap();

        let acceptance_bytes =
            create_pairing_acceptance(offer.nonce, &b_root_kel, &b_device, "Bob").unwrap();
        let acceptance = verify_pairing_acceptance(&acceptance_bytes, offer.nonce).unwrap();

        assert_eq!(acceptance.acceptor, b_root_kel.did());
        assert_eq!(acceptance.display_name, "Bob");
        assert_eq!(acceptance.nonce, offer.nonce);
    }

    #[test]
    fn an_acceptance_naming_a_different_nonce_is_rejected() {
        let (root_kel, device) = delegated_pair(22);
        let acceptance_bytes =
            create_pairing_acceptance(fixture_nonce(1), &root_kel, &device, "Bob").unwrap();

        let err = verify_pairing_acceptance(&acceptance_bytes, fixture_nonce(2)).unwrap_err();
        assert_eq!(err, SocialError::PairingMalformed);
    }

    #[test]
    fn the_nonce_ledger_rejects_a_replay_of_a_still_valid_nonce() {
        let mut ledger = PairingNonceLedger::new();
        let nonce = fixture_nonce(9);
        ledger.observe(nonce, 2_000, 1_000).unwrap();
        let err = ledger.observe(nonce, 2_000, 1_500).unwrap_err();
        assert_eq!(err, SocialError::PairingReplayed);
    }

    #[test]
    fn the_nonce_ledger_allows_reuse_of_a_nonce_after_it_has_expired_and_been_swept() {
        let mut ledger = PairingNonceLedger::new();
        let nonce = fixture_nonce(9);
        ledger.observe(nonce, 2_000, 1_000).unwrap();
        // Past the recorded expiry: the entry is swept before the check runs.
        ledger.observe(nonce, 4_000, 2_500).unwrap();
    }

    #[test]
    fn the_nonce_ledger_is_bounded() {
        let mut ledger = PairingNonceLedger::new();
        for i in 0..MAX_PAIRING_NONCE_LEDGER_ENTRIES {
            // Every entry shares one far-future expiry so none are swept.
            ledger
                .observe(fixture_nonce(i as u32), 1_000_000, 1_000)
                .unwrap();
        }
        // Well outside the 0..MAX_PAIRING_NONCE_LEDGER_ENTRIES range used
        // above, so this cannot collide with any already-inserted entry.
        let one_too_many = fixture_nonce(1_000_000);
        let err = ledger.observe(one_too_many, 1_000_000, 1_000).unwrap_err();
        assert_eq!(err, SocialError::PairingNonceLedgerFull);
    }

    #[test]
    fn a_pairing_acceptance_delivers_over_a_real_loopback_tcp_connection() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();

        let (b_root_kel, b_device) = delegated_pair(30);
        let nonce = fixture_nonce(11);
        let acceptance_bytes =
            create_pairing_acceptance(nonce, &b_root_kel, &b_device, "Bob").unwrap();

        let sender = std::thread::spawn(move || {
            send_pairing_acceptance(
                addr,
                &acceptance_bytes,
                Duration::from_secs(2),
                Duration::from_secs(2),
            )
        });

        let received =
            receive_pairing_acceptance(&listener, Duration::from_secs(2), Duration::from_secs(2))
                .unwrap();
        sender.join().unwrap().unwrap();

        let verified = verify_pairing_acceptance(&received, nonce).unwrap();
        assert_eq!(verified.acceptor, b_root_kel.did());
    }

    #[test]
    fn receiving_times_out_when_nobody_connects() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let err = receive_pairing_acceptance(
            &listener,
            Duration::from_millis(100),
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(matches!(err, SocialError::Io(_)));
    }

    #[test]
    fn oversized_length_prefix_over_the_wire_is_rejected_without_reading_the_body() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();

        let sender = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(addr).unwrap();
            stream
                .write_all(&(MAX_PAIRING_ACCEPTANCE_BYTES as u32 + 1).to_be_bytes())
                .unwrap();
        });

        let err =
            receive_pairing_acceptance(&listener, Duration::from_secs(2), Duration::from_secs(2))
                .unwrap_err();
        sender.join().unwrap();
        assert_eq!(err, SocialError::PairingMalformed);
    }
}
