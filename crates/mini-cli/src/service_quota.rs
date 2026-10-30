//! Conservative durable service-ingest byte reservations. Failed writes may
//! retain a reservation; restart never refunds uncertain publication. This is
//! a service policy, not a quota for owner-invoked/external store writers.
use mini_store::{Backend, FsBackend, Result, StoreError};
use std::path::{Path, PathBuf};
const GLOBAL_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
const PEER_LIMIT: u64 = 512 * 1024 * 1024;
fn io(e: std::io::Error) -> StoreError {
    StoreError::Io(e.to_string())
}
fn counter(path: &Path) -> Result<u64> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(io(e)),
        Ok(m) if !m.is_file() || m.file_type().is_symlink() || m.len() > 32 => {
            return Err(StoreError::Corrupt)
        }
        Ok(_) => {}
    }
    std::fs::read_to_string(path)
        .map_err(io)?
        .parse()
        .map_err(|_| StoreError::Corrupt)
}
fn save(path: &Path, n: u64) -> Result<()> {
    mini_durable::atomic_replace(path, n.to_string().as_bytes()).map_err(io)
}
fn directory(root: &Path) -> Result<PathBuf> {
    let path = root.join("service-quota-v1");
    if let Ok(m) = std::fs::symlink_metadata(&path) {
        if !m.is_dir() || m.file_type().is_symlink() {
            return Err(StoreError::Corrupt);
        }
    }
    mini_durable::create_dir_all(&path).map_err(io)?;
    Ok(path)
}
fn disk_bytes(path: &Path) -> Result<u64> {
    let mut total = 0u64;
    for item in std::fs::read_dir(path).map_err(io)? {
        let item = item.map_err(io)?;
        let m = item.path().symlink_metadata().map_err(io)?;
        if m.file_type().is_symlink() {
            return Err(StoreError::Corrupt);
        }
        let bytes = if m.is_dir() {
            disk_bytes(&item.path())?
        } else if m.is_file() {
            m.len()
        } else {
            return Err(StoreError::Corrupt);
        };
        total = total.checked_add(bytes).ok_or(StoreError::LimitExceeded)?;
    }
    Ok(total)
}
/// Reconcile existing store bytes once at service startup, never per object.
pub(crate) fn initialize(root: &Path) -> Result<()> {
    let _backend = FsBackend::open(root)?;
    let dir = directory(root)?;
    let _lock = mini_durable::lock_exclusive(&dir.join("lock")).map_err(io)?;
    let used = disk_bytes(&root.join("blobs"))?.max(counter(&dir.join("global"))?);
    save(&dir.join("global"), used)
}
pub(crate) struct QuotaBackend {
    inner: FsBackend,
    dir: PathBuf,
    peer: String,
}
impl QuotaBackend {
    pub(crate) fn open(root: &Path, peer: &str) -> Result<Self> {
        let hash = mini_crypto::HashAlgorithm::Blake3.digest(peer.as_bytes());
        Ok(Self {
            inner: FsBackend::open(root)?,
            dir: directory(root)?,
            peer: hash.iter().map(|b| format!("{b:02x}")).collect(),
        })
    }
}
impl Backend for QuotaBackend {
    fn put_blob(&mut self, id: &str, bytes: &[u8]) -> Result<()> {
        let _lock = mini_durable::lock_exclusive(&self.dir.join("lock")).map_err(io)?;
        let existing_len = self
            .inner
            .get_blob(id)?
            .map_or(0, |existing| existing.len());
        let growth = bytes.len().saturating_sub(existing_len) as u64;
        if growth == 0 {
            return self.inner.put_blob(id, bytes);
        }
        let global = counter(&self.dir.join("global"))?
            .checked_add(growth)
            .ok_or(StoreError::LimitExceeded)?;
        let peer_path = self.dir.join(&self.peer);
        let peer = counter(&peer_path)?
            .checked_add(growth)
            .ok_or(StoreError::LimitExceeded)?;
        if global > GLOBAL_LIMIT || peer > PEER_LIMIT {
            return Err(StoreError::LimitExceeded);
        }
        // Reservations precede publication and are never automatically refunded.
        save(&peer_path, peer)?;
        save(&self.dir.join("global"), global)?;
        self.inner.put_blob(id, bytes)
    }
    fn get_blob(&self, id: &str) -> Result<Option<Vec<u8>>> {
        self.inner.get_blob(id)
    }
    fn has_blob(&self, id: &str) -> Result<bool> {
        self.inner.has_blob(id)
    }
    fn put_meta(&mut self, key: &str, bytes: &[u8]) -> Result<()> {
        self.inner.put_meta(key, bytes)
    }
    fn get_meta(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.inner.get_meta(key)
    }
    fn list_meta_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>> {
        self.inner.list_meta_prefix(prefix)
    }
    fn list_meta_prefix_last(&self, prefix: &str, limit: usize) -> Result<Vec<(String, Vec<u8>)>> {
        self.inner.list_meta_prefix_last(prefix, limit)
    }
    fn list_meta_prefix_page(
        &self,
        prefix: &str,
        after: &str,
        limit: usize,
    ) -> Result<Vec<(String, Vec<u8>)>> {
        self.inner.list_meta_prefix_page(prefix, after, limit)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reservations_survive_restart_and_stop_peer_and_global_growth() {
        let tmp = tempfile::tempdir().unwrap();
        initialize(tmp.path()).unwrap();
        let mut b = QuotaBackend::open(tmp.path(), "peer").unwrap();
        b.put_blob("test", b"1234").unwrap();
        drop(b);
        let mut b = QuotaBackend::open(tmp.path(), "peer").unwrap();
        let peer = b.dir.join(&b.peer);
        assert_eq!(counter(&peer).unwrap(), 4);
        b.put_blob("test", b"1234").unwrap();
        assert_eq!(counter(&peer).unwrap(), 4);
        save(&peer, PEER_LIMIT).unwrap();
        assert!(b.put_blob("new", b"x").is_err());
        assert!(!b.has_blob("new").unwrap());
        save(&peer, 0).unwrap();
        save(&b.dir.join("global"), GLOBAL_LIMIT).unwrap();
        assert!(b.put_blob("other", b"x").is_err());
        assert!(!b.has_blob("other").unwrap());
    }
}
