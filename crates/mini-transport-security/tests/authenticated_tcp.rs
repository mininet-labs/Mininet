use std::net::TcpListener;
use std::thread;

use did_mini::{Capabilities, Controller, FreshnessPins};
use mini_bearer::{Bearer, Initiator, Responder, TcpBearer};
use mini_crypto::AgreementSecretKey;
use mini_transport_security::{
    ReplayCache, SessionAuthClaim, SessionRole, TransportPurpose,
};

const AUTH_AAD: &[u8] = b"mini-transport-security/authenticated-tcp-test/v1";

fn identity(seed: u8) -> (Controller, Controller) {
    let mut root =
        Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device = Controller::incept_device_single_from_seeds(
        &root.did(),
        &[seed + 2; 32],
        &[seed + 3; 32],
    )
    .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

#[test]
fn mutually_authenticated_peers_bind_identity_to_one_real_tcp_channel() {
    let (client_root, client_device) = identity(10);
    let (server_root, server_device) = identity(40);
    let client_root_kel = client_root.kel();
    let client_device_kel = client_device.kel();
    let server_root_kel = server_root.kel();
    let server_device_kel = server_device.kel();

    let client_routing = AgreementSecretKey::from_seed(&[70; 32]).public_key();
    let server_routing = AgreementSecretKey::from_seed(&[80; 32]).public_key();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();

        let hello = bearer.recv().unwrap();
        let (mut channel, response) = Responder::respond(&hello).unwrap();
        bearer.send(&response).unwrap();

        let sealed_client_claim = bearer.recv().unwrap();
        let client_claim_bytes = channel.open(&sealed_client_claim, AUTH_AAD).unwrap();
        let client_claim = SessionAuthClaim::from_bytes(&client_claim_bytes).unwrap();
        let mut freshness = FreshnessPins::new();
        let mut replay = ReplayCache::new(32).unwrap();
        let authenticated_client = client_claim
            .verify(
                SessionRole::Initiator,
                TransportPurpose::PeerExchange,
                &channel.channel_binding(),
                1_500,
                &client_root_kel,
                &client_device_kel,
                &mut freshness,
                &mut replay,
            )
            .unwrap();

        let server_claim = SessionAuthClaim::issue(
            &server_root.did(),
            &server_device,
            SessionRole::Responder,
            TransportPurpose::PeerExchange,
            server_routing,
            &channel.channel_binding(),
            1_000,
            2_000,
            [91; 32],
        )
        .unwrap();
        let sealed_server_claim = channel
            .seal(&server_claim.to_bytes().unwrap(), AUTH_AAD)
            .unwrap();
        bearer.send(&sealed_server_claim).unwrap();
        authenticated_client
    });

    let mut bearer = TcpBearer::connect(address).unwrap();
    let (initiator, hello) = Initiator::start().unwrap();
    bearer.send(&hello).unwrap();
    let response = bearer.recv().unwrap();
    let mut channel = initiator.finish(&response).unwrap();

    let client_claim = SessionAuthClaim::issue(
        &client_root.did(),
        &client_device,
        SessionRole::Initiator,
        TransportPurpose::PeerExchange,
        client_routing,
        &channel.channel_binding(),
        1_000,
        2_000,
        [90; 32],
    )
    .unwrap();
    let sealed_client_claim = channel
        .seal(&client_claim.to_bytes().unwrap(), AUTH_AAD)
        .unwrap();
    bearer.send(&sealed_client_claim).unwrap();

    let sealed_server_claim = bearer.recv().unwrap();
    let server_claim_bytes = channel.open(&sealed_server_claim, AUTH_AAD).unwrap();
    let server_claim = SessionAuthClaim::from_bytes(&server_claim_bytes).unwrap();
    let mut freshness = FreshnessPins::new();
    let mut replay = ReplayCache::new(32).unwrap();
    let authenticated_server = server_claim
        .verify(
            SessionRole::Responder,
            TransportPurpose::PeerExchange,
            &channel.channel_binding(),
            1_500,
            &server_root_kel,
            &server_device_kel,
            &mut freshness,
            &mut replay,
        )
        .unwrap();

    let authenticated_client = server.join().unwrap();
    assert_eq!(authenticated_client.root, client_root.did());
    assert_eq!(authenticated_client.routing_key, client_routing);
    assert_eq!(authenticated_server.root, server_root.did());
    assert_eq!(authenticated_server.routing_key, server_routing);
}
