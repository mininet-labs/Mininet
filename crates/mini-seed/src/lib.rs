//! Owner-opt-in retention for public signed objects, separate from source stores.
//!
//! Admission checks bytes and public-envelope integrity, NOT signer authority.
//! Callers MUST first verify signatures, delegation and provenance through their
//! authenticated Ingest path. This pool does not grant publication, governance,
//! wallet or reward authority. Reading never renews a lease. Retention is a local
//! best-effort availability commitment, not proof of replication or payment.
//!
//! Durable state assumes an owner-controlled directory, cooperating processes
//! using the stable pool lock, and a filesystem honoring flush and rename.
//! An administrator or hostile simultaneous filesystem writer is outside this
//! local storage boundary. A failed durable write may already have committed;
//! callers must reload rather than infer state from an error.
#![forbid(unsafe_code)]

use mini_objects::{Object, ObjectId, Payload};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

pub const MAX_OBJECT_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_POOL_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_POOL_OBJECTS: usize = 4096;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const DAY_MS: u64 = 86_400_000;

#[derive(Debug)]
pub struct SeedError(pub String);
impl fmt::Display for SeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for SeedError {}
impl From<std::io::Error> for SeedError {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}
impl From<serde_json::Error> for SeedError {
    fn from(e: serde_json::Error) -> Self {
        Self(e.to_string())
    }
}
pub type Result<T> = std::result::Result<T, SeedError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lease {
    pub id: String,
    pub bytes: u64,
    pub expires_at_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolStatus {
    pub max_bytes: u64,
    pub used_bytes: u64,
    pub objects: usize,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u8,
    max_bytes: u64,
    leases: BTreeMap<String, Lease>,
}
impl Manifest {
    fn used(&self) -> Result<u64> {
        self.leases.values().try_fold(0u64, |n, l| {
            n.checked_add(l.bytes)
                .ok_or_else(|| SeedError("pool byte accounting overflow".into()))
        })
    }
    fn status(&self) -> Result<PoolStatus> {
        Ok(PoolStatus {
            max_bytes: self.max_bytes,
            used_bytes: self.used()?,
            objects: self.leases.len(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Pool {
    root: PathBuf,
}

fn plain_path(path: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    for part in path.components() {
        if matches!(part, Component::ParentDir) {
            return Err(SeedError(
                "pool path cannot traverse parent directories".into(),
            ));
        }
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(SeedError("pool paths cannot traverse symlinks".into()))
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
fn id_valid(id: &str) -> Result<()> {
    if id.len() > 128
        || !id.bytes().all(|b| b.is_ascii_alphanumeric())
        || ObjectId::parse(id).is_err()
    {
        return Err(SeedError("invalid seed object id".into()));
    }
    Ok(())
}
fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>> {
    plain_path(path)?;
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.len() > max {
        return Err(SeedError(
            "pool file is not regular or exceeds its size bound".into(),
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(SeedError("pool file grew beyond its size bound".into()));
    }
    Ok(bytes)
}
fn validate_object(bytes: &[u8], expected: &str) -> Result<()> {
    let object = Object::from_bytes(bytes).map_err(|e| SeedError(e.to_string()))?;
    if !matches!(object.payload, Payload::Public(_)) || object.id().as_str() != expected {
        return Err(SeedError(
            "seed data is encrypted or its content id does not match".into(),
        ));
    }
    object
        .verify_integrity(object.id())
        .map_err(|e| SeedError(e.to_string()))
}
fn is_temp(name: &str, base: &str) -> bool {
    let Some(rest) = name
        .strip_prefix(base)
        .and_then(|s| s.strip_prefix('.'))
        .and_then(|s| s.strip_suffix(".tmp"))
    else {
        return false;
    };
    let Some((pid, seq)) = rest.split_once('-') else {
        return false;
    };
    !pid.is_empty()
        && !seq.is_empty()
        && pid.bytes().all(|b| b.is_ascii_digit())
        && seq.bytes().all(|b| b.is_ascii_digit())
}

impl Pool {
    /// Open or initialize a disabled pool (quota zero). No source-store path is
    /// inferred, linked, moved or deleted. Configure an explicit quota to opt in.
    pub fn open(path: &Path) -> Result<Self> {
        let path = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()?.join(path)
        };
        plain_path(&path)?;
        mini_durable::create_dir_all(&path)?;
        let root = fs::canonicalize(path)?;
        let pool = Self { root };
        let _lock = pool.lock()?;
        let objects = pool.root.join("objects");
        plain_path(&objects)?;
        mini_durable::create_dir_all(&objects)?;
        let manifest = pool.root.join("manifest.json");
        if !manifest.try_exists()? {
            // Missing state in a populated pool is corruption, never an
            // instruction to silently reset retention or delete source data.
            if fs::read_dir(&objects)?.next().is_some() {
                return Err(SeedError("populated pool is missing its manifest".into()));
            }
            pool.save(&Manifest {
                version: 1,
                max_bytes: 0,
                leases: BTreeMap::new(),
            })?;
        }
        let state = pool.load()?;
        pool.cleanup(&state)?;
        Ok(pool)
    }
    fn lock(&self) -> Result<File> {
        plain_path(&self.root)?;
        let path = self.root.join("pool.lock");
        plain_path(&path)?;
        if let Ok(meta) = fs::symlink_metadata(&path) {
            if !meta.is_file() {
                return Err(SeedError("pool lock must be regular".into()));
            }
        }
        Ok(mini_durable::lock_exclusive(&path)?)
    }
    fn load(&self) -> Result<Manifest> {
        let state: Manifest = serde_json::from_slice(&read_bounded(
            &self.root.join("manifest.json"),
            MAX_MANIFEST_BYTES,
        )?)?;
        if state.version != 1
            || state.max_bytes > MAX_POOL_BYTES
            || state.leases.len() > MAX_POOL_OBJECTS
        {
            return Err(SeedError("unsupported or oversized pool manifest".into()));
        }
        plain_path(&self.root.join("objects"))?;
        for (id, lease) in &state.leases {
            id_valid(id)?;
            if lease.id != *id || lease.bytes == 0 || lease.bytes > MAX_OBJECT_BYTES {
                return Err(SeedError("invalid pool lease metadata".into()));
            }
            let path = self.root.join("objects").join(id);
            plain_path(&path)?;
            let meta = fs::symlink_metadata(&path)?;
            if !meta.is_file() || meta.len() != lease.bytes {
                return Err(SeedError(
                    "pool object missing, truncated, or nonregular".into(),
                ));
            }
        }
        if state.used()? > state.max_bytes {
            return Err(SeedError("pool manifest exceeds configured quota".into()));
        }
        Ok(state)
    }
    fn save(&self, state: &Manifest) -> Result<()> {
        let bytes = serde_json::to_vec(state)?;
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(SeedError("pool manifest exceeds size bound".into()));
        }
        plain_path(&self.root.join("manifest.json"))?;
        mini_durable::atomic_replace(&self.root.join("manifest.json"), &bytes)?;
        Ok(())
    }
    /// Delete only recognized pool-owned files absent from the committed
    /// manifest (including an interrupted admit/revoke). Unknown paths fail
    /// closed and are never followed or removed.
    fn cleanup(&self, state: &Manifest) -> Result<()> {
        let dir = self.root.join("objects");
        plain_path(&dir)?;
        let mut removals = Vec::new();
        let mut count = 0usize;
        for entry in fs::read_dir(&dir)? {
            count += 1;
            if count > MAX_POOL_OBJECTS * 2 + 16 {
                return Err(SeedError("too many physical pool files".into()));
            }
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| SeedError("untrusted pool filename".into()))?;
            let meta = fs::symlink_metadata(entry.path())?;
            if !meta.is_file() || meta.file_type().is_symlink() {
                return Err(SeedError("untrusted nonregular pool entry".into()));
            }
            if id_valid(&name).is_ok() {
                if !state.leases.contains_key(&name) {
                    removals.push(entry.path());
                }
            } else {
                let base = name.split('.').next().unwrap_or("");
                if id_valid(base).is_err() || !is_temp(&name, base) {
                    return Err(SeedError("untrusted pool filename".into()));
                }
                removals.push(entry.path());
            }
        }
        // mini-durable uses the same constrained process/sequence temp suffix.
        let mut root_entries = 0;
        for entry in fs::read_dir(&self.root)? {
            root_entries += 1;
            if root_entries > 32 {
                return Err(SeedError("too many pool metadata entries".into()));
            }
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| SeedError("untrusted pool metadata filename".into()))?;
            if matches!(name.as_str(), "manifest.json" | "pool.lock" | "objects") {
                continue;
            }
            if !is_temp(&name, "manifest.json") || !fs::symlink_metadata(entry.path())?.is_file() {
                return Err(SeedError("untrusted pool metadata entry".into()));
            }
            removals.push(entry.path());
        }
        for path in removals {
            fs::remove_file(&path)?;
            mini_durable::sync_parent(&path)?;
        }
        Ok(())
    }
    pub fn configure(&self, max_bytes: u64) -> Result<PoolStatus> {
        let _lock = self.lock()?;
        let mut state = self.load()?;
        self.cleanup(&state)?;
        if max_bytes > MAX_POOL_BYTES || max_bytes < state.used()? {
            return Err(SeedError(
                "quota must cover retained bytes and be at most 1 GiB; prune/revoke first".into(),
            ));
        }
        state.max_bytes = max_bytes;
        self.save(&state)?;
        state.status()
    }
    /// Caller must authenticate provenance before admitting this public object.
    /// This explicit call alone creates or renews a lease; reads never do.
    pub fn admit(&self, object: &Object, days: u16, now_ms: u64) -> Result<Lease> {
        if !(1..=3650).contains(&days) {
            return Err(SeedError("retention must be 1–3650 days".into()));
        }
        let expires_at_ms = u64::from(days)
            .checked_mul(DAY_MS)
            .and_then(|d| now_ms.checked_add(d))
            .ok_or_else(|| SeedError("retention timestamp overflow".into()))?;
        // Public Object fields can be edited in memory. Bound them before
        // serializing or hashing, even though authenticated callers normally
        // supply objects already accepted by the bounded envelope decoder.
        if !matches!(&object.payload, Payload::Public(bytes) if bytes.len() <= mini_objects::MAX_PAYLOAD_BYTES)
            || object.links.len() > mini_objects::MAX_LINKS
            || object.links.iter().any(|link| link.rel.len() > 1024)
            || matches!(&object.object_type, mini_objects::ObjectType::Custom(name) if name.len() > 1024)
        {
            return Err(SeedError(
                "only bounded public objects may be seeded".into(),
            ));
        }
        object
            .verify_integrity(object.id())
            .map_err(|e| SeedError(e.to_string()))?;
        let id = object.id().as_str();
        id_valid(id)?;
        let bytes = object.to_bytes();
        if bytes.len() as u64 > MAX_OBJECT_BYTES {
            return Err(SeedError("object exceeds seed size limit".into()));
        }
        validate_object(&bytes, id)?;
        let _lock = self.lock()?;
        let mut state = self.load()?;
        self.cleanup(&state)?;
        let size = bytes.len() as u64;
        let old = state.leases.get(id).map_or(0, |l| l.bytes);
        let used = state
            .used()?
            .checked_sub(old)
            .and_then(|n| n.checked_add(size))
            .ok_or_else(|| SeedError("pool quota accounting overflow".into()))?;
        if used > state.max_bytes || (old == 0 && state.leases.len() == MAX_POOL_OBJECTS) {
            return Err(SeedError("pool quota or object-count limit reached".into()));
        }
        let path = self.root.join("objects").join(id);
        if old != 0 {
            if read_bounded(&path, MAX_OBJECT_BYTES)? != bytes {
                return Err(SeedError("existing seed object is corrupt".into()));
            }
        } else {
            mini_durable::atomic_replace(&path, &bytes)?;
        }
        let lease = Lease {
            id: id.to_owned(),
            bytes: size,
            expires_at_ms,
        };
        state.leases.insert(id.to_owned(), lease.clone());
        self.save(&state)?;
        Ok(lease)
    }
    pub fn revoke(&self, id: &str) -> Result<bool> {
        id_valid(id)?;
        let _lock = self.lock()?;
        let mut state = self.load()?;
        let removed = state.leases.remove(id).is_some();
        if removed {
            self.save(&state)?;
        }
        self.cleanup(&state)?;
        Ok(removed)
    }
    pub fn prune(&self, now_ms: u64) -> Result<usize> {
        let _lock = self.lock()?;
        let mut state = self.load()?;
        let before = state.leases.len();
        state.leases.retain(|_, l| l.expires_at_ms > now_ms);
        let removed = before - state.leases.len();
        if removed != 0 {
            self.save(&state)?;
        }
        self.cleanup(&state)?;
        Ok(removed)
    }
    pub fn status(&self) -> Result<PoolStatus> {
        let _lock = self.lock()?;
        self.load()?.status()
    }
    /// Inspect one active lease, verifying only that object's exact public
    /// bytes. Manifest accounting/physical lengths remain globally bounded and
    /// fail closed, but unrelated objects are not read or hashed for a quote.
    pub fn lease(&self, id: &str, now_ms: u64) -> Result<Lease> {
        id_valid(id)?;
        let _lock = self.lock()?;
        let state = self.load()?;
        let lease = state
            .leases
            .get(id)
            .filter(|l| l.expires_at_ms > now_ms)
            .ok_or_else(|| SeedError("object is not retained or its lease expired".into()))?;
        validate_object(
            &read_bounded(&self.root.join("objects").join(id), MAX_OBJECT_BYTES)?,
            id,
        )?;
        Ok(lease.clone())
    }
    pub fn inventory(&self, now_ms: u64) -> Result<Vec<Lease>> {
        let _lock = self.lock()?;
        let state = self.load()?;
        let mut leases = Vec::new();
        for lease in state.leases.values().filter(|l| l.expires_at_ms > now_ms) {
            validate_object(
                &read_bounded(&self.root.join("objects").join(&lease.id), MAX_OBJECT_BYTES)?,
                &lease.id,
            )?;
            leases.push(lease.clone());
        }
        Ok(leases)
    }
    pub fn read(&self, id: &str, now_ms: u64) -> Result<Vec<u8>> {
        id_valid(id)?;
        let _lock = self.lock()?;
        let state = self.load()?;
        let lease = state
            .leases
            .get(id)
            .filter(|l| l.expires_at_ms > now_ms)
            .ok_or_else(|| SeedError("object is not retained or its lease expired".into()))?;
        let bytes = read_bounded(&self.root.join("objects").join(&lease.id), MAX_OBJECT_BYTES)?;
        validate_object(&bytes, id)?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_objects::{ObjectBuilder, ObjectType};
    fn object(content: &[u8]) -> Object {
        signed(Payload::Public(content.to_vec()))
    }
    fn signed(payload: Payload) -> Object {
        let root = did_mini::Controller::incept_single_from_seeds(&[1; 32], &[2; 32]).unwrap();
        let device =
            did_mini::Controller::incept_device_single_from_seeds(&root.did(), &[3; 32], &[4; 32])
                .unwrap();
        ObjectBuilder::new(ObjectType::Custom("seed-test".into()))
            .payload(payload)
            .sign(&root.did(), &device)
            .unwrap()
    }
    #[test]
    fn restart_expiry_renewal_and_revocation_are_explicit() {
        let dir = tempfile::tempdir().unwrap();
        let data = object(b"history");
        let id = data.id().as_str();
        let pool = Pool::open(dir.path()).unwrap();
        assert_eq!(pool.status().unwrap().max_bytes, 0);
        assert!(pool.admit(&data, 1, 0).is_err());
        pool.configure(1024 * 1024).unwrap();
        let first = pool.admit(&data, 1, 100).unwrap();
        drop(pool);
        let pool = Pool::open(dir.path()).unwrap();
        assert_eq!(
            pool.read(id, first.expires_at_ms - 1).unwrap(),
            data.to_bytes()
        );
        assert!(pool.read(id, first.expires_at_ms).is_err());
        assert_eq!(pool.lease(id, first.expires_at_ms - 1).unwrap(), first);
        assert!(pool.lease(id, first.expires_at_ms).is_err());
        assert!(pool.lease("../untrusted", 0).is_err());
        assert!(pool.inventory(first.expires_at_ms).unwrap().is_empty());
        assert_eq!(pool.status().unwrap().objects, 1); // expiry alone does not reclaim disk
        let renewed = pool.admit(&data, 2, first.expires_at_ms).unwrap();
        assert_eq!(renewed.expires_at_ms, first.expires_at_ms + 2 * DAY_MS);
        assert_eq!(pool.status().unwrap().used_bytes, first.bytes);
        pool.read(id, renewed.expires_at_ms - 1).unwrap();
        assert_eq!(
            pool.inventory(0).unwrap()[0].expires_at_ms,
            renewed.expires_at_ms
        );
        assert!(pool.revoke(id).unwrap());
        assert!(!pool.revoke(id).unwrap());
        assert!(pool.read(id, 0).is_err());
        assert_eq!(data.payload, Payload::Public(b"history".to_vec()));
    }
    #[test]
    fn quota_pruning_and_overflow_fail_without_writing_objects() {
        let dir = tempfile::tempdir().unwrap();
        let data = object(b"one");
        let pool = Pool::open(dir.path()).unwrap();
        let size = data.to_bytes().len() as u64;
        pool.configure(size).unwrap();
        pool.admit(&data, 1, 0).unwrap();
        let other = object(b"two");
        assert!(pool.admit(&other, 1, 0).is_err());
        assert!(!dir
            .path()
            .join("objects")
            .join(other.id().as_str())
            .exists());
        assert!(pool.configure(size - 1).is_err());
        assert!(pool.admit(&data, 0, 0).is_err());
        assert!(pool.admit(&data, 3651, 0).is_err());
        assert!(pool.admit(&data, 1, u64::MAX).is_err());
        assert_eq!(pool.prune(DAY_MS).unwrap(), 1);
        assert_eq!(pool.status().unwrap().used_bytes, 0);
        pool.configure(0).unwrap();
    }
    #[test]
    fn corrupt_bytes_and_encrypted_payload_are_never_served() {
        let dir = tempfile::tempdir().unwrap();
        let mut data = object(b"public");
        let pool = Pool::open(dir.path()).unwrap();
        pool.configure(1024 * 1024).unwrap();
        pool.admit(&data, 1, 0).unwrap();
        let unrelated = object(b"another valid seed");
        pool.admit(&unrelated, 1, 0).unwrap();
        let path = dir.path().join("objects").join(data.id().as_str());
        let mut bytes = fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(&path, bytes).unwrap();
        assert!(pool.read(data.id().as_str(), 0).is_err());
        assert!(pool.lease(data.id().as_str(), 0).is_err());
        // A targeted quote does not hash an unrelated same-size corrupt blob;
        // it still verifies the requested object's exact bytes. Full inventory
        // remains fail-closed for any advertised corrupt object.
        assert_eq!(
            pool.lease(unrelated.id().as_str(), 0).unwrap().id,
            unrelated.id().as_str()
        );
        assert!(pool.inventory(0).is_err());
        assert!(pool.admit(&data, 1, 0).is_err());
        data.payload = Payload::Encrypted(b"private".to_vec());
        assert!(pool.admit(&data, 1, 0).is_err());
        let encrypted = signed(Payload::Encrypted(b"private".to_vec()));
        encrypted.verify_integrity(encrypted.id()).unwrap();
        assert!(pool.admit(&encrypted, 1, 0).is_err());
        assert!(pool.read("../../outside", 0).is_err());
    }
    #[test]
    fn recovery_reclaims_only_uncommitted_pool_objects() {
        let dir = tempfile::tempdir().unwrap();
        let pool = Pool::open(dir.path()).unwrap();
        let orphan = object(b"orphan");
        let path = dir.path().join("objects").join(orphan.id().as_str());
        fs::write(&path, orphan.to_bytes()).unwrap(); // crash before manifest publication
        drop(pool);
        Pool::open(dir.path()).unwrap();
        assert!(!path.exists());
        fs::write(dir.path().join("objects/notes.txt"), b"owner file").unwrap();
        assert!(Pool::open(dir.path()).is_err());
        assert_eq!(
            fs::read(dir.path().join("objects/notes.txt")).unwrap(),
            b"owner file"
        );
    }
    #[test]
    fn separate_pool_handles_serialize_quota_updates() {
        let dir = tempfile::tempdir().unwrap();
        let pool = Pool::open(dir.path()).unwrap();
        let one = object(b"one");
        let two = object(b"two");
        assert_eq!(one.to_bytes().len(), two.to_bytes().len());
        pool.configure(one.to_bytes().len() as u64).unwrap();
        let second = Pool::open(dir.path()).unwrap();
        let gate = std::sync::Arc::new(std::sync::Barrier::new(2));
        let other_gate = gate.clone();
        let worker = std::thread::spawn(move || {
            other_gate.wait();
            second.admit(&two, 1, 0).is_ok()
        });
        gate.wait();
        let local = pool.admit(&one, 1, 0).is_ok();
        let remote = worker.join().unwrap();
        assert_ne!(local, remote);
        assert_eq!(pool.status().unwrap().objects, 1);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_pool_objects_and_metadata_fail_closed() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), dir.path().join("alias")).unwrap();
        assert!(Pool::open(&dir.path().join("alias")).is_err());
        let pool = Pool::open(&dir.path().join("pool")).unwrap();
        pool.configure(1024 * 1024).unwrap();
        let data = object(b"public");
        let path = dir.path().join("pool/objects").join(data.id().as_str());
        fs::write(outside.path().join("private"), data.to_bytes()).unwrap();
        symlink(outside.path().join("private"), &path).unwrap();
        assert!(pool.admit(&data, 1, 0).is_err());
        assert!(outside.path().join("private").exists());
        fs::remove_file(&path).unwrap();
        fs::remove_file(dir.path().join("pool/manifest.json")).unwrap();
        symlink(
            outside.path().join("private"),
            dir.path().join("pool/manifest.json"),
        )
        .unwrap();
        assert!(pool.status().is_err());
    }
}
