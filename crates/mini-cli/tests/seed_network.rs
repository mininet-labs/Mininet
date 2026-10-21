//! Actual authenticated peer delivery, retention isolation and pending settlement.
use mini_objects::{ObjectBuilder, ObjectType, Payload};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
fn json_value(result: Value) -> Value {
    serde_json::json!({"result":result})
}
fn cmd(h: &Path, s: &Path, args: &[&str]) -> Result<Value, String> {
    let mut a = vec![
        "--home".into(),
        h.display().to_string(),
        "--store".into(),
        s.display().to_string(),
        "--json".into(),
    ];
    a.extend(args.iter().map(|s| s.to_string()));
    mini_cli::run(&a)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
}
fn run(h: &Path, s: &Path, args: &[&str]) -> Value {
    cmd(h, s, args).unwrap_or_else(|e| panic!("{args:?}: {e}"))
}
fn plain(h: &Path, s: &Path, args: &[&str]) -> String {
    let mut a = vec![
        "--home".into(),
        h.display().to_string(),
        "--store".into(),
        s.display().to_string(),
    ];
    a.extend(args.iter().map(|s| s.to_string()));
    mini_cli::run(&a).unwrap()
}
fn setup(root: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let homes: Vec<_> = ["host", "reader", "relay"]
        .iter()
        .map(|n| root.join(n))
        .collect();
    let stores: Vec<_> = ["host-store", "reader-store", "relay-store"]
        .iter()
        .map(|n| root.join(n))
        .collect();
    for (h, s) in homes.iter().zip(&stores) {
        plain(h, s, &["identity", "init"]);
        run(h, s, &["seed", "configure", "--max-bytes", "1048576"]);
    }
    let bundles: Vec<_> = homes
        .iter()
        .zip(&stores)
        .map(|(h, s)| plain(h, s, &["kel", "export"]))
        .collect();
    for (h, s) in homes.iter().zip(&stores) {
        for b in &bundles {
            plain(h, s, &["kel", "trust", b]);
        }
    }
    (homes, stores)
}
fn root(h: &Path) -> String {
    mini_cli::identity::load(h).unwrap().human_did().to_string()
}
fn object(h: &Path, s: &Path, text: &str) -> String {
    let identity = mini_cli::identity::load(h).unwrap();
    let o = ObjectBuilder::new(ObjectType::Custom("seed-test".into()))
        .payload(Payload::Public(text.as_bytes().to_vec()))
        .sign(&identity.human_did(), &identity.device)
        .unwrap();
    mini_cli::store::open_store(s).unwrap().insert(&o).unwrap();
    o.id().as_str().to_string()
}
struct Service {
    h: PathBuf,
    s: PathBuf,
    address: String,
    thread: Option<thread::JoinHandle<Result<Value, String>>>,
}
impl Service {
    fn start(h: &Path, s: &Path, peers: &[String]) -> Self {
        let (h, s, p) = (h.to_path_buf(), s.to_path_buf(), peers.to_vec());
        let (h2, s2) = (h.clone(), s.clone());
        let task = thread::spawn(move || {
            let mut a = vec![
                "seed",
                "serve",
                "--addr",
                "127.0.0.1:0",
                "--idle-timeout-ms",
                "1000",
            ];
            for peer in &p {
                a.extend(["--allow-peer", peer]);
            }
            cmd(&h2, &s2, &a)
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let address = loop {
            if let Ok(v) = cmd(&h, &s, &["service", "status"]) {
                if v["running"] == true {
                    assert_eq!(v["scope"], "seed-pool");
                    break v["address"].as_str().unwrap().to_string();
                }
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        };
        Self {
            h,
            s,
            address,
            thread: Some(task),
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        let _ = cmd(&self.h, &self.s, &["service", "stop"]);
        if let Some(t) = self.thread.take() {
            assert!(t.join().unwrap().is_ok());
        }
    }
}
fn add(h: &Path, s: &Path, id: &str) {
    run(
        h,
        s,
        &[
            "seed",
            "add",
            "--id",
            id,
            "--days",
            "3",
            "--allow-public-seeding",
        ],
    );
}
#[test]
fn selected_history_is_served_reseeded_and_revoked_without_source_loss() {
    let temp = tempfile::tempdir().unwrap();
    let (h, s) = setup(temp.path());
    let id = object(&h[0], &s[0], "public history revision");
    let hidden = object(&h[0], &s[0], "not selected");
    assert!(cmd(&h[0], &s[0], &["seed", "add", "--id", &id, "--days", "3"]).is_err());
    add(&h[0], &s[0], &id);
    let service = Service::start(&h[0], &s[0], &[root(&h[1])]);
    let inventory = run(
        &h[1],
        &s[1],
        &[
            "seed",
            "peers",
            "--addr",
            &service.address,
            "--peer",
            &root(&h[0]),
        ],
    );
    assert_eq!(inventory["result"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(inventory["result"]["items"][0]["id"], id);
    assert!(cmd(
        &h[1],
        &s[1],
        &[
            "seed",
            "fetch",
            "--addr",
            &service.address,
            "--peer",
            &root(&h[0]),
            "--id",
            &hidden
        ]
    )
    .is_err());
    let got = run(
        &h[1],
        &s[1],
        &[
            "seed",
            "fetch",
            "--addr",
            &service.address,
            "--peer",
            &root(&h[0]),
            "--id",
            &id,
            "--reseed-days",
            "2",
        ],
    );
    assert_eq!(got["result"]["delivery_verified"], true);
    assert_eq!(got["result"]["payment_final"], false);
    let earned = run(&h[0], &s[0], &["seed", "earnings"]);
    assert_eq!(earned["result"]["deliveries"].as_array().unwrap().len(), 1);
    let relay = Service::start(&h[1], &s[1], &[root(&h[2])]);
    run(
        &h[2],
        &s[2],
        &[
            "seed",
            "fetch",
            "--addr",
            &relay.address,
            "--peer",
            &root(&h[1]),
            "--id",
            &id,
        ],
    );
    let next = object(&h[0], &s[0], "next history revision");
    add(&h[0], &s[0], &next);
    let followed = run(
        &h[1],
        &s[1],
        &[
            "seed",
            "follow",
            "--addr",
            &service.address,
            "--peer",
            &root(&h[0]),
            "--days",
            "2",
            "--rounds",
            "1",
            "--interval-ms",
            "1000",
            "--max-objects",
            "2",
        ],
    );
    assert_eq!(followed["result"]["transfers"].as_array().unwrap().len(), 1);
    let again = run(
        &h[1],
        &s[1],
        &[
            "seed",
            "follow",
            "--addr",
            &service.address,
            "--peer",
            &root(&h[0]),
            "--days",
            "2",
            "--rounds",
            "1",
            "--interval-ms",
            "1000",
            "--max-objects",
            "2",
        ],
    );
    assert!(again["result"]["transfers"].as_array().unwrap().is_empty());
    run(&h[0], &s[0], &["seed", "revoke", "--id", &id]);
    assert!(cmd(
        &h[1],
        &s[1],
        &[
            "seed",
            "fetch",
            "--addr",
            &service.address,
            "--peer",
            &root(&h[0]),
            "--id",
            &id
        ]
    )
    .is_err());
    assert!(mini_cli::store::open_store(&s[0])
        .unwrap()
        .contains(&mini_objects::ObjectId::parse(&id).unwrap())
        .unwrap());
}
#[test]
fn paid_delivery_exports_real_pending_claim_and_rejects_replay_and_budget() {
    let temp = tempfile::tempdir().unwrap();
    let (h, s) = setup(temp.path());
    let id = object(&h[0], &s[0], "paid retained data");
    add(&h[0], &s[0], &id);
    let payee = [22u8; 32];
    let network = [9u8; 32];
    fn hex(b: &[u8]) -> String {
        b.iter().map(|b| format!("{b:02x}")).collect()
    }
    run(
        &h[0],
        &s[0],
        &[
            "seed",
            "price",
            "--micro-per-mib",
            "7",
            "--payee-hex",
            &hex(&payee),
            "--network-hex",
            &hex(&network),
        ],
    );
    let provider = Service::start(&h[0], &s[0], &[root(&h[1])]);
    let quote = run(
        &h[1],
        &s[1],
        &[
            "seed",
            "quote",
            "--addr",
            &provider.address,
            "--peer",
            &root(&h[0]),
            "--id",
            &id,
        ],
    );
    assert_eq!(quote["result"]["amount_micro"], 7);
    let key = mini_crypto::SigningKey::from_seed(&[33u8; 32]);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let claim = mini_settlement::sign_claim_for_network(
        &key,
        &payee,
        7,
        1,
        now + 60_000,
        &network,
        b"observed-test-chain",
        now,
    )
    .unwrap();
    let path = temp.path().join("wallet.claim");
    std::fs::write(&path, claim.to_wire_bytes().unwrap()).unwrap();
    assert!(cmd(
        &h[1],
        &s[1],
        &[
            "seed",
            "fetch",
            "--addr",
            &provider.address,
            "--peer",
            &root(&h[0]),
            "--id",
            &id,
            "--max-micro",
            "6",
            "--payment-claim",
            path.to_str().unwrap()
        ]
    )
    .is_err());
    let result = run(
        &h[1],
        &s[1],
        &[
            "seed",
            "fetch",
            "--addr",
            &provider.address,
            "--peer",
            &root(&h[0]),
            "--id",
            &id,
            "--max-micro",
            "7",
            "--payment-claim",
            path.to_str().unwrap(),
        ],
    );
    assert_eq!(result["result"]["payment_state"], "AcceptedLocal");
    let out = temp.path().join("submission.claim");
    run(
        &h[0],
        &s[0],
        &[
            "seed",
            "payment-export",
            "--receipt",
            result["result"]["receipt_id"].as_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ],
    );
    assert_eq!(
        mini_settlement::PaymentClaim::from_wire_bytes(&std::fs::read(out).unwrap()).unwrap(),
        claim
    );
    assert!(cmd(
        &h[1],
        &s[1],
        &[
            "seed",
            "fetch",
            "--addr",
            &provider.address,
            "--peer",
            &root(&h[0]),
            "--id",
            &id,
            "--max-micro",
            "7",
            "--payment-claim",
            path.to_str().unwrap()
        ]
    )
    .is_err());
    run(
        &h[1],
        &s[1],
        &[
            "seed",
            "contact",
            "add",
            "--name",
            "paid",
            "--addr",
            &provider.address,
            "--peer",
            &root(&h[0]),
        ],
    );
    let search = run(
        &h[1],
        &s[1],
        &["seed", "search", "--query", "retained", "--contact", "paid"],
    );
    assert!(search["result"]["results"].as_array().unwrap().is_empty());
    assert_eq!(search["result"]["failures"].as_array().unwrap().len(), 1);
    let earnings = run(&h[0], &s[0], &["seed", "earnings"]);
    assert!(earnings["result"]["finalized_micro"].is_null());
}
#[test]
fn private_cold_untrusted_and_unauthorized_content_stays_out() {
    let temp = tempfile::tempdir().unwrap();
    let (h, s) = setup(temp.path());
    let id = object(&h[0], &s[0], "cold data");
    mini_cli::store::open_store(&s[0])
        .unwrap()
        .set_cache_tier(
            &mini_objects::ObjectId::parse(&id).unwrap(),
            mini_store::CacheTier::ColdArchive,
        )
        .unwrap();
    assert!(cmd(
        &h[0],
        &s[0],
        &[
            "seed",
            "add",
            "--id",
            &id,
            "--days",
            "1",
            "--allow-public-seeding"
        ]
    )
    .is_err());
    let outsider = temp.path().join("outsider");
    plain(&outsider, &s[0], &["identity", "init"]);
    let unknown = object(&outsider, &s[0], "unknown author");
    assert!(cmd(
        &h[0],
        &s[0],
        &[
            "seed",
            "add",
            "--id",
            &unknown,
            "--days",
            "1",
            "--allow-public-seeding"
        ]
    )
    .is_err());
    let public = object(&h[0], &s[0], "approved");
    add(&h[0], &s[0], &public);
    let provider = Service::start(&h[0], &s[0], &[root(&h[1])]);
    assert!(cmd(
        &h[2],
        &s[2],
        &[
            "seed",
            "peers",
            "--addr",
            &provider.address,
            "--peer",
            &root(&h[0])
        ]
    )
    .is_err());
    assert!(cmd(
        &h[1],
        &s[1],
        &[
            "seed",
            "peers",
            "--addr",
            &provider.address,
            "--peer",
            &root(&h[2])
        ]
    )
    .is_err());
}

#[test]
fn completed_delivery_keeps_receipt_when_reseed_quota_is_full() {
    let temp = tempfile::tempdir().unwrap();
    let (h, s) = setup(temp.path());
    let id = object(&h[0], &s[0], "delivered regardless of optional reseeding");
    add(&h[0], &s[0], &id);
    run(&h[1], &s[1], &["seed", "configure", "--max-bytes", "0"]);
    let provider = Service::start(&h[0], &s[0], &[root(&h[1])]);
    let got = run(
        &h[1],
        &s[1],
        &[
            "seed",
            "fetch",
            "--addr",
            &provider.address,
            "--peer",
            &root(&h[0]),
            "--id",
            &id,
            "--reseed-days",
            "1",
        ],
    );
    assert_eq!(got["result"]["delivery_verified"], true);
    assert_eq!(got["result"]["reseeded"], false);
    assert!(got["result"]["reseed_error"].is_string());
    assert_eq!(
        run(&h[0], &s[0], &["seed", "earnings"])["result"]["deliveries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn signed_provider_catalog_survives_restart_and_retrieval_falls_back() {
    let temp = tempfile::tempdir().unwrap();
    let (h, s) = setup(temp.path());
    let id = object(
        &h[0],
        &s[0],
        "history available on independent configured endpoints",
    );
    add(&h[0], &s[0], &id);
    let first = Service::start(&h[0], &s[0], &[root(&h[1]), root(&h[2])]);
    run(
        &h[1],
        &s[1],
        &[
            "seed",
            "fetch",
            "--addr",
            &first.address,
            "--peer",
            &root(&h[0]),
            "--id",
            &id,
            "--reseed-days",
            "3",
        ],
    );
    let second = Service::start(&h[1], &s[1], &[root(&h[2])]);
    for (index, address, name) in [
        (0, &first.address, "a-primary"),
        (1, &second.address, "b-backup"),
    ] {
        run(
            &h[index],
            &s[index],
            &["seed", "announce", "--addr", address, "--ttl-ms", "60000"],
        );
        run(
            &h[2],
            &s[2],
            &[
                "seed",
                "contact",
                "add",
                "--name",
                name,
                "--addr",
                address,
                "--peer",
                &root(&h[index]),
            ],
        );
    }
    let refreshed = run(&h[2], &s[2], &["seed", "catalog", "refresh"]);
    assert_eq!(refreshed["result"]["complete"], true);
    let located = run(&h[2], &s[2], &["seed", "locate", "--id", &id]);
    assert_eq!(located["result"]["candidates"].as_array().unwrap().len(), 2);
    assert_eq!(located["result"]["availability_proven"], false);
    drop(first);
    let followed = run(
        &h[2],
        &s[2],
        &[
            "seed",
            "mesh-follow",
            "--days",
            "2",
            "--rounds",
            "1",
            "--interval-ms",
            "1000",
            "--max-objects",
            "1",
        ],
    );
    assert_eq!(followed["result"]["automatic_paid_requests"], 0);
    let got = json_value(followed["result"]["transfers"][0].clone());
    assert_eq!(got["result"]["retrieved"], true);
    assert_eq!(got["result"]["contact"], "b-backup");
    assert_eq!(got["result"]["failures"].as_array().unwrap().len(), 1);
    run(&h[2], &s[2], &["seed", "follow-stop"]);
    let stopped = run(
        &h[2],
        &s[2],
        &[
            "seed",
            "mesh-follow",
            "--days",
            "2",
            "--rounds",
            "1",
            "--interval-ms",
            "1000",
            "--max-objects",
            "1",
        ],
    );
    assert_eq!(stopped["result"]["stop_requested"], true);
    assert!(stopped["result"]["transfers"]
        .as_array()
        .unwrap()
        .is_empty());
    let resumed = run(
        &h[2],
        &s[2],
        &[
            "seed",
            "mesh-follow",
            "--days",
            "2",
            "--rounds",
            "1",
            "--interval-ms",
            "1000",
            "--max-objects",
            "1",
            "--resume",
        ],
    );
    assert_eq!(resumed["result"]["stop_requested"], false);
    assert!(resumed["result"]["transfers"]
        .as_array()
        .unwrap()
        .is_empty());
    run(&h[1], &s[1], &["seed", "revoke", "--id", &id]);
    run(
        &h[2],
        &s[2],
        &["seed", "catalog", "refresh", "--contact", "b-backup"],
    );
    run(
        &h[2],
        &s[2],
        &["seed", "contact", "remove", "--name", "a-primary"],
    );
    assert!(
        run(&h[2], &s[2], &["seed", "locate", "--id", &id])["result"]["candidates"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        run(&h[2], &s[2], &["seed", "retrieve", "--id", &id])["result"]["retrieved"],
        false
    );
}

#[test]
fn contact_activation_and_public_announcement_are_explicit() {
    let temp = tempfile::tempdir().unwrap();
    let (h, s) = setup(temp.path());
    assert!(cmd(
        &h[0],
        &s[0],
        &[
            "seed",
            "contact",
            "add",
            "--name",
            "bad",
            "--addr",
            "0.0.0.0:42",
            "--peer",
            &root(&h[1])
        ]
    )
    .is_err());
    run(
        &h[0],
        &s[0],
        &[
            "seed",
            "contact",
            "add",
            "--name",
            "peer",
            "--addr",
            "127.0.0.1:1",
            "--peer",
            &root(&h[1]),
        ],
    );
    assert!(cmd(
        &h[0],
        &s[0],
        &[
            "seed",
            "contact",
            "add",
            "--name",
            "peer",
            "--addr",
            "127.0.0.1:2",
            "--peer",
            &root(&h[1])
        ]
    )
    .is_err());
    let server = Service::start(&h[1], &s[1], &[root(&h[2])]);
    run(
        &h[2],
        &s[2],
        &[
            "seed",
            "contact",
            "add",
            "--name",
            "unannounced",
            "--addr",
            &server.address,
            "--peer",
            &root(&h[1]),
        ],
    );
    let report = run(&h[2], &s[2], &["seed", "catalog", "refresh"]);
    assert_eq!(report["result"]["complete"], false);
    assert_eq!(report["result"]["refreshed"].as_array().unwrap().len(), 0);
    assert!(
        run(&h[2], &s[2], &["seed", "catalog", "list"])["result"]["providers"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn verified_search_deduplicates_providers_and_returns_continuations() {
    let temp = tempfile::tempdir().unwrap();
    let (h, s) = setup(temp.path());
    let mut ids = Vec::new();
    for text in [
        "nebula history one",
        "nebula history two",
        "nebula history three",
    ] {
        let id = object(&h[0], &s[0], text);
        add(&h[0], &s[0], &id);
        ids.push(id);
    }
    let hidden = object(&h[0], &s[0], "nebula private unselected history");
    let a = Service::start(&h[0], &s[0], &[root(&h[1]), root(&h[2])]);
    run(
        &h[1],
        &s[1],
        &[
            "seed",
            "fetch",
            "--addr",
            &a.address,
            "--peer",
            &root(&h[0]),
            "--id",
            &ids[0],
            "--reseed-days",
            "2",
        ],
    );
    let b = Service::start(&h[1], &s[1], &[root(&h[2])]);
    for (name, addr, peer) in [
        ("a", a.address.as_str(), root(&h[0])),
        ("b", b.address.as_str(), root(&h[1])),
        ("z-offline", "127.0.0.1:1", root(&h[0])),
    ] {
        run(
            &h[2],
            &s[2],
            &[
                "seed", "contact", "add", "--name", name, "--addr", addr, "--peer", &peer,
            ],
        );
    }
    let found = run(&h[2], &s[2], &["seed", "search", "--query", "nebula"]);
    let results = found["result"]["results"].as_array().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(found["result"]["paid_ranking"], false);
    assert_eq!(found["result"]["complete"], false);
    assert_eq!(found["result"]["failures"].as_array().unwrap().len(), 1);
    assert!(results.iter().all(|r| r["content_verified"] == true
        && r["excerpt"].as_str().unwrap().contains("nebula")
        && r["object_id"] != hidden));
    assert_eq!(
        results.iter().find(|r| r["object_id"] == ids[0]).unwrap()["contacts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let first = run(
        &h[2],
        &s[2],
        &[
            "seed",
            "search",
            "--query",
            "nebula",
            "--contact",
            "a",
            "--limit",
            "1",
        ],
    );
    let cursor = first["result"]["continuations"][0]["after"]
        .as_str()
        .unwrap();
    let next = run(
        &h[2],
        &s[2],
        &[
            "seed",
            "search",
            "--query",
            "nebula",
            "--contact",
            "a",
            "--limit",
            "1",
            "--after",
            cursor,
        ],
    );
    assert_eq!(next["result"]["results"].as_array().unwrap().len(), 1);
    assert!(next["result"]["results"][0]["object_id"].as_str().unwrap() > cursor);
    assert!(cmd(
        &h[2],
        &s[2],
        &["seed", "search", "--query", "nebula", "--after", cursor]
    )
    .is_err());
    assert!(cmd(&h[2], &s[2], &["seed", "search", "--query", "\u{1b}danger"]).is_err());
}

#[test]
fn catalog_relay_preserves_provider_signatures_without_activating_endpoints() {
    let temp = tempfile::tempdir().unwrap();
    let (h, s) = setup(temp.path());
    let id = object(&h[0], &s[0], "relay-discovered history");
    add(&h[0], &s[0], &id);
    let a = Service::start(&h[0], &s[0], &[root(&h[1]), root(&h[2])]);
    run(
        &h[0],
        &s[0],
        &[
            "seed", "announce", "--addr", &a.address, "--ttl-ms", "60000",
        ],
    );
    run(
        &h[1],
        &s[1],
        &[
            "seed",
            "contact",
            "add",
            "--name",
            "origin",
            "--addr",
            &a.address,
            "--peer",
            &root(&h[0]),
        ],
    );
    run(&h[1], &s[1], &["seed", "catalog", "refresh"]);
    let b = Service::start(&h[1], &s[1], &[root(&h[2])]);
    run(
        &h[2],
        &s[2],
        &[
            "seed",
            "contact",
            "add",
            "--name",
            "bootstrap",
            "--addr",
            &b.address,
            "--peer",
            &root(&h[1]),
        ],
    );
    let report = run(
        &h[2],
        &s[2],
        &["seed", "catalog", "pull", "--contact", "bootstrap"],
    );
    assert_eq!(report["result"]["endpoints_activated"], 0);
    assert_eq!(
        report["result"]["reports"][0]["report"]["accepted"][0],
        root(&h[0])
    );
    assert_eq!(
        run(&h[2], &s[2], &["seed", "catalog", "list"])["result"]["providers"][0]["provider"],
        root(&h[0])
    );
    assert!(
        run(&h[2], &s[2], &["seed", "locate", "--id", &id])["result"]["candidates"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    run(
        &h[2],
        &s[2],
        &[
            "seed",
            "contact",
            "add",
            "--name",
            "origin",
            "--addr",
            &a.address,
            "--peer",
            &root(&h[0]),
        ],
    );
    assert_eq!(
        run(&h[2], &s[2], &["seed", "retrieve", "--id", &id])["result"]["retrieved"],
        true
    );
    let file = temp.path().join("advertisement.object");
    run(
        &h[1],
        &s[1],
        &[
            "seed",
            "catalog",
            "export",
            "--peer",
            &root(&h[0]),
            "--output",
            file.to_str().unwrap(),
        ],
    );
    run(
        &h[2],
        &s[2],
        &[
            "seed",
            "catalog",
            "import",
            "--file",
            file.to_str().unwrap(),
        ],
    );
    let mut bytes = std::fs::read(&file).unwrap();
    let index = bytes.len() - 1;
    bytes[index] ^= 1;
    std::fs::write(&file, bytes).unwrap();
    assert!(cmd(
        &h[2],
        &s[2],
        &[
            "seed",
            "catalog",
            "import",
            "--file",
            file.to_str().unwrap()
        ]
    )
    .is_err());
    run(&h[0], &s[0], &["seed", "unannounce"]);
    assert_eq!(
        run(&h[1], &s[1], &["seed", "catalog", "refresh"])["result"]["complete"],
        false
    );
}

fn seed_collection(h: &Path, s: &Path, root: &str, kind: &str, file: &Path) -> Value {
    let p = run(
        h,
        s,
        &[
            "seed",
            "collection-preview",
            "--root",
            root,
            "--kind",
            kind,
            "--days",
            "3",
            "--output",
            file.to_str().unwrap(),
        ],
    );
    run(
        h,
        s,
        &[
            "seed",
            "collection-add",
            "--preview",
            file.to_str().unwrap(),
            "--confirm",
            p["result"]["preview_id"].as_str().unwrap(),
        ],
    )
}
#[test]
fn media_collections_resume_with_attempt_and_byte_windows_and_export_exact_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let (h, s) = setup(temp.path());
    run(
        &h[0],
        &s[0],
        &["seed", "configure", "--max-bytes", "8388608"],
    );
    let identity = mini_cli::identity::load(&h[0]).unwrap();
    let mut source = mini_cli::store::open_store(&s[0]).unwrap();
    let mut bytes = vec![17u8; mini_media::CHUNK_SIZE];
    bytes.extend_from_slice(b"remaining tail bytes");
    let manifest = mini_media::publish_media(
        &mut source,
        &identity.human_did(),
        &identity.device,
        "application/octet-stream",
        &bytes,
        0,
        1,
    )
    .unwrap();
    let root = manifest.id.as_str();
    let admission = seed_collection(
        &h[0],
        &s[0],
        root,
        "media",
        &temp.path().join("media-preview.json"),
    );
    assert_eq!(admission["result"]["admitted"], 3);
    let service = Service::start(&h[0], &s[0], &[root_id(&h[1])]);
    run(
        &h[0],
        &s[0],
        &[
            "seed",
            "announce",
            "--addr",
            &service.address,
            "--ttl-ms",
            "60000",
        ],
    );
    run(
        &h[1],
        &s[1],
        &[
            "seed",
            "contact",
            "add",
            "--name",
            "origin",
            "--addr",
            &service.address,
            "--peer",
            &root_id(&h[0]),
        ],
    );
    run(&h[1], &s[1], &["seed", "catalog", "refresh"]);
    let args = [
        "seed",
        "collection-fetch",
        "--root",
        root,
        "--kind",
        "media",
        "--max-objects",
        "16",
        "--max-bytes",
        "4194304",
        "--max-downloads",
        "1",
    ];
    let first = run(&h[1], &s[1], &args);
    assert_eq!(first["result"]["complete"], false);
    assert_eq!(first["result"]["attempts_used"], 1);
    assert_eq!(first["result"]["downloaded"].as_array().unwrap().len(), 1);
    let root_size = source.get(&manifest.id).unwrap().to_bytes().len() + 1;
    let small = run(
        &h[1],
        &s[1],
        &[
            "seed",
            "collection-fetch",
            "--root",
            root,
            "--kind",
            "media",
            "--max-objects",
            "16",
            "--max-bytes",
            &root_size.to_string(),
            "--max-downloads",
            "1",
        ],
    );
    assert_eq!(small["result"]["complete"], false);
    assert_eq!(small["result"]["received_body_bytes"], 0);
    assert!(small["result"]["downloaded"].as_array().unwrap().is_empty());
    let second = run(&h[1], &s[1], &args);
    assert_eq!(second["result"]["complete"], false);
    assert_eq!(second["result"]["reused"], 1);
    let third = run(&h[1], &s[1], &args);
    assert_eq!(third["result"]["complete"], true);
    assert_eq!(third["result"]["reused"], 2);
    assert_eq!(third["result"]["seeded"], false);
    assert!(mini_seed::Pool::open(&h[1].join("seed-pool"))
        .unwrap()
        .inventory(u64::MAX / 2)
        .unwrap()
        .is_empty());
    let output = temp.path().join("restored.bin");
    run(
        &h[1],
        &s[1],
        &[
            "seed",
            "media-export",
            "--root",
            root,
            "--output",
            output.to_str().unwrap(),
        ],
    );
    assert_eq!(std::fs::read(&output).unwrap(), bytes);
    std::fs::write(&output, b"owner edit").unwrap();
    assert!(cmd(
        &h[1],
        &s[1],
        &[
            "seed",
            "media-export",
            "--root",
            root,
            "--output",
            output.to_str().unwrap()
        ]
    )
    .is_err());
    assert_eq!(std::fs::read(output).unwrap(), b"owner edit");
}
fn root_id(h: &Path) -> String {
    root(h)
}
#[test]
fn history_collection_confirmation_binds_shared_objects_and_private_policy() {
    let temp = tempfile::tempdir().unwrap();
    let (h, s) = setup(temp.path());
    let identity = mini_cli::identity::load(&h[0]).unwrap();
    let mut source = mini_cli::store::open_store(&s[0]).unwrap();
    let file = mini_forge::put_file(
        &mut source,
        &identity.human_did(),
        &identity.device,
        b"source code\n",
    )
    .unwrap();
    let executable = mini_forge::put_executable_file(
        &mut source,
        &identity.human_did(),
        &identity.device,
        b"#!/bin/sh\nexit 0\n",
    )
    .unwrap();
    let tree = mini_forge::put_tree(
        &mut source,
        &identity.human_did(),
        &identity.device,
        &[
            mini_forge::TreeEntry {
                name: "main.rs".into(),
                is_dir: false,
                target: file.clone(),
            },
            mini_forge::TreeEntry {
                name: "run.sh".into(),
                is_dir: false,
                target: executable,
            },
        ],
    )
    .unwrap();
    let first = mini_forge::commit(
        &mut source,
        &identity.human_did(),
        &identity.device,
        "first",
        &tree,
        &[],
        1,
        1,
    )
    .unwrap();
    let head = mini_forge::commit(
        &mut source,
        &identity.human_did(),
        &identity.device,
        "second",
        &tree,
        &[first.id().clone()],
        2,
        2,
    )
    .unwrap();
    let path = temp.path().join("history.json");
    let preview = run(
        &h[0],
        &s[0],
        &[
            "seed",
            "collection-preview",
            "--root",
            head.id().as_str(),
            "--kind",
            "history",
            "--days",
            "4",
            "--output",
            path.to_str().unwrap(),
        ],
    );
    assert_eq!(preview["result"]["objects"], 5);
    assert!(cmd(
        &h[0],
        &s[0],
        &[
            "seed",
            "collection-add",
            "--preview",
            path.to_str().unwrap(),
            "--confirm",
            "wrong"
        ]
    )
    .is_err());
    source
        .set_cache_tier(&file, mini_store::CacheTier::PrivateOnly)
        .unwrap();
    assert!(cmd(
        &h[0],
        &s[0],
        &[
            "seed",
            "collection-add",
            "--preview",
            path.to_str().unwrap(),
            "--confirm",
            preview["result"]["preview_id"].as_str().unwrap()
        ]
    )
    .is_err());
    assert!(mini_seed::Pool::open(&h[0].join("seed-pool"))
        .unwrap()
        .leases()
        .unwrap()
        .is_empty());
    source
        .set_cache_tier(&file, mini_store::CacheTier::EphemeralCache)
        .unwrap();
    let admitted = run(
        &h[0],
        &s[0],
        &[
            "seed",
            "collection-add",
            "--preview",
            path.to_str().unwrap(),
            "--confirm",
            preview["result"]["preview_id"].as_str().unwrap(),
        ],
    );
    assert_eq!(admitted["result"]["admitted"], 5);
    assert_eq!(mini_forge::checkout(&source, head.id()).unwrap().len(), 2);
    let duplicate = mini_forge::commit(
        &mut source,
        &identity.human_did(),
        &identity.device,
        "bad parents",
        &tree,
        &[first.id().clone(), first.id().clone()],
        3,
        3,
    )
    .unwrap();
    assert!(cmd(
        &h[0],
        &s[0],
        &[
            "seed",
            "collection-preview",
            "--root",
            duplicate.id().as_str(),
            "--kind",
            "history",
            "--days",
            "1",
            "--output",
            temp.path().join("bad.json").to_str().unwrap()
        ]
    )
    .is_err());
}
#[test]
fn media_reference_expansion_and_history_tree_depth_fail_before_publication() {
    let temp = tempfile::tempdir().unwrap();
    let (h, s) = setup(temp.path());
    let identity = mini_cli::identity::load(&h[0]).unwrap();
    let mut source = mini_cli::store::open_store(&s[0]).unwrap();
    let empty = mini_media::publish_media(
        &mut source,
        &identity.human_did(),
        &identity.device,
        "text/plain",
        b"",
        0,
        0,
    )
    .unwrap();
    let template = source.get(&empty.id).unwrap();
    let Payload::Public(mut payload) = template.payload.clone() else {
        panic!("fixture must be public")
    };
    let last = payload.len() - 1;
    payload[last] ^= 1;
    let wrong = ObjectBuilder::new(ObjectType::MEDIA_MANIFEST)
        .payload(Payload::Public(payload))
        .link("chunk", empty.chunks[0].clone())
        .sign(&identity.human_did(), &identity.device)
        .unwrap();
    source.insert(&wrong).unwrap();
    let wrong_output = temp.path().join("wrong-digest.bin");
    assert!(cmd(
        &h[0],
        &s[0],
        &[
            "seed",
            "media-export",
            "--root",
            wrong.id().as_str(),
            "--output",
            wrong_output.to_str().unwrap()
        ]
    )
    .is_err());
    assert!(!wrong_output.exists());
    assert!(!std::fs::read_dir(temp.path()).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".mininet-media-")));
    let mut builder =
        ObjectBuilder::new(ObjectType::MEDIA_MANIFEST).payload(template.payload.clone());
    for _ in 0..256 {
        builder = builder.link("chunk", empty.chunks[0].clone());
    }
    let repeated = builder
        .sign(&identity.human_did(), &identity.device)
        .unwrap();
    source.insert(&repeated).unwrap();
    let mut block = ObjectBuilder::new(ObjectType::Custom(mini_media::SUPERBLOCK_TYPE.into()))
        .payload(template.payload.clone());
    for _ in 0..17 {
        block = block.link("part", repeated.id().clone());
    }
    let block = block.sign(&identity.human_did(), &identity.device).unwrap();
    source.insert(&block).unwrap();
    let error = cmd(
        &h[0],
        &s[0],
        &[
            "seed",
            "collection-preview",
            "--root",
            block.id().as_str(),
            "--kind",
            "media",
            "--days",
            "1",
            "--output",
            temp.path().join("expanded.json").to_str().unwrap(),
        ],
    )
    .unwrap_err();
    assert!(error.contains("chunk references"), "{error}");
    let file =
        mini_forge::put_file(&mut source, &identity.human_did(), &identity.device, b"x").unwrap();
    let mut tree = mini_forge::put_tree(
        &mut source,
        &identity.human_did(),
        &identity.device,
        &[mini_forge::TreeEntry {
            name: "x".into(),
            is_dir: false,
            target: file,
        }],
    )
    .unwrap();
    for _ in 0..33 {
        tree = mini_forge::put_tree(
            &mut source,
            &identity.human_did(),
            &identity.device,
            &[mini_forge::TreeEntry {
                name: "dir".into(),
                is_dir: true,
                target: tree,
            }],
        )
        .unwrap();
    }
    let commit = mini_forge::commit(
        &mut source,
        &identity.human_did(),
        &identity.device,
        "deep",
        &tree,
        &[],
        0,
        0,
    )
    .unwrap();
    let error = cmd(
        &h[0],
        &s[0],
        &[
            "seed",
            "collection-preview",
            "--root",
            commit.id().as_str(),
            "--kind",
            "history",
            "--days",
            "1",
            "--output",
            temp.path().join("deep.json").to_str().unwrap(),
        ],
    )
    .unwrap_err();
    assert!(error.contains("tree depth"), "{error}");
    assert!(mini_seed::Pool::open(&h[0].join("seed-pool"))
        .unwrap()
        .leases()
        .unwrap()
        .is_empty());
}

#[test]
fn shared_history_tree_fanout_is_bounded_without_materializing_files() {
    let temp = tempfile::tempdir().unwrap();
    let (h, s) = setup(temp.path());
    let identity = mini_cli::identity::load(&h[0]).unwrap();
    let mut source = mini_cli::store::open_store(&s[0]).unwrap();
    let file =
        mini_forge::put_file(&mut source, &identity.human_did(), &identity.device, b"").unwrap();
    let entries: Vec<_> = (0..200)
        .map(|n| mini_forge::TreeEntry {
            name: format!("f{n:03}"),
            is_dir: false,
            target: file.clone(),
        })
        .collect();
    let leaf = mini_forge::put_tree(
        &mut source,
        &identity.human_did(),
        &identity.device,
        &entries,
    )
    .unwrap();
    let directories: Vec<_> = (0..200)
        .map(|n| mini_forge::TreeEntry {
            name: format!("d{n:03}"),
            is_dir: true,
            target: leaf.clone(),
        })
        .collect();
    let middle = mini_forge::put_tree(
        &mut source,
        &identity.human_did(),
        &identity.device,
        &directories,
    )
    .unwrap();
    let top: Vec<_> = ["a", "b", "c"]
        .iter()
        .map(|name| mini_forge::TreeEntry {
            name: (*name).into(),
            is_dir: true,
            target: middle.clone(),
        })
        .collect();
    let tree =
        mini_forge::put_tree(&mut source, &identity.human_did(), &identity.device, &top).unwrap();
    let commit = mini_forge::commit(
        &mut source,
        &identity.human_did(),
        &identity.device,
        "fanout",
        &tree,
        &[],
        0,
        0,
    )
    .unwrap();
    let error = cmd(
        &h[0],
        &s[0],
        &[
            "seed",
            "collection-preview",
            "--root",
            commit.id().as_str(),
            "--kind",
            "history",
            "--days",
            "1",
            "--output",
            temp.path().join("fanout.json").to_str().unwrap(),
        ],
    )
    .unwrap_err();
    assert!(error.contains("expanded checkout"), "{error}");
}
