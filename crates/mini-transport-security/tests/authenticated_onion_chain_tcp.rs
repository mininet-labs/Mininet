use std::net::{SocketAddr, TcpListener};
use std::thread;

use did_mini::{Capabilities, Controller, FreshnessPins, Kel};
use mini_bearer::{Bearer, Responder, TcpBearer};
use mini_crypto::AgreementSecretKey;
use mini_relay::{
    open_onion_destination, ConnectionId, OnionForward, OnionPacket, OnionReplayCache, RelayRole,
};
use mini_transport_policy::PayloadSizeClass;
use mini_transport_security::{
    authenticate_established_responder, build_verified_onion_route, connect_authenticated_tcp,
    AuthenticatedConnection, AuthenticatedDialTarget, LocalSessionIdentity, PeerAdvertisement,
    PeerExpectation, ReplayCache, TransportPurpose, VerifiedPeerAdvertisement, VerifiedRelay,
};

const NETWORK_ID: [u8; 32] = [17; 32];
const ONION_AAD: &[u8] = b"mini-transport-security/authenticated-onion-chain/v1";
const PLAINTEXT: &[u8] = b"authenticated ch1 on every onion hop";
const NOW_MS: u64 = 5_000;
const EXPIRES_AT_MS: u64 = 20_000;

struct Identity {
    root: Controller,
    device: Controller,
    routing_secret: AgreementSecretKey,
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
        Self {
            root,
            device,
            routing_secret: AgreementSecretKey::from_seed(&[seed + 4; 32]),
        }
    }

    fn local(&self) -> LocalSessionIdentity<'_> {
        LocalSessionIdentity::new(
            self.root.did(),
            &self.device,
            self.routing_secret.public_key(),
        )
    }
}

struct Node {
    identity: Identity,
    advertisement: VerifiedPeerAdvertisement,
    listener: TcpListener,
    address: SocketAddr,
}

fn listener() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    (listener, address)
}

fn node(seed: u8) -> Node {
    let identity = Identity::new(seed);
    let (listener, address) = listener();
    let advertisement = PeerAdvertisement::issue(
        NETWORK_ID,
        &identity.root.did(),
        &identity.device,
        identity.routing_secret.public_key(),
        address,
        1_000,
        EXPIRES_AT_MS,
    )
    .unwrap();
    let mut freshness = FreshnessPins::new();
    let mut replay = ReplayCache::new(32).unwrap();
    let advertisement = advertisement
        .verify(
            NETWORK_ID,
            NOW_MS,
            &identity.root.kel(),
            &identity.device.kel(),
            &mut freshness,
            &mut replay,
        )
        .unwrap();
    Node {
        identity,
        advertisement,
        listener,
        address,
    }
}

fn accept_relay(
    listener: TcpListener,
    local: &Identity,
    expected_root: &Kel,
    expected_device: &Kel,
) -> AuthenticatedConnection<TcpBearer> {
    let (stream, _) = listener.accept().unwrap();
    let mut bearer = TcpBearer::from_stream(stream).unwrap();
    let hello = bearer.recv().unwrap();
    let (channel, response) = Responder::respond(&hello).unwrap();
    bearer.send(&response).unwrap();
    let mut freshness = FreshnessPins::new();
    let mut replay = ReplayCache::new(32).unwrap();
    authenticate_established_responder(
        bearer,
        channel,
        local.local(),
        TransportPurpose::Relay,
        1_000,
        EXPIRES_AT_MS,
        NOW_MS,
        PeerExpectation::identity(expected_root, expected_device),
        &mut freshness,
        &mut replay,
    )
    .unwrap()
}

fn connect_relay(
    local: &Identity,
    target: &VerifiedPeerAdvertisement,
    target_root: &Kel,
    target_device: &Kel,
) -> AuthenticatedConnection<TcpBearer> {
    let mut freshness = FreshnessPins::new();
    let mut replay = ReplayCache::new(32).unwrap();
    connect_authenticated_tcp(
        local.local(),
        TransportPurpose::Relay,
        1_000,
        EXPIRES_AT_MS,
        NOW_MS,
        AuthenticatedDialTarget::new(target, target_root, target_device),
        5_000,
        &mut freshness,
        &mut replay,
    )
    .unwrap()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn every_onion_socket_uses_the_same_authenticated_runtime_seam() {
    let client = Identity::new(10);
    let entry = node(30);
    let rendezvous = node(50);
    let delivery = node(70);
    let destination = node(90);

    let client_root_kel = client.root.kel();
    let client_device_kel = client.device.kel();
    let entry_root_for_client = entry.identity.root.kel();
    let entry_device_for_client = entry.identity.device.kel();
    let entry_root_for_rendezvous = entry.identity.root.kel();
    let entry_device_for_rendezvous = entry.identity.device.kel();
    let rendezvous_root_for_entry = rendezvous.identity.root.kel();
    let rendezvous_device_for_entry = rendezvous.identity.device.kel();
    let rendezvous_root_for_delivery = rendezvous.identity.root.kel();
    let rendezvous_device_for_delivery = rendezvous.identity.device.kel();
    let delivery_root_for_rendezvous = delivery.identity.root.kel();
    let delivery_device_for_rendezvous = delivery.identity.device.kel();
    let delivery_root_for_destination = delivery.identity.root.kel();
    let delivery_device_for_destination = delivery.identity.device.kel();
    let destination_root_kel = destination.identity.root.kel();
    let destination_device_kel = destination.identity.device.kel();

    let entry_ad = entry.advertisement.clone();
    let rendezvous_ad = rendezvous.advertisement.clone();
    let delivery_ad = delivery.advertisement.clone();
    let destination_ad = destination.advertisement.clone();

    let rendezvous_token = rendezvous.address.to_string().into_bytes();
    let delivery_token = delivery.address.to_string().into_bytes();
    let destination_token = destination.address.to_string().into_bytes();
    let packet = build_verified_onion_route(
        [
            VerifiedRelay::new(&entry_ad, &rendezvous_token),
            VerifiedRelay::new(&rendezvous_ad, &delivery_token),
            VerifiedRelay::new(&delivery_ad, &destination_token),
        ],
        ConnectionId::from_bytes([23; 16]),
        PayloadSizeClass::Small,
        destination.identity.routing_secret.public_key(),
        PLAINTEXT,
        NOW_MS,
        EXPIRES_AT_MS,
    )
    .unwrap();

    let destination_thread = thread::spawn(move || {
        let mut incoming = accept_relay(
            destination.listener,
            &destination.identity,
            &delivery_root_for_destination,
            &delivery_device_for_destination,
        );
        let opaque = incoming.recv(ONION_AAD).unwrap();
        assert!(!contains(&opaque, PLAINTEXT));
        let mut replay = OnionReplayCache::new(32).unwrap();
        open_onion_destination(
            &opaque,
            &destination.identity.routing_secret,
            NOW_MS,
            &mut replay,
        )
        .unwrap()
    });

    let delivery_thread = thread::spawn(move || {
        let mut incoming = accept_relay(
            delivery.listener,
            &delivery.identity,
            &rendezvous_root_for_delivery,
            &rendezvous_device_for_delivery,
        );
        let bytes = incoming.recv(ONION_AAD).unwrap();
        assert!(!contains(&bytes, PLAINTEXT));
        let packet = OnionPacket::from_bytes(&bytes).unwrap();
        let mut onion_replay = OnionReplayCache::new(32).unwrap();
        let peeled = packet
            .peel(&delivery.identity.routing_secret, NOW_MS, &mut onion_replay)
            .unwrap();
        assert_eq!(peeled.role, RelayRole::Delivery);
        assert_eq!(peeled.next_hop, destination_token);
        let OnionForward::Destination(opaque) = peeled.forward else {
            panic!("delivery must produce the destination envelope");
        };
        let mut outgoing = connect_relay(
            &delivery.identity,
            &destination_ad,
            &destination_root_kel,
            &destination_device_kel,
        );
        outgoing.send(&opaque, ONION_AAD).unwrap();
    });

    let rendezvous_thread = thread::spawn(move || {
        let mut incoming = accept_relay(
            rendezvous.listener,
            &rendezvous.identity,
            &entry_root_for_rendezvous,
            &entry_device_for_rendezvous,
        );
        let bytes = incoming.recv(ONION_AAD).unwrap();
        assert!(!contains(&bytes, PLAINTEXT));
        let packet = OnionPacket::from_bytes(&bytes).unwrap();
        let mut onion_replay = OnionReplayCache::new(32).unwrap();
        let peeled = packet
            .peel(
                &rendezvous.identity.routing_secret,
                NOW_MS,
                &mut onion_replay,
            )
            .unwrap();
        assert_eq!(peeled.role, RelayRole::Rendezvous);
        assert_eq!(peeled.next_hop, delivery_token);
        let OnionForward::Next(next) = peeled.forward else {
            panic!("rendezvous must forward another onion packet");
        };
        let next = next.to_bytes().unwrap();
        let mut outgoing = connect_relay(
            &rendezvous.identity,
            &delivery_ad,
            &delivery_root_for_rendezvous,
            &delivery_device_for_rendezvous,
        );
        outgoing.send(&next, ONION_AAD).unwrap();
    });

    let entry_thread = thread::spawn(move || {
        let mut incoming = accept_relay(
            entry.listener,
            &entry.identity,
            &client_root_kel,
            &client_device_kel,
        );
        let bytes = incoming.recv(ONION_AAD).unwrap();
        assert!(!contains(&bytes, PLAINTEXT));
        let packet = OnionPacket::from_bytes(&bytes).unwrap();
        let mut onion_replay = OnionReplayCache::new(32).unwrap();
        let peeled = packet
            .peel(&entry.identity.routing_secret, NOW_MS, &mut onion_replay)
            .unwrap();
        assert_eq!(peeled.role, RelayRole::Entry);
        assert_eq!(peeled.next_hop, rendezvous_token);
        let OnionForward::Next(next) = peeled.forward else {
            panic!("entry must forward another onion packet");
        };
        let next = next.to_bytes().unwrap();
        let mut outgoing = connect_relay(
            &entry.identity,
            &rendezvous_ad,
            &rendezvous_root_for_entry,
            &rendezvous_device_for_entry,
        );
        outgoing.send(&next, ONION_AAD).unwrap();
    });

    let outer = packet.to_bytes().unwrap();
    assert!(!contains(&outer, PLAINTEXT));
    let mut entry_connection = connect_relay(
        &client,
        &entry_ad,
        &entry_root_for_client,
        &entry_device_for_client,
    );
    assert_eq!(entry_connection.peer().endpoint_id, entry_ad.endpoint_id());
    entry_connection.send(&outer, ONION_AAD).unwrap();

    entry_thread.join().unwrap();
    rendezvous_thread.join().unwrap();
    delivery_thread.join().unwrap();
    assert_eq!(destination_thread.join().unwrap(), PLAINTEXT);
}
