//! The complete Open Beta contribution loop, driven only through the `mini`
//! command layer on one shared store (a single replica) with **no GitHub
//! anywhere**: campaign -> finding -> disposition -> Forge task -> claim ->
//! accepted contribution -> threshold-approved participation grant -> BETA
//! balance. This is issue #338's loop acceptance (single replica); verified
//! multi-node replication of the same loop is a separate, still-open test.

use std::path::PathBuf;

const EPOCH: &str = "0909090909090909090909090909090909090909090909090909090909090909";

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
        "mini-cli-beta-loop-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    p
}

fn field(json: &str, key: &str) -> String {
    let needle = format!("\"{key}\":\"");
    let start = json
        .find(&needle)
        .unwrap_or_else(|| panic!("missing field {key}"))
        + needle.len();
    json[start..].split('"').next().unwrap().to_string()
}

fn number(json: &str, key: &str) -> u64 {
    let needle = format!("\"{key}\":");
    let start = json
        .find(&needle)
        .unwrap_or_else(|| panic!("missing field {key}"))
        + needle.len();
    json[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap()
}

struct Cli {
    store: String,
}

impl Cli {
    fn run(&self, home: &str, json: bool, args: &[&str]) -> String {
        let mut full: Vec<&str> = Vec::new();
        if json {
            full.push("--json");
        }
        full.extend(["--home", home, "--store", &self.store]);
        full.extend_from_slice(args);
        let owned: Vec<String> = full.iter().map(|s| s.to_string()).collect();
        mini_cli::run(&owned).unwrap_or_else(|e| panic!("`{}` failed: {e}", describe(args)))
    }
}

#[test]
fn a_finding_becomes_work_a_contribution_and_a_beta_balance_without_github() {
    let mk = |tag: &str| tempdir(tag).to_str().unwrap().to_string();
    let (operator, tester) = (mk("operator"), mk("tester"));
    let members = [mk("m1"), mk("m2"), mk("m3")];
    let cli = Cli { store: mk("store") };
    for home in [&operator, &tester].into_iter().chain(members.iter()) {
        cli.run(home, false, &["identity", "init"]);
    }
    let did = |home: &str| {
        mini_cli::identity::load(std::path::Path::new(home))
            .unwrap()
            .human_did()
            .as_str()
            .to_string()
    };

    // The operator publishes the project under test and a campaign for it.
    let init = cli.run(&operator, false, &["repo", "init", "beta-app"]);
    let project = init.rsplit(": ").next().unwrap().trim().to_string();
    let campaign = field(
        &cli.run(
            &operator,
            true,
            &[
                "beta",
                "campaign",
                "create",
                &project,
                "--epoch",
                EPOCH,
                "--title",
                "Loop beta",
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
            ],
        ),
        "campaign_id",
    );
    // A tester can discover the campaign from the store alone.
    let listed = cli.run(&tester, true, &["beta", "campaign", "list"]);
    assert!(
        listed.contains(&campaign),
        "campaign discoverable: {listed}"
    );

    // The tester submits a structured, redacted finding.
    let finding = field(
        &cli.run(
            &tester,
            true,
            &[
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
            ],
        ),
        "finding_id",
    );

    // The operator dispositions it and turns it into a native Forge task.
    cli.run(
        &operator,
        true,
        &[
            "beta",
            "finding",
            "disposition",
            &finding,
            "--state",
            "accepted",
            "--rationale",
            "reproduced",
        ],
    );
    let task = field(
        &cli.run(
            &operator,
            true,
            &[
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
            ],
        ),
        "task_id",
    );
    // The tester claims the task natively (anonymously: a fresh signing identity).
    let claim = field(
        &cli.run(
            &tester,
            true,
            &[
                "beta",
                "task",
                "claim",
                &task,
                "--role",
                "implementer",
                "--expires-ms",
                "99999999999999",
                "--path",
                "crates/mini-cli/src/cli.rs",
            ],
        ),
        "claim_id",
    );

    // The tester hands off exact-state review evidence natively: a signed,
    // content-addressed commit is the exact state, and the technical review
    // handoff names that commit, the claim and the evidence. A handoff is
    // evidence only; it records no approval.
    let head = {
        let mut store = mini_store::Store::new(
            mini_store::FsBackend::open(std::path::Path::new(&cli.store)).unwrap(),
        );
        let tester_identity = mini_cli::identity::load(std::path::Path::new(&tester)).unwrap();
        let file = mini_forge::put_file(
            &mut store,
            &tester_identity.human_did(),
            &tester_identity.device,
            b"// fixed help text\n",
        )
        .unwrap();
        let tree = mini_forge::put_tree(
            &mut store,
            &tester_identity.human_did(),
            &tester_identity.device,
            &[mini_forge::TreeEntry {
                name: "cli.rs".into(),
                is_dir: false,
                target: file,
            }],
        )
        .unwrap();
        mini_forge::commit(
            &mut store,
            &tester_identity.human_did(),
            &tester_identity.device,
            "fix help typo",
            &tree,
            &[],
            0,
            0,
        )
        .unwrap()
        .id()
        .as_str()
        .to_string()
    };
    let review = cli.run(
        &tester,
        true,
        &[
            "task",
            "review",
            &task,
            "--claim",
            &claim,
            "--head",
            &head,
            "--kind",
            "peer",
            "--disposition",
            "observations",
            "--findings",
            "help text corrected",
            "--evidence",
            "cargo test -p mini-cli",
            "--limitations",
            "self-reported; no external audit; no approval recorded",
        ],
    );
    assert!(review.contains("\"kind\":\"task.review\""), "{review}");
    // The review vocabulary has no approving disposition at all.
    assert_eq!(field(&review, "disposition"), "observations");
    assert_eq!(field(&review, "reviewed_head"), head);
    // The operator reads the handoff back. `task show` lists only identities
    // the viewer has explicitly trusted, so the operator first trusts the
    // tester's key history, exactly as in `coordination_commands.rs`.
    let tester_kel = cli.run(&tester, false, &["kel", "export"]);
    cli.run(&operator, false, &["kel", "trust", tester_kel.trim()]);
    let shown = cli.run(&operator, true, &["task", "show", &task]);
    assert!(
        shown.contains(&format!("\"reviewed_head\":\"{head}\"")),
        "handoff names the exact commit: {shown}"
    );
    // The anonymous claim signs with a fresh, unsaved identity, so no viewer
    // can have trusted it and it is not in the trusted `claims` list. It is
    // shown separately, as key-controlled only: the store holds the fresh
    // identity's self-certifying key history, which proves who controls the
    // key and nothing about who that is.
    assert!(shown.contains("\"claims\":[]"), "{shown}");
    assert!(shown.contains("\"untrusted_claims\":["), "{shown}");
    assert!(
        shown.contains(&format!(
            "\"id\":\"{claim}\",\"signer_trusted\":false,\"controlled_key_root\":"
        )),
        "anonymous claim listed and labelled: {shown}"
    );
    assert!(
        shown.contains("\"untrusted_claims_not_shown\":0"),
        "{shown}"
    );
    // The human view says the same thing in words.
    let human = cli.run(&operator, false, &["task", "show", &task]);
    assert!(
        human.contains("claims from signers you have not trusted"),
        "{human}"
    );
    assert!(human.contains(&claim), "{human}");

    // The tester makes a one-time claim key; only its public tag reaches the
    // operator, who records the accepted contribution.
    let made = cli.run(&tester, true, &["beta", "claim", "new"]);
    let claim_secret = field(&made, "claim_secret");
    let claim_tag = field(&made, "claim_commitment");
    let accepted = cli.run(
        &operator,
        true,
        &[
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
            "--claim-tag",
            &claim_tag,
        ],
    );
    let contribution = field(&accepted, "contribution_id");
    assert!(!accepted.contains("claim_secret"));

    // The tester holds a fresh BETA account; a 2-of-3 policy grants the reward.
    let account = field(
        &cli.run(&tester, true, &["beta", "account", "new", "--epoch", EPOCH]),
        "account",
    );
    let member_dids: Vec<String> = members.iter().map(|m| did(m)).collect();
    let policy = field(
        &cli.run(
            &operator,
            true,
            &[
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
            ],
        ),
        "policy_id",
    );
    // The tester redeems the private claim to that account under a throwaway
    // identity; the operator's grant takes the account from the redemption.
    let secret_file = tempdir("secret");
    std::fs::write(&secret_file, &claim_secret).unwrap();
    let secret_file = secret_file.to_str().unwrap().to_string();
    let redeemed = cli.run(
        &tester,
        true,
        &[
            "beta",
            "claim",
            "redeem",
            "--contribution",
            &contribution,
            "--secret-file",
            &secret_file,
            "--account",
            &account,
        ],
    );
    let redemption = field(&redeemed, "redemption_id");
    let grant = field(
        &cli.run(
            &operator,
            true,
            &[
                "beta",
                "grant",
                "create",
                &campaign,
                "--epoch",
                EPOCH,
                "--class",
                "participation",
                "--redemption",
                &redemption,
                "--amount",
                "100",
                "--memo",
                "accepted finding",
                "--at-ms",
                "500",
            ],
        ),
        "grant_id",
    );
    let balance = |home: &str| {
        let out = cli.run(
            home,
            true,
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
        );
        number(&out, "amount_micro")
    };
    // One approval is not enough: the reward is not issued by any single key.
    cli.run(
        &members[0],
        true,
        &["beta", "grant-approve", &policy, &grant, "--at-ms", "600"],
    );
    assert_eq!(balance(&tester), 0, "below threshold nothing is issued");
    cli.run(
        &members[1],
        true,
        &["beta", "grant-approve", &policy, &grant, "--at-ms", "601"],
    );
    assert_eq!(balance(&tester), 100, "threshold met: BETA is issued");
}

#[test]
fn a_contributor_supplied_claim_tag_keeps_the_secret_from_the_operator_and_a_wrong_secret_changes_nothing(
) {
    let mk = |tag: &str| tempdir(tag).to_str().unwrap().to_string();
    let (operator, contributor) = (mk("op2"), mk("co2"));
    let cli = Cli {
        store: mk("store2"),
    };
    cli.run(&operator, false, &["identity", "init"]);
    cli.run(&contributor, false, &["identity", "init"]);
    let init = cli.run(&operator, false, &["repo", "init", "claim-app"]);
    let project = init.rsplit(": ").next().unwrap().trim().to_string();
    let campaign = field(
        &cli.run(
            &operator,
            true,
            &[
                "beta",
                "campaign",
                "create",
                &project,
                "--epoch",
                EPOCH,
                "--title",
                "Claim beta",
                "--instructions",
                "x",
                "--route",
                "rust",
                "--starts-ms",
                "100",
                "--ends-ms",
                "10000",
                "--testing-grant",
                "1000",
            ],
        ),
        "campaign_id",
    );

    // The contributor makes the key; only the public tag reaches the operator.
    let made = cli.run(&contributor, true, &["beta", "claim", "new"]);
    let secret = field(&made, "claim_secret");
    let tag = field(&made, "claim_commitment");
    let accepted = cli.run(
        &operator,
        true,
        &[
            "beta",
            "contribution",
            "accept",
            &project,
            "--campaign",
            &campaign,
            "--kind",
            "testing",
            "--summary",
            "contributor-keyed",
            "--evidence",
            "finding",
            "--claim-tag",
            &tag,
        ],
    );
    assert!(
        !accepted.contains("claim_secret"),
        "operator must not see a secret"
    );
    let contribution = field(&accepted, "contribution_id");

    let account = field(
        &cli.run(
            &contributor,
            true,
            &["beta", "account", "new", "--epoch", EPOCH],
        ),
        "account",
    );
    let count = || {
        let store = mini_cli::store::open_store(std::path::Path::new(&cli.store)).unwrap();
        store
            .by_type(&mini_objects::ObjectType::Custom("mini/kel".to_string()))
            .unwrap()
            .len()
    };
    let before = count();

    // A claim tag with no campaign could never be redeemed: refused up front.
    let no_campaign: Vec<String> = [
        "--home",
        &operator,
        "--store",
        &cli.store,
        "beta",
        "contribution",
        "accept",
        &project,
        "--kind",
        "testing",
        "--summary",
        "no campaign",
        "--evidence",
        "x",
        "--claim-tag",
        &tag,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let error = mini_cli::run(&no_campaign).unwrap_err().to_string();
    assert!(error.contains("--campaign required"), "{error}");

    // A wrong secret fails and persists no throwaway carriers.
    let wrong = tempdir("wrong");
    std::fs::write(&wrong, "11".repeat(32)).unwrap();
    let owned: Vec<String> = [
        "--home",
        &contributor,
        "--store",
        &cli.store,
        "beta",
        "claim",
        "redeem",
        "--contribution",
        &contribution,
        "--secret-file",
        wrong.to_str().unwrap(),
        "--account",
        &account,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert!(mini_cli::run(&owned).is_err(), "wrong secret must fail");
    assert_eq!(
        count(),
        before,
        "a failed redemption must leave the store unchanged"
    );

    // The right secret works.
    let good = tempdir("good");
    std::fs::write(&good, &secret).unwrap();
    let redeemed = cli.run(
        &contributor,
        true,
        &[
            "beta",
            "claim",
            "redeem",
            "--contribution",
            &contribution,
            "--secret-file",
            good.to_str().unwrap(),
            "--account",
            &account,
        ],
    );
    assert!(redeemed.contains("redemption_id"));
    assert!(count() > before);
}
