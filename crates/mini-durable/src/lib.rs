//! Filesystem commit primitives. Success requires both file and directory
//! barriers on Unix and native write-through moves on Windows. Unsupported
//! operations fail closed; directory fsync is not emulated on Windows.
//!
//! Callers must serialize read/modify/write with a stable lock file. Lock
//! files must not be unlinked while in use, including by cleanup tools.
//! These APIs assume an owner-controlled local directory and a filesystem
//! and device that honor OS locking, atomic rename, and flush requests.
//! They do not prevent an administrator restoring/deleting the entire state.

#![cfg_attr(not(windows), forbid(unsafe_code))]

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Commit directory entries on Unix. Windows does not support directory
/// FlushFileBuffers: callers must use the native write-through mutation APIs
/// below. An unrelated raw filesystem mutation cannot be committed this way.
pub fn sync_directory(_path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    let directory = File::open(_path)?;
    #[cfg(unix)]
    return directory.sync_all();
    #[cfg(not(unix))]
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "directory durability is unavailable",
    ))
}

fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

/// Commit the parent directory after creating, removing, or renaming a file.
pub fn sync_parent(path: &Path) -> io::Result<()> {
    sync_directory(parent(path))
}

/// Create missing ancestors and commit each new directory entry before use.
pub fn create_dir_all(path: &Path) -> io::Result<()> {
    match fs::metadata(path) {
        Ok(meta) if meta.is_dir() => return Ok(()),
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                "state parent is not a directory",
            ))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    create_dir_all(parent(path))?;
    #[cfg(windows)]
    {
        // Publish a new empty directory with the same native write-through
        // move used for files, instead of attempting unsupported directory fsync.
        let temp = temporary_path(path)?;
        fs::create_dir(&temp)?;
        let result = move_committed(&temp, path);
        if result.is_err() {
            let _ = fs::remove_dir(&temp);
        }
        result
    }
    #[cfg(not(windows))]
    match fs::create_dir(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists && path.is_dir() => {}
        Err(e) => return Err(e),
    }
    #[cfg(not(windows))]
    sync_parent(path)
}

fn open_lock(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
}

/// Acquire a stable OS lock. The returned file owns the lock until dropped.
pub fn lock_exclusive(path: &Path) -> io::Result<File> {
    let file = open_lock(path)?;
    file.lock()?;
    Ok(file)
}

/// Refuse a second live owner instead of blocking service startup.
pub fn try_lock_exclusive(path: &Path) -> io::Result<File> {
    let file = open_lock(path)?;
    file.try_lock().map_err(io::Error::from)?;
    Ok(file)
}

/// Atomically replace a file, flushing contents before a committed rename. On an error after rename the operation may have committed: callers
/// must fail closed and reload, never issue a conflicting retry from memory.
pub fn atomic_replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = temporary_path(path)?;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        move_committed(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn temporary_path(path: &Path) -> io::Result<std::path::PathBuf> {
    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let filename = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing state filename"))?;
    let mut temp_name = filename.to_os_string();
    temp_name.push(format!(
        ".{}-{}.tmp",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    Ok(parent(path).join(temp_name))
}

/// Publish an already flushed local file or new directory. Windows uses the
/// native WRITE_THROUGH contract; never fall back to a non-atomic copy.
fn move_committed(from: &Path, to: &Path) -> io::Result<()> {
    move_entry(from, to, true)
}

fn move_entry(from: &Path, to: &Path, replace: bool) -> io::Result<()> {
    #[cfg(not(windows))]
    {
        if replace {
            fs::rename(from, to)?;
        } else {
            fs::hard_link(from, to)?;
            sync_parent(to)?;
            fs::remove_file(from)?;
        }
        sync_parent(to)
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        fn wide(path: &Path) -> io::Result<Vec<u16>> {
            let mut value: Vec<_> = path.as_os_str().encode_wide().collect();
            if value.contains(&0) {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"));
            }
            value.push(0);
            Ok(value)
        }
        let from = wide(from)?;
        let to = wide(to)?;
        // SAFETY: both vectors are live NUL-terminated UTF-16 strings for the
        // synchronous call. No pointers escape. OS errors remain hard failures.
        if unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_WRITE_THROUGH
                    | if replace {
                        MOVEFILE_REPLACE_EXISTING
                    } else {
                        0
                    },
            )
        } == 0
        {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

/// Publish a caller-staged and already flushed regular file in the same parent
/// directory. The destination may exist. On failure after publication, callers
/// must reload state because a barrier error can follow a successful rename.
pub fn publish_staged_file(from: &Path, to: &Path) -> io::Result<()> {
    if parent(from) != parent(to) || !fs::symlink_metadata(from)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "staged publication requires a regular file in the destination directory",
        ));
    }
    move_committed(from, to)
}

/// Publish a flushed staged file only when the destination does not exist.
/// No replace/remove fallback is used. As with replacement, a barrier failure
/// may occur after publication and callers must account for that possibility.
pub fn publish_staged_file_new(from: &Path, to: &Path) -> io::Result<()> {
    if parent(from) != parent(to) || !fs::symlink_metadata(from)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "staged publication requires a regular file in the destination directory",
        ));
    }
    move_entry(from, to, false)
}

/// Persist a private, non-authoritative staged filename after its contents were
/// flushed. Windows commits the name through write-through moves. A failure may
/// leave the stage at an internal temporary name, so use only before publishing
/// authoritative state, while original data is still intact.
pub fn persist_staged_file(path: &Path) -> io::Result<()> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "stage is not a regular file",
        ));
    }
    #[cfg(not(windows))]
    {
        sync_parent(path)
    }
    #[cfg(windows)]
    {
        let temp = temporary_path(path)?;
        move_entry(path, &temp, false)?;
        move_entry(&temp, path, false)
    }
}

/// Durably remove the authoritative name. On Windows, write-through rename
/// first moves the entry to a recognized local temporary name; deletion of that
/// obsolete temporary is cleanup. A crash may leave a temp, never the old name.
pub fn remove_file(path: &Path) -> io::Result<()> {
    #[cfg(not(windows))]
    {
        fs::remove_file(path)?;
        sync_parent(path)
    }
    #[cfg(windows)]
    {
        if !fs::symlink_metadata(path)?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "durable removal requires a regular file",
            ));
        }
        let temp = temporary_path(path)?;
        move_committed(path, &temp)?;
        let _ = fs::remove_file(temp);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "mini-durable-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn replaces_and_flushes_real_directory() {
        let root = temp("replace");
        create_dir_all(&root.join("nested")).unwrap();
        let path = root.join("nested/state");
        atomic_replace(&path, b"first").unwrap();
        atomic_replace(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        remove_file(&path).unwrap();
        assert!(!path.exists());
        assert_eq!(
            remove_file(&path).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn staged_publication_replaces_existing_file_and_rejects_cross_directory() {
        let root = temp("staged");
        create_dir_all(&root.join("other")).unwrap();
        let staged = root.join("staged");
        let dest = root.join("state");
        atomic_replace(&dest, b"old").unwrap();
        atomic_replace(&staged, b"new").unwrap();
        assert!(publish_staged_file(&staged, &root.join("other/state")).is_err());
        assert_eq!(fs::read(&dest).unwrap(), b"old");
        publish_staged_file(&staged, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"new");
        assert!(!staged.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn staged_creation_never_clobbers_an_existing_destination() {
        let root = temp("new");
        create_dir_all(&root).unwrap();
        let stage = root.join("stage");
        let dest = root.join("dest");
        atomic_replace(&stage, b"new").unwrap();
        persist_staged_file(&stage).unwrap();
        atomic_replace(&dest, b"owner edit").unwrap();
        assert!(publish_staged_file_new(&stage, &dest).is_err());
        assert_eq!(fs::read(&dest).unwrap(), b"owner edit");
        assert_eq!(fs::read(&stage).unwrap(), b"new");
        remove_file(&dest).unwrap();
        publish_staged_file_new(&stage, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"new");
        assert!(!stage.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn locks_exclude_another_handle_and_release_on_drop() {
        let root = temp("lock");
        create_dir_all(&root).unwrap();
        let lock = try_lock_exclusive(&root.join("lock")).unwrap();
        assert!(try_lock_exclusive(&root.join("lock")).is_err());
        drop(lock);
        drop(try_lock_exclusive(&root.join("lock")).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_replace_preserves_old_file() {
        let root = temp("error");
        create_dir_all(&root).unwrap();
        let path = root.join("state");
        fs::create_dir(&path).unwrap();
        assert!(atomic_replace(&path, b"invalid destination").is_err());
        assert!(path.is_dir());
        fs::remove_dir_all(root).unwrap();
    }
}
