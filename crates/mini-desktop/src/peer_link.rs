//! One connection between two desktop clients: anonymous CH1 handshake over
//! TCP, then a single sealed *intent* frame naming which bounded protocol
//! follows — public `MINI/SYNC1` or one private conversation route — then
//! that protocol. The intent frame is the only desktop-specific addition and
//! lets one accepting socket serve both without a second port or a guess.
//!
//! Nothing here authenticates the remote endpoint: public objects verify
//! themselves through `mini-sync`'s ingest and private envelopes through the
//! conversation key. A completed exchange proves the protocol ran, not who
//! the peer was.

use crate::{
    configure_peer_stream, load_desktop_identity, open_sync_state, DesktopIdentity, PEER_IO_TIMEOUT,
};
use did_mini::Did;
use mini_bearer::{Bearer, BearerError, Channel, Initiator, Responder, TcpBearer};
use mini_media::{missing_chunks, read_manifest};
use mini_objects::{Object, ObjectId, ObjectType, OpaqueRoute};
use mini_store::{FsBackend, Store};
use mini_sync::{
    sync_bidirectional, sync_private_route_bidirectional, sync_private_route_responder_any, Ingest,
    IngestOutcome, KelCache, SyncError, SyncRole,
};
use mini_ticket::{
    issue_ticket, read_ticket, CompletedMedia, Service, TicketFields, MAX_TICKET_MANIFESTS,
};
use std::collections::BTreeMap;
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

const INTENT_AAD: &[u8] = b"MININET-DESKTOP/INTENT1";
const INTENT_PUBLIC: u8 = 1;
const INTENT_PRIVATE: u8 = 2;
/// Connections a host serves at the same time; extra ones are refused.
const MAX_CONCURRENT_CONNECTIONS: usize = 4;
const ACCEPT_POLL: Duration = Duration::from_millis(250);
/// End-to-end bound on one exchange (handshake through the last frame).
/// Per-I/O timeouts alone let a peer that dribbles one byte per timeout
/// hold a worker indefinitely; this caps the whole conversation.
const EXCHANGE_DEADLINE: Duration = Duration::from_secs(180);
/// Bytes one exchange may move in either direction before it is cut off.
const EXCHANGE_BYTE_BUDGET: usize = 256 * 1024 * 1024;

/// A bearer that refuses to carry another frame once its deadline has
/// passed or its byte budget is spent. Wrapping the socket here means every
/// protocol on top gets a whole-exchange bound without knowing about it.
pub struct BoundedBearer<B: Bearer> {
    inner: B,
    deadline: Instant,
    remaining_bytes: usize,
    received: u64,
}

impl<B: Bearer> BoundedBearer<B> {
    pub fn new(inner: B, deadline: Instant, byte_budget: usize) -> Self {
        Self {
            inner,
            deadline,
            remaining_bytes: byte_budget,
            received: 0,
        }
    }

    /// Frame bytes received so far (ciphertext, as carried on the wire).
    pub fn received_bytes(&self) -> u64 {
        self.received
    }

    fn charge(&mut self, bytes: usize) -> mini_bearer::Result<()> {
        if Instant::now() >= self.deadline {
            return Err(BearerError::Io("exchange deadline exceeded".into()));
        }
        self.remaining_bytes = self
            .remaining_bytes
            .checked_sub(bytes)
            .ok_or_else(|| BearerError::Io("exchange byte budget exceeded".into()))?;
        Ok(())
    }
}

impl<B: Bearer> Bearer for BoundedBearer<B> {
    fn send(&mut self, frame: &[u8]) -> mini_bearer::Result<()> {
        self.charge(frame.len())?;
        self.inner.send(frame)
    }

    fn recv(&mut self) -> mini_bearer::Result<Vec<u8>> {
        self.charge(0)?;
        let frame = self.inner.recv()?;
        self.charge(frame.len())?;
        self.received = self.received.saturating_add(frame.len() as u64);
        Ok(frame)
    }

    fn try_recv(&mut self) -> mini_bearer::Result<Option<Vec<u8>>> {
        self.charge(0)?;
        let frame = self.inner.try_recv()?;
        if let Some(frame) = frame.as_ref() {
            self.charge(frame.len())?;
            self.received = self.received.saturating_add(frame.len() as u64);
        }
        Ok(frame)
    }

    fn max_frame_bytes(&self) -> Option<usize> {
        self.inner.max_frame_bytes()
    }
}

type Link = BoundedBearer<TcpBearer>;

fn bound(bearer: TcpBearer) -> Link {
    BoundedBearer::new(
        bearer,
        Instant::now() + EXCHANGE_DEADLINE,
        EXCHANGE_BYTE_BUDGET,
    )
}

/// What the dialing side wants from this connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    Public,
    Private(OpaqueRoute),
}

impl Intent {
    fn encode(self) -> Vec<u8> {
        match self {
            Intent::Public => vec![INTENT_PUBLIC],
            Intent::Private(route) => {
                let mut out = Vec::with_capacity(33);
                out.push(INTENT_PRIVATE);
                out.extend_from_slice(route.as_bytes());
                out
            }
        }
    }

    fn decode(bytes: &[u8]) -> Result<Self, String> {
        match bytes {
            [INTENT_PUBLIC] => Ok(Intent::Public),
            [INTENT_PRIVATE, route @ ..] if route.len() == 32 => {
                let mut bytes = [0u8; 32];
                bytes.copy_from_slice(route);
                Ok(Intent::Private(OpaqueRoute::from_bytes(bytes)))
            }
            _ => Err("peer sent an unrecognized intent".into()),
        }
    }
}

/// Outcome of one private-route dial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivateOutcome {
    /// Envelopes exchanged for the route.
    Synced { received: usize, accepted: usize },
    /// The peer does not hold this conversation; nothing was revealed.
    NotOnThisPeer,
}

fn resolve(endpoint: &str) -> Result<SocketAddr, String> {
    endpoint
        .to_socket_addrs()
        .map_err(|error| format!("could not resolve {endpoint}: {error}"))?
        .next()
        .ok_or_else(|| format!("{endpoint} did not resolve to an address"))
}

fn connect(endpoint: &str) -> Result<(Link, Channel), String> {
    let address = resolve(endpoint)?;
    let stream = TcpStream::connect_timeout(&address, PEER_IO_TIMEOUT)
        .map_err(|error| format!("{endpoint} refused or timed out: {error}"))?;
    configure_peer_stream(&stream)?;
    let mut bearer = bound(TcpBearer::from_stream(stream).map_err(|error| error.to_string())?);
    let (initiator, hello) = Initiator::start().map_err(|error| error.to_string())?;
    bearer.send(&hello).map_err(|error| error.to_string())?;
    let response = bearer.recv().map_err(|error| error.to_string())?;
    let channel = initiator
        .finish(&response)
        .map_err(|error| error.to_string())?;
    Ok((bearer, channel))
}

fn accept(stream: TcpStream) -> Result<(Link, Channel), String> {
    stream
        .set_nonblocking(false)
        .map_err(|error| error.to_string())?;
    configure_peer_stream(&stream)?;
    let mut bearer = bound(TcpBearer::from_stream(stream).map_err(|error| error.to_string())?);
    let hello = bearer.recv().map_err(|error| error.to_string())?;
    let (channel, response) = Responder::respond(&hello).map_err(|error| error.to_string())?;
    bearer.send(&response).map_err(|error| error.to_string())?;
    Ok((bearer, channel))
}

fn send_intent(bearer: &mut Link, channel: &mut Channel, intent: Intent) -> Result<(), String> {
    let sealed = channel
        .seal(&intent.encode(), INTENT_AAD)
        .map_err(|error| error.to_string())?;
    bearer.send(&sealed).map_err(|error| error.to_string())
}

fn recv_intent(bearer: &mut Link, channel: &mut Channel) -> Result<Intent, String> {
    let sealed = bearer.recv().map_err(|error| error.to_string())?;
    let plain = channel
        .open(&sealed, INTENT_AAD)
        .map_err(|error| error.to_string())?;
    Intent::decode(&plain)
}

fn public_summary(report: &mini_sync::IngestReport) -> String {
    format!(
        "received {}, accepted {}, identity carriers {}, unknown authors {}, invalid {}",
        report.received, report.accepted, report.carriers, report.unknown_author, report.invalid
    )
}

const HELLO_AAD: &[u8] = b"MININET-DESKTOP/HELLO1";
const TICKET_AAD: &[u8] = b"MININET-DESKTOP/TICKET1";
const MAX_HELLO_BYTES: usize = 300;
const MAX_TICKET_OBJECT_BYTES: usize = 64 * 1024;

fn send_sealed(
    bearer: &mut Link,
    channel: &mut Channel,
    aad: &[u8],
    plain: &[u8],
) -> Result<(), String> {
    let sealed = channel
        .seal(plain, aad)
        .map_err(|error| error.to_string())?;
    bearer.send(&sealed).map_err(|error| error.to_string())
}

fn recv_sealed(
    bearer: &mut Link,
    channel: &mut Channel,
    aad: &[u8],
    max: usize,
) -> Result<Vec<u8>, String> {
    let sealed = bearer.recv().map_err(|error| error.to_string())?;
    let plain = channel
        .open(&sealed, aad)
        .map_err(|error| error.to_string())?;
    if plain.len() > max {
        return Err("peer sent an oversized frame".into());
    }
    Ok(plain)
}

/// Each side states the DID a service ticket may name. It is a claim, not
/// an authentication: a wrong DID only means the ticket credits nobody.
fn exchange_hello(
    bearer: &mut Link,
    channel: &mut Channel,
    role: SyncRole,
    me: &Did,
) -> Result<Did, String> {
    let mine = me.as_str().as_bytes();
    let theirs = match role {
        SyncRole::Responder => {
            send_sealed(bearer, channel, HELLO_AAD, mine)?;
            recv_sealed(bearer, channel, HELLO_AAD, MAX_HELLO_BYTES)?
        }
        SyncRole::Initiator => {
            let theirs = recv_sealed(bearer, channel, HELLO_AAD, MAX_HELLO_BYTES)?;
            send_sealed(bearer, channel, HELLO_AAD, mine)?;
            theirs
        }
    };
    let text = String::from_utf8(theirs).map_err(|_| "peer hello is not UTF-8".to_string())?;
    Did::parse(&text).map_err(|error| format!("peer hello is not a DID: {error}"))
}

/// Media manifests on this device whose every chunk is present, with sizes.
fn complete_manifests(store: &Store<FsBackend>) -> BTreeMap<String, (ObjectId, u64)> {
    let mut out = BTreeMap::new();
    let Ok(ids) = store.by_type(&ObjectType::MEDIA_MANIFEST) else {
        return out;
    };
    for id in ids {
        let Ok(object) = store.get(&id) else { continue };
        let Ok(manifest) = read_manifest(&object) else {
            continue;
        };
        if missing_chunks(store, &manifest).is_ok_and(|missing| missing.is_empty()) {
            out.insert(id.as_str().to_owned(), (id, manifest.total_len));
        }
    }
    out
}

/// Outcome of the ticket handshake after a protocol run.
#[derive(Debug, Default, Clone)]
pub struct TicketOutcome {
    /// Bytes this side attested to the peer.
    pub issued_bytes: u64,
    /// Bytes the peer attested to this side, if its ticket verified.
    pub received_bytes: Option<u64>,
    pub note: Option<String>,
}

impl TicketOutcome {
    fn summary(&self) -> String {
        let mut parts = vec![format!("attested {} KB received", self.issued_bytes / 1024)];
        match self.received_bytes {
            Some(bytes) => parts.push(format!("peer attested {} KB served", bytes / 1024)),
            None => parts.push("no ticket from peer".into()),
        }
        if let Some(note) = &self.note {
            parts.push(note.clone());
        }
        parts.join(", ")
    }
}

/// After the protocol: attest what we received, then accept the peer's
/// attestation of what we served. Ticket problems never undo the exchange
/// itself; they are reported.
#[allow(clippy::too_many_arguments)]
fn exchange_tickets(
    bearer: &mut Link,
    channel: &mut Channel,
    role: SyncRole,
    store: &mut Store<FsBackend>,
    cache: &mut KelCache,
    identity: &DesktopIdentity,
    peer: &Did,
    service: Service,
    bytes_received: u64,
    objects_received: u32,
    completed_media: Vec<CompletedMedia>,
) -> TicketOutcome {
    let me = identity.root.did();
    let mut outcome = TicketOutcome {
        issued_bytes: bytes_received,
        ..Default::default()
    };
    let issue = || -> Result<Vec<u8>, String> {
        let nonce = mini_crypto::random_32().map_err(|error| error.to_string())?;
        let fields = TicketFields {
            provider: peer.clone(),
            service,
            bytes_received,
            objects_received,
            channel_binding: channel.channel_binding(),
            nonce,
            completed_media,
        };
        let sequence = crate::next_object_sequence(store, Some(&me))?;
        let object = issue_ticket(&me, &identity.device, &fields, crate::now_ms(), sequence)
            .map_err(|error| error.to_string())?;
        store.insert(&object).map_err(|error| error.to_string())?;
        Ok(object.to_bytes())
    };
    let mine = match issue() {
        Ok(bytes) => bytes,
        Err(error) => {
            outcome.note = Some(format!("could not issue a ticket: {error}"));
            return outcome;
        }
    };
    let accept = |bearer: &mut Link,
                  channel: &mut Channel,
                  store: &mut Store<FsBackend>,
                  cache: &mut KelCache|
     -> Result<u64, String> {
        let bytes = recv_sealed(bearer, channel, TICKET_AAD, MAX_TICKET_OBJECT_BYTES)?;
        let object = Object::from_bytes(&bytes).map_err(|error| error.to_string())?;
        if Ingest::check(cache, &object) != IngestOutcome::Accepted {
            return Err("peer ticket failed provenance".into());
        }
        let ticket = read_ticket(&object).map_err(|error| error.to_string())?;
        if ticket.fields.provider != me {
            return Err("peer ticket names a different provider".into());
        }
        if ticket.consumer != *peer {
            return Err("peer ticket is not signed by the announced peer".into());
        }
        if ticket.fields.channel_binding != channel.channel_binding() {
            return Err("peer ticket is bound to a different session".into());
        }
        store.insert(&object).map_err(|error| error.to_string())?;
        Ok(ticket.fields.bytes_received)
    };
    let result = match role {
        SyncRole::Initiator => send_sealed(bearer, channel, TICKET_AAD, &mine)
            .and_then(|()| accept(bearer, channel, store, cache)),
        SyncRole::Responder => {
            let got = accept(bearer, channel, store, cache);
            match send_sealed(bearer, channel, TICKET_AAD, &mine) {
                Ok(()) => got,
                Err(error) => Err(format!("could not send ticket: {error}")),
            }
        }
    };
    match result {
        Ok(bytes) => outcome.received_bytes = Some(bytes),
        Err(error) => outcome.note = Some(error),
    }
    outcome
}

fn load_identity(root: &Path, action: &str) -> Result<DesktopIdentity, String> {
    load_desktop_identity(root, false).map_err(|error| {
        format!(
            "identity/device vault unavailable; unlock the identity once before {action}: {error}"
        )
    })
}

/// Dial `endpoint` and run one public exchange.
pub fn dial_public(root: &Path, endpoint: &str) -> Result<String, String> {
    let identity = load_identity(root, "syncing")?;
    let (mut store, mut cache) = open_sync_state(root, &identity)?;
    let (mut bearer, mut channel) = connect(endpoint)?;
    send_intent(&mut bearer, &mut channel, Intent::Public)?;
    let peer = exchange_hello(
        &mut bearer,
        &mut channel,
        SyncRole::Initiator,
        &identity.root.did(),
    )?;
    let before = complete_manifests(&store);
    let start = bearer.received_bytes();
    let report = sync_bidirectional(
        &mut bearer,
        &mut channel,
        &mut store,
        &mut cache,
        SyncRole::Initiator,
    )
    .map_err(|error| error.to_string())?;
    let received = bearer.received_bytes().saturating_sub(start);
    let completed = newly_completed(&before, &complete_manifests(&store));
    let tickets = exchange_tickets(
        &mut bearer,
        &mut channel,
        SyncRole::Initiator,
        &mut store,
        &mut cache,
        &identity,
        &peer,
        Service::PublicSync,
        received,
        report.accepted as u32,
        completed,
    );
    Ok(format!(
        "Peer sync complete: {}. Tickets: {}.",
        public_summary(&report),
        tickets.summary()
    ))
}

fn newly_completed(
    before: &BTreeMap<String, (ObjectId, u64)>,
    after: &BTreeMap<String, (ObjectId, u64)>,
) -> Vec<CompletedMedia> {
    after
        .iter()
        .filter(|(id, _)| !before.contains_key(*id))
        .take(MAX_TICKET_MANIFESTS)
        .map(|(_, (manifest, bytes))| CompletedMedia {
            manifest: manifest.clone(),
            bytes: *bytes,
        })
        .collect()
}

/// Dial `endpoint` and reconcile exactly one private conversation route.
pub fn dial_private(
    root: &Path,
    endpoint: &str,
    route: OpaqueRoute,
) -> Result<PrivateOutcome, String> {
    let identity = load_identity(root, "syncing")?;
    let (mut store, mut cache) = open_sync_state(root, &identity)?;
    let (mut bearer, mut channel) = connect(endpoint)?;
    send_intent(&mut bearer, &mut channel, Intent::Private(route))?;
    let peer = exchange_hello(
        &mut bearer,
        &mut channel,
        SyncRole::Initiator,
        &identity.root.did(),
    )?;
    let start = bearer.received_bytes();
    match sync_private_route_bidirectional(
        &mut bearer,
        &mut channel,
        &mut store,
        route,
        SyncRole::Initiator,
    ) {
        Ok(report) => {
            let received = bearer.received_bytes().saturating_sub(start);
            let _ = exchange_tickets(
                &mut bearer,
                &mut channel,
                SyncRole::Initiator,
                &mut store,
                &mut cache,
                &identity,
                &peer,
                Service::PrivateSync,
                received,
                report.accepted as u32,
                Vec::new(),
            );
            Ok(PrivateOutcome::Synced {
                received: report.received,
                accepted: report.accepted,
            })
        }
        Err(SyncError::PrivateRouteMismatch) => Ok(PrivateOutcome::NotOnThisPeer),
        Err(error) => Err(error.to_string()),
    }
}

/// Serve one accepted connection: handshake, read the intent, run the
/// matching bounded protocol. `private_routes` is empty when the owner has
/// not opted private conversations into hosting; a private intent is then
/// refused before any route is compared.
pub fn serve(
    root: &Path,
    stream: TcpStream,
    private_routes: &[OpaqueRoute],
) -> Result<String, String> {
    let (mut bearer, mut channel) = accept(stream)?;
    let intent = recv_intent(&mut bearer, &mut channel)?;
    if matches!(intent, Intent::Private(_)) && private_routes.is_empty() {
        return Err("peer asked for a private conversation; private hosting is off".into());
    }
    let identity = load_identity(root, "hosting")?;
    let (mut store, mut cache) = open_sync_state(root, &identity)?;
    let peer = exchange_hello(
        &mut bearer,
        &mut channel,
        SyncRole::Responder,
        &identity.root.did(),
    )?;
    match intent {
        Intent::Public => {
            let before = complete_manifests(&store);
            let start = bearer.received_bytes();
            let report = sync_bidirectional(
                &mut bearer,
                &mut channel,
                &mut store,
                &mut cache,
                SyncRole::Responder,
            )
            .map_err(|error| error.to_string())?;
            let received = bearer.received_bytes().saturating_sub(start);
            let completed = newly_completed(&before, &complete_manifests(&store));
            let tickets = exchange_tickets(
                &mut bearer,
                &mut channel,
                SyncRole::Responder,
                &mut store,
                &mut cache,
                &identity,
                &peer,
                Service::PublicSync,
                received,
                report.accepted as u32,
                completed,
            );
            Ok(format!(
                "public exchange: {}; tickets: {}",
                public_summary(&report),
                tickets.summary()
            ))
        }
        Intent::Private(_) => {
            let start = bearer.received_bytes();
            match sync_private_route_responder_any(
                &mut bearer,
                &mut channel,
                &mut store,
                private_routes,
            ) {
                Ok((_, report)) => {
                    let received = bearer.received_bytes().saturating_sub(start);
                    let tickets = exchange_tickets(
                        &mut bearer,
                        &mut channel,
                        SyncRole::Responder,
                        &mut store,
                        &mut cache,
                        &identity,
                        &peer,
                        Service::PrivateSync,
                        received,
                        report.accepted as u32,
                        Vec::new(),
                    );
                    Ok(format!(
                        "private exchange: received {}, accepted {}; tickets: {}",
                        report.received,
                        report.accepted,
                        tickets.summary()
                    ))
                }
                Err(SyncError::PrivateRouteMismatch) => {
                    Ok("private exchange declined: conversation not on this device".into())
                }
                Err(error) => Err(error.to_string()),
            }
        }
    }
}

/// Progress from a host loop, delivered to the UI thread.
#[derive(Debug, Clone)]
pub enum HostEvent {
    /// The listener is bound and accepting.
    Listening { port: u16 },
    /// One connection finished (successfully or not).
    Connection {
        peer: SocketAddr,
        result: Result<String, String>,
    },
    /// The loop ended; `Err` means it could not bind or accept.
    Ended(Result<String, String>),
}

/// Accept connections on `port` until `stop` is set. Each connection is
/// served on its own bounded worker; the loop itself never blocks on a slow
/// peer. `private_routes` is snapshotted once at start — a conversation
/// imported later needs the host restarted to be served.
pub fn run_host(
    root: PathBuf,
    port: u16,
    private_routes: Vec<OpaqueRoute>,
    stop: Arc<AtomicBool>,
    events: Sender<HostEvent>,
) {
    let listener = match TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port)) {
        Ok(listener) => listener,
        Err(error) => {
            let _ = events.send(HostEvent::Ended(Err(format!(
                "could not listen on port {port}: {error}"
            ))));
            return;
        }
    };
    if let Err(error) = listener.set_nonblocking(true) {
        let _ = events.send(HostEvent::Ended(Err(error.to_string())));
        return;
    }
    let _ = events.send(HostEvent::Listening { port });
    let active = Arc::new(AtomicUsize::new(0));
    let root = Arc::new(root);
    let routes = Arc::new(private_routes);
    let started = Instant::now();
    let mut served = 0usize;
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, peer)) => {
                if active.load(Ordering::Relaxed) >= MAX_CONCURRENT_CONNECTIONS {
                    let _ = events.send(HostEvent::Connection {
                        peer,
                        result: Err("refused: too many connections in progress".into()),
                    });
                    continue;
                }
                served = served.saturating_add(1);
                active.fetch_add(1, Ordering::Relaxed);
                let active = Arc::clone(&active);
                let root = Arc::clone(&root);
                let routes = Arc::clone(&routes);
                let events = events.clone();
                std::thread::spawn(move || {
                    let result = serve(&root, stream, &routes);
                    active.fetch_sub(1, Ordering::Relaxed);
                    let _ = events.send(HostEvent::Connection { peer, result });
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(ACCEPT_POLL);
            }
            Err(error) => {
                let _ = events.send(HostEvent::Ended(Err(format!("accept failed: {error}"))));
                return;
            }
        }
    }
    let _ = events.send(HostEvent::Ended(Ok(format!(
        "Stopped hosting after {} connection(s) in {} minute(s).",
        served,
        started.elapsed().as_secs() / 60
    ))));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_frames_round_trip_and_reject_wrong_lengths() {
        assert_eq!(Intent::decode(&Intent::Public.encode()), Ok(Intent::Public));
        let route = OpaqueRoute::from_bytes([5; 32]);
        assert_eq!(
            Intent::decode(&Intent::Private(route).encode()),
            Ok(Intent::Private(route))
        );
        assert!(Intent::decode(&[]).is_err());
        assert!(Intent::decode(&[INTENT_PRIVATE; 10]).is_err());
        assert!(Intent::decode(&[INTENT_PRIVATE; 34]).is_err());
        assert!(Intent::decode(&[9]).is_err());
    }

    #[test]
    fn bounded_bearer_stops_at_deadline_and_byte_budget() {
        let (a, mut b) = mini_bearer::pair();
        let mut bounded = BoundedBearer::new(a, Instant::now() + Duration::from_secs(60), 10);
        bounded.send(&[1; 6]).unwrap();
        assert_eq!(b.recv().unwrap(), vec![1; 6]);
        // 6 + 6 > 10: the second frame is refused before it is sent.
        assert!(bounded.send(&[2; 6]).is_err());
        assert!(b.try_recv().unwrap().is_none());

        let (a, mut b) = mini_bearer::pair();
        let mut expired = BoundedBearer::new(a, Instant::now() - Duration::from_secs(1), 1 << 20);
        assert!(expired.send(&[1]).is_err());
        b.send(&[7]).unwrap();
        assert!(expired.recv().is_err());
    }
}
