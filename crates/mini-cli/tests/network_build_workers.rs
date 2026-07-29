//! Batch 5 distributed-worker proof: a requester sends one exact build over
//! loopback to a separate one-shot worker, which invokes the real Wasmtime
//! runner subprocess and returns digest-bound artifacts.

mod common;

use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::Duration;

fn unique(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "mini-network-build-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn install_runner_next_to_test() {
    let built = common::runner_binary_path();
    let sibling = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join(built.file_name().unwrap());
    if !sibling.exists() {
        fs::copy(built, sibling).unwrap();
    }
}

fn run(args: &[&str]) -> String {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    mini_cli::run(&args).unwrap()
}

#[test]
fn remote_worker_executes_and_requester_verifies_artifact() {
    install_runner_next_to_test();
    let component = common::compile_guest(
        "network_worker",
        r#"fn main() {
            let input = std::fs::read("/workspace/input.txt").unwrap();
            std::fs::write("/artifacts/result.txt", input).unwrap();
        }"#,
    );
    let component_dir = unique("component");
    let workspace = unique("workspace");
    fs::create_dir_all(&component_dir).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    let component_path = component_dir.join("worker.wasm");
    fs::write(&component_path, component).unwrap();
    fs::write(workspace.join("input.txt"), b"verified remote artifact").unwrap();

    let probe = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = probe.local_addr().unwrap().to_string();
    drop(probe);
    let worker_dir = unique("worker");
    let worker_addr = addr.clone();
    let worker_dir_thread = worker_dir.clone();
    let worker = std::thread::spawn(move || {
        run(&[
            "build",
            "serve",
            "--addr",
            &worker_addr,
            "--work-dir",
            worker_dir_thread.to_str().unwrap(),
        ])
    });
    std::thread::sleep(Duration::from_millis(150));

    let output = unique("output");
    let result = run(&[
        "build",
        "dispatch",
        "--peer",
        &addr,
        "--component",
        component_path.to_str().unwrap(),
        "--workspace",
        workspace.to_str().unwrap(),
        "--artifacts-dir",
        output.to_str().unwrap(),
        "--capability",
        "workspace-read",
        "--capability",
        "artifacts-write",
    ]);
    let served = worker.join().unwrap();
    assert!(result.contains("exit_status: Success"), "{result}");
    assert!(served.contains("served one verified build"), "{served}");

    let files: Vec<_> = fs::read_dir(output).unwrap().collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        fs::read(files[0].as_ref().unwrap().path()).unwrap(),
        b"verified remote artifact"
    );
}
