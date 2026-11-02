//! Automatic storage housekeeping. Runs at the start of every command (one
//! cheap free-space query) so ordinary users never manage disk space by hand:
//! when the volume holding the node home runs low, regenerable data is dropped
//! (expired seed leases, the rebuildable inventory-verification cache,
//! interrupted-write temporaries) and a plain-language warning goes to stderr.
//! Receipts, keys, journals and anything append-only are never touched.
use mini_durable::space::{self, SpaceLevel};
use std::path::Path;

/// Free what is safely regenerable when space is short; warn the user.
pub(crate) fn housekeeping(home: &Path, now_ms: u64) {
    let level = space::level(home);
    if level == SpaceLevel::Ok {
        return;
    }
    let pool = home.join("seed-pool");
    if pool.is_dir() {
        if let Ok(p) = mini_seed::Pool::open(&pool) {
            let _ = p.prune(now_ms);
        }
        if level == SpaceLevel::Critical {
            let _ = std::fs::remove_file(pool.join("verified.json"));
        }
    }
    if let Some(w) = space::warning(home) {
        eprintln!("warning: {w}");
    }
}

/// `mini storage status`: plain report of free space and level.
pub(crate) fn status(home: &Path) -> String {
    let level = match space::level(home) {
        SpaceLevel::Ok => "ok",
        SpaceLevel::Low => "low",
        SpaceLevel::Critical => "critical",
    };
    match space::available(home) {
        Some(b) => format!("storage: {level}; {} MiB free", b / (1024 * 1024)),
        None => format!("storage: {level}; free space could not be determined"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reports_a_plain_level_and_housekeeping_is_safe_without_a_pool() {
        let dir = std::env::temp_dir();
        let line = status(&dir);
        assert!(line.starts_with("storage: "), "{line}");
        housekeeping(&dir.join("no-such-home"), 0);
    }
}
