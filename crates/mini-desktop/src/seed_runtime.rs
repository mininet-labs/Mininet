//! Typed, owner-triggered process adapter. No shell, model tool execution or key copying.
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, TryRecvError},
    Arc,
};
use std::time::{Duration, Instant};
const MAX_STDOUT: u64 = 4 * 1024 * 1024;
const MAX_STDERR: u64 = 128 * 1024;
const POLL: Duration = Duration::from_millis(100);
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Node {
    pub program: PathBuf,
    pub home: PathBuf,
    pub store: PathBuf,
}
#[derive(Debug, Clone)]
pub(crate) enum Action {
    NodeIdentity,
    NodeInitialize,
    KelExport,
    KelInspect {
        hex: String,
    },
    KelTrust {
        hex: String,
        confirmation: String,
    },
    Status,
    Inventory,
    Configure {
        max_bytes: u64,
    },
    Revoke {
        id: String,
    },
    Prune,
    ContactList,
    ContactAdd {
        name: String,
        address: String,
        peer: String,
    },
    ContactRemove {
        name: String,
    },
    Announce {
        address: String,
        ttl_ms: u64,
    },
    Unannounce,
    CatalogList,
    CatalogRefresh {
        contact: Option<String>,
    },
    CatalogPull {
        contact: Option<String>,
    },
    Search {
        query: String,
        contact: Option<String>,
        limit: u16,
        after: Option<String>,
    },
    Locate {
        id: String,
    },
    Retrieve {
        id: String,
        reseed_days: Option<u16>,
    },
    CollectionPreview {
        root: String,
        kind: String,
        days: u16,
        output: PathBuf,
    },
    CollectionApply {
        preview: PathBuf,
        confirmation: String,
    },
    CollectionFetch {
        root: String,
        kind: String,
        max_objects: u16,
        max_bytes: u64,
        max_downloads: u16,
    },
    MediaExport {
        root: String,
        output: PathBuf,
    },
    ServiceStatus,
    ServiceStop,
    ServiceRun {
        address: String,
        allowed_peers: Vec<String>,
    },
    Follow {
        days: u16,
        rounds: u32,
        interval_ms: u64,
        max_objects: u16,
        resume: bool,
    },
    FollowStop,
}
fn text(value: &str) -> Result<String, String> {
    if value.is_empty()
        || value.len() > 8192
        || value.starts_with("--")
        || value.chars().any(char::is_control)
    {
        return Err(
            "A field is empty, too long or contains command-like/control characters.".into(),
        );
    }
    Ok(value.into())
}
fn bundle_hex(value: &str) -> Result<String, String> {
    if value.is_empty()
        || value.len() > 8192
        || value.len() % 2 != 0
        || !value.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(
            "Public key bundle must be even-length hexadecimal, at most 8192 characters.".into(),
        );
    }
    Ok(value.to_owned())
}
fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("Choose a nonempty path.".into());
    }
    let full = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path)
    };
    text(full.to_str().ok_or("Paths must be valid UTF-8.")?)?;
    Ok(full)
}
fn path_value(path: &Path) -> Result<String, String> {
    absolute(path)?
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "Paths must be valid UTF-8.".into())
}
impl Action {
    fn persistent(&self) -> bool {
        matches!(self, Self::ServiceRun { .. } | Self::Follow { .. })
    }
    fn args(&self) -> Result<Vec<String>, String> {
        let mut a = Vec::new();
        let mut add = |v: &[&str]| {
            a.extend(v.iter().map(|s| s.to_string()));
        };
        match self {
            Self::NodeIdentity => add(&["identity", "show"]),
            Self::NodeInitialize => add(&["identity", "init"]),
            Self::KelExport => add(&["kel", "export"]),
            Self::KelInspect { hex } => add(&["kel", "inspect", &bundle_hex(hex)?]),
            Self::KelTrust { hex, confirmation } => add(&[
                "kel",
                "trust",
                &bundle_hex(hex)?,
                "--confirm",
                &text(confirmation)?,
            ]),
            Self::Status => add(&["seed", "status"]),
            Self::Inventory => add(&["seed", "inventory"]),
            Self::Prune => add(&["seed", "prune"]),
            Self::Configure { max_bytes } => {
                add(&["seed", "configure", "--max-bytes", &max_bytes.to_string()])
            }
            Self::Revoke { id } => add(&["seed", "revoke", "--id", &text(id)?]),
            Self::ContactList => add(&["seed", "contact", "list"]),
            Self::ContactAdd {
                name,
                address,
                peer,
            } => add(&[
                "seed",
                "contact",
                "add",
                "--name",
                &text(name)?,
                "--addr",
                &text(address)?,
                "--peer",
                &text(peer)?,
            ]),
            Self::ContactRemove { name } => {
                add(&["seed", "contact", "remove", "--name", &text(name)?])
            }
            Self::Announce { address, ttl_ms } => add(&[
                "seed",
                "announce",
                "--addr",
                &text(address)?,
                "--ttl-ms",
                &ttl_ms.to_string(),
            ]),
            Self::Unannounce => add(&["seed", "unannounce"]),
            Self::CatalogList => add(&["seed", "catalog", "list"]),
            Self::CatalogRefresh { contact } | Self::CatalogPull { contact } => {
                add(&[
                    "seed",
                    "catalog",
                    if matches!(self, Self::CatalogPull { .. }) {
                        "pull"
                    } else {
                        "refresh"
                    },
                ]);
                if let Some(c) = contact {
                    add(&["--contact", &text(c)?]);
                }
            }
            Self::Search {
                query,
                contact,
                limit,
                after,
            } => {
                add(&[
                    "seed",
                    "search",
                    "--query",
                    &text(query)?,
                    "--limit",
                    &limit.to_string(),
                ]);
                if let Some(c) = contact {
                    add(&["--contact", &text(c)?]);
                }
                if let Some(id) = after {
                    add(&["--after", &text(id)?]);
                }
            }
            Self::Locate { id } => add(&["seed", "locate", "--id", &text(id)?]),
            Self::Retrieve { id, reseed_days } => {
                add(&["seed", "retrieve", "--id", &text(id)?]);
                if let Some(days) = reseed_days {
                    add(&["--reseed-days", &days.to_string()]);
                }
            }
            Self::CollectionPreview {
                root,
                kind,
                days,
                output,
            } => add(&[
                "seed",
                "collection-preview",
                "--root",
                &text(root)?,
                "--kind",
                &text(kind)?,
                "--days",
                &days.to_string(),
                "--output",
                &path_value(output)?,
            ]),
            Self::CollectionApply {
                preview,
                confirmation,
            } => add(&[
                "seed",
                "collection-add",
                "--preview",
                &path_value(preview)?,
                "--confirm",
                &text(confirmation)?,
            ]),
            Self::CollectionFetch {
                root,
                kind,
                max_objects,
                max_bytes,
                max_downloads,
            } => add(&[
                "seed",
                "collection-fetch",
                "--root",
                &text(root)?,
                "--kind",
                &text(kind)?,
                "--max-objects",
                &max_objects.to_string(),
                "--max-bytes",
                &max_bytes.to_string(),
                "--max-downloads",
                &max_downloads.to_string(),
            ]),
            Self::MediaExport { root, output } => add(&[
                "seed",
                "media-export",
                "--root",
                &text(root)?,
                "--output",
                &path_value(output)?,
            ]),
            Self::ServiceStatus => add(&["service", "status"]),
            Self::ServiceStop => add(&["service", "stop"]),
            Self::ServiceRun {
                address,
                allowed_peers,
            } => {
                if allowed_peers.is_empty() || allowed_peers.len() > 64 {
                    return Err("Choose 1..64 allowed peer roots.".into());
                }
                add(&["seed", "serve", "--addr", &text(address)?]);
                for peer in allowed_peers {
                    add(&["--allow-peer", &text(peer)?]);
                }
            }
            Self::Follow {
                days,
                rounds,
                interval_ms,
                max_objects,
                resume,
            } => {
                add(&[
                    "seed",
                    "mesh-follow",
                    "--days",
                    &days.to_string(),
                    "--rounds",
                    &rounds.to_string(),
                    "--interval-ms",
                    &interval_ms.to_string(),
                    "--max-objects",
                    &max_objects.to_string(),
                ]);
                if *resume {
                    add(&["--resume"]);
                }
            }
            Self::FollowStop => add(&["seed", "follow-stop"]),
        }
        Ok(a)
    }
}
struct Capture {
    file: Option<File>,
    path: PathBuf,
}
impl Capture {
    fn new(label: &str) -> Result<Self, String> {
        let random = mini_crypto::random_32().map_err(|e| e.to_string())?;
        let tag: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let path =
            std::env::temp_dir().join(format!("mininet-seed-{}-{tag}-{label}", std::process::id()));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&path).map_err(|e| e.to_string())?;
        Ok(Self {
            file: Some(file),
            path,
        })
    }
    fn file(&mut self) -> Result<&mut File, String> {
        self.file
            .as_mut()
            .ok_or_else(|| "Capture was closed.".into())
    }
    fn size(&self) -> Result<u64, String> {
        self.file
            .as_ref()
            .ok_or("Capture was closed.")?
            .metadata()
            .map(|m| m.len())
            .map_err(|e| e.to_string())
    }
    fn read(&mut self, limit: u64) -> Result<Vec<u8>, String> {
        let file = self.file()?;
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > limit {
            return Err("Node output exceeded its display limit.".into());
        }
        Ok(bytes)
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = fs::remove_file(&self.path);
    }
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
/// One explicitly started operation. Dropping it requests termination of its owned child.
pub(crate) struct Job {
    receiver: Receiver<Result<Value, String>>,
    cancel: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    delivered: bool,
}
impl Job {
    pub fn start(node: Node, mut action: Action) -> Result<Self, String> {
        // Freeze relative paths before the background worker starts.
        match &mut action {
            Action::CollectionPreview { output, .. } | Action::MediaExport { output, .. } => {
                *output = absolute(output)?
            }
            Action::CollectionApply { preview, .. } => *preview = absolute(preview)?,
            _ => {}
        }
        let args = action.args()?;
        let program = node
            .program
            .canonicalize()
            .map_err(|e| format!("Choose an installed Mininet command tool: {e}"))?;
        if !program.is_file() {
            return Err("The command tool must be a regular executable file.".into());
        }
        let home = if matches!(action, Action::NodeInitialize) {
            // Only explicit initialization may create a missing node profile.
            absolute(&node.home)?
        } else {
            let home = node
                .home
                .canonicalize()
                .map_err(|e| format!("Choose an existing node folder: {e}"))?;
            if !home.is_dir() {
                return Err("The selected node folder is not a directory.".into());
            }
            home
        };
        let home = path_value(&home)?;
        let store = path_value(&node.store)?;
        let cancel = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let (tx, receiver) = mpsc::channel();
        let (worker_cancel, worker_finished) = (cancel.clone(), finished.clone());
        std::thread::Builder::new()
            .name("mininet-seed-operation".into())
            .spawn(move || {
                let result = run(program, home, store, args, action, &worker_cancel);
                let _ = tx.send(result);
                worker_finished.store(true, Ordering::Release);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            receiver,
            cancel,
            finished,
            delivered: false,
        })
    }
    pub fn poll(&mut self) -> Option<Result<Value, String>> {
        if self.delivered {
            return None;
        }
        match self.receiver.try_recv() {
            Ok(result) => {
                self.delivered = true;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.delivered = true;
                Some(Err(
                    "Node operation stopped without a result. Refresh node status before retrying."
                        .into(),
                ))
            }
        }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire) || self.delivered
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}
fn run(
    program: PathBuf,
    home: String,
    store: String,
    args: Vec<String>,
    action: Action,
    cancel: &AtomicBool,
) -> Result<Value, String> {
    if cancel.load(Ordering::Acquire) {
        return Err("Operation cancelled before start.".into());
    }
    let mut stdout = Capture::new("stdout")?;
    let mut stderr = Capture::new("stderr")?;
    let mut command = Command::new(program);
    command.arg("--home").arg(home).arg("--store").arg(store);
    command.arg("--json");
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            stdout.file()?.try_clone().map_err(|e| e.to_string())?,
        ))
        .stderr(Stdio::from(
            stderr.file()?.try_clone().map_err(|e| e.to_string())?,
        ));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = OwnedChild(
        command
            .spawn()
            .map_err(|e| format!("Could not start node tool: {e}"))?,
    );
    drop(command);
    let deadline = if action.persistent() {
        None
    } else {
        Some(Instant::now() + Duration::from_secs(150))
    };
    let status = loop {
        if cancel.load(Ordering::Acquire) {
            return Err("Operation cancelled. Completed writes are not undone; refresh node status before retrying.".into());
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            return Err(
                "Node operation exceeded its time limit. Refresh node status before retrying."
                    .into(),
            );
        }
        if stdout.size()? > MAX_STDOUT || stderr.size()? > MAX_STDERR {
            return Err("Node output exceeded its limit; operation stopped. Refresh node status before retrying.".into());
        }
        if let Some(status) = child.0.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        std::thread::sleep(POLL);
    };
    let output = stdout.read(MAX_STDOUT)?;
    let errors = stderr.read(MAX_STDERR)?;
    if !status.success() {
        if let Ok(value) = serde_json::from_slice::<Value>(&output) {
            if let Some(message) = value["message"].as_str() {
                return Err(message.chars().take(4096).collect());
            }
        }
        let error = String::from_utf8_lossy(&errors);
        return Err(format!(
            "Node tool exited with {status}: {}",
            error.chars().take(4096).collect::<String>()
        ));
    }
    let mut value: Value = serde_json::from_slice(&output)
        .map_err(|e| format!("Node tool did not return a valid result: {e}"))?;
    if value["ok"] != true {
        return Err(
            "Node tool did not confirm success. Refresh node status before retrying.".into(),
        );
    }
    if let Action::CollectionPreview { output, .. } = action {
        let meta = fs::symlink_metadata(&output).map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 2 * 1024 * 1024 {
            return Err("Preview document is not a bounded regular file.".into());
        }
        let mut bytes = Vec::new();
        File::open(output)
            .map_err(|e| e.to_string())?
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("Preview document exceeds its limit.".into());
        }
        let preview: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if preview["id"] != value["result"]["preview_id"] || preview["id"].as_str().is_none() {
            return Err(
                "Preview file differs from the reported preview. Generate a new preview.".into(),
            );
        }
        value["result"]["preview_document"] = preview;
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_arguments_reject_global_flag_injection_and_preserve_literal_queries() {
        for query in ["--home", "--store=/tmp/other", "bad\nquery", ""] {
            assert!(Action::Search {
                query: query.into(),
                contact: None,
                limit: 10,
                after: None
            }
            .args()
            .is_err());
        }
        for hex in ["--home", "ab cd", "abc", "zz", ""] {
            assert!(Action::KelInspect { hex: hex.into() }.args().is_err());
        }
        let query = "literal spaces; $(never-run)";
        let args = Action::Search {
            query: query.into(),
            contact: None,
            limit: 10,
            after: None,
        }
        .args()
        .unwrap();
        assert_eq!(args, ["seed", "search", "--query", query, "--limit", "10"]);
        assert!(Action::ServiceRun {
            address: "127.0.0.1:1".into(),
            allowed_peers: vec![]
        }
        .args()
        .is_err());
    }
    #[test]
    fn capture_bounds_reads_and_removes_only_its_own_file() {
        use std::io::Write;
        let mut capture = Capture::new("test").unwrap();
        let path = capture.path.clone();
        capture.file().unwrap().write_all(b"12345").unwrap();
        assert!(capture.read(4).is_err());
        assert_eq!(capture.read(5).unwrap(), b"12345");
        drop(capture);
        assert!(!path.exists());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn dropping_operation_guard_kills_and_reaps_owned_child() {
        let child = OwnedChild(Command::new("/bin/sleep").arg("30").spawn().unwrap());
        let pid = child.0.id();
        assert!(PathBuf::from(format!("/proc/{pid}")).exists());
        drop(child);
        assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
    }
    /// Exercises the actual installed helper without pulling mini-cli (and its
    /// separate dependency graph) into the desktop renderer.
    #[test]
    #[ignore = "requires MININET_SEED_CLI pointing to the built mini executable"]
    fn real_cli_profile_actions_round_trip() {
        struct TestHome(PathBuf);
        impl Drop for TestHome {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let program =
            PathBuf::from(std::env::var_os("MININET_SEED_CLI").expect("set MININET_SEED_CLI"))
                .canonicalize()
                .unwrap();
        let random = mini_crypto::random_32().unwrap();
        let tag: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let home = TestHome(std::env::temp_dir().join(format!("mininet-desktop-seed-test-{tag}")));
        fs::create_dir(&home.0).unwrap();
        let node = Node {
            program,
            home: home.0.join("node"),
            store: home.0.join("source"),
        };
        fn invoke(node: &Node, action: Action) -> Result<Value, String> {
            let mut job = Job::start(node.clone(), action)?;
            let deadline = Instant::now() + Duration::from_secs(15);
            let result = loop {
                if let Some(result) = job.poll() {
                    break result;
                }
                assert!(Instant::now() < deadline, "CLI action timed out");
                std::thread::sleep(Duration::from_millis(10));
            };
            assert!(job.poll().is_none(), "result must be delivered once");
            result
        }
        assert!(!node.home.exists());
        assert!(Job::start(node.clone(), Action::NodeIdentity).is_err());
        assert!(!node.home.exists(), "inspect must not create a profile");
        let initialized = invoke(&node, Action::NodeInitialize).unwrap();
        assert_eq!(initialized["result"]["created"], true);
        let identity = invoke(&node, Action::NodeIdentity).unwrap();
        assert!(
            invoke(&node, Action::NodeInitialize).is_err(),
            "must not replace keys"
        );
        assert_eq!(invoke(&node, Action::NodeIdentity).unwrap(), identity);
        let peer = Node {
            home: home.0.join("peer"),
            ..node.clone()
        };
        invoke(&peer, Action::NodeInitialize).unwrap();
        let exported = invoke(&peer, Action::KelExport).unwrap();
        let hex = exported["result"]["hex"].as_str().unwrap().to_owned();
        let inspected = invoke(&node, Action::KelInspect { hex: hex.clone() }).unwrap();
        let bundle_id = inspected["result"]["bundle_id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(exported["result"]["bundle_id"], bundle_id);
        assert!(
            !node.home.join("trusted_kels").exists(),
            "inspection must not trust keys"
        );
        assert!(invoke(
            &node,
            Action::KelTrust {
                hex: hex.clone(),
                confirmation: "0".repeat(64)
            }
        )
        .is_err());
        assert!(
            !node.home.join("trusted_kels").exists(),
            "wrong confirmation must not write"
        );
        let trusted = invoke(
            &node,
            Action::KelTrust {
                hex,
                confirmation: bundle_id,
            },
        )
        .unwrap();
        assert_eq!(trusted["result"]["trusted"], true);
        assert_eq!(
            fs::read_dir(node.home.join("trusted_kels"))
                .unwrap()
                .count(),
            2
        );
        for action in [
            Action::NodeIdentity,
            Action::Status,
            Action::Configure { max_bytes: 1024 },
            Action::Inventory,
            Action::ContactList,
            Action::ServiceStatus,
        ] {
            let result = invoke(&node, action).unwrap();
            assert_eq!(result["ok"], true);
        }
    }
}
