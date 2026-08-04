#!/usr/bin/env python3
"""Apply PR #296 stage 1: authenticated runtime and verified onion route."""

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
        raise SystemExit(f"{path}: expected {expected} matches, found {count}: {old[:100]!r}")
    write(path, text.replace(old, new))


cargo = "crates/mini-transport-security/Cargo.toml"
auth = "crates/mini-transport-security/src/auth.rs"
error = "crates/mini-transport-security/src/error.rs"
lib = "crates/mini-transport-security/src/lib.rs"

replace_exact(
    cargo,
    'mini-privacy-policy = { path = "../mini-privacy-policy" }\n',
    'mini-privacy-policy = { path = "../mini-privacy-policy" }\n'
    'mini-relay = { path = "../mini-relay" }\n'
    'mini-transport-policy = { path = "../mini-transport-policy" }\n\n'
    '[dev-dependencies]\n'
    'mini-bridge = { path = "../mini-bridge" }\n',
)

replace_exact(
    auth,
    """pub enum TransportPurpose {
    PeerExchange,
    Relay,
    Messaging,
    StateSync,
    Consensus,
}
""",
    """pub enum TransportPurpose {
    PeerExchange,
    Relay,
    Messaging,
    StateSync,
    Consensus,
    /// One live remote-search request/response exchange. This purpose is
    /// distinct so a proof disclosed for generic peer exchange cannot be
    /// replayed as authenticated search-provider provenance.
    SearchQuery,
}
""",
)
replace_exact(
    auth,
    """            Self::StateSync => 4,
            Self::Consensus => 5,
""",
    """            Self::StateSync => 4,
            Self::Consensus => 5,
            Self::SearchQuery => 6,
""",
)
replace_exact(
    auth,
    """            4 => Ok(Self::StateSync),
            5 => Ok(Self::Consensus),
""",
    """            4 => Ok(Self::StateSync),
            5 => Ok(Self::Consensus),
            6 => Ok(Self::SearchQuery),
""",
)
replace_exact(
    auth,
    """            Self::PeerExchange | Self::Relay | Self::Messaging | Self::StateSync => {
                Capabilities::SIGN
            }
""",
    """            Self::PeerExchange
            | Self::Relay
            | Self::Messaging
            | Self::StateSync
            | Self::SearchQuery => Capabilities::SIGN,
""",
)

replace_exact(
    error,
    """use did_mini::IdentityError;
use mini_crypto::CryptoError;
""",
    """use did_mini::IdentityError;
use mini_bearer::BearerError;
use mini_crypto::CryptoError;
use mini_relay::RelayError;
""",
)
replace_exact(
    error,
    """    InvalidSelectionPolicy,
    MixedTransportNotReviewed,
    Identity(IdentityError),
    Crypto(CryptoError),
""",
    """    InvalidSelectionPolicy,
    MixedTransportNotReviewed,
    /// Every bounded dial candidate failed before a fully authenticated
    /// connection existed. No partially verified state is returned.
    DialExhausted { attempted: usize },
    /// Two onion roles reused a visible endpoint, routing key, root, or device.
    RouteEndpointReuse,
    Bearer(BearerError),
    Relay(RelayError),
    Identity(IdentityError),
    Crypto(CryptoError),
""",
)
replace_exact(
    error,
    """            Self::MixedTransportNotReviewed => write!(
                f,
                "mixed/burst transport is unavailable until the exact executor receives independent review"
            ),
            Self::Identity(error) => write!(f, "identity verification failed: {error}"),
            Self::Crypto(error) => write!(f, "cryptographic operation failed: {error}"),
""",
    """            Self::MixedTransportNotReviewed => write!(
                f,
                "mixed/burst transport is unavailable until the exact executor receives independent review"
            ),
            Self::DialExhausted { attempted } => write!(
                f,
                "all {attempted} bounded transport candidates failed authentication or connection"
            ),
            Self::RouteEndpointReuse => write!(
                f,
                "one visible transport endpoint, routing key, root, or device was assigned multiple onion roles"
            ),
            Self::Bearer(error) => write!(f, "bearer/channel operation failed: {error}"),
            Self::Relay(error) => write!(f, "relay/onion operation failed: {error}"),
            Self::Identity(error) => write!(f, "identity verification failed: {error}"),
            Self::Crypto(error) => write!(f, "cryptographic operation failed: {error}"),
""",
)
replace_exact(
    error,
    """impl From<IdentityError> for TransportSecurityError {
""",
    """impl From<BearerError> for TransportSecurityError {
    fn from(error: BearerError) -> Self {
        Self::Bearer(error)
    }
}

impl From<RelayError> for TransportSecurityError {
    fn from(error: RelayError) -> Self {
        Self::Relay(error)
    }
}

impl From<IdentityError> for TransportSecurityError {
""",
)

replace_exact(lib, "mod replay;\nmod selection;\n", "mod replay;\nmod runtime;\nmod selection;\n")
replace_exact(
    lib,
    """pub use replay::{ReplayCache, MAX_REPLAY_CACHE_ENTRIES};
pub use selection::{
""",
    """pub use replay::{ReplayCache, MAX_REPLAY_CACHE_ENTRIES};
pub use runtime::{
    authenticate_established_initiator, authenticate_established_responder,
    build_verified_onion_route, connect_authenticated_tcp, connect_first_authenticated_tcp,
    AuthenticatedConnection, AuthenticatedDialTarget, LocalSessionIdentity, PeerExpectation,
    VerifiedRelay, SESSION_AUTH_FRAME_AAD,
};
pub use selection::{
""",
)

runtime = r'''//! Executable convergence of discovery, CH1, identity, retry, and onion routing.
//!
//! The lower-level modules intentionally remain reusable primitives. This module
//! is the safety seam a normal caller should use when it needs a named peer: one
//! value owns the bearer, the exact CH1 channel, and the identity verified on
//! that channel. Discovery never becomes trust, and an authenticated identity is
//! not returned separately from the connection it authenticated.

use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use did_mini::{Controller, Did, FreshnessPins, Kel};
use mini_bearer::{Bearer, BearerError, Channel, Initiator, TcpBearer};
use mini_crypto::AgreementPublicKey;
use mini_relay::{
    build_onion, ConnectionId, OnionHop, OnionPacket, RelayRole,
};
use mini_transport_policy::PayloadSizeClass;

use crate::{
    diverse_dial_plan, AuthenticatedPeer, PeerSelectionPolicy, ReplayCache, Result,
    SessionAuthClaim, SessionRole, TransportPurpose, TransportSecurityError,
    VerifiedPeerAdvertisement, MAX_DIAL_TIMEOUT_MS, MIN_DIAL_TIMEOUT_MS,
};

/// AEAD associated data for encrypted authentication claims on CH1.
pub const SESSION_AUTH_FRAME_AAD: &[u8] = b"MINI/TRANSPORT-AUTH1";

/// Local identity disclosed for one typed authenticated session.
#[derive(Debug, Clone, Copy)]
pub struct LocalSessionIdentity<'a> {
    pub root: &'a Did,
    pub device: &'a Controller,
    pub routing_key: AgreementPublicKey,
}

impl<'a> LocalSessionIdentity<'a> {
    pub const fn new(
        root: &'a Did,
        device: &'a Controller,
        routing_key: AgreementPublicKey,
    ) -> Self {
        Self {
            root,
            device,
            routing_key,
        }
    }
}

/// What the remote side must prove on this exact channel.
#[derive(Debug, Clone, Copy)]
pub enum PeerExpectation<'a> {
    /// Verify a known identity without making a discovery-address claim.
    Identity {
        root_kel: &'a Kel,
        device_kel: &'a Kel,
    },
    /// Verify the live peer against the exact signed record selected for dial.
    Advertised {
        advertisement: &'a VerifiedPeerAdvertisement,
        root_kel: &'a Kel,
        device_kel: &'a Kel,
    },
}

impl<'a> PeerExpectation<'a> {
    pub const fn identity(root_kel: &'a Kel, device_kel: &'a Kel) -> Self {
        Self::Identity {
            root_kel,
            device_kel,
        }
    }

    pub const fn advertised(
        advertisement: &'a VerifiedPeerAdvertisement,
        root_kel: &'a Kel,
        device_kel: &'a Kel,
    ) -> Self {
        Self::Advertised {
            advertisement,
            root_kel,
            device_kel,
        }
    }
}

/// One secure-discovery target and the KEL material needed to verify it live.
#[derive(Debug, Clone, Copy)]
pub struct AuthenticatedDialTarget<'a> {
    pub advertisement: &'a VerifiedPeerAdvertisement,
    pub root_kel: &'a Kel,
    pub device_kel: &'a Kel,
}

impl<'a> AuthenticatedDialTarget<'a> {
    pub const fn new(
        advertisement: &'a VerifiedPeerAdvertisement,
        root_kel: &'a Kel,
        device_kel: &'a Kel,
    ) -> Self {
        Self {
            advertisement,
            root_kel,
            device_kel,
        }
    }
}

/// A transport connection whose peer identity is inseparable from the channel
/// on which it was proved.
#[derive(Debug)]
pub struct AuthenticatedConnection<B: Bearer> {
    bearer: B,
    channel: Channel,
    peer: AuthenticatedPeer,
}

impl<B: Bearer> AuthenticatedConnection<B> {
    pub fn peer(&self) -> &AuthenticatedPeer {
        &self.peer
    }

    pub fn channel_binding(&self) -> [u8; 32] {
        self.channel.channel_binding()
    }

    /// Encrypt and send one application frame on the authenticated channel.
    pub fn send(&mut self, plaintext: &[u8], aad: &[u8]) -> Result<()> {
        let ciphertext = self.channel.seal(plaintext, aad)?;
        self.bearer.send(&ciphertext)?;
        Ok(())
    }

    /// Receive, authenticate, and decrypt one application frame.
    pub fn recv(&mut self, aad: &[u8]) -> Result<Vec<u8>> {
        let ciphertext = self.bearer.recv()?;
        Ok(self.channel.open(&ciphertext, aad)?)
    }
}

/// Authenticate an already-established initiator-side channel. The responder
/// proves itself first, so a redirected endpoint cannot collect the initiator's
/// DID before matching the selected advertisement.
#[allow(clippy::too_many_arguments)]
pub fn authenticate_established_initiator<B: Bearer>(
    mut bearer: B,
    mut channel: Channel,
    local: LocalSessionIdentity<'_>,
    purpose: TransportPurpose,
    issued_at_ms: u64,
    expires_at_ms: u64,
    now_ms: u64,
    expected_peer: PeerExpectation<'_>,
    freshness: &mut FreshnessPins,
    replay: &mut ReplayCache,
) -> Result<AuthenticatedConnection<B>> {
    let encrypted_claim = bearer.recv()?;
    let claim_bytes = channel.open(&encrypted_claim, SESSION_AUTH_FRAME_AAD)?;
    let claim = SessionAuthClaim::from_bytes(&claim_bytes)?;

    let mut staged_freshness = freshness.clone();
    let mut staged_replay = replay.clone();
    let peer = verify_expected_claim(
        &claim,
        expected_peer,
        SessionRole::Responder,
        purpose,
        &channel.channel_binding(),
        now_ms,
        &mut staged_freshness,
        &mut staged_replay,
    )?;

    let local_claim = SessionAuthClaim::issue(
        local.root,
        local.device,
        SessionRole::Initiator,
        purpose,
        local.routing_key,
        &channel.channel_binding(),
        issued_at_ms,
        expires_at_ms,
    )?;
    let encrypted_local = channel.seal(&local_claim.to_bytes()?, SESSION_AUTH_FRAME_AAD)?;
    bearer.send(&encrypted_local)?;

    *freshness = staged_freshness;
    *replay = staged_replay;
    Ok(AuthenticatedConnection {
        bearer,
        channel,
        peer,
    })
}

/// Authenticate an already-established responder-side channel. The responder
/// sends its channel-bound proof first; it commits peer freshness/replay state
/// only after the initiator's complete proof verifies.
#[allow(clippy::too_many_arguments)]
pub fn authenticate_established_responder<B: Bearer>(
    mut bearer: B,
    mut channel: Channel,
    local: LocalSessionIdentity<'_>,
    purpose: TransportPurpose,
    issued_at_ms: u64,
    expires_at_ms: u64,
    now_ms: u64,
    expected_peer: PeerExpectation<'_>,
    freshness: &mut FreshnessPins,
    replay: &mut ReplayCache,
) -> Result<AuthenticatedConnection<B>> {
    let local_claim = SessionAuthClaim::issue(
        local.root,
        local.device,
        SessionRole::Responder,
        purpose,
        local.routing_key,
        &channel.channel_binding(),
        issued_at_ms,
        expires_at_ms,
    )?;
    let encrypted_local = channel.seal(&local_claim.to_bytes()?, SESSION_AUTH_FRAME_AAD)?;
    bearer.send(&encrypted_local)?;

    let encrypted_claim = bearer.recv()?;
    let claim_bytes = channel.open(&encrypted_claim, SESSION_AUTH_FRAME_AAD)?;
    let claim = SessionAuthClaim::from_bytes(&claim_bytes)?;

    let mut staged_freshness = freshness.clone();
    let mut staged_replay = replay.clone();
    let peer = verify_expected_claim(
        &claim,
        expected_peer,
        SessionRole::Initiator,
        purpose,
        &channel.channel_binding(),
        now_ms,
        &mut staged_freshness,
        &mut staged_replay,
    )?;

    *freshness = staged_freshness;
    *replay = staged_replay;
    Ok(AuthenticatedConnection {
        bearer,
        channel,
        peer,
    })
}

#[allow(clippy::too_many_arguments)]
fn verify_expected_claim(
    claim: &SessionAuthClaim,
    expected_peer: PeerExpectation<'_>,
    role: SessionRole,
    purpose: TransportPurpose,
    binding: &[u8; 32],
    now_ms: u64,
    freshness: &mut FreshnessPins,
    replay: &mut ReplayCache,
) -> Result<AuthenticatedPeer> {
    match expected_peer {
        PeerExpectation::Identity {
            root_kel,
            device_kel,
        } => claim.verify(
            role,
            purpose,
            binding,
            now_ms,
            root_kel,
            device_kel,
            freshness,
            replay,
        ),
        PeerExpectation::Advertised {
            advertisement,
            root_kel,
            device_kel,
        } => claim.verify_advertised(
            advertisement,
            role,
            purpose,
            binding,
            now_ms,
            root_kel,
            device_kel,
            freshness,
            replay,
        ),
    }
}

/// Dial one signed endpoint over TCP, establish CH1, and authenticate the live
/// responder against that exact endpoint record.
#[allow(clippy::too_many_arguments)]
pub fn connect_authenticated_tcp(
    local: LocalSessionIdentity<'_>,
    purpose: TransportPurpose,
    issued_at_ms: u64,
    expires_at_ms: u64,
    now_ms: u64,
    target: AuthenticatedDialTarget<'_>,
    timeout_ms: u64,
    freshness: &mut FreshnessPins,
    replay: &mut ReplayCache,
) -> Result<AuthenticatedConnection<TcpBearer>> {
    let (bearer, channel) = establish_tcp_initiator(target.advertisement.address(), timeout_ms)?;
    authenticate_established_initiator(
        bearer,
        channel,
        local,
        purpose,
        issued_at_ms,
        expires_at_ms,
        now_ms,
        PeerExpectation::advertised(
            target.advertisement,
            target.root_kel,
            target.device_kel,
        ),
        freshness,
        replay,
    )
}

/// Try a locally seeded, prefix-diverse dial plan in bounded order. A failed
/// connection or identity proof is discarded whole; no failed attempt mutates
/// the caller's freshness pins or replay cache.
#[allow(clippy::too_many_arguments)]
pub fn connect_first_authenticated_tcp(
    local: LocalSessionIdentity<'_>,
    purpose: TransportPurpose,
    issued_at_ms: u64,
    expires_at_ms: u64,
    now_ms: u64,
    targets: &[AuthenticatedDialTarget<'_>],
    local_seed: [u8; 32],
    policy: PeerSelectionPolicy,
    freshness: &mut FreshnessPins,
    replay: &mut ReplayCache,
) -> Result<AuthenticatedConnection<TcpBearer>> {
    let records: Vec<_> = targets
        .iter()
        .map(|target| target.advertisement.clone())
        .collect();
    let plan = diverse_dial_plan(&records, local_seed, policy)?;
    let mut attempted = 0usize;

    for attempt in plan {
        let Some(target) = targets
            .iter()
            .copied()
            .find(|target| {
                target.advertisement.endpoint_id() == attempt.endpoint_id
                    && target.advertisement.address() == attempt.address
                    && target.advertisement.routing_key() == attempt.routing_key
            })
        else {
            continue;
        };
        attempted += 1;
        if let Ok(connection) = connect_authenticated_tcp(
            local,
            purpose,
            issued_at_ms,
            expires_at_ms,
            now_ms,
            target,
            attempt.timeout_ms,
            freshness,
            replay,
        ) {
            return Ok(connection);
        }
    }

    Err(TransportSecurityError::DialExhausted { attempted })
}

fn establish_tcp_initiator(
    address: SocketAddr,
    timeout_ms: u64,
) -> Result<(TcpBearer, Channel)> {
    if !(MIN_DIAL_TIMEOUT_MS..=MAX_DIAL_TIMEOUT_MS).contains(&timeout_ms) {
        return Err(TransportSecurityError::InvalidSelectionPolicy);
    }
    let timeout = Duration::from_millis(timeout_ms);
    let stream = TcpStream::connect_timeout(&address, timeout).map_err(BearerError::from)?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(BearerError::from)?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(BearerError::from)?;
    let mut bearer = TcpBearer::from_stream(stream)?;
    let (initiator, hello) = Initiator::start()?;
    bearer.send(&hello)?;
    let response = bearer.recv()?;
    let channel = initiator.finish(&response)?;
    Ok((bearer, channel))
}

/// One already-verified candidate for a fixed onion role.
#[derive(Debug, Clone, Copy)]
pub struct VerifiedRelay<'a> {
    pub advertisement: &'a VerifiedPeerAdvertisement,
    pub next_hop: &'a [u8],
}

impl<'a> VerifiedRelay<'a> {
    pub const fn new(
        advertisement: &'a VerifiedPeerAdvertisement,
        next_hop: &'a [u8],
    ) -> Self {
        Self {
            advertisement,
            next_hop,
        }
    }
}

/// Build the executable three-hop onion only from three visibly distinct,
/// verified transport endpoints. Distinct pairwise roots can still belong to one
/// hidden operator; that residual operator-independence limit remains explicit.
#[allow(clippy::too_many_arguments)]
pub fn build_verified_onion_route(
    relays: [VerifiedRelay<'_>; 3],
    destination_connection_id: ConnectionId,
    size_class: PayloadSizeClass,
    destination_key: AgreementPublicKey,
    plaintext: &[u8],
    expires_at_ms: u64,
) -> Result<OnionPacket> {
    for left in 0..relays.len() {
        for right in left + 1..relays.len() {
            let a = relays[left].advertisement;
            let b = relays[right].advertisement;
            if a.endpoint_id() == b.endpoint_id()
                || a.routing_key() == b.routing_key()
                || a.root() == b.root()
                || a.device() == b.device()
            {
                return Err(TransportSecurityError::RouteEndpointReuse);
            }
        }
    }

    let roles = [RelayRole::Entry, RelayRole::Rendezvous, RelayRole::Delivery];
    let hops: Vec<_> = relays
        .iter()
        .zip(roles)
        .map(|(relay, role)| OnionHop {
            role,
            routing_key: relay.advertisement.routing_key(),
            next_hop: relay.next_hop.to_vec(),
        })
        .collect();
    Ok(build_onion(
        destination_connection_id,
        size_class,
        &hops,
        destination_key,
        plaintext,
        expires_at_ms,
    )?)
}

#[cfg(test)]
mod tests {
    use did_mini::{Capabilities, Controller, FreshnessPins};
    use mini_crypto::AgreementSecretKey;

    use super::*;
    use crate::{PeerAdvertisement, ReplayCache};

    fn verified(seed: u8, address: &str) -> VerifiedPeerAdvertisement {
        let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed + 2; 32],
            &[seed + 3; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        let routing = AgreementSecretKey::from_seed(&[seed + 4; 32]).public_key();
        let advertisement = PeerAdvertisement::issue(
            [7; 32],
            &root.did(),
            &device,
            routing,
            address.parse().unwrap(),
            1_000,
            2_000,
        )
        .unwrap();
        let mut freshness = FreshnessPins::new();
        let mut replay = ReplayCache::new(8).unwrap();
        advertisement
            .verify(
                [7; 32],
                1_500,
                &root.kel(),
                &device.kel(),
                &mut freshness,
                &mut replay,
            )
            .unwrap()
    }

    #[test]
    fn verified_route_rejects_reusing_one_endpoint_for_two_roles() {
        let a = verified(10, "10.0.0.1:9000");
        let b = verified(20, "10.0.1.1:9000");
        let destination = AgreementSecretKey::from_seed(&[99; 32]);
        let result = build_verified_onion_route(
            [
                VerifiedRelay::new(&a, b"rendezvous"),
                VerifiedRelay::new(&a, b"delivery"),
                VerifiedRelay::new(&b, b"destination"),
            ],
            ConnectionId::from_bytes([1; 16]),
            PayloadSizeClass::Small,
            destination.public_key(),
            b"payload",
            10_000,
        );
        assert_eq!(result, Err(TransportSecurityError::RouteEndpointReuse));
    }

    #[test]
    fn three_distinct_verified_endpoints_build_an_onion() {
        let a = verified(10, "10.0.0.1:9000");
        let b = verified(20, "10.0.1.1:9000");
        let c = verified(30, "10.0.2.1:9000");
        let destination = AgreementSecretKey::from_seed(&[99; 32]);
        let packet = build_verified_onion_route(
            [
                VerifiedRelay::new(&a, b"rendezvous"),
                VerifiedRelay::new(&b, b"delivery"),
                VerifiedRelay::new(&c, b"destination"),
            ],
            ConnectionId::from_bytes([1; 16]),
            PayloadSizeClass::Small,
            destination.public_key(),
            b"payload",
            10_000,
        )
        .unwrap();
        assert_eq!(packet.hop_index, 0);
    }
}
'''
write("crates/mini-transport-security/src/runtime.rs", runtime)

runtime_test = r'''use std::net::{SocketAddr, TcpListener};
use std::thread;

use did_mini::{Capabilities, Controller, FreshnessPins, Kel};
use mini_bearer::{Bearer, Responder, TcpBearer};
use mini_bridge::{
    BridgeDescriptor, DirectBridgeTransport, OpaqueEndpoint, PluggableTransport,
    TransportId, TransportParameters,
};
use mini_crypto::{AgreementPublicKey, AgreementSecretKey};
use mini_transport_security::{
    authenticate_established_initiator, authenticate_established_responder,
    connect_authenticated_tcp, connect_first_authenticated_tcp, diverse_dial_plan,
    AuthenticatedDialTarget, LocalSessionIdentity, PeerAdvertisement, PeerExpectation,
    PeerSelectionPolicy, ReplayCache, TransportPurpose, TransportSecurityError,
    VerifiedPeerAdvertisement,
};

const NETWORK_ID: [u8; 32] = [7; 32];
const APP_AAD: &[u8] = b"runtime-convergence-test";

struct Identity {
    root: Controller,
    device: Controller,
    routing: AgreementPublicKey,
}

impl Identity {
    fn new(seed: u8) -> Self {
        let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed + 2; 32],
            &[seed + 3; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        let routing = AgreementSecretKey::from_seed(&[seed + 4; 32]).public_key();
        Self {
            root,
            device,
            routing,
        }
    }

    fn local(&self) -> LocalSessionIdentity<'_> {
        LocalSessionIdentity::new(&self.root.did(), &self.device, self.routing)
    }
}

fn listener() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    (listener, address)
}

fn verified_advertisement(
    identity: &Identity,
    address: SocketAddr,
) -> VerifiedPeerAdvertisement {
    let advertisement = PeerAdvertisement::issue(
        NETWORK_ID,
        &identity.root.did(),
        &identity.device,
        identity.routing,
        address,
        1_000,
        2_000,
    )
    .unwrap();
    let mut freshness = FreshnessPins::new();
    let mut replay = ReplayCache::new(16).unwrap();
    advertisement
        .verify(
            NETWORK_ID,
            1_500,
            &identity.root.kel(),
            &identity.device.kel(),
            &mut freshness,
            &mut replay,
        )
        .unwrap()
}

fn responder_channel(mut bearer: TcpBearer) -> (TcpBearer, mini_bearer::Channel) {
    let hello = bearer.recv().unwrap();
    let (channel, response) = Responder::respond(&hello).unwrap();
    bearer.send(&response).unwrap();
    (bearer, channel)
}

#[test]
fn signed_discovery_real_ch1_and_application_data_are_one_runtime_object() {
    let client = Identity::new(10);
    let server = Identity::new(40);
    let (listener, address) = listener();
    let advertisement = verified_advertisement(&server, address);
    let server_root_kel = server.root.kel();
    let server_device_kel = server.device.kel();
    let client_root_kel = client.root.kel();
    let client_device_kel = client.device.kel();

    let server_thread = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let (bearer, channel) = responder_channel(TcpBearer::from_stream(stream).unwrap());
        let mut freshness = FreshnessPins::new();
        let mut replay = ReplayCache::new(32).unwrap();
        let mut connection = authenticate_established_responder(
            bearer,
            channel,
            server.local(),
            TransportPurpose::PeerExchange,
            1_000,
            2_000,
            1_500,
            PeerExpectation::identity(&client_root_kel, &client_device_kel),
            &mut freshness,
            &mut replay,
        )
        .unwrap();
        let request = connection.recv(APP_AAD).unwrap();
        connection.send(b"authenticated reply", APP_AAD).unwrap();
        (request, connection.peer().root.clone())
    });

    let mut freshness = FreshnessPins::new();
    let mut replay = ReplayCache::new(32).unwrap();
    let mut connection = connect_authenticated_tcp(
        client.local(),
        TransportPurpose::PeerExchange,
        1_000,
        2_000,
        1_500,
        AuthenticatedDialTarget::new(
            &advertisement,
            &server_root_kel,
            &server_device_kel,
        ),
        5_000,
        &mut freshness,
        &mut replay,
    )
    .unwrap();
    assert_eq!(connection.peer().endpoint_id, advertisement.endpoint_id());
    connection.send(b"authenticated request", APP_AAD).unwrap();
    assert_eq!(connection.recv(APP_AAD).unwrap(), b"authenticated reply");

    let (request, observed_client) = server_thread.join().unwrap();
    assert_eq!(request, b"authenticated request");
    assert_eq!(observed_client, client.root.did());
}

#[test]
fn redirected_genuine_identity_is_rejected_before_client_identity_disclosure() {
    let client = Identity::new(10);
    let advertised = Identity::new(40);
    let redirect = Identity::new(70);
    let (listener, address) = listener();
    let advertisement = verified_advertisement(&advertised, address);
    let advertised_root_kel = advertised.root.kel();
    let advertised_device_kel = advertised.device.kel();
    let client_root_kel = client.root.kel();
    let client_device_kel = client.device.kel();

    let redirect_thread = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let (bearer, channel) = responder_channel(TcpBearer::from_stream(stream).unwrap());
        let mut freshness = FreshnessPins::new();
        let mut replay = ReplayCache::new(32).unwrap();
        authenticate_established_responder(
            bearer,
            channel,
            redirect.local(),
            TransportPurpose::PeerExchange,
            1_000,
            2_000,
            1_500,
            PeerExpectation::identity(&client_root_kel, &client_device_kel),
            &mut freshness,
            &mut replay,
        )
        .unwrap_err()
    });

    let mut freshness = FreshnessPins::new();
    let mut replay = ReplayCache::new(32).unwrap();
    let result = connect_authenticated_tcp(
        client.local(),
        TransportPurpose::PeerExchange,
        1_000,
        2_000,
        1_500,
        AuthenticatedDialTarget::new(
            &advertisement,
            &advertised_root_kel,
            &advertised_device_kel,
        ),
        5_000,
        &mut freshness,
        &mut replay,
    );
    assert_eq!(result.unwrap_err(), TransportSecurityError::IdentityMismatch);
    assert!(replay.is_empty());
    assert_eq!(
        freshness.pinned_sn(advertised_root_kel.scid()),
        None,
        "failed verification must not partially advance freshness state"
    );
    assert!(matches!(
        redirect_thread.join().unwrap(),
        TransportSecurityError::Bearer(mini_bearer::BearerError::Closed)
    ));
}

#[test]
fn bounded_retry_skips_an_unreachable_hint_and_accepts_only_the_verified_peer() {
    let client = Identity::new(10);
    let bad = Identity::new(40);
    let good = Identity::new(70);

    let (closed_listener, bad_address) = listener();
    drop(closed_listener);
    let (good_listener, good_address) = listener();
    let bad_advertisement = verified_advertisement(&bad, bad_address);
    let good_advertisement = verified_advertisement(&good, good_address);
    let bad_root_kel = bad.root.kel();
    let bad_device_kel = bad.device.kel();
    let good_root_kel = good.root.kel();
    let good_device_kel = good.device.kel();
    let client_root_kel = client.root.kel();
    let client_device_kel = client.device.kel();

    let server_thread = thread::spawn(move || {
        let (stream, _) = good_listener.accept().unwrap();
        let (bearer, channel) = responder_channel(TcpBearer::from_stream(stream).unwrap());
        let mut freshness = FreshnessPins::new();
        let mut replay = ReplayCache::new(32).unwrap();
        authenticate_established_responder(
            bearer,
            channel,
            good.local(),
            TransportPurpose::PeerExchange,
            1_000,
            2_000,
            1_500,
            PeerExpectation::identity(&client_root_kel, &client_device_kel),
            &mut freshness,
            &mut replay,
        )
        .unwrap()
        .peer()
        .root
        .clone()
    });

    let policy = PeerSelectionPolicy {
        max_peers: 2,
        max_per_network_prefix: 2,
        dial_timeout_ms: 250,
    };
    let records = [bad_advertisement.clone(), good_advertisement.clone()];
    let seed = (0..=u16::MAX)
        .find_map(|counter| {
            let mut seed = [0u8; 32];
            seed[..2].copy_from_slice(&counter.to_be_bytes());
            let plan = diverse_dial_plan(&records, seed, policy).unwrap();
            (plan.first().map(|item| item.endpoint_id) == Some(bad_advertisement.endpoint_id()))
                .then_some(seed)
        })
        .expect("a local seed ordering the unreachable record first");

    let targets = [
        AuthenticatedDialTarget::new(&bad_advertisement, &bad_root_kel, &bad_device_kel),
        AuthenticatedDialTarget::new(&good_advertisement, &good_root_kel, &good_device_kel),
    ];
    let mut freshness = FreshnessPins::new();
    let mut replay = ReplayCache::new(32).unwrap();
    let connection = connect_first_authenticated_tcp(
        client.local(),
        TransportPurpose::PeerExchange,
        1_000,
        2_000,
        1_500,
        &targets,
        seed,
        policy,
        &mut freshness,
        &mut replay,
    )
    .unwrap();
    assert_eq!(connection.peer().endpoint_id, good_advertisement.endpoint_id());
    assert_eq!(server_thread.join().unwrap(), client.root.did());
}

#[test]
fn an_existing_mini_bridge_channel_enters_the_same_identity_seam() {
    let client = Identity::new(10);
    let server = Identity::new(40);
    let (listener, address) = listener();
    let advertisement = verified_advertisement(&server, address);
    let server_root_kel = server.root.kel();
    let server_device_kel = server.device.kel();
    let client_root_kel = client.root.kel();
    let client_device_kel = client.device.kel();
    let descriptor = BridgeDescriptor::issue(
        &server.root,
        TransportId::DirectTlsV1,
        OpaqueEndpoint::new(address.to_string().into_bytes()).unwrap(),
        TransportParameters::empty(),
        None,
        0,
        60_000,
    )
    .unwrap();
    let bridge_kel: Kel = server.root.kel();

    let server_thread = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let (bearer, channel) = responder_channel(TcpBearer::from_stream(stream).unwrap());
        let mut freshness = FreshnessPins::new();
        let mut replay = ReplayCache::new(32).unwrap();
        authenticate_established_responder(
            bearer,
            channel,
            server.local(),
            TransportPurpose::Relay,
            1_000,
            2_000,
            1_500,
            PeerExpectation::identity(&client_root_kel, &client_device_kel),
            &mut freshness,
            &mut replay,
        )
        .unwrap()
        .peer()
        .endpoint_id
    });

    let transport = DirectBridgeTransport;
    let (bearer, channel) = transport
        .connect(&descriptor, &bridge_kel, 1_000, 5_000)
        .unwrap();
    let mut freshness = FreshnessPins::new();
    let mut replay = ReplayCache::new(32).unwrap();
    let connection = authenticate_established_initiator(
        bearer,
        channel,
        client.local(),
        TransportPurpose::Relay,
        1_000,
        2_000,
        1_500,
        PeerExpectation::advertised(
            &advertisement,
            &server_root_kel,
            &server_device_kel,
        ),
        &mut freshness,
        &mut replay,
    )
    .unwrap();
    assert_eq!(connection.peer().endpoint_id, advertisement.endpoint_id());
    let _ = server_thread.join().unwrap();
}
'''
write("crates/mini-transport-security/tests/runtime_tcp.rs", runtime_test)

print("stage 1 applied")
