//! Stdio host for the per-user Mininet application core.
//!
//! The desktop starts this process explicitly. It never starts network
//! sessions, crawling, relays, wallet work, Forge work, or updates.

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
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());

    loop {
        let request = read_request(&mut reader)
            .map_err(|error| format!("application IPC request failed: {error}"))?
            .ok_or_else(|| "desktop closed the application IPC stream".to_string())?;
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
