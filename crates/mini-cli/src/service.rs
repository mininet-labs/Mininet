//! Owner-invoked native sync service. The listener offers the entire store to
//! explicitly allowed, mutually authenticated encrypted peers only after an explicit sharing acknowledgement.
//! Trusted local KELs and transcript-bound signed session proofs authenticate
//! endpoints; content signatures remain an independent trust boundary.
use std::fs;
use std::io::Read;
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::error::{CliError, Result};
use crate::json::{CommandResult, JsonValue};

#[path = "service_quota.rs"]
pub(crate) mod quota;

const MAX_WORKERS: usize = 4;
const MAX_HANDSHAKES: usize = 8;
const MAX_SOURCE_HANDSHAKES: usize = 2;
const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(25);
const MAX_STATE: usize = 4096;
fn io(e: std::io::Error) -> CliError {
    CliError::Io(e.to_string())
}
fn usage(s: &str) -> CliError {
    CliError::Usage(s.into())
}

#[derive(Default)]
struct State {
    address: String,
    seed_only: bool,
    accepted: u64,
    completed: u64,
    failed: u64,
    active: u64,
}
impl State {
    fn save(&self, home: &Path) -> Result<()> {
        let text = format!(
            "mini-sync-service/v1\n{}\n{}\n{}\n{}\n{}\n{}\n",
            self.address,
            self.accepted,
            self.completed,
            self.failed,
            self.active,
            if self.seed_only { "seed" } else { "store" }
        );
        mini_durable::atomic_replace(&home.join("service.state"), text.as_bytes()).map_err(io)
    }
    fn load(home: &Path) -> Result<Self> {
        let file = match fs::File::open(home.join("service.state")) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(io(e)),
        };
        let mut text = String::new();
        file.take(MAX_STATE as u64 + 1)
            .read_to_string(&mut text)
            .map_err(io)?;
        if text.len() > MAX_STATE {
            return Err(usage("service state exceeds limit"));
        }
        let rows: Vec<_> = text.lines().collect();
        if !(rows.len() == 6 || rows.len() == 7)
            || rows[0] != "mini-sync-service/v1"
            || rows[1].parse::<SocketAddr>().is_err()
        {
            return Err(usage("corrupt service state"));
        }
        let number = |i: usize| {
            rows[i]
                .parse::<u64>()
                .map_err(|_| usage("corrupt service counters"))
        };
        Ok(Self {
            address: rows[1].into(),
            seed_only: match rows.get(6).copied().unwrap_or("store") {
                "seed" => true,
                "store" => false,
                _ => return Err(usage("invalid service scope")),
            },
            accepted: number(2)?,
            completed: number(3)?,
            failed: number(4)?,
            active: number(5)?,
        })
    }
    fn report(&self, running: bool) -> CommandResult {
        CommandResult::new(format!(
            "service {} at {}; accepted {}, completed {}, failed {}, active {}",
            if running { "running" } else { "stopped" },
            self.address,
            self.accepted,
            self.completed,
            self.failed,
            if running { self.active } else { 0 }
        ))
        .field("running", JsonValue::Bool(running))
        .field("address", JsonValue::str(&self.address))
        .field("accepted", JsonValue::num(self.accepted))
        .field("completed", JsonValue::num(self.completed))
        .field("failed", JsonValue::num(self.failed))
        .field(
            "active",
            JsonValue::num(if running { self.active } else { 0 }),
        )
        .field("endpoint_authenticated", JsonValue::Bool(true))
        .field(
            "scope",
            JsonValue::str(if self.seed_only {
                "seed-pool"
            } else {
                "whole-store"
            }),
        )
    }
}
fn is_running(home: &Path) -> Result<bool> {
    match mini_durable::try_lock_exclusive(&home.join("service.lock")) {
        Ok(_lock) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(true),
        Err(e) => Err(io(e)),
    }
}
struct Worker {
    source: IpAddr,
    authenticated: Arc<AtomicBool>,
    handshake_deadline: Instant,
    socket: TcpStream,
    deadline: Instant,
    task: JoinHandle<bool>,
}

pub fn dispatch(home: &Path, store: &Path, args: Vec<String>, json: bool) -> Result<String> {
    let verb = args.first().map(String::as_str).unwrap_or("");
    mini_durable::create_dir_all(home).map_err(io)?;
    let result = match verb {
        "status" | "stop" => {
            if args.len() != 1 { return Err(usage("unexpected service arguments")); }
            let _control = mini_durable::lock_exclusive(&home.join("service.control.lock")).map_err(io)?;
            let running = is_running(home)?;
            let state = State::load(home)?;
            if verb == "stop" && running {
                mini_durable::atomic_replace(&home.join("service.stop"), b"stop\n").map_err(io)?;
            }
            state.report(running).field("stop_requested", JsonValue::Bool(verb == "stop" && running))
        }
        "run" => {
            let mut address = "127.0.0.1:7400".to_string();
            let mut limit = 0u64;
            let mut timeout_ms = 30_000u64;
            let mut acknowledged = false;
            let mut seed_only = false;
            let mut allowed_peers = Vec::new();
            let mut seen = std::collections::BTreeSet::new();
            let mut i = 1;
            while i < args.len() {
                let flag = args[i].as_str();
                if flag != "--allow-peer" && !seen.insert(flag) { return Err(usage("duplicate service flag")); }
                if flag == "--seed-only" { seed_only = true; i += 1; continue; }
                if flag == "--allow-store-sharing" { acknowledged = true; i += 1; continue; }
                let value = args.get(i + 1).ok_or_else(|| usage("missing service flag value"))?;
                match flag {
                    "--addr" => address = value.clone(),
                    "--allow-peer" => {
                        let did = did_mini::Did::parse(value).map_err(|e| CliError::Identity(e.to_string()))?;
                        if allowed_peers.len() >= 64 || allowed_peers.contains(&did) { return Err(usage("peer allowlist duplicate or exceeds64")); }
                        allowed_peers.push(did);
                    }
                    "--max-connections" => limit = value.parse().map_err(|_| usage("invalid max-connections"))?,
                    "--idle-timeout-ms" => timeout_ms = value.parse().map_err(|_| usage("invalid idle-timeout-ms"))?,
                    _ => return Err(usage("unknown service flag")),
                }
                i += 2;
            }
            if acknowledged && seed_only { return Err(usage("choose seed-only or whole-store sharing, not both")); }
            if !acknowledged && !seed_only { return Err(usage("service run requires --allow-store-sharing: all stored objects are offered to allowed peers")); }
            if !(50..=300_000).contains(&timeout_ms) { return Err(usage("idle-timeout-ms must be 50..300000")); }
            if allowed_peers.is_empty() { return Err(usage("service requires at least one --allow-peer DID")); }
            let addr: SocketAddr = address.parse().map_err(|_| usage("service requires a numeric IP:port"))?;
            run(home, store, addr, limit, Duration::from_millis(timeout_ms), allowed_peers, seed_only)?
        }
        _ => return Err(usage("service run [--addr <ip:port>] --allow-store-sharing --allow-peer DID [--max-connections N] [--idle-timeout-ms N] | status | stop")),
    };
    Ok(result.render(json, &format!("service.{verb}")))
}

fn run(
    home: &Path,
    store: &Path,
    addr: SocketAddr,
    limit: u64,
    idle: Duration,
    allowed_peers: Vec<did_mini::Did>,
    seed_only: bool,
) -> Result<CommandResult> {
    crate::identity::load(home)?;
    // Serialize startup with stop/status so an acknowledged stop cannot be
    // removed as a stale request while a new service is starting.
    let control = mini_durable::lock_exclusive(&home.join("service.control.lock")).map_err(io)?;
    let _owner = mini_durable::try_lock_exclusive(&home.join("service.lock"))
        .map_err(|e| CliError::Io(format!("service already running or lock unavailable: {e}")))?;
    quota::initialize(store).map_err(|e| CliError::Store(e.to_string()))?;
    let listener = TcpListener::bind(addr).map_err(io)?;
    listener.set_nonblocking(true).map_err(io)?;
    let mut state = State {
        address: listener.local_addr().map_err(io)?.to_string(),
        seed_only,
        ..State::default()
    };
    {
        match fs::remove_file(home.join("service.stop")) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(io(e)),
        }
        state.save(home)?;
    }
    drop(control);
    let mut workers: Vec<Worker> = Vec::new();
    let active_peers = Arc::new(Mutex::new(std::collections::BTreeSet::<String>::new()));
    let mut stopping = false;
    let mut fatal = None;
    loop {
        stopping |= home.join("service.stop").exists();
        for worker in &workers {
            if stopping
                || Instant::now() >= worker.deadline
                || (!worker.authenticated.load(Ordering::Acquire)
                    && Instant::now() >= worker.handshake_deadline)
            {
                let _ = worker.socket.shutdown(Shutdown::Both);
            }
        }
        let mut changed = false;
        let mut i = 0;
        while i < workers.len() {
            if workers[i].task.is_finished() {
                let worker = workers.swap_remove(i);
                if worker.task.join().unwrap_or(false) {
                    state.completed += 1;
                } else {
                    state.failed += 1;
                }
                state.active = workers.len() as u64;
                changed = true;
            } else {
                i += 1;
            }
        }
        if changed {
            if let Err(e) = state.save(home) {
                fatal = Some(e);
                stopping = true;
            }
        }
        if (stopping || (limit != 0 && state.accepted >= limit)) && workers.is_empty() {
            break;
        }
        if !stopping && (limit == 0 || state.accepted < limit) {
            match listener.accept() {
                Ok((stream, source)) => {
                    state.accepted += 1;
                    if workers
                        .iter()
                        .filter(|w| !w.authenticated.load(Ordering::Acquire))
                        .count()
                        >= MAX_HANDSHAKES
                        || workers
                            .iter()
                            .filter(|w| {
                                w.source == source.ip() && !w.authenticated.load(Ordering::Acquire)
                            })
                            .count()
                            >= MAX_SOURCE_HANDSHAKES
                    {
                        let _ = stream.shutdown(Shutdown::Both);
                        state.failed += 1;
                    } else {
                        let spawn = || -> std::io::Result<Worker> {
                            stream.set_read_timeout(Some(idle))?;
                            stream.set_write_timeout(Some(idle))?;
                            let shutdown = stream.try_clone()?;
                            let home = home.to_path_buf();
                            let store = store.to_path_buf();
                            let allowed_peers = allowed_peers.clone();
                            let authenticated = Arc::new(AtomicBool::new(false));
                            let phase = authenticated.clone();
                            let active_peers = active_peers.clone();
                            let task = thread::Builder::new().name("mini-sync-peer".into()).spawn(
                                move || {
                                    if seed_only {
                                        crate::seed::serve_stream(&home, stream, &allowed_peers)
                                            .is_ok()
                                    } else {
                                        let mut admitted = None;
                                        let result = crate::sync::serve_stream(
                                            &home,
                                            &store,
                                            stream,
                                            &allowed_peers,
                                            |peer| {
                                                let mut active =
                                                    active_peers.lock().map_err(|_| {
                                                        usage("admission lock poisoned")
                                                    })?;
                                                if active.len() >= MAX_WORKERS
                                                    || active.contains(peer.as_str())
                                                {
                                                    return Err(usage(
                                                        "authenticated peer capacity reached",
                                                    ));
                                                }
                                                active.insert(peer.as_str().to_string());
                                                admitted = Some(peer.as_str().to_string());
                                                phase.store(true, Ordering::Release);
                                                Ok(())
                                            },
                                        );
                                        if let Some(peer) = admitted {
                                            if let Ok(mut active) = active_peers.lock() {
                                                active.remove(&peer);
                                            }
                                        }
                                        result.is_ok()
                                    }
                                },
                            )?;
                            Ok(Worker {
                                source: source.ip(),
                                authenticated,
                                handshake_deadline: Instant::now() + HANDSHAKE_DEADLINE,
                                socket: shutdown,
                                deadline: Instant::now() + idle.saturating_mul(4),
                                task,
                            })
                        };
                        match spawn() {
                            Ok(worker) => workers.push(worker),
                            Err(_) => state.failed += 1,
                        }
                        state.active = workers.len() as u64;
                    }
                    if let Err(e) = state.save(home) {
                        fatal = Some(e);
                        stopping = true;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => (),
                Err(e) => {
                    fatal = Some(io(e));
                    stopping = true;
                }
            }
        }
        thread::sleep(POLL);
    }
    state.active = 0;
    state.save(home)?;
    if let Some(e) = fatal {
        return Err(e);
    }
    Ok(state.report(false))
}
