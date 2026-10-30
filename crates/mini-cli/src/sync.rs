//! `mini sync` — live network sync over a real TCP bearer, reusing
//! `mini_bearer`/`mini_sync` exactly the way `mini-bootstrap`'s live demo
//! already proved (D-0062, `mini-sync/tests/sync_over_tcp.rs`) — no new
//! wire protocol. This is the fast-follow `crate::store`'s module docs
//! used to name as deferred: two `mini` homes no longer need a shared
//! filesystem path, only network reachability, to reach the same governed
//! merge Batch 1's exit condition already proved over a shared `--store`.
//!
//! **Honest limit:** a bounded, known number of connections per
//! invocation, then the process exits. `listen` accepts one peer by
//! default, or exactly `--repeat <n>` peers, sequentially, one at a time
//! (never concurrently) — not a real daemon: no signal-based shutdown, no
//! indefinite `--repeat`, no concurrent connection handling. `connect`
//! always dials exactly one peer. Real background/indefinite sync still
//! needs the daemon this crate's own docs already name as deferred
//! (`crate` module docs).

use std::net::TcpListener;
use std::path::Path;

use mini_bearer::{Bearer, Initiator, Responder, TcpBearer};
use mini_objects::ObjectId;
use mini_sync::{
    kel_carrier, receive_retrieval_request, request_retrieval, serve_retrieval, sync_bidirectional,
    IngestReport, RetrievalReport, SyncRole,
};

use crate::error::{CliError, Result};
use crate::store::{build_kel_cache, open_store};

/// Insert this identity's own human + device KELs as ordinary
/// [`mini_sync::KEL_CARRIER`] objects, so a peer receives them the same
/// way it receives any other content — required for the peer's own ingest
/// pipeline to ever accept anything this identity authored. Deterministic
/// (unset timestamp/sequence, same payload every call), so re-running
/// this on every `sync` invocation reinserts the identical object id —
/// idempotent, no store bloat.
fn seed_own_kel_carriers(
    identity: &crate::identity::Identity,
    store: &mut mini_store::Store<mini_store::FsBackend>,
) -> Result<()> {
    let human_did = identity.human_did();
    let human_carrier = kel_carrier(&identity.human.kel(), &human_did, &identity.device)
        .map_err(|e| CliError::Object(e.to_string()))?;
    let device_carrier = kel_carrier(&identity.device.kel(), &human_did, &identity.device)
        .map_err(|e| CliError::Object(e.to_string()))?;
    store
        .insert(&human_carrier)
        .map_err(|e| CliError::Store(e.to_string()))?;
    store
        .insert(&device_carrier)
        .map_err(|e| CliError::Store(e.to_string()))?;
    Ok(())
}

fn format_report(peer: &str, report: &IngestReport) -> String {
    format!(
        "synced with {peer}: received {}, accepted {}, kel carriers {}, \
         unknown_author {}, invalid {}, unsolicited {}",
        report.received,
        report.accepted,
        report.carriers,
        report.unknown_author,
        report.invalid,
        report.unsolicited
    )
}

/// `mini sync listen --addr <host:port> [--repeat <n>]` — bind, then
/// accept and serve `times` peers, sequentially and one at a time (never
/// concurrently), each as [`SyncRole::Responder`] (serves the peer's pull
/// first, then pulls its own). `times` defaults to `1` when unset by the
/// caller (see [`crate::cli`]'s dispatch, which passes `1` for a bare
/// `mini sync listen`); `0` is treated the same as `1` here, so this
/// function alone can never block forever with no peer served at all.
pub fn listen(home: &Path, store_path: &Path, addr: &str, times: u32) -> Result<String> {
    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    seed_own_kel_carriers(&identity, &mut store)?;
    let mut cache = build_kel_cache(home, &identity)?;
    cache
        .hydrate_from_store(&store)
        .map_err(|e| CliError::Sync(e.to_string()))?;

    let listener = TcpListener::bind(addr).map_err(|e| CliError::Io(e.to_string()))?;
    let mut reports = Vec::with_capacity(times.max(1) as usize);
    for _ in 0..times.max(1) {
        let (stream, peer) = listener.accept().map_err(|e| CliError::Io(e.to_string()))?;
        let mut bearer =
            TcpBearer::from_stream(stream).map_err(|e| CliError::Sync(e.to_string()))?;

        let hello = bearer.recv().map_err(|e| CliError::Sync(e.to_string()))?;
        let (mut chan, response) =
            Responder::respond(&hello).map_err(|e| CliError::Sync(e.to_string()))?;
        bearer
            .send(&response)
            .map_err(|e| CliError::Sync(e.to_string()))?;

        let report = sync_bidirectional(
            &mut bearer,
            &mut chan,
            &mut store,
            &mut cache,
            SyncRole::Responder,
        )
        .map_err(|e| CliError::Sync(e.to_string()))?;
        reports.push(format_report(&peer.to_string(), &report));
    }

    Ok(reports.join("\n"))
}

/// `mini sync connect <host:port>` — dial exactly one peer and sync as
/// [`SyncRole::Initiator`] (pulls first, then serves the peer's pull).
pub fn connect(home: &Path, store_path: &Path, addr: &str) -> Result<String> {
    connect_report(home, store_path, addr).map(|report| format_report(addr, &report))
}

/// Structured counterpart used by saved-peer automation. Counts describe
/// received objects only, never acceptance or canonicalization by the peer.
pub fn connect_report(home: &Path, store_path: &Path, addr: &str) -> Result<IngestReport> {
    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    seed_own_kel_carriers(&identity, &mut store)?;
    let mut cache = build_kel_cache(home, &identity)?;
    cache
        .hydrate_from_store(&store)
        .map_err(|e| CliError::Sync(e.to_string()))?;

    // Saved endpoints are numeric addresses: bound connection and stalled-peer
    // I/O without introducing DNS or changing the legacy hostname path.
    let mut bearer = if let Ok(address) = addr.parse::<std::net::SocketAddr>() {
        let stream =
            std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_secs(10))
                .map_err(|e| CliError::Sync(e.to_string()))?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(30)))
            .map_err(|e| CliError::Sync(e.to_string()))?;
        stream
            .set_write_timeout(Some(std::time::Duration::from_secs(30)))
            .map_err(|e| CliError::Sync(e.to_string()))?;
        TcpBearer::from_stream(stream).map_err(|e| CliError::Sync(e.to_string()))?
    } else {
        TcpBearer::connect(addr).map_err(|e| CliError::Sync(e.to_string()))?
    };
    let (init, hello) = Initiator::start().map_err(|e| CliError::Sync(e.to_string()))?;
    bearer
        .send(&hello)
        .map_err(|e| CliError::Sync(e.to_string()))?;
    let response = bearer.recv().map_err(|e| CliError::Sync(e.to_string()))?;
    let mut chan = init
        .finish(&response)
        .map_err(|e| CliError::Sync(e.to_string()))?;

    let report = sync_bidirectional(
        &mut bearer,
        &mut chan,
        &mut store,
        &mut cache,
        SyncRole::Initiator,
    )
    .map_err(|e| CliError::Sync(e.to_string()))?;

    Ok(report)
}

/// A one-shot native release server result. The server exits after one
/// retrieval, keeping the CLI bounded and making the owner choose when to
/// expose a store; it is not a daemon or a background update authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServeReleaseReport {
    /// The exact release seed served.
    pub release_id: String,
    /// Number of selected closure objects offered.
    pub selected: usize,
    /// The connected peer address.
    pub peer: String,
}

/// Retrieve one exact release over the existing encrypted TCP bearer/channel.
/// The caller remains responsible for the subsequent governed verification.
pub fn retrieve_release(
    home: &Path,
    store_path: &Path,
    addr: &str,
    release_id: &ObjectId,
) -> Result<RetrievalReport> {
    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let mut cache = build_kel_cache(home, &identity)?;
    cache
        .hydrate_from_store(&store)
        .map_err(|e| CliError::Sync(e.to_string()))?;

    let mut bearer = TcpBearer::connect(addr).map_err(|e| CliError::Sync(e.to_string()))?;
    let (init, hello) = Initiator::start().map_err(|e| CliError::Sync(e.to_string()))?;
    bearer
        .send(&hello)
        .map_err(|e| CliError::Sync(e.to_string()))?;
    let response = bearer.recv().map_err(|e| CliError::Sync(e.to_string()))?;
    let mut chan = init
        .finish(&response)
        .map_err(|e| CliError::Sync(e.to_string()))?;

    request_retrieval(
        &mut bearer,
        &mut chan,
        &mut store,
        &mut cache,
        std::slice::from_ref(release_id),
    )
    .map_err(|e| CliError::Sync(e.to_string()))
}

/// Serve one exact release retrieval request. The release-specific closure
/// selector lives in `mini-forge`; this module only performs the bearer,
/// bounded framing, and verified transfer composition.
pub fn serve_release(store_path: &Path, addr: &str) -> Result<ServeReleaseReport> {
    let store = open_store(store_path)?;
    let listener = TcpListener::bind(addr).map_err(|e| CliError::Io(e.to_string()))?;
    let (stream, peer) = listener.accept().map_err(|e| CliError::Io(e.to_string()))?;
    let mut bearer = TcpBearer::from_stream(stream).map_err(|e| CliError::Sync(e.to_string()))?;

    let hello = bearer.recv().map_err(|e| CliError::Sync(e.to_string()))?;
    let (mut chan, response) =
        Responder::respond(&hello).map_err(|e| CliError::Sync(e.to_string()))?;
    bearer
        .send(&response)
        .map_err(|e| CliError::Sync(e.to_string()))?;

    let seeds = receive_retrieval_request(&mut bearer, &mut chan)
        .map_err(|e| CliError::Sync(e.to_string()))?;
    if seeds.len() != 1 {
        return Err(CliError::Usage(
            "release retrieval requires exactly one release seed".to_string(),
        ));
    }
    let release_id = seeds[0].clone();
    let ids = {
        let mut selected = mini_forge::release_retrieval_ids(&store, &release_id)
            .map_err(|e| CliError::Forge(e.to_string()))?;
        selected.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        selected
    };
    serve_retrieval(&mut bearer, &mut chan, &store, &ids)
        .map_err(|e| CliError::Sync(e.to_string()))?;

    Ok(ServeReleaseReport {
        release_id: release_id.as_str().to_string(),
        selected: ids.len(),
        peer: peer.to_string(),
    })
}

/// One accepted service session. Each worker reloads its own owner identity,
/// KEL cache and filesystem store; the backend atomically persists immutable
/// objects/index rows and serializes its ordered-index updates across handles.
/// Caller must configure timeouts and retain a shutdown handle for deadlines.
pub(crate) fn serve_stream(
    home: &Path,
    store_path: &Path,
    stream: std::net::TcpStream,
    allowed_peers: &[did_mini::Did],
    admit: impl FnOnce(&did_mini::Did) -> Result<()>,
) -> Result<IngestReport> {
    let identity = crate::identity::load(home)?;
    let mut bearer = TcpBearer::from_stream(stream).map_err(|e| CliError::Sync(e.to_string()))?;
    let hello = bearer.recv().map_err(|e| CliError::Sync(e.to_string()))?;
    let (mut channel, response) =
        Responder::respond(&hello).map_err(|e| CliError::Sync(e.to_string()))?;
    bearer
        .send(&response)
        .map_err(|e| CliError::Sync(e.to_string()))?;
    let transcript = session_transcript(&hello, &response);
    let peer = receive_session_proof(
        &mut bearer,
        &mut channel,
        home,
        &identity,
        &transcript,
        b'i',
        None,
    )?;
    if !allowed_peers.contains(&peer) {
        return Err(CliError::Sync(
            "peer is not authorized for this service".into(),
        ));
    }
    admit(&peer)?;
    let mut store = open_store(store_path)?;
    seed_own_kel_carriers(&identity, &mut store)?;
    let mut cache = build_kel_cache(home, &identity)?;
    cache
        .hydrate_from_store(&store)
        .map_err(|e| CliError::Sync(e.to_string()))?;
    send_session_proof(
        &mut bearer,
        &mut channel,
        &identity,
        &transcript,
        b'r',
        &peer,
    )?;
    drop(store);
    let mut store = mini_store::Store::new(
        crate::service::quota::QuotaBackend::open(store_path, peer.as_str())
            .map_err(|e| CliError::Store(e.to_string()))?,
    );
    mini_sync::sync_bidirectional_paged(
        &mut bearer,
        &mut channel,
        &mut store,
        &mut cache,
        SyncRole::Responder,
    )
    .map_err(|e| CliError::Sync(e.to_string()))
}

const SESSION_AAD: &[u8] = b"MINI/FORGE-SESSION2";
const SESSION_TYPE: &str = "mini/forge-session-v2";
const MAX_SESSION_PROOF: usize = 16 * 1024;

fn session_transcript(hello: &[u8], response: &[u8]) -> [u8; 32] {
    let mut data = b"mini/forge-session/transcript/v2\0".to_vec();
    data.extend_from_slice(&(hello.len() as u32).to_be_bytes());
    data.extend_from_slice(hello);
    data.extend_from_slice(&(response.len() as u32).to_be_bytes());
    data.extend_from_slice(response);
    mini_crypto::HashAlgorithm::Blake3.digest(&data)
}
fn proof_payload(transcript: &[u8; 32], role: u8, audience: &did_mini::Did) -> Vec<u8> {
    let mut payload = b"mini/forge-session/proof/v2\0".to_vec();
    payload.push(role);
    payload.extend_from_slice(transcript);
    payload.extend_from_slice(audience.as_str().as_bytes());
    payload
}
fn send_session_proof(
    bearer: &mut TcpBearer,
    channel: &mut mini_bearer::Channel,
    identity: &crate::identity::Identity,
    transcript: &[u8; 32],
    role: u8,
    audience: &did_mini::Did,
) -> Result<()> {
    let proof =
        mini_objects::ObjectBuilder::new(mini_objects::ObjectType::Custom(SESSION_TYPE.into()))
            .payload(mini_objects::Payload::Public(proof_payload(
                transcript, role, audience,
            )))
            .sign(&identity.human_did(), &identity.device)
            .map_err(|e| CliError::Object(e.to_string()))?;
    let frame = channel
        .seal(&proof.to_bytes(), SESSION_AAD)
        .map_err(|e| CliError::Sync(e.to_string()))?;
    bearer
        .send(&frame)
        .map_err(|e| CliError::Sync(e.to_string()))
}
fn receive_session_proof(
    bearer: &mut TcpBearer,
    channel: &mut mini_bearer::Channel,
    home: &Path,
    identity: &crate::identity::Identity,
    transcript: &[u8; 32],
    role: u8,
    expected_peer: Option<&did_mini::Did>,
) -> Result<did_mini::Did> {
    use mini_forge::IdentityOracle;
    let frame = bearer.recv().map_err(|e| CliError::Sync(e.to_string()))?;
    if frame.len() > MAX_SESSION_PROOF {
        return Err(CliError::Sync("session proof exceeds limit".into()));
    }
    let bytes = channel
        .open(&frame, SESSION_AAD)
        .map_err(|e| CliError::Sync(e.to_string()))?;
    let proof =
        mini_objects::Object::from_bytes(&bytes).map_err(|e| CliError::Object(e.to_string()))?;
    if proof.object_type != mini_objects::ObjectType::Custom(SESSION_TYPE.into())
        || !proof.links.is_empty()
        || proof.payload
            != mini_objects::Payload::Public(proof_payload(transcript, role, &identity.human_did()))
        || expected_peer.is_some_and(|peer| *peer != proof.author_human)
    {
        return Err(CliError::Sync(
            "session peer/transcript proof mismatch".into(),
        ));
    }
    let oracle = crate::store::build_oracle(home, identity)?;
    let root = oracle
        .kel(&proof.author_human)
        .ok_or_else(|| CliError::Sync("session peer root KEL is not trusted".into()))?;
    let device = oracle
        .kel(&proof.author_device)
        .ok_or_else(|| CliError::Sync("session peer device KEL is not trusted".into()))?;
    mini_objects::verify_provenance(&proof, root, device)
        .map_err(|e| CliError::Sync(e.to_string()))?;
    Ok(proof.author_human)
}

/// Mutually authenticate an encrypted session against explicitly trusted KELs
/// and the saved endpoint's pinned root DID, before offering any store objects.
/// This mode fails closed; it never falls back to anonymous legacy sync.
pub fn connect_authenticated_report(
    home: &Path,
    store_path: &Path,
    addr: &str,
    expected_peer: &did_mini::Did,
) -> Result<IngestReport> {
    let identity = crate::identity::load(home)?;
    let address: std::net::SocketAddr = addr
        .parse()
        .map_err(|_| CliError::Usage("authenticated sync requires numeric IP:port".into()))?;
    let stream = std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_secs(10))
        .map_err(|e| CliError::Sync(e.to_string()))?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .map_err(|e| CliError::Sync(e.to_string()))?;
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(30)))
        .map_err(|e| CliError::Sync(e.to_string()))?;
    let shutdown = stream
        .try_clone()
        .map_err(|e| CliError::Sync(e.to_string()))?;
    let (_deadline_guard, deadline_cancel) = std::sync::mpsc::channel::<()>();
    // Dropping the guard on any return cancels the watchdog immediately.
    std::thread::Builder::new()
        .name("mini-sync-deadline".into())
        .spawn(move || {
            if deadline_cancel.recv_timeout(std::time::Duration::from_secs(120))
                == Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            {
                let _ = shutdown.shutdown(std::net::Shutdown::Both);
            }
        })
        .map_err(|e| CliError::Sync(e.to_string()))?;
    let mut bearer = TcpBearer::from_stream(stream).map_err(|e| CliError::Sync(e.to_string()))?;
    let (initiator, hello) = Initiator::start().map_err(|e| CliError::Sync(e.to_string()))?;
    bearer
        .send(&hello)
        .map_err(|e| CliError::Sync(e.to_string()))?;
    let response = bearer.recv().map_err(|e| CliError::Sync(e.to_string()))?;
    let mut channel = initiator
        .finish(&response)
        .map_err(|e| CliError::Sync(e.to_string()))?;
    let transcript = session_transcript(&hello, &response);
    send_session_proof(
        &mut bearer,
        &mut channel,
        &identity,
        &transcript,
        b'i',
        expected_peer,
    )?;
    receive_session_proof(
        &mut bearer,
        &mut channel,
        home,
        &identity,
        &transcript,
        b'r',
        Some(expected_peer),
    )?;
    let mut store = open_store(store_path)?;
    seed_own_kel_carriers(&identity, &mut store)?;
    let mut cache = build_kel_cache(home, &identity)?;
    cache
        .hydrate_from_store(&store)
        .map_err(|e| CliError::Sync(e.to_string()))?;
    mini_sync::sync_bidirectional_paged(
        &mut bearer,
        &mut channel,
        &mut store,
        &mut cache,
        SyncRole::Initiator,
    )
    .map_err(|e| CliError::Sync(e.to_string()))
}
