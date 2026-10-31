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
    // The tester claims the task natively.
    cli.run(
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
    );

    // The operator records the accepted contribution (fresh claim handle).
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
        ],
    );
    let contribution = field(&accepted, "contribution_id");
    assert!(accepted.contains("\"sensitive_output\":true"));

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
