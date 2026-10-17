//! Real socket service lifecycle and authenticated native replication.
use std::io::Write;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

fn command(home: &Path, store: &Path, args: &[&str]) -> Result<String, String> {
    let mut argv = vec![
        "--home".into(),
        home.to_str().unwrap().into(),
        "--store".into(),
        store.to_str().unwrap().into(),
    ];
    argv.extend(args.iter().map(|s| s.to_string()));
    mini_cli::run(&argv).map_err(|e| e.to_string())
}
fn run(home: &Path, store: &Path, args: &[&str]) -> String {
    command(home, store, args).unwrap_or_else(|e| panic!("{args:?}: {e}"))
}
fn field<'a>(text: &'a str, name: &str) -> &'a str {
    let marker = format!("\"{name}\":\"");
    let s = &text[text.find(&marker).unwrap() + marker.len()..];
    &s[..s.find('"').unwrap()]
}
fn wait_ready(home: &Path, store: &Path, old: Option<&str>) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(status) = command(home, store, &["--json", "service", "status"]) {
            if status.contains("\"running\":true") {
                let address = field(&status, "address");
                if !address.is_empty() && Some(address) != old {
                    return address.into();
                }
            }
        }
        assert!(Instant::now() < deadline, "service never became ready");
        thread::sleep(Duration::from_millis(10));
    }
}
fn start(
    home: &Path,
    store: &Path,
    peers: &[String],
) -> thread::JoinHandle<Result<String, String>> {
    let (home, store, peers) = (home.to_path_buf(), store.to_path_buf(), peers.to_vec());
    thread::spawn(move || {
        let mut args = vec![
            "--json",
            "service",
            "run",
            "--addr",
            "127.0.0.1:0",
            "--allow-store-sharing",
            "--idle-timeout-ms",
            "500",
        ];
        for peer in &peers {
            args.extend(["--allow-peer", peer]);
        }
        command(&home, &store, &args)
    })
}
fn setup(temp: &Path, names: &[&str]) -> (Vec<PathBuf>, Vec<PathBuf>, Vec<String>) {
    let homes: Vec<_> = names.iter().map(|n| temp.join(n)).collect();
    let stores: Vec<_> = names
        .iter()
        .map(|n| temp.join(format!("{n}-store")))
        .collect();
    for (home, store) in homes.iter().zip(&stores) {
        run(home, store, &["identity", "init"]);
    }
    let bundles: Vec<_> = homes
        .iter()
        .zip(&stores)
        .map(|(h, s)| run(h, s, &["kel", "export"]))
        .collect();
    for (home, store) in homes.iter().zip(&stores) {
        for bundle in &bundles {
            run(home, store, &["kel", "trust", bundle]);
        }
    }
    let roots = homes
        .iter()
        .map(|h| {
            mini_cli::identity::load(h)
                .unwrap()
                .human_did()
                .as_str()
                .to_string()
        })
        .collect();
    (homes, stores, roots)
}

fn reject_replayed_proof(address: &str, home: &Path, owner: &str) {
    use mini_bearer::{Bearer, Initiator, TcpBearer};
    use mini_objects::{ObjectBuilder, ObjectType, Payload};
    let identity = mini_cli::identity::load(home).unwrap();
    let handshake = || {
        let stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let (initiator, hello) = Initiator::start().unwrap();
        bearer.send(&hello).unwrap();
        let response = bearer.recv().unwrap();
        let channel = initiator.finish(&response).unwrap();
        (bearer, channel, hello, response)
    };
    let (mut bearer, mut channel, hello, response) = handshake();
    let mut transcript = b"mini/forge-session/transcript/v1\0".to_vec();
    transcript.extend_from_slice(&(hello.len() as u32).to_be_bytes());
    transcript.extend_from_slice(&hello);
    transcript.extend_from_slice(&(response.len() as u32).to_be_bytes());
    transcript.extend_from_slice(&response);
    let mut payload = b"mini/forge-session/proof/v1\0".to_vec();
    payload.push(b'i');
    payload.extend_from_slice(&mini_crypto::HashAlgorithm::Blake3.digest(&transcript));
    payload.extend_from_slice(owner.as_bytes());
    let proof = ObjectBuilder::new(ObjectType::Custom("mini/forge-session-v1".into()))
        .payload(Payload::Public(payload))
        .sign(&identity.human_did(), &identity.device)
        .unwrap();
    bearer
        .send(
            &channel
                .seal(&proof.to_bytes(), b"MINI/FORGE-SESSION1")
                .unwrap(),
        )
        .unwrap();
    // First transcript is accepted: responder sends its proof, then waits for
    // sync. Re-encrypting that signed proof on a fresh channel must fail.
    let response = bearer.recv().unwrap();
    channel.open(&response, b"MINI/FORGE-SESSION1").unwrap();
    drop(bearer);
    let (mut bearer, mut channel, _, _) = handshake();
    bearer
        .send(
            &channel
                .seal(&proof.to_bytes(), b"MINI/FORGE-SESSION1")
                .unwrap(),
        )
        .unwrap();
    assert!(
        bearer.recv().is_err(),
        "replayed proof authenticated a fresh session"
    );
}

#[test]
fn authenticated_service_isolates_bad_peers_and_stops_and_restarts() {
    let temp = tempfile::tempdir().unwrap();
    let (homes, stores, roots) = setup(temp.path(), &["owner", "alice", "bob", "denied"]);
    let created = run(
        &homes[1],
        &stores[1],
        &[
            "--json",
            "repo",
            "init",
            "alice-code",
            "--maintainer",
            &roots[1],
            "--min-approvals",
            "1",
        ],
    );
    let project = field(&created, "project_id").to_string();
    assert!(command(
        &homes[0],
        &stores[0],
        &["service", "run", "--allow-peer", &roots[1]]
    )
    .is_err());
    let server = start(&homes[0], &stores[0], &roots[1..3]);
    let address = wait_ready(&homes[0], &stores[0], None);
    assert!(command(
        &homes[0],
        &stores[0],
        &[
            "service",
            "run",
            "--addr",
            "127.0.0.1:0",
            "--allow-store-sharing",
            "--allow-peer",
            &roots[1]
        ]
    )
    .is_err());
    reject_replayed_proof(&address, &homes[1], &roots[0]);
    // An idle peer occupies only one bounded worker; malformed frames die
    // independently and never stop subsequent authenticated sessions.
    let stalled = TcpStream::connect(&address).unwrap();
    let mut malformed = TcpStream::connect(&address).unwrap();
    malformed.write_all(&[0xff; 4]).unwrap();
    drop(malformed);
    for i in [1, 2] {
        run(
            &homes[i],
            &stores[i],
            &["remote", "add", "owner", &address, "--peer", &roots[0]],
        );
        let synced = run(
            &homes[i],
            &stores[i],
            &["--json", "remote", "sync", "owner"],
        );
        assert!(synced.contains("\"ok\":true"));
    }
    // Bob receives Alice's signed project through the persistent owner host.
    let status = run(&homes[2], &stores[2], &["repo", "status", &project]);
    assert!(status.contains("alice-code"));
    // A trusted but non-allowlisted root still cannot sync. A wrong saved
    // endpoint pin cannot fall back to the legacy anonymous protocol.
    run(
        &homes[3],
        &stores[3],
        &["remote", "add", "owner", &address, "--peer", &roots[0]],
    );
    assert!(command(&homes[3], &stores[3], &["remote", "sync", "owner"]).is_err());
    run(
        &homes[1],
        &stores[1],
        &["remote", "add", "wrong", &address, "--peer", &roots[2]],
    );
    assert!(command(&homes[1], &stores[1], &["remote", "sync", "wrong"]).is_err());
    drop(stalled);
    let stopped = run(&homes[0], &stores[0], &["--json", "service", "stop"]);
    assert!(stopped.contains("\"stop_requested\":true"));
    let finished = server.join().unwrap().unwrap();
    assert!(finished.contains("\"running\":false"));
    assert!(
        run(&homes[0], &stores[0], &["--json", "service", "status"]).contains("\"running\":false")
    );
    let restart = start(&homes[0], &stores[0], &roots[1..3]);
    wait_ready(&homes[0], &stores[0], Some(&address));
    run(&homes[0], &stores[0], &["service", "stop"]);
    restart.join().unwrap().unwrap();
}
