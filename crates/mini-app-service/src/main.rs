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

fn interrupts_delivery(command: &Command) -> bool {
    matches!(
        command,
        Command::LockIdentity
            | Command::Shutdown
            | Command::SetDeliveryEnabled { enabled: false }
            | Command::AuthorizeDeliveryPeer { .. }
            | Command::RevokeDeliveryPeer { .. }
            | Command::CancelDelivery { .. }
            | Command::SetDeliveryListener { .. }
            | Command::StopDeliveryListener
    )
}

type Input = Result<Option<mini_app_protocol::Request>, mini_app_protocol::ProtocolError>;

fn forward_input(
    sender: &std::sync::mpsc::SyncSender<(Input, Option<mini_app_service::DeliveryStopGuard>)>,
    input: Input,
    stop: Option<mini_app_service::DeliveryStopGuard>,
    interrupt: &mini_app_service::DeliveryInterrupt,
) -> bool {
    match sender.try_send((input, stop)) {
        Ok(()) => true,
        Err(std::sync::mpsc::TrySendError::Disconnected(_)) => false,
        Err(std::sync::mpsc::TrySendError::Full((input, stop))) => {
            // Backpressure must not trap a later stop frame behind an idle
            // network exchange. Pause delivery until this bounded handoff is
            // processed, without creating an unbounded request queue.
            let stop = stop.or_else(|| Some(interrupt.stop()));
            sender.send((input, stop)).is_ok()
        }
    }
}

fn run_stdio() -> Result<(), String> {
    let mut core = Core::open(data_root()).map_err(|error| error.message)?;
    // Input decoding has one bounded handoff slot. The authority thread owns
    // identity and storage and performs bounded idle scheduling; no key is
    // copied into a renderer or independent networking worker.
    let interrupt = core.delivery_interrupt_handle();
    let (requests, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin.lock());
        loop {
            let next = read_request(&mut reader);
            let finished = !matches!(&next, Ok(Some(_)));
            let stop = if finished
                || matches!(&next, Ok(Some(request)) if interrupts_delivery(&request.command))
            {
                Some(interrupt.stop())
            } else {
                None
            };
            if !forward_input(&requests, next, stop, &interrupt) || finished {
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
            Ok((Ok(Some(request)), stop)) => {
                let shutdown = matches!(request.command, Command::Shutdown);
                let response = match core.handle(request.command) {
                    Ok(reply) => Response::ok(request.request_id, reply),
                    Err(error) => Response::error(request.request_id, error),
                };
                drop(stop);
                write_response(&mut writer, &response)
                    .map_err(|error| format!("application IPC response failed: {error}"))?;
                if shutdown {
                    return Ok(());
                }
            }
            Ok((Ok(None), _)) => return Ok(()),
            Ok((Err(error), _)) => return Err(format!("application IPC request failed: {error}")),
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_control_handoff_stops_delivery_until_backpressured_work_is_processed() {
        let interrupt = mini_app_service::DeliveryInterrupt::default();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        assert!(forward_input(
            &sender,
            Ok(Some(mini_app_protocol::Request::new(1, Command::Status))),
            None,
            &interrupt
        ));
        let child_interrupt = interrupt.clone();
        let worker = std::thread::spawn(move || {
            forward_input(
                &sender,
                Ok(Some(mini_app_protocol::Request::new(2, Command::Status))),
                None,
                &child_interrupt,
            )
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while !interrupt.is_stopped() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(interrupt.is_stopped());
        let (first, first_stop) = receiver.recv().unwrap();
        assert!(matches!(first, Ok(Some(request)) if request.request_id == 1));
        assert!(first_stop.is_none());
        let (second, stop) = receiver.recv().unwrap();
        assert!(matches!(second, Ok(Some(request)) if request.request_id == 2));
        assert!(worker.join().unwrap());
        assert!(interrupt.is_stopped());
        drop(stop);
        assert!(!interrupt.is_stopped());
    }

    #[test]
    fn authority_reducing_controls_interrupt_delivery_but_reads_do_not() {
        for command in [
            Command::LockIdentity,
            Command::Shutdown,
            Command::SetDeliveryEnabled { enabled: false },
            Command::CancelDelivery {
                operation_id: "send:1".into(),
            },
            Command::RevokeDeliveryPeer {
                peer_id: "peer".into(),
            },
            Command::StopDeliveryListener,
        ] {
            assert!(interrupts_delivery(&command));
        }
        for command in [
            Command::Status,
            Command::DeliveryRuntimeStatus,
            Command::SetDeliveryEnabled { enabled: true },
            Command::ListDeliveries { limit: 64 },
        ] {
            assert!(!interrupts_delivery(&command));
        }
    }
}
