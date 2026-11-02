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

/// A housekeeper frees *regenerable* data under `dir` at a safe point (the
/// caller holds no durable-state locks, unlike reclaimers that run inside a
/// write). It is told the current level so it can do more when the device is
/// `Critical`. Housekeepers may take locks and do real work; they must still
/// delete only regenerable or unambiguously garbage data.
pub type Housekeeper = fn(dir: &Path, level: SpaceLevel);

fn housekeepers() -> &'static Mutex<Vec<Housekeeper>> {
    static H: OnceLock<Mutex<Vec<Housekeeper>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(Vec::new()))
}

/// Register a housekeeper (idempotent per function pointer).
pub fn register_housekeeper(h: Housekeeper) {
    if let Ok(mut v) = housekeepers().lock() {
        if !v.iter().any(|x| *x as usize == h as usize) {
            v.push(h);
        }
    }
}

/// Outcome of one [`housekeep`] pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HousekeepReport {
    /// Level before cleaning.
    pub before: SpaceLevel,
    /// Level after cleaning.
    pub after: SpaceLevel,
    /// Free bytes after cleaning, when the platform reports it.
    pub available: Option<u64>,
    /// Best-effort bytes recovered (free-space delta, never negative).
    pub freed_bytes: u64,
}

/// Run every registered housekeeper (and the built-in stale-temporary sweep)
/// when the volume holding `dir` is not `Ok`. Cheap when space is fine: one
/// free-space query. Call it at startup and on a timer from the host.
pub fn housekeep(dir: &Path) -> HousekeepReport {
    let before = level(dir);
    let start = available(dir);
    if before == SpaceLevel::Ok {
        return HousekeepReport {
            before,
            after: before,
            available: start,
            freed_bytes: 0,
        };
    }
    sweep_stale_temporaries(dir);
    let hs: Vec<Housekeeper> = housekeepers().lock().map(|v| v.clone()).unwrap_or_default();
    for h in hs {
        h(dir, before);
    }
    let end = available(dir);
    HousekeepReport {
        before,
        after: level(dir),
        available: end,
        freed_bytes: match (start, end) {
            (Some(a), Some(b)) => b.saturating_sub(a),
            _ => 0,
        },
    }
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
    sweep_tree(dir, 0)
}

/// Same sweep, descending at most four levels (durable writers keep their
/// temporaries beside their own files, e.g. `app-service/outbox`). Symlinks
/// are never followed.
fn sweep_tree(dir: &Path, depth: u8) -> u64 {
    let mut freed = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if meta.is_dir() {
            if depth < 4 {
                freed += sweep_tree(&entry.path(), depth + 1);
            }
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !meta.is_file() || !is_atomic_temp(&name) {
            continue;
        }
        let old = meta
            .modified()
            .ok()
            .and_then(|m| SystemTime::now().duration_since(m).ok())
            .is_some_and(|age| age >= STALE_TEMP_AGE);
        if old && std::fs::remove_file(entry.path()).is_ok() {
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
    fn housekeepers_run_only_when_space_is_not_ok_and_registration_is_idempotent() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static RUNS: AtomicUsize = AtomicUsize::new(0);
        fn counting(_: &Path, _: SpaceLevel) {
            RUNS.fetch_add(1, Ordering::SeqCst);
        }
        register_housekeeper(counting);
        register_housekeeper(counting);
        let dir = std::env::temp_dir();
        let report = housekeep(&dir);
        if report.before == SpaceLevel::Ok {
            assert_eq!(RUNS.load(Ordering::SeqCst), 0, "no work when space is fine");
            assert_eq!(report.freed_bytes, 0);
        }
        let n = housekeepers()
            .lock()
            .unwrap()
            .iter()
            .filter(|x| **x as usize == counting as Housekeeper as usize)
            .count();
        assert_eq!(n, 1);
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
        // Nested temporaries are found; only the old one is removed.
        let nested = dir.join("a/b");
        std::fs::create_dir_all(&nested).unwrap();
        let old_tmp = nested.join("x.json.7-7.tmp");
        std::fs::write(&old_tmp, b"yy").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&old_tmp)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(3600))
            .unwrap();
        assert_eq!(sweep_stale_temporaries(&dir), 2);
        assert!(!old_tmp.exists() && fresh.exists() && real.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
