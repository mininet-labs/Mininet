//! Durable exact-public-object delivery journal. The single owning service
//! serializes callers; this module performs no network, signing or KEL trust.
use mini_app_protocol::{validate_operation_id, ErrorCode, ServiceError, MAX_OPERATION_ID_BYTES};
use mini_objects::{Object, ObjectId, Payload};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const MAGIC: &[u8; 8] = b"MINIOUT1";
const MAX_OBJECT: usize = 128 * 1024;
const MAX_OPERATIONS: usize = 1024;
const MAX_RECORD: usize = MAX_OBJECT + 1024;
const MAX_ID: usize = 128;
const MAX_BACKOFF_MS: u64 = 300_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeliveryState {
    Queued,
    InFlight,
    RetryWaiting,
    Delivered,
    Cancelled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeliveryFailure {
    Unavailable,
    Rejected,
    Interrupted,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeliveryRecord {
    pub operation_id: String,
    pub peer_id: String,
    pub object_id: String,
    pub object_bytes: Vec<u8>,
    pub state: DeliveryState,
    pub attempts: u32,
    pub next_attempt_ms: u64,
    pub last_failure: Option<DeliveryFailure>,
}
#[derive(Debug)]
pub(crate) struct Outbox {
    root: PathBuf,
    // Only bounded scheduling metadata; exact payloads remain on disk. None
    // poisons scheduling after an ambiguous durable-write failure until reopen.
    due: RefCell<Option<BTreeMap<String, (DeliveryState, u64)>>>,
}
fn bad(message: impl Into<String>) -> ServiceError {
    ServiceError::new(ErrorCode::BadRequest, message)
}
fn storage(message: impl Into<String>) -> ServiceError {
    ServiceError::new(ErrorCode::Storage, message)
}
fn io(error: std::io::Error) -> ServiceError {
    ServiceError::new(ErrorCode::Io, error.to_string())
}
fn valid_id(id: &str) -> Result<(), ServiceError> {
    validate_operation_id(id).map_err(|e| bad(format!("invalid delivery identifier: {e}")))
}
fn filename(id: &str) -> String {
    let mut name = String::with_capacity(id.len() * 2 + 4);
    use std::fmt::Write;
    for byte in id.bytes() {
        let _ = write!(name, "{byte:02x}");
    }
    name.push_str(".out");
    name
}
fn record_name(name: &str) -> bool {
    name.strip_suffix(".out").is_some_and(|hex| {
        !hex.is_empty()
            && hex.len() <= MAX_OPERATION_ID_BYTES * 2
            && hex.len() % 2 == 0
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn orphan_name(name: &str) -> bool {
    let Some((base, tail)) = name.split_once(".out.") else {
        return false;
    };
    if !record_name(&format!("{base}.out")) {
        return false;
    }
    let Some(tail) = tail.strip_suffix(".tmp") else {
        return false;
    };
    let Some((pid, sequence)) = tail.split_once('-') else {
        return false;
    };
    !pid.is_empty()
        && !sequence.is_empty()
        && pid.bytes().all(|b| b.is_ascii_digit())
        && sequence.bytes().all(|b| b.is_ascii_digit())
}
fn directory(root: &Path) -> Result<PathBuf, ServiceError> {
    let absolute = if root.is_absolute() {
        root.to_owned()
    } else {
        std::env::current_dir().map_err(io)?.join(root)
    };
    let mut checked = PathBuf::new();
    for component in absolute.components() {
        if component == Component::ParentDir {
            return Err(bad("outbox directory cannot contain parent traversal"));
        }
        checked.push(component);
        match fs::symlink_metadata(&checked) {
            Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
                return Err(storage(
                    "outbox directory components must be real directories",
                ))
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(io(e)),
        }
    }
    mini_durable::create_dir_all(&absolute).map_err(io)?;
    fs::canonicalize(absolute).map_err(io)
}
fn validate(record: &DeliveryRecord) -> Result<(), ServiceError> {
    valid_id(&record.operation_id)?;
    valid_id(&record.peer_id)?;
    if record.object_bytes.is_empty()
        || record.object_bytes.len() > MAX_OBJECT
        || record.object_id.len() > MAX_ID
    {
        return Err(bad("outbound object exceeds bounds"));
    }
    let id = ObjectId::parse(&record.object_id).map_err(|e| bad(e.to_string()))?;
    let object = Object::from_bytes(&record.object_bytes).map_err(|e| bad(e.to_string()))?;
    if object.id() != &id
        || object.to_bytes() != record.object_bytes
        || !matches!(object.payload, Payload::Public(_))
        || object.signatures().is_empty()
    {
        return Err(bad(
            "outbox requires canonical signed public bytes matching the exact object ID",
        ));
    }
    let state_valid = match record.state {
        DeliveryState::Queued => {
            record.attempts == 0 && record.next_attempt_ms == 0 && record.last_failure.is_none()
        }
        DeliveryState::InFlight | DeliveryState::Delivered => {
            record.attempts > 0 && record.next_attempt_ms == 0 && record.last_failure.is_none()
        }
        DeliveryState::RetryWaiting => record.attempts > 0 && record.last_failure.is_some(),
        DeliveryState::Cancelled => record.next_attempt_ms == 0 && record.last_failure.is_none(),
    };
    if !state_valid {
        return Err(storage("invalid persisted delivery state/counters"));
    }
    Ok(())
}
impl Outbox {
    pub fn open(root: PathBuf) -> Result<Self, ServiceError> {
        let outbox = Self {
            root: directory(&root)?,
            due: RefCell::new(Some(BTreeMap::new())),
        };
        let (records, orphans) = outbox.scan()?;
        for record in &records {
            outbox.due.borrow_mut().as_mut().unwrap().insert(
                record.operation_id.clone(),
                (record.state, record.next_attempt_ms),
            );
        }
        // Validate the whole journal before recovery mutates any record. Atomic
        // temporary files never count as committed operations or receipts.
        for record in records {
            if record.state == DeliveryState::InFlight {
                let mut recovered = record;
                recovered.state = DeliveryState::RetryWaiting;
                recovered.last_failure = Some(DeliveryFailure::Interrupted);
                recovered.next_attempt_ms = 0;
                outbox.write(&recovered)?;
            }
        }
        for path in orphans {
            mini_durable::remove_file(&path).map_err(io)?;
        }
        Ok(outbox)
    }
    fn path(&self, id: &str) -> PathBuf {
        self.root.join(filename(id))
    }
    fn read(&self, path: &Path) -> Result<DeliveryRecord, ServiceError> {
        let metadata = fs::symlink_metadata(path).map_err(io)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_RECORD as u64
        {
            return Err(storage("outbox record is not a bounded regular file"));
        }
        let file = File::open(path).map_err(io)?;
        if !file.metadata().map_err(io)?.is_file() {
            return Err(storage("outbox record changed during open"));
        }
        let mut bytes = Vec::new();
        file.take(MAX_RECORD as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(io)?;
        if bytes.len() > MAX_RECORD {
            return Err(storage("outbox record exceeds byte bound"));
        }
        let record = decode(&bytes)?;
        validate(&record).map_err(|e| storage(format!("invalid outbox record: {}", e.message)))?;
        if path.file_name().and_then(|s| s.to_str())
            != Some(filename(&record.operation_id).as_str())
        {
            return Err(storage("outbox filename differs from stored operation ID"));
        }
        Ok(record)
    }
    fn scan(&self) -> Result<(Vec<DeliveryRecord>, Vec<PathBuf>), ServiceError> {
        let mut records = Vec::new();
        let mut orphans = Vec::new();
        for (index, entry) in fs::read_dir(&self.root).map_err(io)?.enumerate() {
            if index >= MAX_OPERATIONS * 2 {
                return Err(storage("outbox directory exceeds entry limit"));
            }
            let entry = entry.map_err(io)?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| storage("outbox filename is not UTF-8"))?;
            let path = entry.path();
            if record_name(name) {
                if records.len() == MAX_OPERATIONS {
                    return Err(storage("outbox exceeds retained operation limit"));
                }
                records.push(self.read(&path)?);
            } else if orphan_name(name) {
                let metadata = fs::symlink_metadata(&path).map_err(io)?;
                if !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || metadata.len() > MAX_RECORD as u64
                {
                    return Err(storage("unsafe orphan outbox temporary file"));
                }
                orphans.push(path);
            } else {
                return Err(storage("unexpected outbox directory entry"));
            }
        }
        records.sort_by(|a, b| a.operation_id.cmp(&b.operation_id));
        Ok((records, orphans))
    }
    fn retained_count(&self) -> Result<usize, ServiceError> {
        let mut count = 0;
        for (index, entry) in fs::read_dir(&self.root).map_err(io)?.enumerate() {
            if index >= MAX_OPERATIONS * 2 {
                return Err(storage("outbox directory exceeds entry limit"));
            }
            let entry = entry.map_err(io)?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| storage("outbox filename is not UTF-8"))?;
            if !record_name(name) && !orphan_name(name) {
                return Err(storage("unexpected outbox directory entry"));
            }
            let metadata = fs::symlink_metadata(entry.path()).map_err(io)?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() > MAX_RECORD as u64
            {
                return Err(storage("unsafe outbox entry"));
            }
            if record_name(name) {
                count += 1;
            }
            if count > MAX_OPERATIONS {
                return Err(storage("outbox exceeds retained operation limit"));
            }
        }
        Ok(count)
    }
    fn write(&self, record: &DeliveryRecord) -> Result<(), ServiceError> {
        validate(record)?;
        let path = self.path(&record.operation_id);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
                return Err(storage("outbox destination is not a regular file"))
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(io(e)),
        }
        let mut cache = self
            .due
            .borrow_mut()
            .take()
            .ok_or_else(|| storage("outbox requires reopen after a failed durable write"))?;
        // An error may follow successful rename but failed parent fsync. Never
        // keep using scheduling metadata that might disagree with durable state.
        mini_durable::atomic_replace(&path, &encode(record)).map_err(io)?;
        cache.insert(
            record.operation_id.clone(),
            (record.state, record.next_attempt_ms),
        );
        *self.due.borrow_mut() = Some(cache);
        Ok(())
    }
    pub fn next_due(&self, now_ms: u64) -> Result<Option<String>, ServiceError> {
        let cache = self.due.borrow();
        let cache = cache
            .as_ref()
            .ok_or_else(|| storage("outbox requires reopen after a failed durable write"))?;
        Ok(cache
            .iter()
            .filter(|(_, (state, at))| {
                matches!(state, DeliveryState::Queued | DeliveryState::RetryWaiting)
                    && *at <= now_ms
            })
            .min_by_key(|(id, (_, at))| (*at, *id))
            .map(|(id, _)| id.clone()))
    }
    pub fn enqueue(
        &self,
        operation_id: &str,
        peer_id: &str,
        object_id: &str,
        object_bytes: &[u8],
    ) -> Result<DeliveryRecord, ServiceError> {
        valid_id(operation_id)?;
        valid_id(peer_id)?;
        if object_bytes.is_empty() || object_bytes.len() > MAX_OBJECT || object_id.len() > MAX_ID {
            return Err(bad("outbound object exceeds bounds"));
        }
        let record = DeliveryRecord {
            operation_id: operation_id.into(),
            peer_id: peer_id.into(),
            object_id: object_id.into(),
            object_bytes: object_bytes.into(),
            state: DeliveryState::Queued,
            attempts: 0,
            next_attempt_ms: 0,
            last_failure: None,
        };
        validate(&record)?;
        if let Some(existing) = self.get(operation_id)? {
            if existing.peer_id == record.peer_id
                && existing.object_id == record.object_id
                && existing.object_bytes == record.object_bytes
            {
                return Ok(existing);
            }
            return Err(bad(
                "delivery operation ID already binds different peer or bytes",
            ));
        }
        if self.retained_count()? >= MAX_OPERATIONS {
            return Err(bad("outbox retained operation capacity reached"));
        }
        self.write(&record)?;
        Ok(record)
    }
    pub fn get(&self, id: &str) -> Result<Option<DeliveryRecord>, ServiceError> {
        valid_id(id)?;
        let path = self.path(id);
        match fs::symlink_metadata(&path) {
            Ok(_) => self.read(&path).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(e)),
        }
    }
    pub fn list(&self, limit: usize) -> Result<Vec<DeliveryRecord>, ServiceError> {
        self.list_after(None, limit)
    }
    pub fn list_after(
        &self,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<DeliveryRecord>, ServiceError> {
        if limit == 0 || limit > MAX_OPERATIONS {
            return Err(bad("outbox list limit must be 1..1024"));
        }
        if let Some(after) = after {
            valid_id(after)?;
        }
        let keys: Vec<String> = self
            .due
            .borrow()
            .as_ref()
            .ok_or_else(|| storage("outbox state uncertain; reopen required"))?
            .keys()
            .filter(|id| after.is_none_or(|after| id.as_str() > after))
            .take(limit)
            .cloned()
            .collect();
        keys.iter().map(|id| self.require(id)).collect()
    }
    fn require(&self, id: &str) -> Result<DeliveryRecord, ServiceError> {
        self.get(id)?
            .ok_or_else(|| bad("unknown delivery operation"))
    }
    pub fn begin_attempt(&self, id: &str, now_ms: u64) -> Result<DeliveryRecord, ServiceError> {
        let mut record = self.require(id)?;
        if !matches!(
            record.state,
            DeliveryState::Queued | DeliveryState::RetryWaiting
        ) || now_ms < record.next_attempt_ms
        {
            return Err(bad(
                "delivery is terminal, already in flight, or not yet due",
            ));
        }
        record.state = DeliveryState::InFlight;
        record.attempts = record.attempts.saturating_add(1);
        record.next_attempt_ms = 0;
        record.last_failure = None;
        self.write(&record)?;
        Ok(record)
    }
    pub fn retry(
        &self,
        id: &str,
        now_ms: u64,
        failure: DeliveryFailure,
    ) -> Result<DeliveryRecord, ServiceError> {
        let mut record = self.require(id)?;
        if record.state != DeliveryState::InFlight {
            return Err(bad("only an in-flight delivery may record a retry"));
        }
        let delay = 1000u64
            .saturating_mul(1u64 << record.attempts.saturating_sub(1).min(9))
            .min(MAX_BACKOFF_MS);
        record.state = DeliveryState::RetryWaiting;
        record.last_failure = Some(failure);
        record.next_attempt_ms = now_ms.saturating_add(delay);
        self.write(&record)?;
        Ok(record)
    }
    pub fn acknowledge(&self, id: &str) -> Result<DeliveryRecord, ServiceError> {
        let mut record = self.require(id)?;
        if record.state == DeliveryState::Delivered {
            return Ok(record);
        }
        if record.state != DeliveryState::InFlight {
            return Err(bad("only an in-flight delivery may be acknowledged"));
        }
        record.state = DeliveryState::Delivered;
        record.next_attempt_ms = 0;
        record.last_failure = None;
        self.write(&record)?;
        Ok(record)
    }
    pub fn cancel(&self, id: &str) -> Result<DeliveryRecord, ServiceError> {
        let mut record = self.require(id)?;
        if record.state == DeliveryState::Cancelled {
            return Ok(record);
        }
        if record.state == DeliveryState::Delivered {
            return Err(bad("delivered receipts cannot be cancelled"));
        }
        record.state = DeliveryState::Cancelled;
        record.next_attempt_ms = 0;
        record.last_failure = None;
        self.write(&record)?;
        Ok(record)
    }
}
fn text(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
}
fn encode(record: &DeliveryRecord) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    text(&mut out, &record.operation_id);
    text(&mut out, &record.peer_id);
    text(&mut out, &record.object_id);
    out.push(match record.state {
        DeliveryState::Queued => 0,
        DeliveryState::InFlight => 1,
        DeliveryState::RetryWaiting => 2,
        DeliveryState::Delivered => 3,
        DeliveryState::Cancelled => 4,
    });
    out.extend_from_slice(&record.attempts.to_be_bytes());
    out.extend_from_slice(&record.next_attempt_ms.to_be_bytes());
    out.push(match record.last_failure {
        None => 0,
        Some(DeliveryFailure::Unavailable) => 1,
        Some(DeliveryFailure::Rejected) => 2,
        Some(DeliveryFailure::Interrupted) => 3,
    });
    out.extend_from_slice(&(record.object_bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(&record.object_bytes);
    out
}
struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], ServiceError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| storage("outbox length overflow"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| storage("truncated outbox record"))?;
        self.offset = end;
        Ok(bytes)
    }
    fn number(&mut self) -> Result<u32, ServiceError> {
        Ok(u32::from_be_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| storage("invalid outbox integer"))?,
        ))
    }
    fn string(&mut self, max: usize) -> Result<String, ServiceError> {
        let length = self.number()? as usize;
        if length > max {
            return Err(storage("outbox string exceeds bound"));
        }
        std::str::from_utf8(self.take(length)?)
            .map(str::to_owned)
            .map_err(|_| storage("outbox string is not UTF-8"))
    }
}
fn decode(bytes: &[u8]) -> Result<DeliveryRecord, ServiceError> {
    if bytes.len() > MAX_RECORD {
        return Err(storage("outbox record exceeds bound"));
    }
    let mut reader = Reader { bytes, offset: 0 };
    if reader.take(MAGIC.len())? != MAGIC {
        return Err(storage("unsupported outbox record version"));
    }
    let operation_id = reader.string(MAX_OPERATION_ID_BYTES)?;
    let peer_id = reader.string(MAX_OPERATION_ID_BYTES)?;
    let object_id = reader.string(MAX_ID)?;
    let state = match reader.take(1)?[0] {
        0 => DeliveryState::Queued,
        1 => DeliveryState::InFlight,
        2 => DeliveryState::RetryWaiting,
        3 => DeliveryState::Delivered,
        4 => DeliveryState::Cancelled,
        _ => return Err(storage("invalid outbox state")),
    };
    let attempts = reader.number()?;
    let next_attempt_ms = u64::from_be_bytes(
        reader
            .take(8)?
            .try_into()
            .map_err(|_| storage("invalid outbox timestamp"))?,
    );
    let last_failure = match reader.take(1)?[0] {
        0 => None,
        1 => Some(DeliveryFailure::Unavailable),
        2 => Some(DeliveryFailure::Rejected),
        3 => Some(DeliveryFailure::Interrupted),
        _ => return Err(storage("invalid outbox failure")),
    };
    let length = reader.number()? as usize;
    if length > MAX_OBJECT {
        return Err(storage("outbox object exceeds bound"));
    }
    let object_bytes = reader.take(length)?.to_vec();
    if reader.offset != bytes.len() {
        return Err(storage("trailing outbox bytes"));
    }
    Ok(DeliveryRecord {
        operation_id,
        peer_id,
        object_id,
        object_bytes,
        state,
        attempts,
        next_attempt_ms,
        last_failure,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::{Capabilities, Controller};
    use mini_objects::{ObjectBuilder, ObjectType};
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let root = fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!(
                    "mini-outbox-{}-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos(),
                    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn object(encrypted: bool) -> Object {
        let mut root = Controller::incept_single_from_seeds(&[11; 32], &[12; 32]).unwrap();
        let device =
            Controller::incept_device_single_from_seeds(&root.did(), &[13; 32], &[14; 32]).unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        ObjectBuilder::new(ObjectType::POST)
            .sequence(1)
            .payload(if encrypted {
                Payload::Encrypted(b"private".to_vec())
            } else {
                Payload::Public(b"exact signed public bytes".to_vec())
            })
            .sign(&root.did(), &device)
            .unwrap()
    }
    #[test]
    fn bounded_pages_reach_operations_after_terminal_receipts() {
        let directory = Directory::new();
        let outbox = Outbox::open(directory.0.clone()).unwrap();
        let object = object(false);
        let bytes = object.to_bytes();
        let id = object.id().as_str();
        for number in 0..65 {
            let operation_id = format!("operation:{number:03}");
            outbox.enqueue(&operation_id, "peer", id, &bytes).unwrap();
            if number < 64 {
                outbox.cancel(&operation_id).unwrap();
            }
        }
        let first = outbox.list(64).unwrap();
        assert_eq!(first.len(), 64);
        let second = outbox
            .list_after(Some(&first[63].operation_id), 64)
            .unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].operation_id, "operation:064");
        assert!(outbox
            .list_after(Some("operation:064"), 64)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn restart_recovers_attempt_without_resigning_and_receipts_are_idempotent() {
        let dir = Directory::new();
        let outbox = Outbox::open(dir.0.clone()).unwrap();
        let object = object(false);
        let bytes = object.to_bytes();
        let queued = outbox
            .enqueue("post:1", "peer:1", object.id().as_str(), &bytes)
            .unwrap();
        assert_eq!(queued.state, DeliveryState::Queued);
        assert_eq!(outbox.next_due(0).unwrap().as_deref(), Some("post:1"));
        assert_eq!(
            outbox
                .enqueue("post:1", "peer:1", object.id().as_str(), &bytes)
                .unwrap(),
            queued
        );
        assert!(outbox
            .enqueue("post:1", "peer:2", object.id().as_str(), &bytes)
            .is_err());
        let attempting = outbox.begin_attempt("post:1", 10).unwrap();
        assert_eq!(attempting.attempts, 1);
        assert_eq!(outbox.next_due(10).unwrap(), None);
        assert_eq!(outbox.get("post:1").unwrap().unwrap(), attempting);
        drop(outbox);
        let outbox = Outbox::open(dir.0.clone()).unwrap();
        let recovered = outbox.get("post:1").unwrap().unwrap();
        assert_eq!(recovered.object_bytes, bytes);
        assert_eq!(recovered.attempts, 1);
        assert_eq!(recovered.state, DeliveryState::RetryWaiting);
        assert_eq!(recovered.last_failure, Some(DeliveryFailure::Interrupted));
        assert_eq!(recovered.next_attempt_ms, 0);
        assert_eq!(outbox.next_due(0).unwrap().as_deref(), Some("post:1"));
        assert_eq!(outbox.begin_attempt("post:1", 0).unwrap().attempts, 2);
        let delivered = outbox.acknowledge("post:1").unwrap();
        assert_eq!(outbox.acknowledge("post:1").unwrap(), delivered);
        assert_eq!(outbox.next_due(u64::MAX).unwrap(), None);
        assert!(outbox.cancel("post:1").is_err());
        assert!(outbox.begin_attempt("post:1", 0).is_err());
        assert_eq!(
            outbox
                .enqueue("post:1", "peer:1", object.id().as_str(), &bytes)
                .unwrap(),
            delivered
        );
        drop(outbox);
        let outbox = Outbox::open(dir.0.clone()).unwrap();
        assert_eq!(outbox.get("post:1").unwrap().unwrap(), delivered);
    }
    #[test]
    fn earliest_due_operation_prevents_lexical_retry_starvation() {
        let dir = Directory::new();
        let outbox = Outbox::open(dir.0.clone()).unwrap();
        let object = object(false);
        let bytes = object.to_bytes();
        for id in ["a", "z", "y"] {
            outbox
                .enqueue(id, "peer", object.id().as_str(), &bytes)
                .unwrap();
        }
        assert_eq!(outbox.next_due(0).unwrap().as_deref(), Some("a"));
        outbox.begin_attempt("a", 0).unwrap();
        outbox.retry("a", 0, DeliveryFailure::Unavailable).unwrap();
        // Even when the early lexical retry becomes due, older queued work wins.
        assert_eq!(outbox.next_due(1000).unwrap().as_deref(), Some("y"));
        for id in ["y", "z"] {
            outbox.begin_attempt(id, 1000).unwrap();
            outbox
                .retry(id, 1000, DeliveryFailure::Unavailable)
                .unwrap();
        }
        assert_eq!(outbox.next_due(2000).unwrap().as_deref(), Some("a"));
        outbox.begin_attempt("a", 2000).unwrap();
        outbox.cancel("a").unwrap();
        assert_eq!(outbox.next_due(2000).unwrap().as_deref(), Some("y"));
    }
    #[test]
    fn retry_backoff_and_terminal_transitions_are_bounded_and_saturating() {
        let dir = Directory::new();
        let outbox = Outbox::open(dir.0.clone()).unwrap();
        let object = object(false);
        let bytes = object.to_bytes();
        outbox
            .enqueue("b", "peer", object.id().as_str(), &bytes)
            .unwrap();
        assert!(outbox.acknowledge("b").is_err());
        assert!(outbox.retry("b", 0, DeliveryFailure::Unavailable).is_err());
        outbox.begin_attempt("b", 0).unwrap();
        let retry = outbox
            .retry("b", 100, DeliveryFailure::Unavailable)
            .unwrap();
        assert_eq!(retry.next_attempt_ms, 1100);
        assert!(outbox.begin_attempt("b", 1099).is_err());
        outbox.begin_attempt("b", 1100).unwrap();
        assert_eq!(
            outbox
                .retry("b", 1100, DeliveryFailure::Rejected)
                .unwrap()
                .next_attempt_ms,
            3100
        );
        let mut saturated = outbox.begin_attempt("b", 3100).unwrap();
        saturated.attempts = u32::MAX;
        outbox.write(&saturated).unwrap();
        assert_eq!(
            outbox
                .retry("b", 1, DeliveryFailure::Unavailable)
                .unwrap()
                .next_attempt_ms,
            1 + MAX_BACKOFF_MS
        );
        assert_eq!(
            outbox.begin_attempt("b", u64::MAX).unwrap().attempts,
            u32::MAX
        );
        assert_eq!(
            outbox
                .retry("b", u64::MAX - 1, DeliveryFailure::Interrupted)
                .unwrap()
                .next_attempt_ms,
            u64::MAX
        );
        let cancelled = outbox.cancel("b").unwrap();
        assert_eq!(outbox.cancel("b").unwrap(), cancelled);
        assert!(outbox.acknowledge("b").is_err());
        outbox
            .enqueue("a", "peer", object.id().as_str(), &bytes)
            .unwrap();
        assert_eq!(
            outbox
                .list(2)
                .unwrap()
                .iter()
                .map(|r| r.operation_id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert!(outbox.list(0).is_err() && outbox.list(MAX_OPERATIONS + 1).is_err());
    }
    #[test]
    fn invalid_and_oversized_records_fail_before_recovery_changes_state() {
        let dir = Directory::new();
        let outbox = Outbox::open(dir.0.clone()).unwrap();
        let object = object(false);
        let bytes = object.to_bytes();
        assert!(outbox
            .enqueue("../escape", "peer", object.id().as_str(), &bytes)
            .is_err());
        assert!(outbox
            .enqueue(
                "private",
                "peer",
                object.id().as_str(),
                &self::object(true).to_bytes()
            )
            .is_err());
        assert!(outbox
            .enqueue(
                "large",
                "peer",
                object.id().as_str(),
                &vec![0; MAX_OBJECT + 1]
            )
            .is_err());
        outbox
            .enqueue("good", "peer", object.id().as_str(), &bytes)
            .unwrap();
        outbox.begin_attempt("good", 0).unwrap();
        let before = fs::read(outbox.path("good")).unwrap();
        let malformed = outbox.path("bad");
        fs::write(&malformed, b"malformed").unwrap();
        assert!(Outbox::open(dir.0.clone()).is_err());
        assert_eq!(fs::read(outbox.path("good")).unwrap(), before);
        let file = fs::OpenOptions::new().write(true).open(&malformed).unwrap();
        file.set_len(MAX_RECORD as u64 + 1).unwrap();
        drop(file);
        assert!(Outbox::open(dir.0.clone()).is_err());
        fs::remove_file(&malformed).unwrap();
        let orphan = dir.0.join(format!("{}.999-1.tmp", filename("good")));
        fs::write(&orphan, b"uncommitted").unwrap();
        let reopened = Outbox::open(dir.0.clone()).unwrap();
        assert!(!orphan.exists());
        assert_eq!(reopened.get("good").unwrap().unwrap().object_bytes, bytes);
        let mut malformed = reopened.get("good").unwrap().unwrap();
        malformed.attempts = 0;
        fs::write(reopened.path("good"), encode(&malformed)).unwrap();
        assert!(Outbox::open(dir.0.clone()).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn symlink_records_and_symlink_roots_are_not_followed() {
        let dir = Directory::new();
        let outside = Directory::new();
        fs::write(outside.0.join("target"), b"outside").unwrap();
        std::os::unix::fs::symlink(outside.0.join("target"), dir.0.join(filename("operation")))
            .unwrap();
        assert!(Outbox::open(dir.0.clone()).is_err());
        assert_eq!(fs::read(outside.0.join("target")).unwrap(), b"outside");
        let alias = dir.0.join("alias");
        std::os::unix::fs::symlink(&outside.0, &alias).unwrap();
        assert!(Outbox::open(alias).is_err());
    }
}
