//! Actual authenticated peer delivery, retention isolation and pending settlement.
use mini_objects::{ObjectBuilder, ObjectType, Payload};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
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
    let got = run(&h[2], &s[2], &["seed", "retrieve", "--id", &id]);
    assert_eq!(got["result"]["retrieved"], true);
    assert_eq!(got["result"]["contact"], "b-backup");
    assert_eq!(got["result"]["failures"].as_array().unwrap().len(), 1);
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
