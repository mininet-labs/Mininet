//! A per-home monotonic counter for the author-scoped `sequence` field
//! every signed object needs. The counter is protected by an OS-backed
//! exclusive lock so separate `mini` processes using the same home cannot
//! allocate the same sequence number.

use std::fs::{self, OpenOptions};
use std::path::Path;

use crate::error::{CliError, Result};
use fs4::FileExt;

fn counter_path(home: &Path) -> std::path::PathBuf {
    home.join("sequence")
}

fn lock_path(home: &Path) -> std::path::PathBuf {
    home.join("sequence.lock")
}

/// The next unused sequence number for this home, persisting the
/// increment before returning it. The lock is held across the complete
/// read-modify-write operation and is released automatically if the process
/// exits or an I/O error returns early.
pub fn next(home: &Path) -> Result<u64> {
    fs::create_dir_all(home).map_err(|e| CliError::Io(e.to_string()))?;

    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(lock_path(home))
        .map_err(|e| CliError::Io(e.to_string()))?;
    lock.lock().map_err(|e| CliError::Io(e.to_string()))?;

    let path = counter_path(home);
    let current: u64 = match fs::read_to_string(&path) {
        Ok(s) => s.trim().parse().unwrap_or(0),
        Err(_) => 0,
    };
    let next = current
        .checked_add(1)
        .ok_or_else(|| CliError::Io("sequence counter exhausted".to_string()))?;
    fs::write(&path, next.to_string()).map_err(|e| CliError::Io(e.to_string()))?;
    Ok(next)
}

/// The current wall-clock time in milliseconds, the `timestamp_ms` every
/// signed object needs.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_numbers_increase_and_persist_across_calls() {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "mini-cli-seq-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert_eq!(next(&p).unwrap(), 1);
        assert_eq!(next(&p).unwrap(), 2);
        assert_eq!(next(&p).unwrap(), 3);
    }

    #[test]
    fn sequence_lock_is_created_and_reused() {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "mini-cli-seq-lock-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        assert_eq!(next(&p).unwrap(), 1);
        assert!(lock_path(&p).is_file());
        assert_eq!(next(&p).unwrap(), 2);
    }

    #[test]
    fn concurrent_allocations_are_unique_and_monotonic() {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "mini-cli-seq-concurrent-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let values = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8).map(|_| scope.spawn(|| next(&p).unwrap())).collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });

        let mut sorted = values;
        sorted.sort_unstable();
        assert_eq!(sorted, (1..=8).collect::<Vec<_>>());
    }
}
