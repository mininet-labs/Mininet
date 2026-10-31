//! Drives `mini beta grant-policy|grant|grant-approve|grant-status` through the
//! real CLI entry point. Test value only.

use std::path::PathBuf;

use mini_beta::{create_campaign, BetaEpochId};
use mini_objects::{ObjectBuilder, ObjectType, Payload};

const EPOCH: &str = "0909090909090909090909090909090909090909090909090909090909090909";
const ACCT: &str = "0707070707070707070707070707070707070707070707070707070707070707";

/// Names the command without echoing arguments or output: those carry account
/// and object ids, which must never reach a log or panic message.
fn describe<S: AsRef<str>>(args: &[S]) -> String {
    args.iter()
        .map(AsRef::as_ref)
        .filter(|a| !a.starts_with("--") && a.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
        .take(4)
        .collect::<Vec<_>>()
        .join(" ")
}

fn tempdir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mini-cli-grants-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    p
}

fn try_run(args: &[&str]) -> Result<String, mini_cli::CliError> {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    mini_cli::run(&owned)
}

fn run(args: &[&str]) -> String {
    try_run(args).unwrap_or_else(|e| panic!("command `{}` failed: {e}", describe(args)))
}

struct Fixture {
    homes: Vec<String>,
    dids: Vec<String>,
    store: String,
    campaign: String,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let mut homes = Vec::new();
        let mut dids = Vec::new();
        for i in 0..4 {
            let h = tempdir(&format!("{tag}-home{i}"));
            run(&["--home", h.to_str().unwrap(), "identity", "init"]);
            dids.push(
                mini_cli::identity::load(&h)
                    .unwrap()
                    .human_did()
                    .to_string(),
            );
            homes.push(h.to_str().unwrap().to_string());
        }
        let store_path = tempdir(&format!("{tag}-store"));
        let home0 = std::path::Path::new(&homes[0]);
        // The CLI refuses a sequence counter below the store's signed floor, so
        // advance home 0's counter (it is per-home) past the two objects the
        // fixture signs directly. These scratch commands fail by design.
        let mut seq = 1u64..;
        let author = mini_cli::identity::load(home0).unwrap();
        let mut store = mini_cli::store::open_store(&store_path).unwrap();
        let target = ObjectBuilder::new(ObjectType::Custom("mininet.beta/test-release/v1".into()))
            .timestamp_ms(1)
            .sequence(seq.next().unwrap())
            .payload(Payload::Public(b"grants-cli".to_vec()))
            .sign(&author.human_did(), &author.device)
            .unwrap();
        for _ in 0..2 {
            let scratch = tempdir(&format!("{tag}-scratch"));
            let _ = try_run(&[
                "--home",
                &homes[0],
                "--store",
                scratch.to_str().unwrap(),
                "beta",
                "grant",
                "create",
                target.id().as_str(),
                "--epoch",
                EPOCH,
                "--account",
                ACCT,
                "--class",
                "testing",
                "--amount",
                "1",
                "--memo",
                "x",
            ]);
        }
        store.insert(&target).unwrap();
        let campaign = create_campaign(
            &mut store,
            &author.human_did(),
            &author.device,
            target.id(),
            BetaEpochId::new([9; 32]).unwrap(),
            "grants beta",
            "test grants",
            &["rust".to_string()],
            10,
            100_000,
            1_000,
            2,
            seq.next().unwrap(),
        )
        .unwrap();
        Fixture {
            homes,
            dids,
            store: store_path.to_str().unwrap().to_string(),
            campaign: campaign.id().as_str().to_string(),
        }
    }

    fn cmd(&self, who: usize, rest: &[&str]) -> Result<String, mini_cli::CliError> {
        let mut v = vec!["--home", &self.homes[who], "--store", &self.store, "beta"];
        v.extend_from_slice(rest);
        try_run(&v)
    }

    fn id_after(out: &str, prefix: &str) -> String {
        out.lines()
            .find_map(|l| l.strip_prefix(prefix))
            .unwrap_or_else(|| panic!("no {prefix:?} in command output"))
            .split_whitespace()
            .next()
            .unwrap()
            .to_string()
    }

    /// Policy among identities 0..3, thresholds 2/3.
    fn policy(&self) -> String {
        let out = self
            .cmd(
                0,
                &[
                    "grant-policy",
                    "create",
                    &self.campaign,
                    "--member",
                    &self.dids[0],
                    "--member",
                    &self.dids[1],
                    "--member",
                    &self.dids[2],
                    "--testing-threshold",
                    "2",
                    "--participation-threshold",
                    "3",
                    "--testing-amount",
                    "500",
                    "--reward-band",
                    "100",
                    "--reward-band",
                    "200",
                    "--valid-from-ms",
                    "20",
                    "--valid-until-ms",
                    "90000",
                    "--at-ms",
                    "20",
                ],
            )
            .unwrap();
        assert!(out.contains("BETA"));
        Self::id_after(&out, "grant policy created: ")
    }

    fn grant(&self, amount: &str) -> String {
        let out = self
            .cmd(
                0,
                &[
                    "grant",
                    "create",
                    &self.campaign,
                    "--epoch",
                    EPOCH,
                    "--account",
                    ACCT,
                    "--class",
                    "testing",
                    "--amount",
                    amount,
                    "--memo",
                    "t",
                    "--at-ms",
                    "30",
                ],
            )
            .unwrap();
        assert!(out.contains("BETA"));
        Self::id_after(&out, "grant authorization created: ")
    }

    fn approve(&self, who: usize, policy: &str, grant: &str) -> Result<String, mini_cli::CliError> {
        self.cmd(who, &["grant-approve", policy, grant, "--at-ms", "40"])
            .map(|o| Self::id_after(&o, "grant approval created: "))
    }
}

#[test]
fn policy_and_testing_grant_create_ok() {
    let f = Fixture::new("create");
    let policy = f.policy();
    let grant = f.grant("500");
    assert!(policy.starts_with('z'));
    assert!(grant.starts_with('z'));
}

#[test]
fn class_and_contribution_mismatch_rejected() {
    let f = Fixture::new("mismatch");
    // Testing grant must not carry a contribution.
    let err = f
        .cmd(
            0,
            &[
                "grant",
                "create",
                &f.campaign,
                "--epoch",
                EPOCH,
                "--account",
                ACCT,
                "--class",
                "testing",
                "--contribution",
                &f.campaign,
                "--amount",
                "500",
                "--memo",
                "t",
                "--at-ms",
                "30",
            ],
        )
        .unwrap_err();
    assert!(err.to_string().contains("beta"), "{err}");
    // Participation grant requires one.
    assert!(f
        .cmd(
            0,
            &[
                "grant",
                "create",
                &f.campaign,
                "--epoch",
                EPOCH,
                "--account",
                ACCT,
                "--class",
                "participation",
                "--amount",
                "100",
                "--memo",
                "t",
                "--at-ms",
                "30",
            ],
        )
        .is_err());
}

#[test]
fn approve_by_non_member_rejected() {
    let f = Fixture::new("nonmember");
    let policy = f.policy();
    let grant = f.grant("500");
    assert!(f.approve(3, &policy, &grant).is_err());
}

#[test]
fn threshold_met_accepts_and_unmet_does_not() {
    let f = Fixture::new("threshold");
    let policy = f.policy();
    let grant = f.grant("500");
    let a0 = f.approve(0, &policy, &grant).unwrap();
    let a1 = f.approve(1, &policy, &grant).unwrap();

    let one = f
        .cmd(0, &["grant-status", &policy, &grant, "--approval", &a0])
        .unwrap();
    assert!(one.contains("NOT accepted"), "{one}");

    let out = try_run(&[
        "--json",
        "--home",
        &f.homes[0],
        "--store",
        &f.store,
        "beta",
        "grant-status",
        &policy,
        &grant,
        "--approval",
        &a0,
        "--approval",
        &a1,
    ])
    .unwrap();
    assert!(out.contains("\"accepted\":true"), "{out}");
    assert!(out.contains("\"approvals_counted\":2"), "{out}");
    assert!(out.contains("\"threshold\":2"), "{out}");
}

#[test]
fn duplicate_approvals_from_one_member_count_once() {
    let f = Fixture::new("dup");
    let policy = f.policy();
    let grant = f.grant("500");
    let a = f.approve(0, &policy, &grant).unwrap();
    let b = f.approve(0, &policy, &grant).unwrap();
    assert_ne!(a, b);
    let out = try_run(&[
        "--json",
        "--home",
        &f.homes[0],
        "--store",
        &f.store,
        "beta",
        "grant-status",
        &policy,
        &grant,
        "--approval",
        &a,
        "--approval",
        &b,
    ])
    .unwrap();
    assert!(out.contains("\"accepted\":false"), "{out}");
    assert!(out.contains("\"approvals_counted\":1"), "{out}");
}

#[test]
fn too_many_members_is_usage_error() {
    let f = Fixture::new("bounds");
    let mut args: Vec<String> = [
        "--home",
        &f.homes[0],
        "--store",
        &f.store,
        "beta",
        "grant-policy",
        "create",
        &f.campaign,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for _ in 0..65 {
        args.push("--member".into());
        args.push(f.dids[0].clone());
    }
    for a in [
        "--testing-threshold",
        "2",
        "--participation-threshold",
        "3",
        "--testing-amount",
        "5",
        "--reward-band",
        "1",
        "--valid-from-ms",
        "20",
        "--valid-until-ms",
        "30",
    ] {
        args.push(a.into());
    }
    let err = mini_cli::run(&args).unwrap_err();
    assert!(matches!(err, mini_cli::CliError::Usage(_)), "{err}");
}
