use std::thread;

use did_mini::Controller;
use mini_bearer::{pair, Bearer, Channel, InProcessBearer, Initiator, Responder, TcpBearer};
use mini_crypto::{AeadKey, AeadSuite};
use mini_objects::{
    ObjectEnvelopeV2, ObjectType, OpaqueRoute, PrivateObject, RetentionClass, StorageDescriptor,
};
use mini_store::{MemoryBackend, Store};
use mini_sync::{sync_private_route_bidirectional, SyncError, SyncRole};

fn channels(a: &mut InProcessBearer, b: &mut InProcessBearer) -> (Channel, Channel) {
    let (initiator, hello) = Initiator::start().unwrap();
    a.send(&hello).unwrap();
    let (responder_channel, response) = Responder::respond(&b.recv().unwrap()).unwrap();
    b.send(&response).unwrap();
    let initiator_channel = initiator.finish(&a.recv().unwrap()).unwrap();
    (initiator_channel, responder_channel)
}

fn envelope(route: OpaqueRoute, key: &AeadKey, seed: u8, sequence: u64) -> ObjectEnvelopeV2 {
    let author = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let private = PrivateObject::new(
        ObjectType::Custom("mininet/private-message/v1".to_string()),
        author.did(),
        author.did(),
        sequence * 1_000,
        sequence,
        Vec::new(),
        Vec::new(),
        format!("message {sequence}").into_bytes(),
    )
    .sign_with(&author);
    ObjectEnvelopeV2::seal(
        &private,
        key,
        route,
        StorageDescriptor {
            retention: RetentionClass::Standard,
        },
    )
    .unwrap()
}

#[test]
fn selected_private_route_converges_in_both_directions() {
    let route = OpaqueRoute::from_bytes([8; 32]);
    let key = AeadKey::from_suite_bytes(AeadSuite::DEFAULT, &[9; 32]).unwrap();
    let first = envelope(route, &key, 10, 1);
    let second = envelope(route, &key, 20, 2);
    let mut a_store = Store::new(MemoryBackend::new());
    let mut b_store = Store::new(MemoryBackend::new());
    a_store.insert_private(&first).unwrap();
    b_store.insert_private(&second).unwrap();

    let (mut a_bearer, mut b_bearer) = pair();
    let (mut a_channel, mut b_channel) = channels(&mut a_bearer, &mut b_bearer);
    let responder = thread::spawn(move || {
        let report = sync_private_route_bidirectional(
            &mut b_bearer,
            &mut b_channel,
            &mut b_store,
            route,
            SyncRole::Responder,
        )
        .unwrap();
        (b_store, report)
    });
    let a_report = sync_private_route_bidirectional(
        &mut a_bearer,
        &mut a_channel,
        &mut a_store,
        route,
        SyncRole::Initiator,
    )
    .unwrap();
    let (b_store, b_report) = responder.join().unwrap();

    assert_eq!(a_report.accepted, 1);
    assert_eq!(b_report.accepted, 1);
    assert_eq!(a_store.private_by_route(&route).unwrap().len(), 2);
    assert_eq!(b_store.private_by_route(&route).unwrap().len(), 2);
}

#[test]
fn mismatched_routes_fail_before_envelope_exchange() {
    let a_route = OpaqueRoute::from_bytes([1; 32]);
    let b_route = OpaqueRoute::from_bytes([2; 32]);
    let key = AeadKey::from_suite_bytes(AeadSuite::DEFAULT, &[9; 32]).unwrap();
    let private = envelope(a_route, &key, 10, 1);
    let mut a_store = Store::new(MemoryBackend::new());
    let mut b_store = Store::new(MemoryBackend::new());
    a_store.insert_private(&private).unwrap();

    let (mut a_bearer, mut b_bearer) = pair();
    let (mut a_channel, mut b_channel) = channels(&mut a_bearer, &mut b_bearer);
    let responder = thread::spawn(move || {
        let result = sync_private_route_bidirectional(
            &mut b_bearer,
            &mut b_channel,
            &mut b_store,
            b_route,
            SyncRole::Responder,
        );
        (b_store, result)
    });
    let a_result = sync_private_route_bidirectional(
        &mut a_bearer,
        &mut a_channel,
        &mut a_store,
        a_route,
        SyncRole::Initiator,
    );
    let (b_store, b_result) = responder.join().unwrap();

    assert!(matches!(a_result, Err(SyncError::PrivateRouteMismatch)));
    assert!(matches!(b_result, Err(SyncError::PrivateRouteMismatch)));
    assert_eq!(a_store.private_by_route(&a_route).unwrap().len(), 1);
    assert!(b_store.private_by_route(&a_route).unwrap().is_empty());
    assert!(b_store.private_by_route(&b_route).unwrap().is_empty());
}

#[test]
fn selected_private_route_converges_over_real_tcp() {
    let route = OpaqueRoute::from_bytes([18; 32]);
    let key = AeadKey::from_suite_bytes(AeadSuite::DEFAULT, &[19; 32]).unwrap();
    let first = envelope(route, &key, 10, 1);
    let second = envelope(route, &key, 20, 2);
    let mut client_store = Store::new(MemoryBackend::new());
    let mut server_store = Store::new(MemoryBackend::new());
    client_store.insert_private(&first).unwrap();
    server_store.insert_private(&second).unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let hello = bearer.recv().unwrap();
        let (mut channel, response) = Responder::respond(&hello).unwrap();
        bearer.send(&response).unwrap();
        let report = sync_private_route_bidirectional(
            &mut bearer,
            &mut channel,
            &mut server_store,
            route,
            SyncRole::Responder,
        )
        .unwrap();
        (server_store, report)
    });

    let stream = std::net::TcpStream::connect(address).unwrap();
    let mut bearer = TcpBearer::from_stream(stream).unwrap();
    let (initiator, hello) = Initiator::start().unwrap();
    bearer.send(&hello).unwrap();
    let mut channel = initiator.finish(&bearer.recv().unwrap()).unwrap();
    let client_report = sync_private_route_bidirectional(
        &mut bearer,
        &mut channel,
        &mut client_store,
        route,
        SyncRole::Initiator,
    )
    .unwrap();
    let (server_store, server_report) = server.join().unwrap();

    assert_eq!(client_report.accepted, 1);
    assert_eq!(server_report.accepted, 1);
    assert_eq!(client_store.private_by_route(&route).unwrap().len(), 2);
    assert_eq!(server_store.private_by_route(&route).unwrap().len(), 2);
}
