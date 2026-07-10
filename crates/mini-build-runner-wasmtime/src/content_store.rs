//! Content-addressing integrity checks. `ExecutionRequest` carries only
//! digests (`component_digest`, `source_digest`) -- resolving a digest to
//! actual bytes is the coordinator's job, via a shared store directory
//! convention (`<store-dir>/objects/<hex digest>` for the component,
//! `<store-dir>/workspaces/<hex digest>/` for a source snapshot
//! directory). This module never trusts that convention alone: every
//! byte read from the store is re-hashed and compared against the digest
//! the coordinator claimed, so a corrupt or lying store can never make it
//! into the sandbox as if it were the signed input (exit criterion 1).

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Result, RunnerError};

pub fn hex_digest(digest: &[u8; 32]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Read the component's `.wasm` bytes from `<store_dir>/objects/<hex>`
/// and verify they hash to `expected`.
pub fn read_verified_component(store_dir: &Path, expected: &[u8; 32]) -> Result<Vec<u8>> {
    let path = store_dir.join("objects").join(hex_digest(expected));
    let bytes = fs::read(&path)?;
    let actual: [u8; 32] = blake3::hash(&bytes).into();
    if &actual != expected {
        return Err(RunnerError::DigestMismatch {
            expected: *expected,
            actual,
        });
    }
    Ok(bytes)
}

/// The workspace directory for a source snapshot digest, verified by
/// recomputing the same directory-tree digest a coordinator would have
/// computed when it wrote the snapshot: a blake3 hash over every
/// `(relative path, content)` pair, sorted by path so the digest is
/// independent of filesystem iteration order.
pub fn verified_workspace_dir(store_dir: &Path, expected: &[u8; 32]) -> Result<PathBuf> {
    let dir = store_dir.join("workspaces").join(hex_digest(expected));
    let actual = hash_directory_tree(&dir)?;
    if &actual != expected {
        return Err(RunnerError::DigestMismatch {
            expected: *expected,
            actual,
        });
    }
    Ok(dir)
}

/// Deterministic content digest of a directory tree: sorted relative
/// paths, each hashed together with its file contents. Used both to
/// verify an existing snapshot (above) and, symmetrically, by a
/// coordinator preparing one -- kept `pub` so `mini-build-runner-wasmtime`
/// itself is the one place this construction is defined, rather than
/// re-implemented ad hoc by every caller.
pub fn hash_directory_tree(dir: &Path) -> Result<[u8; 32]> {
    let mut entries = Vec::new();
    collect_files(dir, dir, &mut entries)?;
    entries.sort();
    let mut hasher = blake3::Hasher::new();
    for (rel_path, abs_path) in &entries {
        hasher.update(rel_path.as_bytes());
        hasher.update(&[0u8]); // separator: a path can never contain a NUL
        let contents = fs::read(abs_path)?;
        hasher.update(&(contents.len() as u64).to_le_bytes());
        hasher.update(&contents);
    }
    Ok(*hasher.finalize().as_bytes())
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<()> {
    let read_dir = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files(root, &path, out)?;
        } else if file_type.is_file() {
            let rel = path
                .strip_prefix(root)
                .expect("path is under root by construction")
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
        // Symlinks are deliberately neither followed nor hashed: a
        // workspace snapshot is a plain file tree, and a symlink escaping
        // the snapshot root would defeat the digest's own guarantee.
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn a_component_round_trips_through_the_store() {
        let tmp = tempfile::tempdir().unwrap();
        let objects = tmp.path().join("objects");
        fs::create_dir_all(&objects).unwrap();
        let bytes = b"pretend wasm bytes".to_vec();
        let digest: [u8; 32] = blake3::hash(&bytes).into();
        fs::write(objects.join(hex_digest(&digest)), &bytes).unwrap();

        let read_back = read_verified_component(tmp.path(), &digest).unwrap();
        assert_eq!(read_back, bytes);
    }

    #[test]
    fn a_tampered_component_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let objects = tmp.path().join("objects");
        fs::create_dir_all(&objects).unwrap();
        let bytes = b"pretend wasm bytes".to_vec();
        let digest: [u8; 32] = blake3::hash(&bytes).into();
        // Store *different* bytes under the claimed digest's path -- a
        // lying or corrupted store.
        fs::write(objects.join(hex_digest(&digest)), b"tampered").unwrap();

        assert!(matches!(
            read_verified_component(tmp.path(), &digest),
            Err(RunnerError::DigestMismatch { .. })
        ));
    }

    #[test]
    fn workspace_digest_is_independent_of_iteration_order() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("sub")).unwrap();
        fs::File::create(tmp.path().join("a.txt"))
            .unwrap()
            .write_all(b"a")
            .unwrap();
        fs::File::create(tmp.path().join("sub/b.txt"))
            .unwrap()
            .write_all(b"b")
            .unwrap();

        let d1 = hash_directory_tree(tmp.path()).unwrap();
        let d2 = hash_directory_tree(tmp.path()).unwrap();
        assert_eq!(d1, d2);
    }

    #[test]
    fn a_tampered_workspace_snapshot_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let ws_root = tmp.path().join("workspaces");
        let snapshot = ws_root.join("deadbeef");
        fs::create_dir_all(&snapshot).unwrap();
        fs::File::create(snapshot.join("f.txt"))
            .unwrap()
            .write_all(b"original")
            .unwrap();
        let real_digest = hash_directory_tree(&snapshot).unwrap();

        // A coordinator claims a *different* digest than what's on disk.
        let mut wrong = real_digest;
        wrong[0] ^= 1;
        let wrong_hex = hex_digest(&wrong);
        fs::rename(&snapshot, ws_root.join(&wrong_hex)).unwrap();

        assert!(matches!(
            verified_workspace_dir(tmp.path(), &wrong),
            Err(RunnerError::DigestMismatch { .. })
        ));
    }
}
