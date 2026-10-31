//! Beta MINI wallet CLI: accounts, provisional balances and transfers over the
//! durable signed-object execution core. Test value only.

use std::path::PathBuf;

use mini_objects::{ObjectBuilder, ObjectType, Payload};

const EPOCH_HEX: &str = "0707070707070707070707070707070707070707070707070707070707070707";

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
        "mini-cli-beta-wallet-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    p
}

fn run(args: &[&str]) -> String {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    mini_cli::run(&owned).unwrap_or_else(|e| panic!("command `{}` failed: {e}", describe(args)))
}

fn run_err(args: &[&str]) -> String {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    mini_cli::run(&owned)
        .expect_err(&format!("expected failure for `{}`", describe(args)))
        .to_string()
}

/// First string value of `"key":"..."` in the single-line JSON envelope.
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

struct World {
    alice_home: String,
    bob_home: String,
    store: String,
    campaign: String,
    alice_account: String,
    bob_account: String,
}

/// Everything runs through the CLI as separate identities: an operator
/// publishes the campaign, a 2-of-3 policy and a testing grant for Alice; two
/// policy members approve; Alice and Bob each hold a fresh BETA account.
fn world() -> World {
    let mk = |tag: &str| tempdir(tag).to_str().unwrap().to_string();
    let (operator, alice_home, bob_home) = (mk("operator"), mk("alice"), mk("bob"));
    let members = [mk("m1"), mk("m2"), mk("m3")];
    let store = mk("store");
    for home in [&operator, &alice_home, &bob_home]
        .into_iter()
        .chain(members.iter())
    {
        run(&["--home", home, "identity", "init"]);
    }
    let did = |home: &str| {
        mini_cli::identity::load(std::path::Path::new(home))
            .unwrap()
            .human_did()
            .as_str()
            .to_string()
    };
    let json = |home: &str, args: &[&str]| {
        let mut full = vec!["--json", "--home", home, "--store", store.as_str()];
        full.extend_from_slice(args);
        run(&full)
    };
    let alice_account = field(
        &json(
            &alice_home,
            &["beta", "account", "new", "--epoch", EPOCH_HEX],
        ),
        "account",
    );
    let bob_account = field(
        &json(&bob_home, &["beta", "account", "new", "--epoch", EPOCH_HEX]),
        "account",
    );

    // Any exact object can be the build under test; use a signed marker.
    // Any exact object can be the build under test. A separate library signer
    // keeps the operator's sequence counter exclusively CLI-owned.
    let builder = did_mini::Controller::incept_single_from_seeds(&[99; 32], &[100; 32]).unwrap();
    let mut s = mini_cli::store::open_store(std::path::Path::new(&store)).unwrap();
    let target = ObjectBuilder::new(ObjectType::Custom("mininet.beta-wallet/test-target".into()))
        .timestamp_ms(1)
        .sequence(1)
        .payload(Payload::Public(b"wallet-cli-fixture".to_vec()))
        .sign(&builder.did(), &builder)
        .unwrap();
    s.insert(&target).unwrap();
    drop(s);

    let campaign = field(
        &json(
            &operator,
            &[
                "beta",
                "campaign",
                "create",
                target.id().as_str(),
                "--epoch",
                EPOCH_HEX,
                "--title",
                "wallet beta",
                "--instructions",
                "exercise the wallet",
                "--route",
                "wallet",
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
    let member_dids: Vec<String> = members.iter().map(|m| did(m)).collect();
    let policy = field(
        &json(
            &operator,
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
        &json(
            &operator,
            &[
                "beta",
                "grant",
                "create",
                &campaign,
                "--epoch",
                EPOCH_HEX,
                "--account",
                &alice_account,
                "--class",
                "testing",
                "--amount",
                "100",
                "--memo",
                "testing balance",
                "--at-ms",
                "500",
            ],
        ),
        "grant_id",
    );
    for (index, member) in members.iter().take(2).enumerate() {
        let at = (600 + index).to_string();
        json(
            member,
            &["beta", "grant-approve", &policy, &grant, "--at-ms", &at],
        );
    }
    World {
        alice_home,
        bob_home,
        store,
        campaign,
        alice_account,
        bob_account,
    }
}

fn balance(w: &World, home: &str, account: &str) -> u64 {
    let out = run(&[
        "--json",
        "--home",
        home,
        "--store",
        &w.store,
        "beta",
        "wallet",
        "balance",
        "--epoch",
        EPOCH_HEX,
        "--campaign",
        &w.campaign,
        "--account",
        account,
    ]);
    number(&out, "amount_micro")
}

#[test]
fn accepted_grant_funds_a_balance_that_is_always_labelled_beta_and_provisional() {
    let w = world();
    let out = run(&[
        "--json",
        "--home",
        &w.alice_home,
        "--store",
        &w.store,
        "beta",
        "wallet",
        "balance",
        "--epoch",
        EPOCH_HEX,
        "--campaign",
        &w.campaign,
    ]);
    assert_eq!(number(&out, "amount_micro"), 100);
    assert!(out.contains("\"denomination\":\"BETA\""));
    assert!(out.contains("\"provisional\":true"));
    assert!(out.contains("not production MINI"));
    assert_eq!(balance(&w, &w.bob_home, &w.bob_account), 0);

    let human = run(&[
        "--home",
        &w.alice_home,
        "--store",
        &w.store,
        "beta",
        "wallet",
        "balance",
        "--epoch",
        EPOCH_HEX,
        "--campaign",
        &w.campaign,
    ]);
    assert!(human.contains("0.000100 BETA"), "{human}");
}

#[test]
fn transfer_moves_value_with_change_and_conserves_the_supply() {
    let w = world();
    let sent = run(&[
        "--json",
        "--home",
        &w.alice_home,
        "--store",
        &w.store,
        "beta",
        "transfer",
        "send",
        "--epoch",
        EPOCH_HEX,
        "--campaign",
        &w.campaign,
        "--from",
        &w.alice_account,
        "--to",
        &w.bob_account,
        "--amount",
        "30",
        "--memo",
        "lunch",
    ]);
    assert_eq!(number(&sent, "amount_micro"), 30);
    assert_eq!(number(&sent, "change_micro"), 70);
    assert_eq!(balance(&w, &w.alice_home, &w.alice_account), 70);
    assert_eq!(balance(&w, &w.bob_home, &w.bob_account), 30);
    let out = run(&[
        "--json",
        "--home",
        &w.bob_home,
        "--store",
        &w.store,
        "beta",
        "wallet",
        "balance",
        "--epoch",
        EPOCH_HEX,
        "--campaign",
        &w.campaign,
    ]);
    assert_eq!(
        number(&out, "total_issued_micro"),
        100,
        "transfers mint nothing"
    );

    // Bob can spend what he received, using the transferred output as input.
    run(&[
        "--home",
        &w.bob_home,
        "--store",
        &w.store,
        "beta",
        "transfer",
        "send",
        "--epoch",
        EPOCH_HEX,
        "--campaign",
        &w.campaign,
        "--from",
        &w.bob_account,
        "--to",
        &w.alice_account,
        "--amount",
        "30",
        "--memo",
        "repay",
    ]);
    assert_eq!(balance(&w, &w.alice_home, &w.alice_account), 100);
    assert_eq!(balance(&w, &w.bob_home, &w.bob_account), 0);
}

#[test]
fn unsafe_transfers_are_refused_before_anything_is_signed() {
    let w = world();
    let base = |from: &str, to: &str, amount: &str, home: &str| {
        run_err(&[
            "--home",
            home,
            "--store",
            &w.store,
            "beta",
            "transfer",
            "send",
            "--epoch",
            EPOCH_HEX,
            "--campaign",
            &w.campaign,
            "--from",
            from,
            "--to",
            to,
            "--amount",
            amount,
            "--memo",
            "x",
        ])
    };
    let too_much = base(&w.alice_account, &w.bob_account, "101", &w.alice_home);
    assert!(too_much.contains("insufficient BETA balance"), "{too_much}");
    let zero = base(&w.alice_account, &w.bob_account, "0", &w.alice_home);
    assert!(zero.contains("positive"), "{zero}");
    // Bob cannot spend Alice's account, even knowing its id.
    let theft = base(&w.alice_account, &w.bob_account, "10", &w.bob_home);
    assert!(
        theft.contains("not a BETA account registered by this identity"),
        "{theft}"
    );
    // The recipient must be a registered account of this epoch.
    let ghost = "ab".repeat(32);
    let unregistered = base(&w.alice_account, &ghost, "10", &w.alice_home);
    assert!(
        unregistered.contains("not a registered BETA account"),
        "{unregistered}"
    );
    // Nothing above changed any balance.
    assert_eq!(balance(&w, &w.alice_home, &w.alice_account), 100);
}

#[test]
fn issuance_needs_an_explicit_campaign_anchor_and_a_valid_epoch() {
    let w = world();
    let missing = run_err(&[
        "--home",
        &w.alice_home,
        "--store",
        &w.store,
        "beta",
        "wallet",
        "balance",
        "--epoch",
        EPOCH_HEX,
    ]);
    assert!(missing.contains("--campaign"), "{missing}");
    let bad_epoch = run_err(&[
        "--home",
        &w.alice_home,
        "--store",
        &w.store,
        "beta",
        "account",
        "new",
        "--epoch",
        "07",
    ]);
    assert!(bad_epoch.contains("64 hex"), "{bad_epoch}");
    // Another epoch's resolver sees none of this epoch's value.
    let other = "08".repeat(32);
    let out = run(&[
        "--json",
        "--home",
        &w.alice_home,
        "--store",
        &w.store,
        "beta",
        "wallet",
        "balance",
        "--epoch",
        &other,
        "--campaign",
        &w.campaign,
    ]);
    assert_eq!(number(&out, "total_issued_micro"), 0);
}

#[test]
fn accounts_are_fresh_per_registration_and_listed_per_owner() {
    let w = world();
    let second = field(
        &run(&[
            "--json",
            "--home",
            &w.alice_home,
            "--store",
            &w.store,
            "beta",
            "account",
            "new",
            "--epoch",
            EPOCH_HEX,
        ]),
        "account",
    );
    assert_ne!(second, w.alice_account, "accounts are never reused");
    let list = run(&[
        "--json",
        "--home",
        &w.alice_home,
        "--store",
        &w.store,
        "beta",
        "account",
        "list",
        "--epoch",
        EPOCH_HEX,
    ]);
    assert!(list.contains(&w.alice_account) && list.contains(&second));
    assert!(
        !list.contains(&w.bob_account),
        "another owner's account is not listed"
    );
}
