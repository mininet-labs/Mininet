//! The actual isolated execution: translates a declared `Capability` list
//! into a deny-by-default `WasiCtx`, configures fuel/epoch/memory limits,
//! runs the component via the `wasi:cli/command` world, and reports a
//! [`SandboxOutcome`]. This is the only module in the tree that touches
//! `wasmtime`/`wasmtime-wasi` execution APIs directly (D-0069).
//!
//! ## What "deny-by-default" means here, precisely
//!
//! Filesystem and network access are structurally denied: an
//! undeclared [`Capability::WorkspaceRead`]/[`ScratchWrite`]/
//! [`ArtifactsWrite`] means no `preopened_dir` call happens at all for
//! that directory, so the guest's `wasi:filesystem` imports have nothing
//! to resolve a path against. An undeclared [`Capability::NetworkHost`]
//! means the `socket_addr_check` closure has an empty allow-list, so
//! every connection attempt is refused regardless of what `wasi:sockets`
//! calls the guest makes.
//!
//! `wasi:clocks/monotonic-clock` and `wasi:random/random` are **not**
//! structurally removable this way: `wasmtime_wasi::bindings::sync::
//! Command` binds the full `wasi:cli/command` world, which treats clocks
//! and secure randomness as ambient interfaces every command gets, not
//! interfaces a world can selectively omit without hand-authoring a
//! narrower WIT world (a real follow-up, not done here). Today
//! [`Capability::ClockMonotonic`] and [`Capability::RandomDeterministic`]
//! are enforced as *declared policy* -- checked against the request and
//! carried into the provenance record -- and, for
//! `RandomDeterministic` specifically, the host RNG really is swapped for
//! [`crate::random::DeterministicRng`]. But a component with neither
//! capability declared still has a working (non-deterministic) clock and
//! RNG available to it. This is stated here, not glossed over, per
//! D-0069's honesty requirement.

use std::collections::HashSet;
use std::future::Future;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mini_pipeline::{Capability, ResourceLimits};
use mini_pipeline_protocol::{ExitStatus, ResourceExceeded};
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::bindings::sync::Command;
use wasmtime_wasi::pipe::MemoryOutputPipe;
use wasmtime_wasi::{
    add_to_linker_sync, DirPerms, FilePerms, ResourceTable, SocketAddrUse, WasiCtx, WasiCtxBuilder,
    WasiView,
};

use crate::error::{Result, RunnerError};
use crate::limiter::MemoryLimiter;
use crate::random::DeterministicRng;

/// Everything the caller (`main.rs`) needs out of one execution, beyond
/// what it already knows (the request itself).
pub struct SandboxOutcome {
    pub exit_status: ExitStatus,
    pub fuel_consumed: u64,
    pub wall_clock_ms: u64,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// The concrete host directories a request's capabilities may draw on.
/// `None` for a directory the request didn't declare a capability for --
/// callers must not synthesize a path in that case, since a present path
/// with an absent capability would be a silent, harder-to-audit way to
/// grant access the request never asked for.
pub struct Workspace<'a> {
    pub workspace_dir: Option<&'a Path>,
    pub scratch_dir: Option<&'a Path>,
    pub artifacts_dir: Option<&'a Path>,
}

struct RunnerState {
    table: ResourceTable,
    wasi: WasiCtx,
    limiter: MemoryLimiter,
}

impl WasiView for RunnerState {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
}

pub fn execute(
    component_bytes: &[u8],
    workspace: &Workspace<'_>,
    capabilities: &[Capability],
    limits: &ResourceLimits,
    deterministic_seed: [u8; 32],
) -> Result<SandboxOutcome> {
    limits
        .validate()
        .map_err(|e| RunnerError::Wasmtime(format!("invalid resource limits: {e}")))?;

    let mut config = Config::new();
    config.consume_fuel(true);
    config.epoch_interruption(true);
    let engine = Engine::new(&config).map_err(|e| RunnerError::Wasmtime(e.to_string()))?;

    let component = Component::from_binary(&engine, component_bytes)
        .map_err(|e| RunnerError::Wasmtime(format!("component compilation failed: {e}")))?;

    let mut linker: Linker<RunnerState> = Linker::new(&engine);
    add_to_linker_sync(&mut linker).map_err(|e| RunnerError::Wasmtime(e.to_string()))?;

    let stdout = MemoryOutputPipe::new(limits.max_stdout_bytes as usize);
    let stderr = MemoryOutputPipe::new(limits.max_stderr_bytes as usize);

    let wasi = build_wasi_ctx(
        workspace,
        capabilities,
        deterministic_seed,
        stdout.clone(),
        stderr.clone(),
    )?;

    let mut store = Store::new(
        &engine,
        RunnerState {
            table: ResourceTable::new(),
            wasi,
            limiter: MemoryLimiter::new(limits.max_memory_bytes),
        },
    );
    store.limiter(|state| &mut state.limiter);
    store
        .set_fuel(limits.max_fuel)
        .map_err(|e| RunnerError::Wasmtime(e.to_string()))?;
    store.set_epoch_deadline(1);
    store.epoch_deadline_trap();

    // Wall-clock emergency stop (D-0069: "never the primary reproducibility
    // mechanism", fuel already is that -- this is purely a backstop for
    // cases fuel accounting doesn't catch, e.g. a tight host-call loop
    // that burns wall time without burning wasm-level fuel).
    let watchdog_engine = engine.clone();
    let deadline = Duration::from_millis(limits.max_wall_clock_ms);
    let watchdog = std::thread::spawn(move || {
        std::thread::sleep(deadline);
        watchdog_engine.increment_epoch();
    });

    let start = Instant::now();
    let run_result = (|| -> anyhow::Result<std::result::Result<(), ()>> {
        let command = Command::instantiate(&mut store, &component, &linker)?;
        command.wasi_cli_run().call_run(&mut store)
    })();
    let wall_clock_ms = start.elapsed().as_millis() as u64;

    // The watchdog thread either already fired (harmless: `call_run` has
    // already returned by the time we get here) or is still sleeping and
    // can be left to finish and exit on its own -- it holds no lock and
    // touches nothing but the (still-alive) `Engine` handle.
    drop(watchdog);

    let fuel_consumed = limits
        .max_fuel
        .saturating_sub(store.get_fuel().unwrap_or(0));
    let stdout_bytes = stdout.contents().to_vec();
    let stderr_bytes = stderr.contents().to_vec();
    let hit_memory_limit = store.data().limiter.hit_limit();

    let mut exit_status = classify_outcome(run_result);
    // Both of the checks below exist because a refused resource grant
    // does not necessarily surface as a clean, typed trap at the
    // component boundary: a refused `memory.grow` returns -1 to the
    // guest (the WebAssembly-spec-compliant behavior `ResourceLimiter`
    // documents) rather than trapping, and a `MemoryOutputPipe` write
    // past capacity reaches the guest as an ordinary WASI stream error,
    // which a guest's own language runtime (e.g. Rust's `println!`,
    // which panics and aborts on any stdout write failure) can turn into
    // a generic `GuestTrap` with a message that has nothing to do with
    // the actual cause. Rather than pattern-match trap message text
    // (tried first; proved unreliable -- see D-0069 follow-up notes),
    // this reclassifies from the definitively observable post-run state:
    // a full output pipe or a limiter that actually refused a grant is
    // stronger evidence than whatever the guest's own crash message says.
    if matches!(exit_status, ExitStatus::Success | ExitStatus::GuestTrap(_)) {
        if stdout_bytes.len() as u64 >= limits.max_stdout_bytes {
            exit_status = ExitStatus::ResourceExceeded(ResourceExceeded::StdoutBytes);
        } else if stderr_bytes.len() as u64 >= limits.max_stderr_bytes {
            exit_status = ExitStatus::ResourceExceeded(ResourceExceeded::StderrBytes);
        } else if hit_memory_limit {
            exit_status = ExitStatus::ResourceExceeded(ResourceExceeded::Memory);
        }
    }

    Ok(SandboxOutcome {
        exit_status,
        fuel_consumed,
        wall_clock_ms,
        stdout: stdout_bytes,
        stderr: stderr_bytes,
    })
}

fn classify_outcome(run_result: anyhow::Result<std::result::Result<(), ()>>) -> ExitStatus {
    match run_result {
        Ok(Ok(())) => ExitStatus::Success,
        Ok(Err(())) => ExitStatus::GuestTrap(
            "component's wasi:cli/run returned a failing exit status".to_string(),
        ),
        Err(e) => {
            if let Some(trap) = e.downcast_ref::<wasmtime::Trap>() {
                match trap {
                    wasmtime::Trap::OutOfFuel => {
                        return ExitStatus::ResourceExceeded(ResourceExceeded::Fuel)
                    }
                    wasmtime::Trap::Interrupt => {
                        return ExitStatus::ResourceExceeded(ResourceExceeded::WallClock)
                    }
                    _ => {}
                }
            }
            ExitStatus::GuestTrap(format!("{e:#}"))
        }
    }
}

fn build_wasi_ctx(
    workspace: &Workspace<'_>,
    capabilities: &[Capability],
    deterministic_seed: [u8; 32],
    stdout: MemoryOutputPipe,
    stderr: MemoryOutputPipe,
) -> Result<WasiCtx> {
    let mut builder = WasiCtxBuilder::new();
    // stdin is closed by default (WasiCtxBuilder::new()'s own contract) --
    // never opened; there is no `Capability` for it because a build step
    // reading interactive input is never a meaningful policy choice.
    builder.stdout(stdout).stderr(stderr);

    let mut network_hosts = Vec::new();
    let mut secrets = Vec::new();
    let mut has_random_deterministic = false;

    for cap in capabilities {
        match cap {
            Capability::WorkspaceRead => {
                let dir = workspace.workspace_dir.ok_or_else(|| {
                    RunnerError::Wasmtime(
                        "workspace:read declared but no workspace directory provided".to_string(),
                    )
                })?;
                builder
                    .preopened_dir(dir, "/workspace", DirPerms::READ, FilePerms::READ)
                    .map_err(|e| RunnerError::Wasmtime(e.to_string()))?;
            }
            Capability::ScratchWrite => {
                let dir = workspace.scratch_dir.ok_or_else(|| {
                    RunnerError::Wasmtime(
                        "scratch:write declared but no scratch directory provided".to_string(),
                    )
                })?;
                builder
                    .preopened_dir(dir, "/scratch", DirPerms::all(), FilePerms::all())
                    .map_err(|e| RunnerError::Wasmtime(e.to_string()))?;
            }
            Capability::ArtifactsWrite => {
                let dir = workspace.artifacts_dir.ok_or_else(|| {
                    RunnerError::Wasmtime(
                        "artifacts:write declared but no artifacts directory provided".to_string(),
                    )
                })?;
                builder
                    .preopened_dir(dir, "/artifacts", DirPerms::all(), FilePerms::all())
                    .map_err(|e| RunnerError::Wasmtime(e.to_string()))?;
            }
            Capability::ClockMonotonic => {
                // See module docs: ambient in the Command world, nothing
                // further to wire up. Recorded as declared policy only.
            }
            Capability::RandomDeterministic => {
                has_random_deterministic = true;
            }
            Capability::NetworkHost(host) => network_hosts.push(host.clone()),
            Capability::SecretRead(name) => secrets.push(name.clone()),
        }
    }

    // Secrets have no WASI transport of their own yet (there is no
    // `wasi:secrets` interface in the Command world); `SecretRead` is
    // accepted as declared policy and recorded, but grants nothing today.
    // A step that actually needs secret material cannot get it through
    // this runner yet -- a real gap, stated rather than hidden.
    let _ = secrets;

    if has_random_deterministic {
        builder
            .secure_random(DeterministicRng::new(deterministic_seed))
            .insecure_random(DeterministicRng::new(deterministic_seed));
    }

    if network_hosts.is_empty() {
        builder
            .allow_tcp(false)
            .allow_udp(false)
            .allow_ip_name_lookup(false)
            .socket_addr_check(|_: SocketAddr, _: SocketAddrUse| {
                Box::pin(async { false }) as Pin<Box<dyn Future<Output = bool> + Send + Sync>>
            });
    } else {
        let allowed_ips = resolve_allowed_ips(&network_hosts);
        builder
            .allow_tcp(true)
            .allow_udp(true)
            .allow_ip_name_lookup(true)
            .socket_addr_check(move |addr: SocketAddr, _use: SocketAddrUse| {
                let allowed = allowed_ips.contains(&addr.ip());
                Box::pin(async move { allowed })
                    as Pin<Box<dyn Future<Output = bool> + Send + Sync>>
            });
    }

    Ok(builder.build())
}

/// Resolve each declared `network:host("...")` capability's hostname to
/// the IP address set `socket_addr_check` will compare connections
/// against. Resolved once, at sandbox setup time -- a DNS change after
/// setup is not re-checked, a known limitation of address-based (rather
/// than TLS-SNI- or proxy-based) network capability enforcement, shared
/// with most sandboxes that gate on `SocketAddr` rather than terminating
/// the connection themselves.
fn resolve_allowed_ips(hosts: &[String]) -> Arc<HashSet<IpAddr>> {
    let mut set = HashSet::new();
    for host in hosts {
        if let Ok(addrs) = (host.as_str(), 0u16).to_socket_addrs() {
            for addr in addrs {
                set.insert(addr.ip());
            }
        }
    }
    Arc::new(set)
}
