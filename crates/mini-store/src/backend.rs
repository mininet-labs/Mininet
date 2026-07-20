//! Storage backends: a small key/value + blob abstraction so devices use the
//! filesystem (and later SQLite/OPFS) while tests use memory — same `Store`
//! logic above all of them.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

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

    /// The last `limit` metadata entries whose key starts with `prefix`, in
    /// *descending* key order — the bounded counterpart a "most recent N"
    /// query needs instead of calling [`Self::list_meta_prefix`] (which
    /// always reads the entire matching subtree) and reversing/truncating
    /// the result client-side, first concrete slice of Batch 5's "local
    /// object indexing at scale" (`docs/design/self-hosted-forge-spine.md`)
    /// following on from D-0327's `Store::recent`.
    ///
    /// The default implementation here is **not** itself a bounded-I/O
    /// scan — it still reads the whole matching subtree via
    /// `list_meta_prefix` before reversing and truncating, exactly the cost
    /// `Store::recent` already paid before this method existed. Override
    /// this when a backend can genuinely stop scanning once `limit` results
    /// are found, as [`MemoryBackend`] now does; `FsBackend` does not yet
    /// (a real bounded reverse walk over a plain directory tree needs
    /// either a sorted-order early-stopping traversal or an on-disk sorted
    /// index this backend doesn't have — real follow-up work, not silently
    /// assumed solved here, the same honest caveat D-0327 already recorded
    /// for the forward case).
    fn list_meta_prefix_last(&self, prefix: &str, limit: usize) -> Result<Vec<(String, Vec<u8>)>> {
        let mut all = self.list_meta_prefix(prefix)?;
        all.reverse();
        all.truncate(limit);
        Ok(all)
    }
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
    fn list_meta_prefix_last(&self, prefix: &str, limit: usize) -> Result<Vec<(String, Vec<u8>)>> {
        // `\u{7f}` (DEL) sorts above every character this store's keys ever
        // use (ASCII alphanumerics plus `-_./`, all below `0x7A`), so
        // `prefix..prefix+DEL` is an exact, exclusive upper bound on
        // `prefix`'s own range -- unlike `list_meta_prefix`'s `take_while`
        // (which cannot run in reverse, since `TakeWhile` is not a
        // `DoubleEndedIterator`), this lets `BTreeMap::range` itself do the
        // bounding, so `.rev().take(limit)` is a genuine O(log n + limit)
        // scan from the end, not a full-subtree read.
        let upper = format!("{prefix}\u{7f}");
        Ok(self
            .meta
            .range(prefix.to_string()..upper)
            .rev()
            .take(limit)
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

    /// Narrow a metadata-prefix query to the deepest directory that can
    /// contain a match. A trailing slash means every segment is complete;
    /// otherwise the last segment may be only a filename prefix, so traversal
    /// starts at its parent.
    fn meta_prefix_walk_root(&self, prefix: &str) -> Result<PathBuf> {
        let base = self.root.join("meta");
        if prefix.is_empty() {
            return Ok(base);
        }
        if prefix.starts_with('/')
            || prefix.contains("//")
            || !prefix.bytes().all(|b| {
                b.is_ascii_alphanumeric() || b == b'/' || b == b'-' || b == b'_' || b == b'.'
            })
        {
            return Err(StoreError::Io("invalid meta prefix".to_string()));
        }

        let without_trailing = prefix.strip_suffix('/').unwrap_or(prefix);
        if without_trailing.is_empty()
            || without_trailing
                .split('/')
                .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        {
            return Err(StoreError::Io("invalid meta prefix".to_string()));
        }

        let complete_segments = if prefix.ends_with('/') {
            without_trailing
        } else {
            without_trailing
                .rsplit_once('/')
                .map(|(parent, _)| parent)
                .unwrap_or("")
        };
        Ok(if complete_segments.is_empty() {
            base
        } else {
            base.join(complete_segments)
        })
    }

    /// Validate every directory from `meta/` through `target`. Inspecting only
    /// the final path is insufficient because `symlink_metadata` follows
    /// symlinks in intermediate components.
    fn safe_existing_meta_directory(&self, target: &Path) -> Result<bool> {
        let base = self.root.join("meta");
        let relative = target
            .strip_prefix(&base)
            .map_err(|_| StoreError::Io("meta path escape".to_string()))?;
        let mut current = base;

        let check = |path: &Path| -> Result<bool> {
            match fs::symlink_metadata(path) {
                Ok(metadata) if metadata.file_type().is_symlink() => Err(StoreError::Io(
                    "symlink in metadata index traversal".to_string(),
                )),
                Ok(metadata) => Ok(metadata.is_dir()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(e) => Err(e.into()),
            }
        };

        if !check(&current)? {
            return Ok(false);
        }
        for component in relative.components() {
            match component {
                Component::Normal(segment) => current.push(segment),
                _ => return Err(StoreError::Io("invalid meta prefix".to_string())),
            }
            if !check(&current)? {
                return Ok(false);
            }
        }
        Ok(true)
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

    /// Validate every existing parent component without following symlinks.
    /// Checking only the final path would still allow `meta/<symlink>/row` to
    /// escape the store before the final component is inspected.
    fn safe_existing_parent(base: &Path, path: &Path, label: &str) -> Result<bool> {
        let parent = path
            .parent()
            .ok_or_else(|| StoreError::Io(format!("{label} has no parent directory")))?;
        let relative = parent
            .strip_prefix(base)
            .map_err(|_| StoreError::Io(format!("{label} path escapes its store directory")))?;
        let mut current = base.to_path_buf();

        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(StoreError::Io(format!("{label} path contains a symlink")))
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(StoreError::Io(format!("{label} parent is not a directory")))
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        }

        for component in relative.components() {
            match component {
                Component::Normal(segment) => current.push(segment),
                _ => {
                    return Err(StoreError::Io(format!(
                        "{label} has an invalid path component"
                    )))
                }
            }
            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(StoreError::Io(format!("{label} path contains a symlink")))
                }
                Ok(metadata) if !metadata.is_dir() => {
                    return Err(StoreError::Io(format!("{label} parent is not a directory")))
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(e) => return Err(e.into()),
            }
        }
        Ok(true)
    }

    fn existing_regular_file(path: &Path, base: &Path, label: &str) -> Result<bool> {
        if !Self::safe_existing_parent(base, path, label)? {
            return Ok(false);
        }
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                Err(StoreError::Io(format!("{label} is a symlink")))
            }
            Ok(metadata) if !metadata.is_file() => {
                Err(StoreError::Io(format!("{label} is not a regular file")))
            }
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    fn read_existing_regular_file(
        path: &Path,
        base: &Path,
        label: &str,
    ) -> Result<Option<Vec<u8>>> {
        if Self::existing_regular_file(path, base, label)? {
            Ok(Some(fs::read(path)?))
        } else {
            Ok(None)
        }
    }
}

impl Backend for FsBackend {
    fn put_blob(&mut self, id: &str, bytes: &[u8]) -> Result<()> {
        let p = self.blob_path(id)?;
        // Content-addressed: an existing blob under this id SHOULD be these
        // exact bytes. If a local blob got corrupted, repair it atomically
        // rather than trusting the stale/corrupt copy forever. Refuse local
        // symlink/non-file poison instead of following it outside the store.
        if let Some(existing) =
            Self::read_existing_regular_file(&p, &self.root.join("blobs"), "blob")?
        {
            if existing == bytes {
                return Ok(());
            }
            return Self::atomic_write(&p, bytes);
        }
        Self::atomic_write(&p, bytes)
    }
    fn get_blob(&self, id: &str) -> Result<Option<Vec<u8>>> {
        Self::read_existing_regular_file(&self.blob_path(id)?, &self.root.join("blobs"), "blob")
    }
    fn has_blob(&self, id: &str) -> Result<bool> {
        Self::existing_regular_file(&self.blob_path(id)?, &self.root.join("blobs"), "blob")
    }
    fn put_meta(&mut self, key: &str, value: &[u8]) -> Result<()> {
        Self::atomic_write(&self.meta_path(key)?, value)
    }
    fn get_meta(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Self::read_existing_regular_file(
            &self.meta_path(key)?,
            &self.root.join("meta"),
            "metadata entry",
        )
    }
    fn list_meta_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>> {
        // Start at the narrowest safe subtree rather than walking every index
        // for every query. This keeps author/type/link lookups proportional to
        // the requested index slice as the object store grows.
        let mut out = Vec::new();
        let base = self.root.join("meta");
        let walk_root = self.meta_prefix_walk_root(prefix)?;
        if !self.safe_existing_meta_directory(&walk_root)? {
            return Ok(out);
        }

        let mut stack = vec![walk_root];
        while let Some(dir) = stack.pop() {
            let entries = match fs::read_dir(&dir) {
                Ok(e) => e,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                let file_type = entry.file_type()?;
                if file_type.is_symlink() {
                    return Err(StoreError::Io(
                        "symlink in metadata index traversal".to_string(),
                    ));
                }
                if path.extension().map(|e| e == "tmp").unwrap_or(false) {
                    continue;
                }
                if file_type.is_dir() {
                    stack.push(path);
                } else if file_type.is_file() {
                    let key = path
                        .strip_prefix(&base)
                        .map_err(|_| StoreError::Io("meta path escape".to_string()))?
                        .to_string_lossy()
                        .replace('\\', "/");
                    if key.starts_with(prefix) {
                        out.push((key, fs::read(&path)?));
                    }
                } else {
                    return Err(StoreError::Io(
                        "non-file entry in metadata index traversal".to_string(),
                    ));
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::{Backend, FsBackend, MemoryBackend};
    use std::path::PathBuf;

    #[test]
    fn memory_backend_last_returns_descending_order_bounded_by_limit() {
        let mut backend = MemoryBackend::new();
        for (k, v) in [
            ("idx/time/00000000000000001000/a", "a"),
            ("idx/time/00000000000000002000/b", "b"),
            ("idx/time/00000000000000003000/c", "c"),
            ("head/alice/profile", "unrelated"),
        ] {
            backend.put_meta(k, v.as_bytes()).unwrap();
        }

        assert_eq!(
            backend.list_meta_prefix_last("idx/time/", 10).unwrap(),
            vec![
                ("idx/time/00000000000000003000/c".to_string(), b"c".to_vec()),
                ("idx/time/00000000000000002000/b".to_string(), b"b".to_vec()),
                ("idx/time/00000000000000001000/a".to_string(), b"a".to_vec()),
            ]
        );
        assert_eq!(
            backend.list_meta_prefix_last("idx/time/", 2).unwrap(),
            vec![
                ("idx/time/00000000000000003000/c".to_string(), b"c".to_vec()),
                ("idx/time/00000000000000002000/b".to_string(), b"b".to_vec()),
            ]
        );
        assert_eq!(
            backend.list_meta_prefix_last("idx/time/", 0).unwrap(),
            Vec::<(String, Vec<u8>)>::new()
        );
        assert!(backend
            .list_meta_prefix_last("missing/prefix/", 5)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn fs_backend_default_last_impl_agrees_with_memory_backend() {
        let dir = std::env::temp_dir().join(format!(
            "mini-store-list-last-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut fs_backend = FsBackend::open(&dir).unwrap();
        let mut mem_backend = MemoryBackend::new();
        for (k, v) in [
            ("idx/time/00000000000000001000/a", "a"),
            ("idx/time/00000000000000002000/b", "b"),
            ("idx/time/00000000000000003000/c", "c"),
        ] {
            fs_backend.put_meta(k, v.as_bytes()).unwrap();
            mem_backend.put_meta(k, v.as_bytes()).unwrap();
        }

        assert_eq!(
            fs_backend.list_meta_prefix_last("idx/time/", 2).unwrap(),
            mem_backend.list_meta_prefix_last("idx/time/", 2).unwrap()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prefix_walk_root_uses_only_complete_path_segments() {
        let backend = FsBackend {
            root: PathBuf::from("store"),
        };
        let meta = PathBuf::from("store").join("meta");

        assert_eq!(backend.meta_prefix_walk_root("").unwrap(), meta);
        assert_eq!(
            backend.meta_prefix_walk_root("idx/type/w8/").unwrap(),
            meta.join("idx/type/w8")
        );
        assert_eq!(
            backend.meta_prefix_walk_root("idx/type/w").unwrap(),
            meta.join("idx/type")
        );
        assert_eq!(backend.meta_prefix_walk_root("idx").unwrap(), meta);
    }

    #[test]
    fn prefix_walk_root_rejects_hostile_or_ambiguous_prefixes() {
        let backend = FsBackend {
            root: PathBuf::from("store"),
        };
        for prefix in ["/idx", "idx//type", "idx/../head", "idx/./head", "/"] {
            assert!(backend.meta_prefix_walk_root(prefix).is_err(), "{prefix}");
        }
    }
}
