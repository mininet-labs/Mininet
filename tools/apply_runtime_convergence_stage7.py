#!/usr/bin/env python3
"""Finalize PR #296 connection state and authenticated onion composition."""

from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    target = Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(text, encoding="utf-8")


def replace(path: str, old: str, new: str, expected: int = 1) -> None:
    text = read(path)
    count = text.count(old)
    if count != expected:
        raise SystemExit(f"{path}: expected {expected}, found {count}: {old[:120]!r}")
    write(path, text.replace(old, new))


def insert_before(path: str, marker: str, block: str) -> None:
    text = read(path)
    if text.count(marker) != 1:
        raise SystemExit(f"{path}: marker mismatch: {marker[:100]!r}")
    write(path, text.replace(marker, block + marker, 1))


error = "crates/mini-transport-security/src/error.rs"
runtime = "crates/mini-transport-security/src/runtime.rs"
query = "crates/mini-search-federation-net/src/query.rs"
search_lib = "crates/mini-search-federation-net/src/lib.rs"
query_test = "crates/mini-search-federation-net/tests/authenticated_query_over_tcp.rs"
planning = "docs/planning/privacy-transport-runtime-convergence.md"
decision = "docs/DECISION_LOG.md"
threat = "docs/THREAT_MODEL.md"
readme = "crates/mini-transport-security/README.md"
f6 = "docs/design/f6-private-query-transport.md"

# A Channel increments its send counter before Bearer::send. If the bearer then
# fails, delivery is ambiguous and continuing would risk counter desynchrony.
replace(
    error,
    """    /// Two onion roles reused a visible endpoint, routing key, root, or device.
    RouteEndpointReuse,
    Bearer(BearerError),
""",
    """    /// Two onion roles reused a visible endpoint, routing key, root, or device.
    RouteEndpointReuse,
    /// A bearer send/receive or channel-open failure made the ordered CH1 state
    /// ambiguous. The connection is permanently unusable and must be replaced.
    ConnectionPoisoned,
    Bearer(BearerError),
""",
)
replace(
    error,
    """            Self::RouteEndpointReuse => write!(
                f,
                "one visible transport endpoint, routing key, root, or device was assigned multiple onion roles"
            ),
            Self::Bearer(error) => write!(f, "bearer/channel operation failed: {error}"),
""",
    """            Self::RouteEndpointReuse => write!(
                f,
                "one visible transport endpoint, routing key, root, or device was assigned multiple onion roles"
            ),
            Self::ConnectionPoisoned => write!(
                f,
                "authenticated connection is unusable after an ambiguous bearer/channel failure"
            ),
            Self::Bearer(error) => write!(f, "bearer/channel operation failed: {error}"),
""",
)
replace(
    runtime,
    """pub struct AuthenticatedConnection<B: Bearer> {
    bearer: B,
    channel: Channel,
    peer: AuthenticatedPeer,
}
""",
    """pub struct AuthenticatedConnection<B: Bearer> {
    bearer: B,
    channel: Channel,
    peer: AuthenticatedPeer,
    usable: bool,
}
""",
)
replace(
    runtime,
    """    /// Encrypt and send one application frame on the authenticated channel.
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
""",
    """    /// Encrypt and send one application frame on the authenticated channel.
    /// A bearer failure after sealing permanently poisons the connection because
    /// CH1's local send counter has already advanced and remote receipt is
    /// unknowable.
    pub fn send(&mut self, plaintext: &[u8], aad: &[u8]) -> Result<()> {
        self.ensure_usable()?;
        let ciphertext = self.channel.seal(plaintext, aad)?;
        if let Err(error) = self.bearer.send(&ciphertext) {
            self.usable = false;
            return Err(error.into());
        }
        Ok(())
    }

    /// Receive, authenticate, and decrypt one application frame. Any bearer or
    /// AEAD failure poisons the ordered connection rather than letting a caller
    /// continue from an uncertain stream position.
    pub fn recv(&mut self, aad: &[u8]) -> Result<Vec<u8>> {
        self.ensure_usable()?;
        let ciphertext = match self.bearer.recv() {
            Ok(ciphertext) => ciphertext,
            Err(error) => {
                self.usable = false;
                return Err(error.into());
            }
        };
        match self.channel.open(&ciphertext, aad) {
            Ok(plaintext) => Ok(plaintext),
            Err(error) => {
                self.usable = false;
                Err(error.into())
            }
        }
    }

    fn ensure_usable(&self) -> Result<()> {
        if self.usable {
            Ok(())
        } else {
            Err(TransportSecurityError::ConnectionPoisoned)
        }
    }
""",
)
replace(
    runtime,
    """    Ok(AuthenticatedConnection {
        bearer,
        channel,
        peer,
    })
""",
    """    Ok(AuthenticatedConnection {
        bearer,
        channel,
        peer,
        usable: true,
    })
""",
    expected=2,
)

poison_test = r'''    #[derive(Debug)]
    struct FailingBearer;

    impl Bearer for FailingBearer {
        fn send(&mut self, _frame: &[u8]) -> mini_bearer::Result<()> {
            Err(BearerError::Closed)
        }

        fn recv(&mut self) -> mini_bearer::Result<Vec<u8>> {
            Err(BearerError::Closed)
        }

        fn try_recv(&mut self) -> mini_bearer::Result<Option<Vec<u8>>> {
            Err(BearerError::Closed)
        }
    }

    #[test]
    fn bearer_send_failure_permanently_poisons_the_ordered_connection() {
        let (root, device) = {
            let mut root =
                Controller::incept_single_from_seeds(&[80; 32], &[81; 32]).unwrap();
            let device = Controller::incept_device_single_from_seeds(
                &root.did(),
                &[82; 32],
                &[83; 32],
            )
            .unwrap();
            root.delegate_device(&device.did(), Capabilities::primary())
                .unwrap();
            (root, device)
        };
        let routing = AgreementSecretKey::from_seed(&[84; 32]).public_key();
        let (initiator, hello) = mini_bearer::Initiator::start().unwrap();
        let (_responder, response) = mini_bearer::Responder::respond(&hello).unwrap();
        let channel = initiator.finish(&response).unwrap();
        let peer = AuthenticatedPeer {
            root: root.did(),
            device: device.did(),
            endpoint_id: crate::TransportEndpointId::derive(&device.did(), &routing),
            routing_key: routing,
            capabilities: Capabilities::primary(),
            purpose: TransportPurpose::PeerExchange,
        };
        let mut connection = AuthenticatedConnection {
            bearer: FailingBearer,
            channel,
            peer,
            usable: true,
        };

        assert_eq!(
            connection.send(b"first", b"aad"),
            Err(TransportSecurityError::Bearer(BearerError::Closed))
        );
        assert_eq!(
            connection.send(b"second", b"aad"),
            Err(TransportSecurityError::ConnectionPoisoned)
        );
        assert_eq!(
            connection.recv(b"aad"),
            Err(TransportSecurityError::ConnectionPoisoned)
        );
    }

'''
insert_before(
    runtime,
    "    #[test]\n    fn verified_route_rejects_reusing_one_endpoint_for_two_roles() {\n",
    poison_test,
)

# Keep provider derivation inside the sealed named-query constructor. Exporting
# it would invite callers to label arbitrary legacy results with a valid channel
# hash, weakening the very API boundary this PR adds.
replace(
    query,
    """pub fn authenticated_provider_pseudonym<B: Bearer>(
""",
    """fn authenticated_provider_pseudonym<B: Bearer>(
""",
)
replace(
    search_lib,
    """pub use query::{
    authenticated_provider_pseudonym, remote_query, remote_query_authenticated, serve_query,
    serve_query_authenticated, AuthenticatedQueryResults, WireResult, MAX_QUERY_RESULTS,
    MAX_QUERY_TEXT_BYTES,
};
""",
    """pub use query::{
    remote_query, remote_query_authenticated, serve_query, serve_query_authenticated,
    AuthenticatedQueryResults, WireResult, MAX_QUERY_RESULTS, MAX_QUERY_TEXT_BYTES,
};
""",
)
replace(
    query_test,
    """use mini_search_federation_net::{
    authenticated_provider_pseudonym, merge_authenticated_remote_results,
    remote_query_authenticated, serve_query_authenticated, NetError,
};
""",
    """use mini_search_federation_net::{
    merge_authenticated_remote_results, remote_query_authenticated, serve_query_authenticated,
    NetError,
};
""",
)
replace(
    query_test,
    """    let expected_provider = authenticated_provider_pseudonym(&connection);
    let remote = remote_query_authenticated(&mut connection, "hello", &profile, 8).unwrap();
    assert_eq!(remote.provider(), &expected_provider);
    assert_eq!(remote.results().len(), 1);

    let merged = merge_authenticated_remote_results(Vec::new(), remote, 8).unwrap();
""",
    """    let remote = remote_query_authenticated(&mut connection, "hello", &profile, 8).unwrap();
    let expected_provider = remote.provider().clone();
    assert_eq!(remote.results().len(), 1);

    let merged = merge_authenticated_remote_results(Vec::new(), remote, 8).unwrap();
""",
)
replace(
    f6,
    """- `authenticated_provider_pseudonym` accepts the sealed connection, then
  domain-separates and hashes both its verified `TransportEndpointId` and exact
""",
    """- The named query constructor internally domain-separates and hashes both the
  sealed connection's verified `TransportEndpointId` and exact
""",
)
replace(
    f6,
    """- `AuthenticatedQueryResults` has private fields. External callers can inspect
""",
    """- The provider-derivation helper is private, and `AuthenticatedQueryResults`
  has private fields. External callers can inspect
""",
)

# A full socket chain proving that the same runtime seam authenticates every
# client->entry, entry->rendezvous, rendezvous->delivery, and delivery->destination
# connection while the onion remains independently layered.
authenticated_onion = r'''use std::net::{SocketAddr, TcpListener};
use std::thread;

use did_mini::{Capabilities, Controller, FreshnessPins, Kel};
use mini_bearer::{Bearer, Responder, TcpBearer};
use mini_crypto::AgreementSecretKey;
use mini_relay::{
    open_onion_destination, ConnectionId, OnionForward, OnionPacket, OnionReplayCache,
    RelayRole,
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
    let entry_root_kel = entry.identity.root.kel();
    let entry_device_kel = entry.identity.device.kel();
    let rendezvous_root_kel = rendezvous.identity.root.kel();
    let rendezvous_device_kel = rendezvous.identity.device.kel();
    let delivery_root_kel = delivery.identity.root.kel();
    let delivery_device_kel = delivery.identity.device.kel();
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
            &delivery_root_kel,
            &delivery_device_kel,
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
            &rendezvous_root_kel,
            &rendezvous_device_kel,
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
            &entry_root_kel,
            &entry_device_kel,
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
            &delivery_root_kel,
            &delivery_device_kel,
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
            &rendezvous_root_kel,
            &rendezvous_device_kel,
        );
        outgoing.send(&next, ONION_AAD).unwrap();
    });

    let outer = packet.to_bytes().unwrap();
    assert!(!contains(&outer, PLAINTEXT));
    let mut entry_connection = connect_relay(
        &client,
        &entry_ad,
        &entry.identity.root.kel(),
        &entry.identity.device.kel(),
    );
    assert_eq!(entry_connection.peer().endpoint_id, entry_ad.endpoint_id());
    entry_connection.send(&outer, ONION_AAD).unwrap();

    entry_thread.join().unwrap();
    rendezvous_thread.join().unwrap();
    delivery_thread.join().unwrap();
    assert_eq!(destination_thread.join().unwrap(), PLAINTEXT);
}
'''
write(
    "crates/mini-transport-security/tests/authenticated_onion_chain_tcp.rs",
    authenticated_onion,
)

replace(
    planning,
    """| Failed-attempt state atomicity | **PASS** | Freshness/replay values are cloned and committed only after full verification and successful exchange. | Crash-persistent replay state remains the host application's responsibility. |
""",
    """| Failed-attempt state atomicity | **PASS** | Freshness/replay values are cloned and committed only after full verification and successful exchange. | Crash-persistent replay state remains the host application's responsibility. |
| Ordered connection state after transport failure | **PASS** | `AuthenticatedConnection` permanently poisons itself after bearer send/receive or channel-open failure, so a caller cannot continue after an ambiguous CH1 counter/stream position. | Recovery requires a new channel; the generic lower-level `Channel` + `Bearer` APIs remain caller-managed. |
""",
)
replace(
    planning,
    """  end-to-end signed-discovery -> local selection -> verified onion-route ->
  three-relay-socket -> destination-only plaintext test.
""",
    """  signed-discovery -> local selection -> verified onion-route ->
  three-relay-socket -> destination-only plaintext test, plus a full chain where
  client, every relay-to-relay hop, and delivery-to-destination all use typed
  `Relay`-purpose `AuthenticatedConnection`s.
""",
)
replace(
    planning,
    """  signed advertisements feeding real three-socket onion forwarding, provider
""",
    """  signed advertisements feeding real onion forwarding with CH1 authentication
  on every socket, connection poisoning after ambiguous transport failure, provider
""",
)
replace(
    decision,
    """validity-window, fail-closed relay and destination replay state; advertisement
expiry/network rechecks; bounded selection input; channel-scoped authenticated
search-provider provenance; and wrong-purpose rejection. Focused
""",
    """validity-window, fail-closed relay and destination replay state; advertisement
expiry/network rechecks; bounded selection input; permanent connection poisoning
on ambiguous bearer/channel failure; authenticated CH1 on every socket in a full
onion chain; sealed channel-scoped search-provider provenance; and wrong-purpose
rejection. Focused
""",
)
replace(
    threat,
    """| **Partial freshness/replay mutation on failed authentication** | Runtime verification clones `FreshnessPins` and `ReplayCache` and commits only after the full exchange succeeds. | **Closed in-process.** Crash-persistent replay state remains a host responsibility. |
""",
    """| **Partial freshness/replay mutation on failed authentication** | Runtime verification clones `FreshnessPins` and `ReplayCache` and commits only after the full exchange succeeds. | **Closed in-process.** Crash-persistent replay state remains a host responsibility. |
| **Continuing after ambiguous bearer send/receive failure** | `AuthenticatedConnection` poisons itself permanently after send/receive/channel-open failure, preventing reuse after CH1 counters or stream delivery become uncertain. | **Closed for the runtime type.** Specialist callers composing raw `Channel` and `Bearer` values retain this responsibility. |
""",
)
replace(
    readme,
    """- `AuthenticatedConnection<B>` owns one bearer, the exact CH1 channel, and the
  peer verified on that channel as one object. It exposes authenticated `send`
  and `recv`, not detachable raw identity state.
""",
    """- `AuthenticatedConnection<B>` owns one bearer, the exact CH1 channel, and the
  peer verified on that channel as one object. It exposes authenticated `send`
  and `recv`, not detachable raw identity state, and permanently poisons itself
  after an ambiguous bearer/channel failure instead of risking counter reuse or
  stream desynchronization.
""",
)
replace(
    readme,
    """  integration test starts with signed advertisements and local selection, then
  forwards only ciphertext across three real relay sockets until the destination
  alone recovers plaintext.
""",
    """  integration tests start with signed advertisements and local selection, then
  forward only ciphertext until the destination alone recovers plaintext. One
  full-chain test uses a typed `Relay`-purpose authenticated CH1 connection for
  client-to-entry, both relay-to-relay hops, and delivery-to-destination.
""",
)

print("stage 7 applied")
