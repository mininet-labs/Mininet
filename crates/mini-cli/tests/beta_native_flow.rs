//! Open-Beta proof: anonymous participant evidence is usable on two completely
//! independent Mininet stores over ordinary verified `mini sync`, with no GitHub
//! transport/identity path in the test.

use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use mini_beta::{
    create_campaign, read_contribution_receipt, read_finding, BetaEpochId, BETA_CONTRIBUTION_TYPE,
    BETA_FINDING_TYPE,
};
use mini_objects::{ObjectBuilder, ObjectType, Payload};

fn tempdir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mini-cli-beta-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    p
}

fn run(args: &[&str]) -> String {
    let owned: Vec<String> = args.iter().map(|value| value.to_string()).collect();
    mini_cli::run(&owned).unwrap_or_else(|error| panic!("command {args:?} failed: {error}"))
}

fn run_with_retry(args: &[&str]) -> String {
    let owned: Vec<String> = args.iter().map(|value| value.to_string()).collect();
    for attempt in 0..50 {
        match mini_cli::run(&owned) {
            Ok(out) => return out,
            Err(error) if attempt < 49 => {
                let _ = error;
                thread::sleep(Duration::from_millis(20));
            }
            Err(error) => panic!("command {args:?} failed after retries: {error}"),
        }
    }
    unreachable!()
}

fn free_loopback_addr() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    addr.to_string()
}

fn hex_decode(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).unwrap();
            u8::from_str_radix(text, 16).unwrap()
        })
        .collect()
}

#[test]
fn anonymous_finding_round_trips_over_verified_sync_without_persistent_author_identity() {
    let alice_home = tempdir("alice-home");
    let alice_store = tempdir("alice-store");
    let bob_home = tempdir("bob-home");
    let bob_store = tempdir("bob-store");

    run(&["--home", alice_home.to_str().unwrap(), "identity", "init"]);
    run(&["--home", bob_home.to_str().unwrap(), "identity", "init"]);

    // Bootstrap/operator campaign publication uses Alice's temporary operational
    // identity. Participant findings below must not reuse it.
    let alice = mini_cli::identity::load(&alice_home).unwrap();
    let mut alice_objects = mini_cli::store::open_store(&alice_store).unwrap();
    let target = ObjectBuilder::new(ObjectType::Custom(
        "mininet.beta/test-release/v1".to_string(),
    ))
    .timestamp_ms(1)
    .sequence(1)
    .payload(Payload::Public(b"open-beta-test-release".to_vec()))
    .sign(&alice.human_did(), &alice.device)
    .unwrap();
    alice_objects.insert(&target).unwrap();
    let epoch = BetaEpochId::new([9; 32]).unwrap();
    let campaign = create_campaign(
        &mut alice_objects,
        &alice.human_did(),
        &alice.device,
        target.id(),
        epoch,
        "native beta",
        "submit privacy-redacted evidence without GitHub",
        &["rust".to_string(), "sync".to_string()],
        10,
        100_000,
        1_000,
        2,
        2,
    )
    .unwrap();
    drop(alice_objects);

    let campaign_id = campaign.id().as_str().to_string();
    let campaign_json = run(&[
        "--json",
        "--home",
        alice_home.to_str().unwrap(),
        "--store",
        alice_store.to_str().unwrap(),
        "beta",
        "campaign",
        "list",
    ]);
    assert!(campaign_json.contains(&campaign_id));
    assert!(campaign_json.contains("\"ok\":true"));

    // A submission without the explicit redaction acknowledgement must fail.
    let unredacted: Vec<String> = [
        "--home",
        alice_home.to_str().unwrap(),
        "--store",
        alice_store.to_str().unwrap(),
        "beta",
        "finding",
        "submit",
        &campaign_id,
        "--evidence-class",
        "rust-toolchain",
        "--severity",
        "medium",
        "--component",
        "mini-cli",
        "--summary",
        "missing redaction ack",
        "--environment",
        "test",
        "--steps",
        "run",
        "--expected",
        "accept only after redaction",
        "--observed",
        "candidate evidence",
        "--evidence",
        "sha256:redacted",
        "--limitations",
        "synthetic test evidence",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    let error = mini_cli::run(&unredacted).unwrap_err();
    assert!(error.to_string().contains("--privacy-redacted"));

    for index in 0..2 {
        let summary = format!("artifact-scoped finding {index}");
        let out = run(&[
            "--json",
            "--home",
            alice_home.to_str().unwrap(),
            "--store",
            alice_store.to_str().unwrap(),
            "beta",
            "finding",
            "submit",
            &campaign_id,
            "--evidence-class",
            "rust-toolchain",
            "--severity",
            "medium",
            "--component",
            "mini-cli",
            "--summary",
            &summary,
            "--environment",
            "rust test",
            "--steps",
            "submit and sync",
            "--expected",
            "verified anonymous artifact",
            "--observed",
            "verified anonymous artifact",
            "--evidence",
            "digest:redacted-fixture",
            "--limitations",
            "does not claim network-layer anonymity",
            "--privacy-redacted",
        ]);
        assert!(out.contains("\"artifact_scoped_author\":true"));
    }

    let alice_objects = mini_cli::store::open_store(&alice_store).unwrap();
    let finding_ids = alice_objects
        .by_type(&ObjectType::Custom(BETA_FINDING_TYPE.to_string()))
        .unwrap();
    assert_eq!(finding_ids.len(), 2);
    let first = read_finding(&alice_objects, &finding_ids[0]).unwrap();
    let second = read_finding(&alice_objects, &finding_ids[1]).unwrap();
    assert_ne!(first.record_author, second.record_author);
    assert_ne!(first.record_author, alice.human_did());
    assert_ne!(second.record_author, alice.human_did());
    drop(alice_objects);

    // Bob explicitly trusts only Alice's operational campaign KEL. The two
    // participant roots are not pre-trusted: their self-certifying KEL carriers
    // must bootstrap through mini-sync's strict ingest path in-band.
    let alice_kel = run(&["--home", alice_home.to_str().unwrap(), "kel", "export"]);
    run(&[
        "--home",
        bob_home.to_str().unwrap(),
        "kel",
        "trust",
        &alice_kel,
    ]);

    let addr = free_loopback_addr();
    let bob_home_string = bob_home.to_str().unwrap().to_string();
    let bob_store_string = bob_store.to_str().unwrap().to_string();
    let listen_addr = addr.clone();
    let server = thread::spawn(move || {
        mini_cli::run(&[
            "--home".to_string(),
            bob_home_string,
            "--store".to_string(),
            bob_store_string,
            "sync".to_string(),
            "listen".to_string(),
            "--addr".to_string(),
            listen_addr,
        ])
        .unwrap()
    });
    let client_report = run_with_retry(&[
        "--home",
        alice_home.to_str().unwrap(),
        "--store",
        alice_store.to_str().unwrap(),
        "sync",
        "connect",
        &addr,
    ]);
    let server_report = server.join().unwrap();
    assert!(client_report.contains("accepted"), "{client_report}");
    assert!(server_report.contains("accepted"), "{server_report}");

    let bob_findings = run(&[
        "--json",
        "--home",
        bob_home.to_str().unwrap(),
        "--store",
        bob_store.to_str().unwrap(),
        "beta",
        "finding",
        "list",
        "--campaign",
        &campaign_id,
    ]);
    assert!(bob_findings.contains(finding_ids[0].as_str()));
    assert!(bob_findings.contains(finding_ids[1].as_str()));
}

#[test]
fn accepted_contribution_stores_only_claim_commitment_not_private_preimage() {
    let home = tempdir("claim-home");
    let store_path = tempdir("claim-store");
    run(&["--home", home.to_str().unwrap(), "identity", "init"]);
    let source_author = did_mini::Controller::incept_single().unwrap();
    let mut store = mini_cli::store::open_store(&store_path).unwrap();
    let source = ObjectBuilder::new(ObjectType::Custom(
        "mininet.beta/test-source/v1".to_string(),
    ))
    .timestamp_ms(1)
    .sequence(1)
    .payload(Payload::Public(b"accepted-work".to_vec()))
    .sign(&source_author.did(), &source_author)
    .unwrap();
    store.insert(&source).unwrap();
    drop(store);

    let out = run(&[
        "--home",
        home.to_str().unwrap(),
        "--store",
        store_path.to_str().unwrap(),
        "beta",
        "contribution",
        "accept",
        source.id().as_str(),
        "--kind",
        "testing",
        "--summary",
        "accepted native beta evidence",
        "--evidence",
        "digest:fixture",
    ]);
    let secret_hex = out
        .lines()
        .find_map(|line| line.strip_prefix("PRIVATE claim secret: "))
        .expect("private claim secret output");
    let commitment_hex = out
        .lines()
        .find_map(|line| line.strip_prefix("public claim commitment: "))
        .expect("claim commitment output");
    assert_ne!(secret_hex, commitment_hex);

    let store = mini_cli::store::open_store(&store_path).unwrap();
    let ids = store
        .by_type(&ObjectType::Custom(BETA_CONTRIBUTION_TYPE.to_string()))
        .unwrap();
    assert_eq!(ids.len(), 1);
    let receipt = read_contribution_receipt(&store, &ids[0]).unwrap();
    assert_eq!(
        commitment_hex,
        receipt
            .claim_tag
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let stored = store.get(&ids[0]).unwrap().to_bytes();
    let secret = hex_decode(secret_hex);
    assert!(
        !stored
            .windows(secret.len())
            .any(|window| window == secret.as_slice()),
        "private claim preimage must never be stored in the public contribution object"
    );
}
