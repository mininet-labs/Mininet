//! `mini pr list --offset/--page-size`: flag parsing, the fail-closed page
//! bound, and the `next_offset` field. Forge-level paging semantics (stable
//! order, no repeats, work cap) are covered in `mini-forge`'s governance tests.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn tempdir(tag: &str) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut p: PathBuf = std::env::temp_dir();
    p.push(format!(
        "mini-cli-pr-list-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    p.to_str().unwrap().to_string()
}

fn run(args: &[&str]) -> Result<String, String> {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    mini_cli::run(&owned).map_err(|e| e.to_string())
}

struct Setup {
    home: String,
    store: String,
    project: String,
}

fn setup() -> Setup {
    let (home, store) = (tempdir("home"), tempdir("store"));
    run(&["--home", &home, "identity", "init"]).unwrap();
    let init = run(&["--home", &home, "--store", &store, "repo", "init", "paged"]).unwrap();
    let project = init.rsplit(": ").next().unwrap().trim().to_string();
    Setup {
        home,
        store,
        project,
    }
}

fn list(s: &Setup, extra: &[&str], json: bool) -> Result<String, String> {
    let mut args: Vec<&str> = Vec::new();
    if json {
        args.push("--json");
    }
    args.extend([
        "--home", &s.home, "--store", &s.store, "pr", "list", &s.project,
    ]);
    args.extend_from_slice(extra);
    run(&args)
}

#[test]
fn unpaged_and_paged_listing_of_an_empty_project_succeed() {
    let s = setup();
    assert!(list(&s, &[], false).is_ok());
    let paged = list(&s, &["--offset", "0", "--page-size", "10"], true).unwrap();
    assert!(paged.contains("\"proposals\":[]"), "{paged}");
    assert!(paged.contains("\"next_offset\":null"), "{paged}");
}

#[test]
fn bad_paging_flags_are_refused_before_any_work() {
    let s = setup();
    for bad in [
        &["--offset", "abc"][..],
        &["--offset", "-1"][..],
        &["--page-size", "x"][..],
    ] {
        let err = list(&s, bad, false).unwrap_err();
        assert!(err.contains("non-negative integer"), "{err}");
    }
    // The forge bound is enforced, not bypassed by the CLI: zero and oversized
    // pages fail closed.
    assert!(list(&s, &["--page-size", "0"], false).is_err());
    assert!(list(&s, &["--page-size", "1000000"], false).is_err());
}

#[test]
fn unknown_trailing_arguments_are_still_rejected() {
    let s = setup();
    let err = list(&s, &["--offset", "0", "--bogus"], false).unwrap_err();
    assert!(err.contains("unexpected pr list arguments"), "{err}");
}
