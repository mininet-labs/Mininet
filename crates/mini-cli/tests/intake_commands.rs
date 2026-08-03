//! Exercises Mininet Intake plus the Track B5 publication bridge through
//! the CLI's real command-dispatch path (`mini_cli::run`, exactly what the
//! compiled `mini` binary's `main.rs` calls -- not direct
//! `mini_intake`/`mini_intake_social` library calls, but also not a
//! spawned subprocess; matches this crate's other command-group test
//! files' own convention, see D-0078's decision-log entry for the same
//! honest phrasing): intake a local text file, advance its review state,
//! and publish the accepted result as a real signed `mini-social` post --
//! the end-to-end workflow D-0429's own Required follow-up named as still
//! missing before this batch.

use std::fs;
use std::path::PathBuf;

fn tempdir(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "mini-cli-intake-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    path
}

fn run(args: &[&str]) -> String {
    let owned: Vec<String> = args.iter().map(|value| (*value).to_string()).collect();
    mini_cli::run(&owned).unwrap_or_else(|error| panic!("command {args:?} failed: {error}"))
}

fn run_err(args: &[&str]) -> String {
    let owned: Vec<String> = args.iter().map(|value| (*value).to_string()).collect();
    match mini_cli::run(&owned) {
        Ok(output) => panic!("command {args:?} unexpectedly succeeded: {output}"),
        Err(error) => error.to_string(),
    }
}

fn value_after(output: &str, marker: &str) -> String {
    output
        .split(marker)
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .trim_end_matches("...")
        .to_string()
}

fn intake_id_of(output: &str) -> String {
    value_after(output, "intake id ")
}

#[test]
fn add_advance_and_publish_post_end_to_end() {
    let home = tempdir("home");
    let store = tempdir("store");
    let home_str = home.to_str().unwrap().to_string();
    let store_str = store.to_str().unwrap().to_string();

    run(&["--home", &home_str, "identity", "init"]);

    let notes_path = home.join("notes.txt");
    fs::write(&notes_path, "hello from the intake CLI").unwrap();

    let added = run(&[
        "--home",
        &home_str,
        "--store",
        &store_str,
        "intake",
        "add",
        notes_path.to_str().unwrap(),
    ]);
    assert!(added.contains("review_state=Unreviewed"));
    assert!(added.contains("media_type=TextPlain"));
    let id = intake_id_of(&added);

    let shown = run(&[
        "--home", &home_str, "--store", &store_str, "intake", "show", &id,
    ]);
    assert!(shown.contains("review_state=Unreviewed"));
    assert!(shown.contains("links=0"));

    // Publishing before Accepted must be refused, matching
    // mini-intake-social's own NotAccepted check.
    let refused = run_err(&[
        "--home",
        &home_str,
        "--store",
        &store_str,
        "intake",
        "publish-post",
        &id,
    ]);
    assert!(refused.contains("intake error"));

    run(&[
        "--home",
        &home_str,
        "--store",
        &store_str,
        "intake",
        "advance",
        &id,
        "under-review",
    ]);
    let advanced = run(&[
        "--home", &home_str, "--store", &store_str, "intake", "advance", &id, "accepted",
    ]);
    assert!(advanced.contains("UnderReview -> Accepted"));

    let published = run(&[
        "--home",
        &home_str,
        "--store",
        &store_str,
        "intake",
        "publish-post",
        &id,
    ]);
    assert!(published.starts_with("published post "));

    let shown_after = run(&[
        "--home", &home_str, "--store", &store_str, "intake", "show", &id,
    ]);
    assert!(shown_after.contains("review_state=Accepted"));
    assert!(shown_after.contains("links=1"));
}

#[test]
fn publishing_the_same_intake_twice_through_the_real_binary_is_idempotent() {
    let home = tempdir("home-dup");
    let store = tempdir("store-dup");
    let home_str = home.to_str().unwrap().to_string();
    let store_str = store.to_str().unwrap().to_string();

    run(&["--home", &home_str, "identity", "init"]);
    let notes_path = home.join("notes.txt");
    fs::write(&notes_path, "publish me only once").unwrap();
    let added = run(&[
        "--home",
        &home_str,
        "--store",
        &store_str,
        "intake",
        "add",
        notes_path.to_str().unwrap(),
    ]);
    let id = intake_id_of(&added);
    run(&[
        "--home",
        &home_str,
        "--store",
        &store_str,
        "intake",
        "advance",
        &id,
        "under-review",
    ]);
    run(&[
        "--home", &home_str, "--store", &store_str, "intake", "advance", &id, "accepted",
    ]);

    let first = run(&[
        "--home",
        &home_str,
        "--store",
        &store_str,
        "intake",
        "publish-post",
        &id,
    ]);
    assert!(first.starts_with("published post "));

    let second = run(&[
        "--home",
        &home_str,
        "--store",
        &store_str,
        "intake",
        "publish-post",
        &id,
    ]);
    assert!(second.contains("already published as post"));

    let shown = run(&[
        "--home", &home_str, "--store", &store_str, "intake", "show", &id,
    ]);
    assert!(shown.contains("links=1"));
}

#[test]
fn an_illegal_review_transition_is_rejected() {
    let home = tempdir("home-illegal");
    let store = tempdir("store-illegal");
    let home_str = home.to_str().unwrap().to_string();
    let store_str = store.to_str().unwrap().to_string();

    run(&["--home", &home_str, "identity", "init"]);
    let notes_path = home.join("notes.txt");
    fs::write(&notes_path, "cannot skip review").unwrap();
    let added = run(&[
        "--home",
        &home_str,
        "--store",
        &store_str,
        "intake",
        "add",
        notes_path.to_str().unwrap(),
    ]);
    let id = intake_id_of(&added);

    // Unreviewed -> Accepted directly is not a legal transition.
    let refused = run_err(&[
        "--home", &home_str, "--store", &store_str, "intake", "advance", &id, "accepted",
    ]);
    assert!(refused.contains("illegal review transition"));
}

#[test]
fn an_unknown_intake_id_is_a_clean_usage_error() {
    let home = tempdir("home-unknown");
    let store = tempdir("store-unknown");
    let home_str = home.to_str().unwrap().to_string();
    let store_str = store.to_str().unwrap().to_string();
    run(&["--home", &home_str, "identity", "init"]);

    let fake_id = "00".repeat(34); // structurally hex, not a real intake id
    let error = run_err(&[
        "--home", &home_str, "--store", &store_str, "intake", "show", &fake_id,
    ]);
    assert!(error.contains("usage error"));
}

#[test]
fn intake_rejects_the_json_flag() {
    let home = tempdir("home-json");
    let home_str = home.to_str().unwrap().to_string();
    run(&["--home", &home_str, "identity", "init"]);

    let owned = vec![
        "--home".to_string(),
        home_str,
        "--json".to_string(),
        "intake".to_string(),
        "show".to_string(),
        "00".to_string(),
    ];
    let error = mini_cli::run(&owned).unwrap_err();
    assert!(error.to_string().contains("--json is not yet supported"));
}
