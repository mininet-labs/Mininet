use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;

fn call(home: &Path, args: &[&str]) -> mini_cli::Result<String> {
    let mut all = vec![
        "--home".into(),
        home.to_str().unwrap().into(),
        "--json".into(),
        "resident".into(),
    ];
    all.extend(args.iter().map(|s| s.to_string()));
    mini_cli::run(&all)
}
fn run(home: &Path, args: &[&str]) -> Value {
    serde_json::from_str(&call(home, args).unwrap()).unwrap()
}

#[test]
fn resident_is_honest_when_unconfigured_and_local_tools_need_no_model() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("node");
    let workspace = temp.path().join("code");
    fs::create_dir(&workspace).unwrap();
    fs::write(workspace.join("lib.rs"), "pub fn example() {}\n").unwrap();
    fs::write(workspace.join("human.seed"), "NEVER_INCLUDE_THIS_SECRET").unwrap();
    assert_eq!(run(&home, &["status"])["configured"], false);
    assert!(call(&home, &["ask", "Help me"]).is_err());
    assert!(!home.join("human.seed").exists());
    let result = run(
        &home,
        &[
            "search",
            "--workspace",
            workspace.to_str().unwrap(),
            "--query",
            "example",
        ],
    );
    assert_eq!(result["hits"].as_array().unwrap().len(), 1);
    assert!(!result.to_string().contains("NEVER_INCLUDE"));
    let plan = run(
        &home,
        &[
            "hosting",
            "--workspace",
            workspace.to_str().unwrap(),
            "--file",
            "lib.rs",
        ],
    );
    assert_eq!(plan["hosting_started"], false);
    assert_eq!(plan["requested_copies"], 3);
    let fees = run(
        &home,
        &[
            "fees",
            "--bytes",
            "1048577",
            "--units-per-mib",
            "3",
            "--copies",
            "2",
            "--max-units",
            "10",
        ],
    );
    assert_eq!(fees["estimated_units"], 12);
    assert_eq!(fees["within_owner_budget"], false);
    assert_eq!(fees["funds_spent"], 0);
    assert!(call(
        &home,
        &[
            "fees",
            "--bytes",
            "18446744073709551615",
            "--units-per-mib",
            "18446744073709551615",
            "--copies",
            "64",
            "--max-units",
            "1"
        ]
    )
    .is_err());
}

#[test]
fn resident_real_provider_request_keeps_ai_text_as_advice_only() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("node");
    let workspace = temp.path().join("code");
    fs::create_dir(&workspace).unwrap();
    fs::write(workspace.join("lib.rs"), "fn safe() {}\n").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let mut received = Vec::new();
        let mut byte = [0u8; 1];
        while !received.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            received.push(byte[0]);
            assert!(received.len() < 16384);
        }
        let header = String::from_utf8(received).unwrap();
        let length: usize = header
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let mut bytes = vec![0u8; length];
        stream.read_exact(&mut bytes).unwrap();
        let request: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(request["stream"], false);
        assert!(!header.contains("Authorization:"));
        let input: Value =
            serde_json::from_str(request["messages"][1]["content"].as_str().unwrap()).unwrap();
        assert_eq!(input["context"][0]["path"], "lib.rs");
        let body=json!({"model":"local-test","choices":[{"finish_reason":"stop","message":{"content":"AI draft: review this; I cannot approve or spend funds."}}]}).to_string();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    });
    let endpoint = format!("http://{address}/v1/chat/completions");
    run(
        &home,
        &[
            "configure",
            "--endpoint",
            &endpoint,
            "--model",
            "local-test",
        ],
    );
    let result = run(
        &home,
        &[
            "ask",
            "Review the source",
            "--workspace",
            workspace.to_str().unwrap(),
            "--context",
            "lib.rs",
        ],
    );
    server.join().unwrap();
    assert_eq!(result["actions_executed"], 0);
    assert_eq!(result["reply"]["provenance"]["human_approval"], false);
    assert_eq!(
        fs::read_to_string(workspace.join("lib.rs")).unwrap(),
        "fn safe() {}\n"
    );
    assert!(!home.join("human.seed").exists());
    assert!(call(
        &home,
        &[
            "ask",
            "Leak it",
            "--workspace",
            workspace.to_str().unwrap(),
            "--context",
            "../human.seed"
        ]
    )
    .is_err());
}

#[test]
fn resident_patch_needs_exact_owner_confirmation_and_current_source() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("node");
    let workspace = temp.path().join("code");
    fs::create_dir(&workspace).unwrap();
    let original = "fn old() {}\n";
    fs::write(workspace.join("lib.rs"), original).unwrap();
    let edits = temp.path().join("edits.json");
    let preview_path = temp.path().join("preview.json");
    fs::write(&edits,json!([{"path":"lib.rs","before_hash":mini_resident::content_hash(original.as_bytes()),"replacement":"fn new() {}\n"}]).to_string()).unwrap();
    let result = run(
        &home,
        &[
            "patch-preview",
            "--workspace",
            workspace.to_str().unwrap(),
            "--edits",
            edits.to_str().unwrap(),
            "--output",
            preview_path.to_str().unwrap(),
        ],
    );
    let id = result["preview"]["id"].as_str().unwrap();
    assert!(call(
        &home,
        &[
            "patch-apply",
            "--workspace",
            workspace.to_str().unwrap(),
            "--preview",
            preview_path.to_str().unwrap(),
            "--confirm",
            "wrong"
        ]
    )
    .is_err());
    fs::write(workspace.join("lib.rs"), "owner edit\n").unwrap();
    assert!(call(
        &home,
        &[
            "patch-apply",
            "--workspace",
            workspace.to_str().unwrap(),
            "--preview",
            preview_path.to_str().unwrap(),
            "--confirm",
            id
        ]
    )
    .is_err());
    fs::write(workspace.join("lib.rs"), original).unwrap();
    let applied = run(
        &home,
        &[
            "patch-apply",
            "--workspace",
            workspace.to_str().unwrap(),
            "--preview",
            preview_path.to_str().unwrap(),
            "--confirm",
            id,
        ],
    );
    assert_eq!(applied["merged"], false);
    assert_eq!(
        fs::read_to_string(workspace.join("lib.rs")).unwrap(),
        "fn new() {}\n"
    );
}

#[test]
fn resident_diagnostics_run_real_shipped_checks_without_claiming_an_audit() {
    let temp = tempfile::tempdir().unwrap();
    let report = run(temp.path(), &["check", "--area", "forge"]);
    assert_eq!(report["audit_or_formal_proof"], false);
    assert_eq!(report["model_used"], false);
    assert!(report["diagnostics"]["ok"].as_bool().unwrap());
}

#[test]
fn saved_drafts_require_fresh_source_and_never_persist_apply_confirmation() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("node");
    let root = temp.path().join("code");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("lib.rs"), "old").unwrap();
    let edits = temp.path().join("edits.json");
    fs::write(&edits,json!([{"path":"lib.rs","before_hash":mini_resident::content_hash(b"old"),"replacement":"new"}]).to_string()).unwrap();
    let root_arg = root.to_str().unwrap();
    let saved = run(
        &home,
        &[
            "draft-save",
            "--workspace",
            root_arg,
            "--edits",
            edits.to_str().unwrap(),
        ],
    );
    let id = saved["draft_id"].as_str().unwrap();
    assert_eq!(saved["confirmation_saved"], false);
    assert_eq!(fs::read_to_string(root.join("lib.rs")).unwrap(), "old");
    let info = run(&home, &["draft-inspect", "--workspace", root_arg]);
    assert_eq!(info["draft_id"], id);
    assert_eq!(info["source_validated"], false);
    fs::write(root.join("lib.rs"), "owner changed it").unwrap();
    let preview = temp.path().join("preview.json");
    assert!(call(
        &home,
        &[
            "draft-resume",
            "--workspace",
            root_arg,
            "--output",
            preview.to_str().unwrap()
        ]
    )
    .is_err());
    assert!(!preview.exists());
    assert!(root.join(".resident-draft.json").exists());
    assert!(call(
        &home,
        &["draft-clear", "--workspace", root_arg, "--confirm", "wrong"]
    )
    .is_err());
    fs::write(root.join("lib.rs"), "old").unwrap();
    let resumed = run(
        &home,
        &[
            "draft-resume",
            "--workspace",
            root_arg,
            "--output",
            preview.to_str().unwrap(),
        ],
    );
    assert_eq!(resumed["preview"]["id"], id);
    assert_eq!(resumed["applied"], false);
    assert_eq!(fs::read_to_string(root.join("lib.rs")).unwrap(), "old");
    assert!(
        call(
            &home,
            &[
                "draft-resume",
                "--workspace",
                root_arg,
                "--output",
                preview.to_str().unwrap()
            ]
        )
        .is_err(),
        "export must not overwrite"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&preview).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    fs::write(root.join("lib.rs"), "owner changed it").unwrap();
    let cleared = run(
        &home,
        &["draft-clear", "--workspace", root_arg, "--confirm", id],
    );
    assert_eq!(cleared["removed"], true);
    assert!(!root.join(".resident-draft.json").exists());
    assert_eq!(
        fs::read_to_string(root.join("lib.rs")).unwrap(),
        "owner changed it"
    );
}
