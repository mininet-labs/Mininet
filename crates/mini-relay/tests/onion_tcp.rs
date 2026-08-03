use std::net::{SocketAddr, TcpListener};
use std::thread;

use mini_bearer::{Bearer, TcpBearer};
use mini_crypto::AgreementSecretKey;
use mini_relay::{
    build_onion, open_onion_destination, ConnectionId, OnionForward, OnionHop,
    OnionPacket, OnionReplayCache, RelayRole,
};
use mini_transport_policy::PayloadSizeClass;

fn listener() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    (listener, address)
}

fn send(address: SocketAddr, bytes: &[u8]) {
    let mut bearer = TcpBearer::connect(address).unwrap();
    bearer.send(bytes).unwrap();
}

#[test]
fn three_independent_tcp_relays_forward_only_layered_ciphertext() {
    let entry_secret = AgreementSecretKey::from_seed(&[1; 32]);
    let rendezvous_secret = AgreementSecretKey::from_seed(&[2; 32]);
    let delivery_secret = AgreementSecretKey::from_seed(&[3; 32]);
    let destination_secret = AgreementSecretKey::from_seed(&[9; 32]);

    let (destination_listener, destination_address) = listener();
    let destination_thread = thread::spawn(move || {
        let (stream, _) = destination_listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let opaque = bearer.recv().unwrap();
        open_onion_destination(&opaque, &destination_secret).unwrap()
    });

    let (delivery_listener, delivery_address) = listener();
    let delivery_thread = thread::spawn(move || {
        let (stream, _) = delivery_listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let bytes = bearer.recv().unwrap();
        assert!(!contains(&bytes, b"private over three real sockets"));
        let packet = OnionPacket::from_bytes(&bytes).unwrap();
        let mut replay = OnionReplayCache::new(32).unwrap();
        let peeled = packet.peel(&delivery_secret, 5_000, &mut replay).unwrap();
        assert_eq!(peeled.role, RelayRole::Delivery);
        assert_eq!(peeled.next_hop, destination_address.to_string().as_bytes());
        let OnionForward::Destination(opaque) = peeled.forward else {
            panic!("delivery hop must produce a destination envelope");
        };
        assert!(!contains(&opaque, b"private over three real sockets"));
        send(destination_address, &opaque);
    });

    let (rendezvous_listener, rendezvous_address) = listener();
    let rendezvous_thread = thread::spawn(move || {
        let (stream, _) = rendezvous_listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let bytes = bearer.recv().unwrap();
        assert!(!contains(&bytes, b"private over three real sockets"));
        let packet = OnionPacket::from_bytes(&bytes).unwrap();
        let mut replay = OnionReplayCache::new(32).unwrap();
        let peeled = packet
            .peel(&rendezvous_secret, 5_000, &mut replay)
            .unwrap();
        assert_eq!(peeled.role, RelayRole::Rendezvous);
        assert_eq!(peeled.next_hop, delivery_address.to_string().as_bytes());
        let OnionForward::Next(next) = peeled.forward else {
            panic!("rendezvous hop must forward another onion packet");
        };
        let next_bytes = next.to_bytes().unwrap();
        assert!(!contains(
            &next_bytes,
            b"private over three real sockets"
        ));
        send(delivery_address, &next_bytes);
    });

    let (entry_listener, entry_address) = listener();
    let entry_thread = thread::spawn(move || {
        let (stream, _) = entry_listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let bytes = bearer.recv().unwrap();
        assert!(!contains(&bytes, b"private over three real sockets"));
        let packet = OnionPacket::from_bytes(&bytes).unwrap();
        let mut replay = OnionReplayCache::new(32).unwrap();
        let peeled = packet.peel(&entry_secret, 5_000, &mut replay).unwrap();
        assert_eq!(peeled.role, RelayRole::Entry);
        assert_eq!(
            peeled.next_hop,
            rendezvous_address.to_string().as_bytes()
        );
        let OnionForward::Next(next) = peeled.forward else {
            panic!("entry hop must forward another onion packet");
        };
        let next_bytes = next.to_bytes().unwrap();
        assert!(!contains(
            &next_bytes,
            b"private over three real sockets"
        ));
        send(rendezvous_address, &next_bytes);
    });

    let hops = vec![
        OnionHop {
            role: RelayRole::Entry,
            routing_key: AgreementSecretKey::from_seed(&[1; 32]).public_key(),
            next_hop: rendezvous_address.to_string().into_bytes(),
        },
        OnionHop {
            role: RelayRole::Rendezvous,
            routing_key: AgreementSecretKey::from_seed(&[2; 32]).public_key(),
            next_hop: delivery_address.to_string().into_bytes(),
        },
        OnionHop {
            role: RelayRole::Delivery,
            routing_key: AgreementSecretKey::from_seed(&[3; 32]).public_key(),
            next_hop: destination_address.to_string().into_bytes(),
        },
    ];
    let packet = build_onion(
        ConnectionId::from_bytes([7; 16]),
        PayloadSizeClass::Small,
        &hops,
        AgreementSecretKey::from_seed(&[9; 32]).public_key(),
        b"private over three real sockets",
        10_000,
    )
    .unwrap();
    let outer = packet.to_bytes().unwrap();
    assert!(!contains(&outer, b"private over three real sockets"));
    send(entry_address, &outer);

    entry_thread.join().unwrap();
    rendezvous_thread.join().unwrap();
    delivery_thread.join().unwrap();
    assert_eq!(
        destination_thread.join().unwrap(),
        b"private over three real sockets"
    );
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}
