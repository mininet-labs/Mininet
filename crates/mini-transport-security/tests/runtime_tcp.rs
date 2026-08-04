use std::net::{SocketAddr, TcpListener};
use std::thread;

use did_mini::{Capabilities, Controller, FreshnessPins, Kel};
use mini_bearer::{Bearer, Responder, TcpBearer};
use mini_bridge::{
    BridgeDescriptor, DirectBridgeTransport, OpaqueEndpoint, PluggableTransport, TransportId,
    TransportParameters,
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
        LocalSessionIdentity::new(self.root.did(), &self.device, self.routing)
    }
}

fn listener() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    (listener, address)
}

fn verified_advertisement(identity: &Identity, address: SocketAddr) -> VerifiedPeerAdvertisement {
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
        AuthenticatedDialTarget::new(&advertisement, &server_root_kel, &server_device_kel),
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
        AuthenticatedDialTarget::new(&advertisement, &advertised_root_kel, &advertised_device_kel),
        5_000,
        &mut freshness,
        &mut replay,
    );
    assert_eq!(
        result.unwrap_err(),
        TransportSecurityError::IdentityMismatch
    );
    assert!(replay.is_empty());
    assert_eq!(
        freshness.pinned_sn(advertised_root_kel.scid()),
        None,
        "failed verification must not partially advance freshness state"
    );
    assert!(matches!(
        redirect_thread.join().unwrap(),
        TransportSecurityError::Bearer(_)
    ));
}

#[test]
fn bounded_retry_skips_an_unreachable_hint_and_accepts_only_the_verified_peer() {
    let client = Identity::new(10);
    let bad = Identity::new(40);
    let good = Identity::new(70);

    // Keep the first address bound but deliberately unserviced. This proves
    // the read timeout/retry path without racing ephemeral-port reuse.
    let (_unresponsive_listener, bad_address) = listener();
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
    assert_eq!(
        connection.peer().endpoint_id,
        good_advertisement.endpoint_id()
    );
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
        PeerExpectation::advertised(&advertisement, &server_root_kel, &server_device_kel),
        &mut freshness,
        &mut replay,
    )
    .unwrap();
    assert_eq!(connection.peer().endpoint_id, advertisement.endpoint_id());
    let _ = server_thread.join().unwrap();
}
