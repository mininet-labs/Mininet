//! Saved peers carry signed collaboration objects over the existing encrypted
//! channel without sharing stores or turning transport into merge authority.
use std::fs;
use std::net::TcpListener;
use std::path::Path;
use std::thread;
use std::time::Duration;

fn run(home: &Path, args: &[&str]) -> String {
    let mut all = vec!["--home".to_string(), home.to_str().unwrap().to_string()];
    all.extend(args.iter().map(|s| s.to_string()));
    mini_cli::run(&all).unwrap_or_else(|e| panic!("{args:?}: {e}"))
}

#[test]
fn saved_peer_transfers_a_proposal_without_canonicalizing_it() {
    let temp = tempfile::tempdir().unwrap();
    let author = temp.path().join("author");
    let reviewer = temp.path().join("reviewer");
    for home in [&author, &reviewer] {
        run(home, &["identity", "init"]);
    }
    let author_kel = run(&author, &["kel", "export"]);
    let reviewer_kel = run(&reviewer, &["kel", "export"]);
    run(&author, &["kel", "trust", &reviewer_kel]);
    run(&reviewer, &["kel", "trust", &author_kel]);
    let initialized = run(&author, &["--json", "repo", "init", "demo"]);
    assert!(initialized.contains("\"project_id\":"));
    let project = fs::read_to_string(author.join("projects/demo")).unwrap();
    let file = temp.path().join("lib.rs");
    fs::write(&file, "pub fn offline() {}\n").unwrap();
    let commit = run(
        &author,
        &[
            "--json",
            "repo",
            "commit",
            "demo",
            "--branch",
            "main",
            "--message",
            "first",
            file.to_str().unwrap(),
        ],
    );
    let head = commit
        .split("\"commit_id\":\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let proposed = run(
        &author,
        &[
            "pr",
            "propose",
            "demo",
            "--branch",
            "main",
            "--title",
            "offline collaboration",
            "--head",
            head,
        ],
    );
    let proposal = proposed.split_whitespace().last().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    drop(listener);
    run(&reviewer, &["remote", "add", "origin", &address]);
    let server_home = author.clone();
    let server_addr = address.clone();
    let server =
        thread::spawn(move || run(&server_home, &["sync", "listen", "--addr", &server_addr]));
    let args = vec![
        "--home".to_string(),
        reviewer.to_str().unwrap().to_string(),
        "--json".into(),
        "remote".into(),
        "sync".into(),
        "origin".into(),
    ];
    let mut result = None;
    for _ in 0..50 {
        match mini_cli::run(&args) {
            Ok(output) => {
                result = Some(output);
                break;
            }
            Err(mini_cli::CliError::Sync(_)) => thread::sleep(Duration::from_millis(20)),
            Err(e) => panic!("remote sync failed: {e}"),
        }
    }
    let output = result.expect("listener did not become ready");
    server.join().unwrap();
    assert!(output.contains("\"kind\":\"remote.sync\""));
    assert!(output.contains("\"canonicalized\":false"));
    let proposals = run(&reviewer, &["--json", "pr", "list", project.trim()]);
    assert!(proposals.contains(proposal));
    let detail = run(&reviewer, &["--json", "pr", "show", proposal]);
    assert!(detail.contains(head));
    assert!(detail.contains("\"approvals\":0"));
    assert!(!author.join("store").is_symlink());
    assert_ne!(author.join("store"), reviewer.join("store"));
}
