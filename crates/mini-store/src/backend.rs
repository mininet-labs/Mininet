//! Storage backends: a small key/value + blob abstraction so devices use the
//! filesystem (and later SQLite/OPFS) while tests use memory — same `Store`
//! logic above all of them.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::{Result, StoreError};

/// A minimal persistent backend: content blobs (by id) plus ordered metadata
/// entries (index rows, head slots). All keys are ASCII, `/`-separated.
pub trait Backend {
    /// Persist a blob under `id` (idempotent).
    fn put_blob(&mut self, id: &str, bytes: &[u8]) -> Result<()>;
    /// Fetch a blob by `id`.
    fn get_blob(&self, id: &str) -> Result<Option<Vec<u8>>>;
    /// Whether a blob exists.
    fn has_blob(&self, id: &str) -> Result<bool>;
    /// Write a metadata entry (overwrites).
    fn put_meta(&mut self, key: &str, value: &[u8]) -> Result<()>;
    /// Read a metadata entry.
    fn get_meta(&self, key: &str) -> Result<Option<Vec<u8>>>;
    /// All metadata entries whose key starts with `prefix`, key-ordered
    /// (deterministic across backends).
    fn list_meta_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>>;
}

/// In-memory backend for tests.
#[derive(Debug, Default)]
pub struct MemoryBackend {
    blobs: BTreeMap<String, Vec<u8>>,
    meta: BTreeMap<String, Vec<u8>>,
}

impl MemoryBackend {
    /// A new, empty backend.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Backend for MemoryBackend {
    fn put_blob(&mut self, id: &str, bytes: &[u8]) -> Result<()> {
        self.blobs.insert(id.to_string(), bytes.to_vec());
        Ok(())
    }
    fn get_blob(&self, id: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.blobs.get(id).cloned())
    }
    fn has_blob(&self, id: &str) -> Result<bool> {
        Ok(self.blobs.contains_key(id))
    }
    fn put_meta(&mut self, key: &str, value: &[u8]) -> Result<()> {
        self.meta.insert(key.to_string(), value.to_vec());
        Ok(())
    }
    fn get_meta(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.meta.get(key).cloned())
    }
    fn list_meta_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>> {
        Ok(self
            .meta
            .range(prefix.to_string()..)
            .take_while(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }
}

/// Filesystem backend: `blobs/<first-2-chars>/<id>` with atomic tmp+rename
/// writes, and metadata as files under `meta/` (path-encoded keys).
#[derive(Debug)]
pub struct FsBackend {
    root: PathBuf,
}

impl FsBackend {
    /// Open (creating directories as needed) a backend rooted at `root`.
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root.join("blobs"))?;
        fs::create_dir_all(root.join("meta"))?;
        Ok(FsBackend {
            root: root.to_path_buf(),
        })
    }

    fn blob_path(&self, id: &str) -> Result<PathBuf> {
        // Ids are multibase (alphanumeric); refuse anything path-hostile.
        if id.len() < 3 || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(StoreError::Io("invalid blob id".to_string()));
        }
        Ok(self.root.join("blobs").join(&id[..2]).join(id))
    }

    fn meta_path(&self, key: &str) -> Result<PathBuf> {
        // Keys are `/`-separated ASCII segments; each segment becomes a path
        // component. Refuse traversal and empty segments.
        if key.is_empty()
            || !key.bytes().all(|b| {
                b.is_ascii_alphanumeric() || b == b'/' || b == b'-' || b == b'_' || b == b'.'
            })
            || key
                .split('/')
                .any(|seg| seg.is_empty() || seg == "." || seg == "..")
        {
            return Err(StoreError::Io("invalid meta key".to_string()));
        }
        Ok(self.root.join("meta").join(key))
    }

    fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
        }
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

impl Backend for FsBackend {
    fn put_blob(&mut self, id: &str, bytes: &[u8]) -> Result<()> {
        let p = self.blob_path(id)?;
        if p.exists() {
            // Content-addressed: an existing blob under this id SHOULD be these
            // exact bytes. If a local blob got corrupted, repair it atomically
            // rather than trusting the stale/corrupt copy forever.
            match fs::read(&p) {
                Ok(existing) if existing == bytes => return Ok(()),
                Ok(_) => return Self::atomic_write(&p, bytes), // repair
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Self::atomic_write(&p, bytes)
    }
    fn get_blob(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let p = self.blob_path(id)?;
        match fs::read(&p) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    fn has_blob(&self, id: &str) -> Result<bool> {
        Ok(self.blob_path(id)?.exists())
    }
    fn put_meta(&mut self, key: &str, value: &[u8]) -> Result<()> {
        Self::atomic_write(&self.meta_path(key)?, value)
    }
    fn get_meta(&self, key: &str) -> Result<Option<Vec<u8>>> {
        match fs::read(self.meta_path(key)?) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    fn list_meta_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>> {
        // Walk meta/ and filter; ordered for determinism.
        let mut out = Vec::new();
        let base = self.root.join("meta");
        let mut stack = vec![base.clone()];
        while let Some(dir) = stack.pop() {
            let entries = match fs::read_dir(&dir) {
                Ok(e) => e,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map(|e| e == "tmp").unwrap_or(false) {
                    continue;
                }
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let key = path
                        .strip_prefix(&base)
                        .map_err(|_| StoreError::Io("meta path escape".to_string()))?
                        .to_string_lossy()
                        .replace('\\', "/");
                    if key.starts_with(prefix) {
                        out.push((key, fs::read(&path)?));
                    }
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }
}
