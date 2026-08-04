//! Executable convergence of discovery, CH1, identity, retry, and onion routing.
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
use mini_relay::{build_onion, ConnectionId, OnionHop, OnionPacket, RelayRole};
use mini_transport_policy::PayloadSizeClass;

use crate::{
    diverse_dial_plan, AuthenticatedPeer, PeerSelectionPolicy, ReplayCache, Result,
    SessionAuthClaim, SessionRole, TransportPurpose, TransportSecurityError,
    VerifiedPeerAdvertisement, MAX_DIAL_TIMEOUT_MS, MIN_DIAL_TIMEOUT_MS,
};

/// AEAD associated data for encrypted authentication claims on CH1.
pub const SESSION_AUTH_FRAME_AAD: &[u8] = b"MINI/TRANSPORT-AUTH1";

/// Local identity disclosed for one typed authenticated session.
#[derive(Debug, Clone)]
pub struct LocalSessionIdentity<'a> {
    pub root: Did,
    pub device: &'a Controller,
    pub routing_key: AgreementPublicKey,
}

impl<'a> LocalSessionIdentity<'a> {
    pub const fn new(root: Did, device: &'a Controller, routing_key: AgreementPublicKey) -> Self {
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
        &local.root,
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
        &local.root,
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
            role, purpose, binding, now_ms, root_kel, device_kel, freshness, replay,
        ),
        PeerExpectation::Advertised {
            advertisement,
            root_kel,
            device_kel,
        } => {
            ensure_advertisement_live(advertisement, now_ms)?;
            claim.verify_advertised(
                advertisement,
                role,
                purpose,
                binding,
                now_ms,
                root_kel,
                device_kel,
                freshness,
                replay,
            )
        }
    }
}

fn ensure_advertisement_live(advertisement: &VerifiedPeerAdvertisement, now_ms: u64) -> Result<()> {
    if now_ms > advertisement.expires_at_ms() {
        return Err(TransportSecurityError::Expired);
    }
    Ok(())
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
    ensure_advertisement_live(target.advertisement, now_ms)?;
    let (bearer, channel) = establish_tcp_initiator(target.advertisement.address(), timeout_ms)?;
    authenticate_established_initiator(
        bearer,
        channel,
        local,
        purpose,
        issued_at_ms,
        expires_at_ms,
        now_ms,
        PeerExpectation::advertised(target.advertisement, target.root_kel, target.device_kel),
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
    if targets.len() > crate::MAX_SELECTION_CANDIDATES {
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
    let mut attempted = 0usize;

    for attempt in plan {
        let Some(target) = targets.iter().copied().find(|target| {
            target.advertisement.endpoint_id() == attempt.endpoint_id
                && target.advertisement.address() == attempt.address
                && target.advertisement.routing_key() == attempt.routing_key
        }) else {
            continue;
        };
        attempted += 1;
        if let Ok(connection) = connect_authenticated_tcp(
            local.clone(),
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

fn establish_tcp_initiator(address: SocketAddr, timeout_ms: u64) -> Result<(TcpBearer, Channel)> {
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
    pub const fn new(advertisement: &'a VerifiedPeerAdvertisement, next_hop: &'a [u8]) -> Self {
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
        now_ms,
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
            1_500,
            10_000,
        );
        assert_eq!(result, Err(TransportSecurityError::RouteEndpointReuse));
    }

    #[test]
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

        let mut foreign_root = Controller::incept_single_from_seeds(&[60; 32], &[61; 32]).unwrap();
        let foreign_device =
            Controller::incept_device_single_from_seeds(&foreign_root.did(), &[62; 32], &[63; 32])
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
            1_500,
            10_000,
        )
        .unwrap();
        assert_eq!(packet.hop_index, 0);
    }
}
