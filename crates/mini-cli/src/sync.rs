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
use mini_sync::{kel_carrier, sync_bidirectional, IngestReport, SyncRole};

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
    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    seed_own_kel_carriers(&identity, &mut store)?;
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

    let report = sync_bidirectional(
        &mut bearer,
        &mut chan,
        &mut store,
        &mut cache,
        SyncRole::Initiator,
    )
    .map_err(|e| CliError::Sync(e.to_string()))?;

    Ok(format_report(addr, &report))
}
