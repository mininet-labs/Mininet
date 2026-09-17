//! Desktop-side client for the per-user application core.
//!
//! A dedicated worker thread owns the child process and its framed stdio. The
//! egui thread only enqueues capability-shaped commands into a bounded queue;
//! it never owns service stdin/stdout and never receives private key material.

use mini_app_protocol::{read_response, write_request, Command, Reply, Request, ResponseBody};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const COMMAND_QUEUE: usize = 32;
const SHUTDOWN_WAIT: Duration = Duration::from_secs(2);

struct Work {
    command: Command,
    response: mpsc::Sender<Result<Reply, String>>,
}

pub struct Client {
    sender: SyncSender<Work>,
}

impl Client {
    pub fn spawn() -> Result<Self, String> {
        let executable = service_executable()?;
        let mut child = ProcessCommand::new(&executable)
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                format!(
                    "could not start application core {}: {error}",
                    executable.display()
                )
            })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "application core stdin was not piped".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "application core stdout was not piped".to_string())?;
        let (sender, receiver) = mpsc::sync_channel(COMMAND_QUEUE);
        std::thread::spawn(move || worker(child, stdin, stdout, receiver));
        Ok(Self { sender })
    }

    pub fn request(&self, command: Command) -> Result<Receiver<Result<Reply, String>>, String> {
        let (response, receiver) = mpsc::channel();
        match self.sender.try_send(Work { command, response }) {
            Ok(()) => Ok(receiver),
            Err(TrySendError::Full(_)) => Err(
                "application core command queue is full; retry after current work finishes"
                    .to_string(),
            ),
            Err(TrySendError::Disconnected(_)) => {
                Err("application core process is not available".to_string())
            }
        }
    }
}

fn worker(
    mut child: Child,
    mut stdin: ChildStdin,
    mut stdout: ChildStdout,
    receiver: Receiver<Work>,
) {
    let mut request_id = 1u64;
    while let Ok(work) = receiver.recv() {
        let id = request_id;
        request_id = request_id.saturating_add(1);
        let request = Request::new(id, work.command);
        let (result, fatal) = exchange(&mut stdin, &mut stdout, &request);
        let _ = work.response.send(result);
        if fatal {
            while let Ok(pending) = receiver.try_recv() {
                let _ = pending
                    .response
                    .send(Err("application core IPC stopped".to_string()));
            }
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
    }

    let shutdown = Request::new(request_id, Command::Shutdown);
    let _ = write_request(&mut stdin, &shutdown);
    let _ = read_response(&mut stdout);
    if wait_for_exit(&mut child).is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn exchange(
    stdin: &mut ChildStdin,
    stdout: &mut ChildStdout,
    request: &Request,
) -> (Result<Reply, String>, bool) {
    if let Err(error) = write_request(stdin, request) {
        return (Err(error.to_string()), true);
    }
    let response = match read_response(stdout) {
        Ok(Some(response)) => response,
        Ok(None) => {
            return (
                Err("application core closed its response stream".to_string()),
                true,
            )
        }
        Err(error) => return (Err(error.to_string()), true),
    };
    if response.request_id != request.request_id {
        return (
            Err(format!(
                "application core response id mismatch: expected {}, got {}",
                request.request_id, response.request_id
            )),
            true,
        );
    }
    match response.body {
        ResponseBody::Ok(reply) => (Ok(reply), false),
        ResponseBody::Err(error) => (Err(error.message), false),
    }
}

fn wait_for_exit(child: &mut Child) -> Result<(), ()> {
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Ok(None) if started.elapsed() < SHUTDOWN_WAIT => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => return Err(()),
        }
    }
}

fn service_executable() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("MININET_APP_SERVICE") {
        return Ok(PathBuf::from(path));
    }
    let current = std::env::current_exe()
        .map_err(|error| format!("could not locate Mininet executable: {error}"))?;
    let directory = current
        .parent()
        .ok_or_else(|| "Mininet executable has no parent directory".to_string())?;
    let name = if cfg!(windows) {
        "mininet-app-service.exe"
    } else {
        "mininet-app-service"
    };
    Ok(directory.join(name))
}

pub fn operation_id(kind: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    format!(
        "desktop:{kind}:{}:{timestamp}:{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use super::operation_id;

    #[test]
    fn operation_ids_are_retry_keys_not_user_content() {
        let first = operation_id("post");
        let second = operation_id("post");
        assert_ne!(first, second);
        assert!(first.starts_with("desktop:post:"));
        assert!(first.len() <= mini_app_protocol::MAX_OPERATION_ID_BYTES);
        assert!(first
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')));
    }
}
