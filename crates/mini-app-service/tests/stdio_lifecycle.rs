//! Installed-host IPC remains responsive while networking is locked/default off.
use mini_app_protocol::{
    read_response, write_request, Command, ErrorCode, Reply, Request, ResponseBody,
};
use std::{
    io::BufReader,
    process::{Command as ProcessCommand, Stdio},
    time::{Duration, Instant},
};
#[test]
fn stdio_host_schedules_idle_without_unlocking_or_enabling_network_and_closes_cleanly() {
    let root = std::env::temp_dir().join(format!(
        "mini-core-host-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut child = ProcessCommand::new(env!("CARGO_BIN_EXE_mininet-app-service"))
        .arg("--stdio")
        .env("MININET_HOME", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let output = child.stdout.take().unwrap();
    let (responses, received) = std::sync::mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let mut output = BufReader::new(output);
        while let Ok(Some(response)) = read_response(&mut output) {
            if responses.send(response).is_err() {
                break;
            }
        }
    });
    for (id, command) in [
        (1, Command::Status),
        (2, Command::SetDeliveryEnabled { enabled: true }),
        (3, Command::DeliveryRuntimeStatus),
    ] {
        write_request(&mut input, &Request::new(id, command)).unwrap();
        let response = received
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|error| {
                let _ = child.kill();
                panic!("IPC host failed to respond: {error}");
            });
        assert_eq!(response.request_id, id);
        match (id, response.body) {
            (1, ResponseBody::Ok(Reply::Status(status))) => {
                assert!(!status.root_created && !status.identity_unlocked)
            }
            (2, ResponseBody::Err(error)) => assert_eq!(error.code, ErrorCode::RootMissing),
            (3, ResponseBody::Ok(Reply::DeliveryRuntime(status))) => {
                assert!(!status.enabled && !status.listening)
            }
            _ => panic!("unexpected host response"),
        }
    }
    // Keep stdin open over an idle tick, then confirm authority did not change.
    std::thread::sleep(Duration::from_millis(275));
    write_request(&mut input, &Request::new(4, Command::DeliveryRuntimeStatus)).unwrap();
    let response = received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(
        matches!(response.body, ResponseBody::Ok(Reply::DeliveryRuntime(status)) if !status.enabled && !status.listening)
    );
    drop(input);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("IPC host did not exit after EOF");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    reader.join().unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
