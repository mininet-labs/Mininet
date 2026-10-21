//! Bounded pipelined application IPC. The renderer never receives keys.
//! Failed transports do not replay uncertain mutations automatically: callers
//! retain their operation IDs and explicitly retry against a fresh locked core.
use mini_app_protocol::{
    read_response, write_request, Command, Reply, Request, Response, ResponseBody,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Child, Command as ProcessCommand, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
const COMMAND_QUEUE: usize = 32;
const MAX_INFLIGHT: usize = 16;
const SHUTDOWN_WAIT: Duration = Duration::from_secs(2);
type ReplySender = mpsc::Sender<Result<Reply, String>>;
#[derive(Default)]
struct Budget {
    normal: AtomicUsize,
    control: AtomicUsize,
}
struct Permit {
    budget: Arc<Budget>,
    control: bool,
}
fn is_control(command: &Command) -> bool {
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
impl Permit {
    fn acquire(budget: &Arc<Budget>, command: &Command) -> Result<Self, String> {
        let control = is_control(command);
        let (counter, limit) = if control {
            (&budget.control, 4)
        } else {
            (&budget.normal, 12)
        };
        counter
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < limit).then_some(count + 1)
            })
            .map_err(|_| {
                "application core request capacity reached; request was not forwarded, retry later"
                    .to_string()
            })?;
        Ok(Self {
            budget: budget.clone(),
            control,
        })
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        let counter = if self.control {
            &self.budget.control
        } else {
            &self.budget.normal
        };
        counter.fetch_sub(1, Ordering::AcqRel);
    }
}
struct Pending {
    response: ReplySender,
    _permit: Permit,
}
struct Work {
    command: Command,
    response: ReplySender,
    permit: Permit,
}
enum Event {
    Work(Work),
    Reply(u64, Result<Response, String>),
    Wake,
}
pub struct Client {
    sender: SyncSender<Event>,
    running: Arc<AtomicBool>,
    budget: Arc<Budget>,
    supervisor: Option<JoinHandle<()>>,
}
impl Client {
    pub fn spawn() -> Result<Self, String> {
        Self::spawn_for(&service_executable()?)
    }
    fn spawn_for(executable: &Path) -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel(COMMAND_QUEUE);
        let initial = Session::spawn(executable, sender.clone(), 1)?;
        let running = Arc::new(AtomicBool::new(true));
        let worker_running = running.clone();
        let executable = executable.to_path_buf();
        let events = sender.clone();
        let supervisor =
            thread::spawn(move || worker(executable, initial, receiver, events, worker_running));
        Ok(Self {
            sender,
            running,
            budget: Arc::new(Budget::default()),
            supervisor: Some(supervisor),
        })
    }
    pub fn request(&self, command: Command) -> Result<Receiver<Result<Reply, String>>, String> {
        let permit = Permit::acquire(&self.budget, &command)?;
        let (response, receiver) = mpsc::channel();
        match self.sender.try_send(Event::Work(Work {
            command,
            response,
            permit,
        })) {
            Ok(()) => Ok(receiver),
            Err(TrySendError::Full(_)) => Err(
                "application core command queue is full; retry after current work finishes".into(),
            ),
            Err(TrySendError::Disconnected(_)) => {
                Err("application core supervisor is not available".into())
            }
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        let _ = self.sender.try_send(Event::Wake);
        if let Some(supervisor) = self.supervisor.take() {
            let _ = supervisor.join();
        }
    }
}
/// The bounded event channel is also used by reader/writer threads. A retired
/// session must never deadlock joining an I/O thread blocked on a full channel.
fn emit(sender: &SyncSender<Event>, mut event: Event, stop: &AtomicBool) {
    while !stop.load(Ordering::Acquire) {
        match sender.try_send(event) {
            Ok(()) => return,
            Err(TrySendError::Full(returned)) => {
                event = returned;
                thread::sleep(Duration::from_millis(10));
            }
            Err(TrySendError::Disconnected(_)) => return,
        }
    }
}
struct Session {
    child: Child,
    writer: Option<SyncSender<Request>>,
    reader: Option<JoinHandle<()>>,
    writer_thread: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    generation: u64,
}
impl Session {
    fn spawn(
        executable: &Path,
        events: SyncSender<Event>,
        generation: u64,
    ) -> Result<Self, String> {
        let mut child = ProcessCommand::new(executable)
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
        let Some(mut stdin) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err("application core stdin was not piped".into());
        };
        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err("application core stdout was not piped".into());
        };
        let stop = Arc::new(AtomicBool::new(false));
        let reader_stop = stop.clone();
        let reader_events = events.clone();
        let reader = thread::spawn(move || {
            while !reader_stop.load(Ordering::Acquire) {
                let result = read_response(&mut stdout)
                    .map_err(|error| error.to_string())
                    .and_then(|response| {
                        response.ok_or_else(|| "application core closed its response stream".into())
                    });
                let fatal = result.is_err();
                emit(
                    &reader_events,
                    Event::Reply(generation, result),
                    &reader_stop,
                );
                if fatal {
                    break;
                }
            }
        });
        let (writer, requests) = mpsc::sync_channel::<Request>(MAX_INFLIGHT);
        let writer_stop = stop.clone();
        let writer_thread = thread::spawn(move || {
            while let Ok(request) = requests.recv() {
                if writer_stop.load(Ordering::Acquire) {
                    break;
                }
                if let Err(error) = write_request(&mut stdin, &request) {
                    emit(
                        &events,
                        Event::Reply(generation, Err(error.to_string())),
                        &writer_stop,
                    );
                    break;
                }
            }
        });
        Ok(Self {
            child,
            writer: Some(writer),
            reader: Some(reader),
            writer_thread: Some(writer_thread),
            stop,
            generation,
        })
    }
    fn forward(&self, request: Request) -> Result<(), String> {
        self.writer
            .as_ref()
            .ok_or("application core writer closed")?
            .try_send(request)
            .map_err(|_| "application core writer unavailable or at capacity".into())
    }
    fn abort(mut self) {
        self.stop.store(true, Ordering::Release);
        self.writer.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.join();
    }
    fn shutdown(mut self) {
        let _ = self.forward(Request::new(0, Command::Shutdown));
        if wait_for_exit(&mut self.child).is_err() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        self.stop.store(true, Ordering::Release);
        self.writer.take();
        self.join();
    }
    fn join(&mut self) {
        if let Some(handle) = self.reader.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.writer_thread.take() {
            let _ = handle.join();
        }
    }
}
#[derive(Default)]
struct PendingRequests {
    next: u64,
    entries: BTreeMap<u64, Pending>,
}
impl PendingRequests {
    fn forward(
        &mut self,
        work: Work,
        mut send: impl FnMut(Request) -> Result<(), String>,
    ) -> Result<(), String> {
        if self.entries.len() >= MAX_INFLIGHT {
            let _=work.response.send(Err("application core has 16 requests in flight; request was not forwarded, retry later".into()));
            return Ok(());
        }
        let Some(id) = self.next.checked_add(1) else {
            let _ = work.response.send(Err(
                "application core request ID space exhausted; restart the desktop".into(),
            ));
            return Ok(());
        };
        self.next = id;
        let request = Request::new(id, work.command);
        if let Err(error) = request.validate() {
            let _ = work.response.send(Err(error.to_string()));
            return Ok(());
        }
        self.entries.insert(
            id,
            Pending {
                response: work.response,
                _permit: work.permit,
            },
        );
        send(request)
    }
    fn complete(&mut self, response: Response) -> Result<(), String> {
        let sender = self.entries.remove(&response.request_id).ok_or_else(|| {
            format!(
                "application core returned unknown or duplicate response ID {}",
                response.request_id
            )
        })?;
        let result = match response.body {
            ResponseBody::Ok(reply) => Ok(reply),
            ResponseBody::Err(error) => Err(error.message),
        };
        let _ = sender.response.send(result);
        Ok(())
    }
    fn fail_all(&mut self, reason: &str) {
        for (_, sender) in std::mem::take(&mut self.entries) {
            let _=sender.response.send(Err(format!("application core connection failed ({reason}); request may have completed; retry using the same operation ID")));
        }
    }
}
fn worker(
    executable: PathBuf,
    initial: Session,
    receiver: Receiver<Event>,
    events: SyncSender<Event>,
    running: Arc<AtomicBool>,
) {
    let mut generation = initial.generation;
    let mut session = Some(initial);
    let mut pending = PendingRequests::default();
    while running.load(Ordering::Acquire) {
        let Ok(event) = receiver.recv() else {
            break;
        };
        if !running.load(Ordering::Acquire) {
            if let Event::Work(work) = event {
                let _ = work.response.send(Err(
                    "application core client stopped; request was not forwarded".into(),
                ));
            }
            break;
        }
        let failure = match event {
            Event::Wake => None,
            Event::Work(work) => {
                if session.is_none() {
                    let Some(next) = generation.checked_add(1) else {
                        let _ = work
                            .response
                            .send(Err("application core session ID space exhausted".into()));
                        continue;
                    };
                    generation = next;
                    match Session::spawn(&executable, events.clone(), generation) {
                        Ok(active) => session = Some(active),
                        Err(error) => {
                            let _ = work.response.send(Err(error));
                            continue;
                        }
                    }
                }
                pending
                    .forward(work, |request| {
                        session.as_ref().expect("active session").forward(request)
                    })
                    .err()
            }
            Event::Reply(id, result) if id == generation && session.is_some() => match result {
                Ok(response) => pending.complete(response).err(),
                Err(error) => Some(error),
            },
            Event::Reply(_, _) => None, // Already-retired session; never correlate to a new child.
        };
        if let Some(reason) = failure {
            pending.fail_all(&reason);
            if let Some(active) = session.take() {
                active.abort();
            }
        }
    }
    pending.fail_all("desktop client closed");
    if let Some(active) = session {
        active.shutdown();
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
        COUNTER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| value
                .checked_add(1))
            .expect("desktop operation ID counter exhausted")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn work(command: Command, budget: &Arc<Budget>) -> (Work, Receiver<Result<Reply, String>>) {
        let permit = Permit::acquire(budget, &command).unwrap();
        let (response, receiver) = mpsc::channel();
        (
            Work {
                command,
                response,
                permit,
            },
            receiver,
        )
    }
    #[test]
    fn requests_pipeline_before_responses_and_correlate_in_reverse_order() {
        let budget = Arc::new(Budget::default());
        let mut pending = PendingRequests::default();
        let mut wire = Vec::new();
        let (first, first_reply) = work(Command::Status, &budget);
        let (second, second_reply) = work(Command::LockIdentity, &budget);
        pending
            .forward(first, |request| {
                write_request(&mut wire, &request).map_err(|e| e.to_string())
            })
            .unwrap();
        pending
            .forward(second, |request| {
                write_request(&mut wire, &request).map_err(|e| e.to_string())
            })
            .unwrap();
        let mut input = wire.as_slice();
        let a = mini_app_protocol::read_request(&mut input)
            .unwrap()
            .unwrap();
        let b = mini_app_protocol::read_request(&mut input)
            .unwrap()
            .unwrap();
        assert_eq!(a.command, Command::Status);
        assert_eq!(b.command, Command::LockIdentity);
        assert_eq!(pending.entries.len(), 2);
        pending
            .complete(Response::ok(b.request_id, Reply::Ack))
            .unwrap();
        assert_eq!(second_reply.recv().unwrap().unwrap(), Reply::Ack);
        assert!(first_reply.try_recv().is_err());
        pending
            .complete(Response::ok(a.request_id, Reply::DeliveryPeers(Vec::new())))
            .unwrap();
        assert_eq!(
            first_reply.recv().unwrap().unwrap(),
            Reply::DeliveryPeers(Vec::new())
        );
        assert!(pending
            .complete(Response::ok(a.request_id, Reply::Ack))
            .is_err());
    }
    #[test]
    fn normal_saturation_reserves_controls_and_uncertainty_releases_all_permits() {
        let budget = Arc::new(Budget::default());
        let mut pending = PendingRequests::default();
        let mut receivers = Vec::new();
        for _ in 0..12 {
            let (work, receiver) = work(Command::Status, &budget);
            receivers.push(receiver);
            pending.forward(work, |_| Ok(())).unwrap();
        }
        assert!(Permit::acquire(&budget, &Command::Status).is_err());
        assert!(Permit::acquire(&budget, &Command::SetDeliveryEnabled { enabled: true }).is_err());
        for _ in 0..4 {
            let (work, receiver) = work(Command::LockIdentity, &budget);
            receivers.push(receiver);
            pending.forward(work, |_| Ok(())).unwrap();
        }
        assert_eq!(pending.entries.len(), MAX_INFLIGHT);
        assert!(Permit::acquire(&budget, &Command::LockIdentity).is_err());
        assert!(pending.complete(Response::ok(999, Reply::Ack)).is_err());
        pending.fail_all("unknown response ID");
        for receiver in receivers {
            let error = receiver.recv().unwrap().unwrap_err();
            assert!(error.contains("may have completed"));
            assert!(error.contains("same operation ID"));
        }
        assert!(Permit::acquire(&budget, &Command::Status).is_ok());
        assert!(Permit::acquire(&budget, &Command::LockIdentity).is_ok());
    }
    #[test]
    fn allocation_exhaustion_never_wraps_or_forwards() {
        let budget = Arc::new(Budget::default());
        let mut pending = PendingRequests {
            next: u64::MAX,
            ..Default::default()
        };
        let (work, reply) = work(Command::Status, &budget);
        pending
            .forward(work, |_| panic!("exhausted allocator forwarded a request"))
            .unwrap();
        assert!(reply
            .recv()
            .unwrap()
            .unwrap_err()
            .contains("ID space exhausted"));
        assert!(pending.entries.is_empty());
    }
    #[cfg(unix)]
    #[test]
    fn real_child_accepts_control_before_first_reply_and_returns_reverse_responses() {
        let executable =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/delayed-app-service.py");
        let client = Client::spawn_for(&executable).unwrap();
        let first = client.request(Command::Status).unwrap();
        let second = client.request(Command::LockIdentity).unwrap();
        assert!(matches!(
            second
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .unwrap(),
            Reply::Status(_)
        ));
        assert!(matches!(
            first.recv_timeout(Duration::from_secs(2)).unwrap().unwrap(),
            Reply::Status(_)
        ));
        // Drop joins supervisor after graceful exit or kill/reap, including both I/O threads.
        drop(client);
    }
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
