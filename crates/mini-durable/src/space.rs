//! Simple free-space guard shared by every durable write.
//!
//! Goals: a full disk must never corrupt state or surprise a non-technical
//! user. Before a durable write the guard checks free space, runs registered
//! *reclaimers* (safe, regenerable clean-ups) if short, and only then fails
//! with a plain [`io::ErrorKind::StorageFull`] error. It also reports a
//! [`SpaceLevel`] so apps can warn *before* writes start failing.
//!
//! The query is best-effort: if the platform cannot report free space the
//! guard fails open (the write proceeds and the OS decides), because blocking
//! all writes on a failed query would itself be an outage.
//!
//! Reclaimers must only delete data that can be regenerated or that is
//! unambiguously garbage (interrupted temporaries, rebuildable caches,
//! expired leases). They must never delete user data, keys, receipts or
//! anything append-only.

use std::io;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

/// Headroom always kept free so small control writes (journals, locks, KEL
/// events) can still commit when bulk data has filled the disk.
pub const RESERVE_BYTES: u64 = 8 * 1024 * 1024;
/// Below this much free space apps should warn the user.
pub const LOW_BYTES: u64 = 256 * 1024 * 1024;
/// Interrupted-write temporaries older than this are garbage.
const STALE_TEMP_AGE: Duration = Duration::from_secs(600);

/// How much headroom remains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpaceLevel {
    /// Less than [`RESERVE_BYTES`] free: bulk writes are refused.
    Critical,
    /// Less than [`LOW_BYTES`] free: warn the user.
    Low,
    /// Plenty of room, or the platform did not report.
    Ok,
}

/// A reclaimer frees space under `dir` and returns the bytes it freed.
pub type Reclaimer = fn(dir: &Path) -> u64;

fn registry() -> &'static Mutex<Vec<Reclaimer>> {
    static R: OnceLock<Mutex<Vec<Reclaimer>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(vec![sweep_stale_temporaries as Reclaimer]))
}

/// Register a safe reclaimer (idempotent per function pointer). Features call
/// this once at startup, e.g. to drop a rebuildable cache.
pub fn register_reclaimer(r: Reclaimer) {
    if let Ok(mut v) = registry().lock() {
        if !v.iter().any(|x| *x as usize == r as usize) {
            v.push(r);
        }
    }
}

/// Free bytes available to this user on the volume holding `path`, if known.
pub fn available(path: &Path) -> Option<u64> {
    let probe = nearest_existing(path)?;
    fs4::available_space(probe).ok()
}

fn nearest_existing(path: &Path) -> Option<&Path> {
    let mut p = Some(path);
    while let Some(x) = p {
        if x.exists() {
            return Some(x);
        }
        p = x.parent();
    }
    None
}

/// Current headroom level for the volume holding `path`.
pub fn level(path: &Path) -> SpaceLevel {
    match available(path) {
        Some(a) if a < RESERVE_BYTES => SpaceLevel::Critical,
        Some(a) if a < LOW_BYTES => SpaceLevel::Low,
        _ => SpaceLevel::Ok,
    }
}

/// A short, plain-language warning for `level`, or `None` when all is well.
pub fn warning(path: &Path) -> Option<String> {
    match level(path) {
        SpaceLevel::Ok => None,
        SpaceLevel::Low => Some(
            "This device is running low on storage. Mininet is freeing what it safely can; \
             remove unused apps or downloads to keep everything working."
                .into(),
        ),
        SpaceLevel::Critical => Some(
            "This device is almost out of storage. Free some space now; Mininet has paused \
             large writes to protect your data and will resume automatically."
                .into(),
        ),
    }
}

/// Make sure `needed` more bytes (plus the reserve) can be written near
/// `path`, running reclaimers if short. Errors with
/// [`io::ErrorKind::StorageFull`] only after reclaiming did not help.
pub fn ensure_space(path: &Path, needed: u64) -> io::Result<()> {
    let dir = path.parent().unwrap_or(path);
    let want = needed.saturating_add(RESERVE_BYTES);
    let Some(have) = available(dir) else {
        return Ok(());
    };
    if have >= want {
        return Ok(());
    }
    let reclaimers: Vec<Reclaimer> = registry().lock().map(|v| v.clone()).unwrap_or_default();
    for r in reclaimers {
        r(dir);
        if available(dir).is_none_or(|a| a >= want) {
            return Ok(());
        }
    }
    Err(io::Error::new(
        io::ErrorKind::StorageFull,
        "no space left on device: free some storage and run the action again; nothing was lost",
    ))
}

/// Built-in reclaimer: delete interrupted atomic-write temporaries
/// (`<name>.<pid>-<seq>.tmp`) older than ten minutes in `dir`.
fn sweep_stale_temporaries(dir: &Path) -> u64 {
    let mut freed = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_atomic_temp(&name) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let old = meta
            .modified()
            .ok()
            .and_then(|m| SystemTime::now().duration_since(m).ok())
            .is_some_and(|age| age >= STALE_TEMP_AGE);
        if meta.is_file() && old && std::fs::remove_file(entry.path()).is_ok() {
            freed += meta.len();
        }
    }
    freed
}

fn is_atomic_temp(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".tmp") else {
        return false;
    };
    let Some((_, suffix)) = stem.rsplit_once('.') else {
        return false;
    };
    suffix.split_once('-').is_some_and(|(pid, seq)| {
        !pid.is_empty()
            && !seq.is_empty()
            && pid.bytes().chain(seq.bytes()).all(|b| b.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_name_recognition_is_strict() {
        assert!(is_atomic_temp("state.json.12-3.tmp"));
        assert!(!is_atomic_temp("state.json"));
        assert!(!is_atomic_temp("notes.tmp"));
        assert!(!is_atomic_temp("a.b-c.tmp"));
    }

    #[test]
    fn plenty_of_space_passes_and_absurd_requests_fail_plainly() {
        let dir = std::env::temp_dir();
        assert!(ensure_space(&dir.join("x"), 1024).is_ok());
        if available(&dir).is_some() {
            let e = ensure_space(&dir.join("x"), u64::MAX / 2).unwrap_err();
            assert_eq!(e.kind(), io::ErrorKind::StorageFull);
        }
        assert!(matches!(
            level(&dir),
            SpaceLevel::Ok | SpaceLevel::Low | SpaceLevel::Critical
        ));
    }

    #[test]
    fn reclaimer_runs_when_short_and_stops_once_satisfied() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        fn counting(_: &Path) -> u64 {
            CALLS.fetch_add(1, Ordering::SeqCst);
            0
        }
        register_reclaimer(counting);
        register_reclaimer(counting);
        let dir = std::env::temp_dir();
        if available(&dir).is_some() {
            let _ = ensure_space(&dir.join("x"), u64::MAX / 2);
            assert!(
                CALLS.load(Ordering::SeqCst) >= 1,
                "reclaimer ran when short"
            );
            let n = registry()
                .lock()
                .unwrap()
                .iter()
                .filter(|x| **x as usize == counting as Reclaimer as usize)
                .count();
            assert_eq!(n, 1, "registration is idempotent");
        }
    }

    #[test]
    fn stale_temporaries_are_swept_but_fresh_ones_and_real_files_are_kept() {
        let dir = std::env::temp_dir().join(format!("mini-space-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fresh = dir.join("a.json.1-1.tmp");
        let real = dir.join("a.json");
        std::fs::write(&fresh, b"x").unwrap();
        std::fs::write(&real, b"x").unwrap();
        assert_eq!(sweep_stale_temporaries(&dir), 0, "fresh temp is kept");
        assert!(fresh.exists() && real.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
