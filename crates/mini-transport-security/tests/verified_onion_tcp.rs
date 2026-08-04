use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener};
use std::thread;

use did_mini::{Capabilities, Controller, FreshnessPins};
use mini_bearer::{Bearer, TcpBearer};
use mini_crypto::AgreementSecretKey;
use mini_relay::{
    open_onion_destination, ConnectionId, OnionForward, OnionPacket, OnionReplayCache, RelayRole,
};
use mini_transport_policy::PayloadSizeClass;
use mini_transport_security::{
    build_verified_onion_route, diverse_dial_plan, PeerAdvertisement, PeerSelectionPolicy,
    ReplayCache, TransportEndpointId, VerifiedPeerAdvertisement, VerifiedRelay,
};

const NETWORK_ID: [u8; 32] = [7; 32];
const PLAINTEXT: &[u8] = b"verified discovery through three onion sockets";

struct RelayNode {
    advertisement: VerifiedPeerAdvertisement,
    secret: AgreementSecretKey,
    listener: TcpListener,
    address: SocketAddr,
}

fn listener() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    (listener, address)
}

fn verified_node(seed: u8) -> RelayNode {
    let (listener, address) = listener();
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    let secret = AgreementSecretKey::from_seed(&[seed + 4; 32]);
    let advertisement = PeerAdvertisement::issue(
        NETWORK_ID,
        &root.did(),
        &device,
        secret.public_key(),
        address,
        1_000,
        2_000,
    )
    .unwrap();
    let mut freshness = FreshnessPins::new();
    let mut replay = ReplayCache::new(16).unwrap();
    let advertisement = advertisement
        .verify(
            NETWORK_ID,
            1_500,
            &root.kel(),
            &device.kel(),
            &mut freshness,
            &mut replay,
        )
        .unwrap();
    RelayNode {
        advertisement,
        secret,
        listener,
        address,
    }
}

fn send(address: SocketAddr, bytes: &[u8]) {
    let mut bearer = TcpBearer::connect(address).unwrap();
    bearer.send(bytes).unwrap();
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn signed_discovery_selection_and_verified_route_forward_ciphertext_over_three_sockets() {
    let mut nodes: HashMap<TransportEndpointId, RelayNode> = HashMap::new();
    let mut records = Vec::new();
    for seed in [10u8, 30, 50] {
        let node = verified_node(seed);
        records.push(node.advertisement.clone());
        assert!(nodes
            .insert(node.advertisement.endpoint_id(), node)
            .is_none());
    }

    let plan = diverse_dial_plan(
        &records,
        [9; 32],
        PeerSelectionPolicy {
            max_peers: 3,
            max_per_network_prefix: 3,
            dial_timeout_ms: 1_000,
        },
    )
    .unwrap();
    assert_eq!(plan.len(), 3);

    let entry = nodes.remove(&plan[0].endpoint_id).unwrap();
    let rendezvous = nodes.remove(&plan[1].endpoint_id).unwrap();
    let delivery = nodes.remove(&plan[2].endpoint_id).unwrap();
    assert!(nodes.is_empty());

    let (destination_listener, destination_address) = listener();
    let destination_secret = AgreementSecretKey::from_seed(&[99; 32]);
    let destination_public = destination_secret.public_key();

    let rendezvous_token = rendezvous.address.to_string().into_bytes();
    let delivery_token = delivery.address.to_string().into_bytes();
    let destination_token = destination_address.to_string().into_bytes();

    let packet = build_verified_onion_route(
        [
            VerifiedRelay::new(&entry.advertisement, &rendezvous_token),
            VerifiedRelay::new(&rendezvous.advertisement, &delivery_token),
            VerifiedRelay::new(&delivery.advertisement, &destination_token),
        ],
        ConnectionId::from_bytes([4; 16]),
        PayloadSizeClass::Small,
        destination_public,
        PLAINTEXT,
        10_000,
    )
    .unwrap();

    let destination_thread = thread::spawn(move || {
        let (stream, _) = destination_listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let opaque = bearer.recv().unwrap();
        assert!(!contains(&opaque, PLAINTEXT));
        open_onion_destination(&opaque, &destination_secret).unwrap()
    });

    let delivery_address = delivery.address;
    let delivery_thread = thread::spawn(move || {
        let (stream, _) = delivery.listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let bytes = bearer.recv().unwrap();
        assert!(!contains(&bytes, PLAINTEXT));
        let packet = OnionPacket::from_bytes(&bytes).unwrap();
        let mut replay = OnionReplayCache::new(32).unwrap();
        let peeled = packet.peel(&delivery.secret, 5_000, &mut replay).unwrap();
        assert_eq!(peeled.role, RelayRole::Delivery);
        assert_eq!(peeled.next_hop, destination_token);
        let OnionForward::Destination(opaque) = peeled.forward else {
            panic!("delivery must forward a destination-only envelope");
        };
        assert!(!contains(&opaque, PLAINTEXT));
        send(destination_address, &opaque);
    });

    let rendezvous_address = rendezvous.address;
    let rendezvous_thread = thread::spawn(move || {
        let (stream, _) = rendezvous.listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let bytes = bearer.recv().unwrap();
        assert!(!contains(&bytes, PLAINTEXT));
        let packet = OnionPacket::from_bytes(&bytes).unwrap();
        let mut replay = OnionReplayCache::new(32).unwrap();
        let peeled = packet.peel(&rendezvous.secret, 5_000, &mut replay).unwrap();
        assert_eq!(peeled.role, RelayRole::Rendezvous);
        assert_eq!(peeled.next_hop, delivery_token);
        let OnionForward::Next(next) = peeled.forward else {
            panic!("rendezvous must forward another onion packet");
        };
        let next = next.to_bytes().unwrap();
        assert!(!contains(&next, PLAINTEXT));
        send(delivery_address, &next);
    });

    let entry_address = entry.address;
    let entry_thread = thread::spawn(move || {
        let (stream, _) = entry.listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let bytes = bearer.recv().unwrap();
        assert!(!contains(&bytes, PLAINTEXT));
        let packet = OnionPacket::from_bytes(&bytes).unwrap();
        let mut replay = OnionReplayCache::new(32).unwrap();
        let peeled = packet.peel(&entry.secret, 5_000, &mut replay).unwrap();
        assert_eq!(peeled.role, RelayRole::Entry);
        assert_eq!(peeled.next_hop, rendezvous_token);
        let OnionForward::Next(next) = peeled.forward else {
            panic!("entry must forward another onion packet");
        };
        let next = next.to_bytes().unwrap();
        assert!(!contains(&next, PLAINTEXT));
        send(rendezvous_address, &next);
    });

    let outer = packet.to_bytes().unwrap();
    assert!(!contains(&outer, PLAINTEXT));
    send(entry_address, &outer);

    entry_thread.join().unwrap();
    rendezvous_thread.join().unwrap();
    delivery_thread.join().unwrap();
    assert_eq!(destination_thread.join().unwrap(), PLAINTEXT);
}
