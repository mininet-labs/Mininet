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

use crate::{configure_peer_stream, load_desktop_identity, open_sync_state, PEER_IO_TIMEOUT};
use mini_bearer::{Bearer, Channel, Initiator, Responder, TcpBearer};
use mini_objects::OpaqueRoute;
use mini_store::{FsBackend, Store};
use mini_sync::{
    sync_bidirectional, sync_private_route_bidirectional, sync_private_route_responder_any,
    SyncError, SyncRole,
};
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

fn connect(endpoint: &str) -> Result<(TcpBearer, Channel), String> {
    let address = resolve(endpoint)?;
    let stream = TcpStream::connect_timeout(&address, PEER_IO_TIMEOUT)
        .map_err(|error| format!("{endpoint} refused or timed out: {error}"))?;
    configure_peer_stream(&stream)?;
    let mut bearer = TcpBearer::from_stream(stream).map_err(|error| error.to_string())?;
    let (initiator, hello) = Initiator::start().map_err(|error| error.to_string())?;
    bearer.send(&hello).map_err(|error| error.to_string())?;
    let response = bearer.recv().map_err(|error| error.to_string())?;
    let channel = initiator
        .finish(&response)
        .map_err(|error| error.to_string())?;
    Ok((bearer, channel))
}

fn accept(stream: TcpStream) -> Result<(TcpBearer, Channel), String> {
    stream
        .set_nonblocking(false)
        .map_err(|error| error.to_string())?;
    configure_peer_stream(&stream)?;
    let mut bearer = TcpBearer::from_stream(stream).map_err(|error| error.to_string())?;
    let hello = bearer.recv().map_err(|error| error.to_string())?;
    let (channel, response) = Responder::respond(&hello).map_err(|error| error.to_string())?;
    bearer.send(&response).map_err(|error| error.to_string())?;
    Ok((bearer, channel))
}

fn send_intent(
    bearer: &mut TcpBearer,
    channel: &mut Channel,
    intent: Intent,
) -> Result<(), String> {
    let sealed = channel
        .seal(&intent.encode(), INTENT_AAD)
        .map_err(|error| error.to_string())?;
    bearer.send(&sealed).map_err(|error| error.to_string())
}

fn recv_intent(bearer: &mut TcpBearer, channel: &mut Channel) -> Result<Intent, String> {
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

/// Dial `endpoint` and run one public exchange.
pub fn dial_public(root: &Path, endpoint: &str) -> Result<String, String> {
    let identity = load_desktop_identity(root, false).map_err(|error| {
        format!(
            "identity/device vault unavailable; unlock the identity once before syncing: {error}"
        )
    })?;
    let (mut store, mut cache) = open_sync_state(root, &identity)?;
    let (mut bearer, mut channel) = connect(endpoint)?;
    send_intent(&mut bearer, &mut channel, Intent::Public)?;
    let report = sync_bidirectional(
        &mut bearer,
        &mut channel,
        &mut store,
        &mut cache,
        SyncRole::Initiator,
    )
    .map_err(|error| error.to_string())?;
    Ok(format!("Peer sync complete: {}.", public_summary(&report)))
}

/// Dial `endpoint` and reconcile exactly one private conversation route.
pub fn dial_private(
    root: &Path,
    endpoint: &str,
    route: OpaqueRoute,
) -> Result<PrivateOutcome, String> {
    let mut store = Store::new(FsBackend::open(root).map_err(|error| error.to_string())?);
    let (mut bearer, mut channel) = connect(endpoint)?;
    send_intent(&mut bearer, &mut channel, Intent::Private(route))?;
    match sync_private_route_bidirectional(
        &mut bearer,
        &mut channel,
        &mut store,
        route,
        SyncRole::Initiator,
    ) {
        Ok(report) => Ok(PrivateOutcome::Synced {
            received: report.received,
            accepted: report.accepted,
        }),
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
    match recv_intent(&mut bearer, &mut channel)? {
        Intent::Public => {
            let identity = load_desktop_identity(root, false).map_err(|error| {
                format!("identity/device vault unavailable; unlock the identity once before hosting: {error}")
            })?;
            let (mut store, mut cache) = open_sync_state(root, &identity)?;
            let report = sync_bidirectional(
                &mut bearer,
                &mut channel,
                &mut store,
                &mut cache,
                SyncRole::Responder,
            )
            .map_err(|error| error.to_string())?;
            Ok(format!("public exchange: {}", public_summary(&report)))
        }
        Intent::Private(_) if private_routes.is_empty() => {
            Err("peer asked for a private conversation; private hosting is off".into())
        }
        Intent::Private(_) => {
            let mut store = Store::new(FsBackend::open(root).map_err(|error| error.to_string())?);
            match sync_private_route_responder_any(
                &mut bearer,
                &mut channel,
                &mut store,
                private_routes,
            ) {
                Ok((_, report)) => Ok(format!(
                    "private exchange: received {}, accepted {}",
                    report.received, report.accepted
                )),
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
}
