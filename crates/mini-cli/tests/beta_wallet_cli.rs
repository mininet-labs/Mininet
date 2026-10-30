//! Beta MINI wallet CLI: accounts, provisional balances and transfers over the
//! durable signed-object execution core. Test value only.

use std::path::PathBuf;

use did_mini::Controller;
use mini_beta::{
    create_campaign, create_grant_authorization, BetaAccountId, BetaEpochId, GrantClass,
};
use mini_beta_grants::{create_grant_approval, create_grant_policy};
use mini_objects::{ObjectBuilder, ObjectType, Payload};

const EPOCH_HEX: &str = "0707070707070707070707070707070707070707070707070707070707070707";

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
    mini_cli::run(&owned).unwrap_or_else(|e| panic!("command {args:?} failed: {e}"))
}

fn run_err(args: &[&str]) -> String {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    mini_cli::run(&owned)
        .expect_err(&format!("expected failure for {args:?}"))
        .to_string()
}

/// First string value of `"key":"..."` in the single-line JSON envelope.
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

fn signer(seed: u8) -> Controller {
    Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap()
}

struct World {
    alice_home: String,
    bob_home: String,
    store: String,
    campaign: String,
    alice_account: String,
    bob_account: String,
}

/// Alice and Bob each register a fresh account through the CLI; a campaign with
/// a 2-of-3 policy then funds Alice with an accepted 100-unit testing grant.
fn world() -> World {
    let alice_home = tempdir("alice").to_str().unwrap().to_string();
    let bob_home = tempdir("bob").to_str().unwrap().to_string();
    let store = tempdir("store").to_str().unwrap().to_string();
    run(&["--home", &alice_home, "identity", "init"]);
    run(&["--home", &bob_home, "identity", "init"]);

    let register = |home: &str| {
        let out = run(&[
            "--json", "--home", home, "--store", &store, "beta", "account", "new", "--epoch",
            EPOCH_HEX,
        ]);
        field(&out, "account")
    };
    let alice_account = register(&alice_home);
    let bob_account = register(&bob_home);

    let author = signer(10);
    let authorizers = [signer(20), signer(30), signer(40)];
    let epoch = BetaEpochId::new([7; 32]).unwrap();
    let mut s = mini_cli::store::open_store(std::path::Path::new(&store)).unwrap();
    let target = ObjectBuilder::new(ObjectType::Custom("mininet.beta-wallet/test-target".into()))
        .timestamp_ms(1)
        .sequence(1)
        .payload(Payload::Public(b"wallet-cli-fixture".to_vec()))
        .sign(&author.did(), &author)
        .unwrap();
    s.insert(&target).unwrap();
    let campaign = create_campaign(
        &mut s,
        &author.did(),
        &author,
        target.id(),
        epoch,
        "wallet beta",
        "exercise the wallet",
        &["wallet".to_string()],
        100,
        10_000,
        1_000,
        100,
        2,
    )
    .unwrap();
    let members: Vec<_> = authorizers.iter().map(Controller::did).collect();
    let policy = create_grant_policy(
        &mut s,
        &author.did(),
        &author,
        campaign.id(),
        &members,
        2,
        2,
        100,
        &[25, 50, 100],
        200,
        9_000,
        150,
        3,
    )
    .unwrap();
    let mut account_bytes = [0u8; 32];
    for (i, byte) in account_bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&alice_account[i * 2..i * 2 + 2], 16).unwrap();
    }
    let grant = create_grant_authorization(
        &mut s,
        &author.did(),
        &author,
        campaign.id(),
        None,
        epoch,
        BetaAccountId::new(account_bytes).unwrap(),
        GrantClass::Testing,
        100,
        "testing balance",
        500,
        4,
    )
    .unwrap();
    for (index, member) in authorizers.iter().take(2).enumerate() {
        create_grant_approval(
            &mut s,
            &member.did(),
            member,
            policy.id(),
            grant.id(),
            600 + index as u64,
            1,
        )
        .unwrap();
    }
    World {
        alice_home,
        bob_home,
        store,
        campaign: campaign.id().as_str().to_string(),
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
