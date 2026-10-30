//! The native Open Beta contribution loop across TWO independent stores that
//! share nothing but a real TCP `mini sync` connection (issue #338's
//! replication half; `beta_native_loop.rs` is the single-replica half).
//! No GitHub anywhere. Both nodes must derive the identical provisional BETA
//! snapshot digest, so no balance database or trusted server is involved.

use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

const EPOCH: &str = "0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a";

fn tempdir(tag: &str) -> String {
    let mut p = PathBuf::from(std::env::temp_dir());
    p.push(format!(
        "mini-cli-beta-2node-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    p.to_str().unwrap().to_string()
}

fn field(json: &str, key: &str) -> String {
    let needle = format!("\"{key}\":\"");
    let start = json
        .find(&needle)
        .unwrap_or_else(|| panic!("{key} in {json}"))
        + needle.len();
    json[start..].split('"').next().unwrap().to_string()
}

fn number(json: &str, key: &str) -> u64 {
    let needle = format!("\"{key}\":");
    let start = json
        .find(&needle)
        .unwrap_or_else(|| panic!("{key} in {json}"))
        + needle.len();
    json[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap()
}

fn run(args: &[String]) -> String {
    mini_cli::run(args).unwrap_or_else(|e| panic!("{args:?} failed: {e}"))
}

/// A node: one home (its identity) bound to one store.
struct Node {
    home: String,
    store: String,
}

impl Node {
    fn json(&self, args: &[&str]) -> String {
        self.as_identity(&self.home, args, true)
    }
    fn plain(&self, args: &[&str]) -> String {
        self.as_identity(&self.home, args, false)
    }
    /// Run as another identity against this node's store (same machine).
    fn as_identity(&self, home: &str, args: &[&str], json: bool) -> String {
        let mut full: Vec<String> = Vec::new();
        if json {
            full.push("--json".into());
        }
        full.extend([
            "--home".into(),
            home.to_string(),
            "--store".into(),
            self.store.clone(),
        ]);
        full.extend(args.iter().map(|s| s.to_string()));
        run(&full)
    }
}

/// One bidirectional sync over a real loopback TCP connection.
fn sync(listener: &Node, connector: &Node) {
    let probe = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = probe.local_addr().unwrap().to_string();
    drop(probe);
    let (home, store, listen_addr) = (listener.home.clone(), listener.store.clone(), addr.clone());
    let server = thread::spawn(move || {
        mini_cli::run(&[
            "--home".into(),
            home,
            "--store".into(),
            store,
            "sync".into(),
            "listen".into(),
            "--addr".into(),
            listen_addr,
        ])
        .unwrap()
    });
    let args: Vec<String> = [
        "--home",
        connector.home.as_str(),
        "--store",
        connector.store.as_str(),
        "sync",
        "connect",
        addr.as_str(),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for attempt in 0..100 {
        match mini_cli::run(&args) {
            Ok(_) => break,
            Err(e) if attempt == 99 => panic!("sync connect failed: {e}"),
            Err(_) => thread::sleep(Duration::from_millis(20)),
        }
    }
    server.join().unwrap();
}

#[test]
fn the_beta_loop_converges_across_two_independent_stores_over_real_sync() {
    let operator = Node {
        home: tempdir("op-home"),
        store: tempdir("op-store"),
    };
    let tester = Node {
        home: tempdir("te-home"),
        store: tempdir("te-store"),
    };
    // Policy members act on the operator's machine, in the operator's store.
    let members: Vec<String> = (0..3).map(|i| tempdir(&format!("member{i}"))).collect();
    for home in [&operator.home, &tester.home]
        .into_iter()
        .chain(members.iter())
    {
        run(&[
            "--home".into(),
            home.clone(),
            "identity".into(),
            "init".into(),
        ]);
    }
    // Out-of-band KEL trust: the operator trusts everyone, the tester trusts
    // the operator and the policy members (needed to verify their approvals).
    let kel = |home: &str| run(&["--home".into(), home.into(), "kel".into(), "export".into()]);
    let trust = |home: &str, bundle: &str| {
        run(&[
            "--home".into(),
            home.into(),
            "kel".into(),
            "trust".into(),
            bundle.into(),
        ]);
    };
    let (op_kel, te_kel) = (kel(&operator.home), kel(&tester.home));
    trust(&operator.home, &te_kel);
    trust(&tester.home, &op_kel);
    for member in &members {
        let bundle = kel(member);
        trust(&operator.home, &bundle);
        trust(&tester.home, &bundle);
    }
    let did = |home: &str| {
        mini_cli::identity::load(std::path::Path::new(home))
            .unwrap()
            .human_did()
            .as_str()
            .to_string()
    };

    // Operator: project + campaign, then replicate to the tester.
    let init = operator.plain(&["repo", "init", "beta-app"]);
    let project = init.rsplit(": ").next().unwrap().trim().to_string();
    let campaign = field(
        &operator.json(&[
            "beta",
            "campaign",
            "create",
            &project,
            "--epoch",
            EPOCH,
            "--title",
            "Replicated",
            "--instructions",
            "Report what breaks",
            "--route",
            "rust",
            "--starts-ms",
            "100",
            "--ends-ms",
            "10000",
            "--testing-grant",
            "1000",
        ]),
        "campaign_id",
    );
    assert!(
        !tester
            .json(&["beta", "campaign", "list"])
            .contains(&campaign),
        "the tester's independent store knows nothing before sync"
    );
    sync(&tester, &operator);
    assert!(
        tester
            .json(&["beta", "campaign", "list"])
            .contains(&campaign),
        "the campaign replicated to the tester over TCP"
    );

    // Tester: fresh BETA account + redacted finding, replicate back.
    let account = field(
        &tester.json(&["beta", "account", "new", "--epoch", EPOCH]),
        "account",
    );
    let finding = field(
        &tester.json(&[
            "beta",
            "finding",
            "submit",
            &campaign,
            "--privacy-redacted",
            "--evidence-class",
            "other",
            "--severity",
            "low",
            "--component",
            "cli",
            "--summary",
            "Typo in help",
            "--environment",
            "linux x86_64",
            "--steps",
            "run mini --help",
            "--expected",
            "correct text",
            "--observed",
            "typo",
            "--evidence",
            "screenshot-redacted",
            "--limitations",
            "single machine",
        ]),
        "finding_id",
    );
    sync(&operator, &tester);

    // Operator: disposition -> task -> accepted contribution -> 2-of-3 grant.
    operator.json(&[
        "beta",
        "finding",
        "disposition",
        &finding,
        "--state",
        "accepted",
        "--rationale",
        "reproduced",
    ]);
    operator.json(&[
        "beta",
        "finding",
        "to-task",
        "beta-app",
        &finding,
        "--route",
        "rust",
        "--risk",
        "low",
        "--title",
        "Fix help typo",
        "--description",
        "Correct the help text",
        "--path",
        "crates/mini-cli/src/cli.rs",
        "--acceptance",
        "help text is correct",
        "--non-goal",
        "no behaviour change",
    ]);
    let contribution = field(
        &operator.json(&[
            "beta",
            "contribution",
            "accept",
            &finding,
            "--campaign",
            &campaign,
            "--kind",
            "testing",
            "--summary",
            "Found and reported a real defect",
            "--evidence",
            "finding",
        ]),
        "contribution_id",
    );
    let member_dids: Vec<String> = members.iter().map(|m| did(m)).collect();
    let policy = field(
        &operator.json(&[
            "beta",
            "grant-policy",
            "create",
            &campaign,
            "--member",
            &member_dids[0],
            "--member",
            &member_dids[1],
            "--member",
            &member_dids[2],
            "--testing-threshold",
            "2",
            "--participation-threshold",
            "2",
            "--testing-amount",
            "100",
            "--reward-band",
            "100",
            "--valid-from-ms",
            "200",
            "--valid-until-ms",
            "9000",
            "--at-ms",
            "150",
        ]),
        "policy_id",
    );
    let grant = field(
        &operator.json(&[
            "beta",
            "grant",
            "create",
            &campaign,
            "--epoch",
            EPOCH,
            "--account",
            &account,
            "--class",
            "participation",
            "--contribution",
            &contribution,
            "--amount",
            "100",
            "--memo",
            "accepted finding",
            "--at-ms",
            "500",
        ]),
        "grant_id",
    );
    let balance = |node: &Node, home: &str| {
        node.as_identity(
            home,
            &[
                "beta",
                "wallet",
                "balance",
                "--epoch",
                EPOCH,
                "--campaign",
                &campaign,
                "--account",
                &account,
            ],
            true,
        )
    };
    // One approval: replicated everywhere, still nothing issued anywhere.
    operator.as_identity(
        &members[0],
        &["beta", "grant-approve", &policy, &grant, "--at-ms", "600"],
        true,
    );
    sync(&tester, &operator);
    assert_eq!(number(&balance(&tester, &tester.home), "amount_micro"), 0);
    assert_eq!(
        number(&balance(&operator, &operator.home), "amount_micro"),
        0
    );
    // The threshold-completing approval, then replicate to the tester.
    operator.as_identity(
        &members[1],
        &["beta", "grant-approve", &policy, &grant, "--at-ms", "601"],
        true,
    );
    sync(&tester, &operator);

    let at_operator = balance(&operator, &operator.home);
    let at_tester = balance(&tester, &tester.home);
    assert_eq!(number(&at_operator, "amount_micro"), 100);
    assert_eq!(
        number(&at_tester, "amount_micro"),
        100,
        "the tester sees the reward"
    );
    assert_eq!(
        field(&at_operator, "snapshot_digest"),
        field(&at_tester, "snapshot_digest"),
        "both nodes derive the identical provisional snapshot from the same objects"
    );
}
