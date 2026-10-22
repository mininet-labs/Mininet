//! Stdio host for the per-user Mininet application core.
//!
//! The desktop starts this process explicitly. It never starts network
//! sessions without an owner networking grant and unlocked identity. It never
//! starts crawling, relays, wallet work, Forge work, or updates.

#![cfg_attr(windows, windows_subsystem = "windows")]
#![forbid(unsafe_code)]

use mini_app_protocol::{read_request, write_response, Command, ErrorCode, Response, ServiceError};
use mini_app_service::Core;
use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;

fn data_root() -> PathBuf {
    let home = std::env::var_os("MININET_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .map(|root| root.join("Mininet"))
        })
        .unwrap_or_else(|| PathBuf::from("Mininet"));
    home.join("objects")
}

fn run_stdio() -> Result<(), String> {
    let mut core = Core::open(data_root()).map_err(|error| error.message)?;
    // Input decoding has one bounded handoff slot. The authority thread owns
    // identity and storage and performs bounded idle scheduling; no key is
    // copied into a renderer or independent networking worker.
    let (requests, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin.lock());
        loop {
            let next = read_request(&mut reader);
            let finished = !matches!(&next, Ok(Some(_)));
            if requests.send(next).is_err() || finished {
                break;
            }
        }
    });
    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());
    let interval = std::time::Duration::from_millis(250);
    let mut next_poll = std::time::Instant::now() + interval;
    let mut network_error_reported = false;
    loop {
        let wait = next_poll.saturating_duration_since(std::time::Instant::now());
        match receiver.recv_timeout(wait) {
            Ok(Ok(Some(request))) => {
                let shutdown = matches!(request.command, Command::Shutdown);
                let response = match core.handle(request.command) {
                    Ok(reply) => Response::ok(request.request_id, reply),
                    Err(error) => Response::error(request.request_id, error),
                };
                write_response(&mut writer, &response)
                    .map_err(|error| format!("application IPC response failed: {error}"))?;
                if shutdown {
                    return Ok(());
                }
            }
            Ok(Ok(None)) => return Ok(()),
            Ok(Err(error)) => return Err(format!("application IPC request failed: {error}")),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => (),
        }
        if std::time::Instant::now() >= next_poll {
            match core.poll_delivery() {
                Ok(()) => network_error_reported = false,
                Err(_) if !network_error_reported => {
                    // No endpoints, post bodies, keys or raw peer errors in logs.
                    eprintln!(
                        "public delivery is temporarily unavailable; inspect delivery status"
                    );
                    network_error_reported = true;
                }
                Err(_) => (),
            }
            next_poll = std::time::Instant::now() + interval;
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next();
    if mode.as_deref() != Some("--stdio") || args.next().is_some() {
        eprintln!("usage: mininet-app-service --stdio");
        std::process::exit(2);
    }
    if let Err(error) = run_stdio() {
        let fallback = ServiceError::new(ErrorCode::Internal, error);
        eprintln!("mininet application service stopped: {}", fallback.message);
        std::process::exit(1);
    }
}
