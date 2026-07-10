//! A per-home monotonic counter for the author-scoped `sequence` field
//! every signed object needs. Not concurrency-safe across parallel `mini`
//! invocations against the same home (single-process CLI use only, the
//! same assumption every other local-only file in `<home>` already makes)
//! — a real daemon (`mini-devd`, deferred) would own this properly.

use std::fs;
use std::path::Path;

use crate::error::{CliError, Result};

fn counter_path(home: &Path) -> std::path::PathBuf {
    home.join("sequence")
}

/// The next unused sequence number for this home, persisting the
/// increment before returning it.
pub fn next(home: &Path) -> Result<u64> {
    let path = counter_path(home);
    let current: u64 = match fs::read_to_string(&path) {
        Ok(s) => s.trim().parse().unwrap_or(0),
        Err(_) => 0,
    };
    let next = current + 1;
    fs::create_dir_all(home).map_err(|e| CliError::Io(e.to_string()))?;
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
}
