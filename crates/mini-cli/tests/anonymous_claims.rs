//! Claims from signers the operator has not trusted are shown as key-controlled only.
//!
//! `mini beta task claim` signs with a fresh, unsaved identity, so no viewer can
//! have trusted it. `mini task show` therefore lists it apart from the trusted
//! `claims`, as `untrusted_claims` (which also covers any peer the operator
//! simply has not trusted), from the self-certifying key history the store
//! holds. These tests pin the safety properties of that view: the list is
//! capped, a claim whose signer has no key history in the store never appears,
//! and a claim by a trusted identity is listed once, as trusted.

use std::fs;
use std::path::PathBuf;

fn tempdir(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "mini-cli-anon-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    path
}

fn run(args: &[&str]) -> String {
    let owned: Vec<String> = args.iter().map(|value| (*value).to_string()).collect();
    mini_cli::run(&owned).unwrap_or_else(|error| panic!("command {args:?} failed: {error}"))
}

fn last_word(output: &str) -> String {
    output.split_whitespace().last().unwrap().to_string()
}

fn value_after(output: &str, marker: &str) -> String {
    output
        .split(marker)
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string()
}

struct World {
    home: PathBuf,
    store: PathBuf,
    task: String,
}

impl World {
    fn new(tag: &str) -> Self {
        let home = tempdir(&format!("{tag}-home"));
        let store = tempdir(&format!("{tag}-store"));
        let (h, s) = (home.to_str().unwrap(), store.to_str().unwrap());
        run(&["--home", h, "identity", "init"]);
        let shown = run(&["--home", h, "identity", "show"]);
        let did = shown
            .lines()
            .find(|line| line.starts_with("human:"))
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .to_string();
        run(&[
            "--home",
            h,
            "--store",
            s,
            "repo",
            "init",
            "anon-demo",
            "--maintainer",
            &did,
            "--min-approvals",
            "1",
        ]);
        let charter = value_after(
            &run(&[
                "--home",
                h,
                "--store",
                s,
                "team",
                "propose",
                "anon-demo",
                "--group-id",
                "wg-anon",
                "--name",
                "Anon",
                "--purpose",
                "route work",
                "--path",
                "crates/mini-cli/**",
                "--autonomous",
                "ordinary work",
                "--reserved",
                "amendments",
                "--term-policy",
                "expiring terms",
                "--appeal-policy",
                "cross-group appeal",
            ]),
            "working-group charter proposed: ",
        );
        let task = last_word(&run(&[
            "--home",
            h,
            "--store",
            s,
            "task",
            "create",
            "anon-demo",
            "--team",
            &charter,
            "--route",
            "rust",
            "--risk",
            "routine",
            "--title",
            "anonymous claims",
            "--description",
            "check the claim view",
            "--path",
            "crates/mini-cli/**",
            "--evidence",
            "cargo test -p mini-cli",
            "--acceptance",
            "claims are visible",
            "--non-goal",
            "none",
        ]));
        World { home, store, task }
    }

    fn anonymous_claim(&self) {
        run(&[
            "--home",
            self.home.to_str().unwrap(),
            "--store",
            self.store.to_str().unwrap(),
            "beta",
            "task",
            "claim",
            &self.task,
            "--role",
            "tester",
            "--path",
            "crates/mini-cli/**",
            "--expires-ms",
            "4102444800000",
        ]);
    }

    fn show_json(&self) -> String {
        run(&[
            "--json",
            "--home",
            self.home.to_str().unwrap(),
            "--store",
            self.store.to_str().unwrap(),
            "task",
            "show",
            &self.task,
        ])
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.home);
        let _ = fs::remove_dir_all(&self.store);
    }
}

#[test]
fn the_untrusted_list_is_capped_and_the_rest_are_counted() {
    let world = World::new("cap");
    for _ in 0..34 {
        world.anonymous_claim();
    }
    let shown = world.show_json();
    assert_eq!(
        shown.matches("\"signer_trusted\":false").count(),
        32,
        "{shown}"
    );
    assert!(
        shown.contains("\"untrusted_claims_not_shown\":2"),
        "{shown}"
    );
    // None of them is a trusted claim.
    assert!(shown.contains("\"claims\":[]"), "{shown}");
}

#[test]
fn a_claim_whose_signer_has_no_key_history_in_the_store_never_appears() {
    let world = World::new("forged");
    world.anonymous_claim();
    // A claim signed by an identity whose key history was never stored: it
    // verifies against nothing, so it must not be listed as untrusted either.
    let mut store = mini_store::Store::new(
        mini_store::FsBackend::open(std::path::Path::new(&world.store)).unwrap(),
    );
    let human = did_mini::Controller::incept_single().unwrap();
    let device = did_mini::Controller::incept_device(
        &human.did(),
        vec![mini_crypto::SigningKey::generate().unwrap()],
        1,
        vec![mini_crypto::SigningKey::generate().unwrap()],
        1,
    )
    .unwrap();
    let mut delegating = human;
    delegating
        .delegate_device(&device.did(), did_mini::Capabilities::POST)
        .unwrap();
    let task_id = mini_objects::ObjectId::parse(&world.task).unwrap();
    mini_forge::create_work_claim(
        &mut store,
        &delegating.did(),
        &device,
        &task_id,
        "impostor",
        &["crates/mini-cli/**".to_string()],
        None,
        4_102_444_800_000,
        "",
        1,
        1,
    )
    .unwrap();
    drop(store);
    let shown = world.show_json();
    assert_eq!(
        shown.matches("\"signer_trusted\":false").count(),
        1,
        "{shown}"
    );
    assert!(!shown.contains("impostor"), "{shown}");
}

#[test]
fn a_peer_attaching_hundreds_of_claims_cannot_grow_the_view_and_the_newest_survive() {
    let world = World::new("flood");
    let mut store = mini_store::Store::new(
        mini_store::FsBackend::open(std::path::Path::new(&world.store)).unwrap(),
    );
    // One peer, two key-history carriers, 300 valid claims with rising times;
    // all leases are in the future. Another 40 claims are signed by a
    // different identity whose key history is not in the store, with the very
    // newest times: they verify against nothing and must not push real ones
    // out of the pool.
    let mut human = did_mini::Controller::incept_single().unwrap();
    let device = did_mini::Controller::incept_device(
        &human.did(),
        vec![mini_crypto::SigningKey::generate().unwrap()],
        1,
        vec![mini_crypto::SigningKey::generate().unwrap()],
        1,
    )
    .unwrap();
    human
        .delegate_device(&device.did(), did_mini::Capabilities::POST)
        .unwrap();
    for kel in [human.kel(), device.kel()] {
        store
            .insert(&mini_sync::kel_carrier(&kel, &human.did(), &device).unwrap())
            .unwrap();
    }
    let task_id = mini_objects::ObjectId::parse(&world.task).unwrap();
    for n in 1..=300u64 {
        mini_forge::create_work_claim(
            &mut store,
            &human.did(),
            &device,
            &task_id,
            &format!("flood {n}"),
            &["crates/mini-cli/**".to_string()],
            None,
            4_102_444_800_000,
            "",
            n,
            n,
        )
        .unwrap();
    }
    let mut stranger = did_mini::Controller::incept_single().unwrap();
    let stranger_device = did_mini::Controller::incept_device(
        &stranger.did(),
        vec![mini_crypto::SigningKey::generate().unwrap()],
        1,
        vec![mini_crypto::SigningKey::generate().unwrap()],
        1,
    )
    .unwrap();
    stranger
        .delegate_device(&stranger_device.did(), did_mini::Capabilities::POST)
        .unwrap();
    for n in 1..=40u64 {
        mini_forge::create_work_claim(
            &mut store,
            &stranger.did(),
            &stranger_device,
            &task_id,
            &format!("unverifiable {n}"),
            &["crates/mini-cli/**".to_string()],
            None,
            4_102_444_800_000,
            "",
            1_000 + n,
            n,
        )
        .unwrap();
    }
    drop(store);
    let shown = world.show_json();
    // Only 32 listed, the pool (256) minus the 32 shown is counted, and the
    // task is flagged as having older claims that were not examined.
    assert_eq!(
        shown.matches("\"signer_trusted\":false").count(),
        32,
        "{shown}"
    );
    assert!(
        shown.contains("\"untrusted_claims_not_shown\":224"),
        "{shown}"
    );
    assert!(
        shown.contains("\"untrusted_claims_older_not_examined\":true"),
        "{shown}"
    );
    // The newest real claims are the ones kept; unverifiable ones never appear.
    assert!(shown.contains("flood 300"), "{shown}");
    assert!(!shown.contains("unverifiable"), "{shown}");
    assert!(!shown.contains("flood 1\""), "{shown}");
}

#[test]
fn an_untrusted_persistent_peer_is_listed_as_untrusted_then_as_trusted_once_trusted() {
    let world = World::new("trusted");
    let bob = tempdir("trusted-bob");
    let (h, s) = (world.home.to_str().unwrap(), world.store.to_str().unwrap());
    run(&["--home", bob.to_str().unwrap(), "identity", "init"]);
    // Bob is an ordinary peer with a persistent identity, not an anonymous
    // claimant. His key history reaches the store (as it would over sync) but
    // the operator has not trusted it.
    {
        let identity = mini_cli::identity::load(&bob).unwrap();
        let mut store = mini_store::Store::new(
            mini_store::FsBackend::open(std::path::Path::new(&world.store)).unwrap(),
        );
        for kel in [identity.human.kel(), identity.device.kel()] {
            let carrier =
                mini_sync::kel_carrier(&kel, &identity.human_did(), &identity.device).unwrap();
            store.insert(&carrier).unwrap();
        }
    }
    run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        s,
        "task",
        "claim",
        &world.task,
        "--role",
        "rust contributor",
        "--path",
        "crates/mini-cli/**",
        "--expires-ms",
        "4102444800000",
    ]);
    // Untrusted: listed apart from trusted claims, and not called throwaway.
    let before = world.show_json();
    assert!(before.contains("\"claims\":[]"), "{before}");
    assert!(before.contains("\"signer_trusted\":false"), "{before}");
    assert!(before.contains("rust contributor"), "{before}");
    let human = run(&["--home", h, "--store", s, "task", "show", &world.task]);
    assert!(
        human.contains("claims from signers you have not trusted"),
        "{human}"
    );
    assert!(
        !human.contains("throwaway identity, not a known"),
        "{human}"
    );
    // Once the operator trusts Bob, the claim moves to the trusted list and is
    // not listed twice.
    let bob_kel = run(&["--home", bob.to_str().unwrap(), "kel", "export"]);
    run(&["--home", h, "kel", "trust", bob_kel.trim()]);
    let after = world.show_json();
    assert!(after.contains("rust contributor"), "{after}");
    assert!(after.contains("\"untrusted_claims\":[]"), "{after}");
    assert!(!after.contains("\"claims\":[]"), "{after}");
    let _ = fs::remove_dir_all(bob);
}
