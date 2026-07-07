//! The self-governance loop, end to end: an outside contributor's PR is
//! reviewed and merged by an identity-root quorum; approvals are commit-bound; one root
//! counts once; the maintainer set amends itself under its own policy; forks
//! are surfaced deterministically; and PR discussion rides the CRDT.

use did_mini::{Capabilities, Controller, Did};
use mini_crdt::{op_add, replay};
use mini_crypto::HashAlgorithm;
use mini_forge::{
    amend, approve, attest, commit, merge, project, propose, put_file, put_tree, release,
    resolve_project, verify_governed_release, ForgeError, KelDirectory, Policy, ReleasePolicy,
    TreeEntry, CHAIN_TYPE, PROJECT_TYPE,
};
use mini_media::publish_media;
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{MemoryBackend, Store};

fn identity_root(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary()).unwrap();
    (root, device)
}

fn second_device(root: &mut Controller, seed: u8) -> Controller {
    let d = Controller::incept_device_single_from_seeds(&root.did(), &[seed; 32], &[seed + 1; 32])
        .unwrap();
    root.delegate_device(&d.did(), Capabilities::primary()).unwrap();
    d
}

fn a_commit(store: &mut Store<MemoryBackend>, h: &Did, d: &Controller, text: &[u8], seq: u64) -> Object {
    let f = put_file(store, h, d, text).unwrap();
    let t = put_tree(store, h, d, &[TreeEntry { name: "f.rs".into(), is_dir: false, target: f }])
        .unwrap();
    commit(store, h, d, "change", &t, &[], 100, seq).unwrap()
}

struct World {
    store: Store<MemoryBackend>,
    proj: ObjectId,
    a: (Controller, Controller),
    b: (Controller, Controller),
    c: (Controller, Controller),
    contrib: (Controller, Controller),
}

impl World {
    /// An oracle vouching for every base participant. Tests add extra devices /
    /// identity roots via `with`.
    fn oracle(&self) -> KelDirectory {
        let mut d = KelDirectory::new();
        for (r, dev) in [&self.a, &self.b, &self.c, &self.contrib] {
            d.insert(r.kel());
            d.insert(dev.kel());
        }
        d
    }
}

fn vouch(dir: &mut KelDirectory, cs: &[&Controller]) {
    for c in cs {
        dir.insert(c.kel());
    }
}

fn put_str_for_test(w: &mut Vec<u8>, s: &str) {
    w.extend_from_slice(&(s.len() as u32).to_be_bytes());
    w.extend_from_slice(s.as_bytes());
}

fn encode_policy_for_test(w: &mut Vec<u8>, policy: &Policy) {
    w.extend_from_slice(&policy.min_approvals.to_be_bytes());
    w.extend_from_slice(&(policy.maintainers.len() as u32).to_be_bytes());
    for m in &policy.maintainers {
        put_str_for_test(w, m.as_str());
    }
}

fn world() -> World {
    let a = identity_root(10);
    let b = identity_root(50);
    let c = identity_root(90);
    let contrib = identity_root(130);
    let mut store = Store::new(MemoryBackend::new());
    let policy = Policy {
        min_approvals: 2,
        maintainers: vec![a.0.did(), b.0.did(), c.0.did()],
    };
    let proj = project(&mut store, &a.0.did(), &a.1, "mininet", &policy)
        .unwrap()
        .id()
        .clone();
    World { store, proj, a, b, c, contrib }
}

#[test]
fn outside_contributor_pr_merges_under_identity_root_quorum() {
    let mut w = world();
    let head = a_commit(&mut w.store, &w.contrib.0.did(), &w.contrib.1, b"fix", 1);

    // Anyone may propose — the contributor is NOT a maintainer.
    let pr = propose(
        &mut w.store, &w.contrib.0.did(), &w.contrib.1, &w.proj, "main", "fix bug",
        head.id(), &w.proj, 200, 1,
    )
    .unwrap();

    // Two maintainers approve, bound to the exact head.
    approve(&mut w.store, &w.a.0.did(), &w.a.1, pr.id(), head.id(), true, 300, 1).unwrap();
    approve(&mut w.store, &w.b.0.did(), &w.b.1, pr.id(), head.id(), true, 301, 1).unwrap();

    // A maintainer records the merge.
    merge(&mut w.store, &w.c.0.did(), &w.c.1, &w.proj, &w.proj, pr.id(), 400, 1).unwrap();

    let state = resolve_project(&w.store, &w.oracle(), &w.proj).unwrap();
    assert_eq!(state.entries, 1);
    assert_eq!(state.branches, vec![("main".to_string(), head.id().clone())]);
    assert!(!state.forks_detected);

    // A merge recorded by a NON-maintainer is ignored entirely.
    let head2 = a_commit(&mut w.store, &w.contrib.0.did(), &w.contrib.1, b"more", 2);
    let pr2 = propose(
        &mut w.store, &w.contrib.0.did(), &w.contrib.1, &w.proj, "main", "more",
        head2.id(), &state.tip, 500, 2,
    )
    .unwrap();
    approve(&mut w.store, &w.a.0.did(), &w.a.1, pr2.id(), head2.id(), true, 501, 2).unwrap();
    approve(&mut w.store, &w.b.0.did(), &w.b.1, pr2.id(), head2.id(), true, 502, 2).unwrap();
    merge(&mut w.store, &w.contrib.0.did(), &w.contrib.1, &w.proj, &state.tip, pr2.id(), 503, 1)
        .unwrap();
    assert_eq!(resolve_project(&w.store, &w.oracle(), &w.proj).unwrap().entries, 1);
}

#[test]
fn author_never_counts_and_one_identity_root_counts_once() {
    let mut w = world();
    // Maintainer A authors the PR themselves.
    let head = a_commit(&mut w.store, &w.a.0.did(), &w.a.1, b"mine", 1);
    let pr = propose(
        &mut w.store, &w.a.0.did(), &w.a.1, &w.proj, "main", "self", head.id(), &w.proj, 200, 1,
    )
    .unwrap();

    // A approves their own PR (never counts); B approves from TWO devices
    // (counts once). Quorum = 1 < 2 → no merge applies.
    approve(&mut w.store, &w.a.0.did(), &w.a.1, pr.id(), head.id(), true, 300, 1).unwrap();
    let b_phone = second_device(&mut w.b.0, 70);
    approve(&mut w.store, &w.b.0.did(), &w.b.1, pr.id(), head.id(), true, 301, 1).unwrap();
    approve(&mut w.store, &w.b.0.did(), &b_phone, pr.id(), head.id(), true, 302, 2).unwrap();
    merge(&mut w.store, &w.a.0.did(), &w.a.1, &w.proj, &w.proj, pr.id(), 400, 1).unwrap();
    let mut o = w.oracle();
    vouch(&mut o, &[&b_phone]);
    assert_eq!(resolve_project(&w.store, &o, &w.proj).unwrap().entries, 0);

    // A second distinct identity root approves → now it applies.
    approve(&mut w.store, &w.c.0.did(), &w.c.1, pr.id(), head.id(), true, 500, 1).unwrap();
    assert_eq!(resolve_project(&w.store, &o, &w.proj).unwrap().entries, 1);
}

#[test]
fn approvals_are_bound_to_the_exact_commit() {
    let mut w = world();
    let reviewed = a_commit(&mut w.store, &w.contrib.0.did(), &w.contrib.1, b"v1", 1);
    let swapped = a_commit(&mut w.store, &w.contrib.0.did(), &w.contrib.1, b"evil v2", 2);

    // The PR links the SWAPPED head, but approvals name the reviewed one.
    let pr = propose(
        &mut w.store, &w.contrib.0.did(), &w.contrib.1, &w.proj, "main", "swap",
        swapped.id(), &w.proj, 200, 1,
    )
    .unwrap();
    approve(&mut w.store, &w.a.0.did(), &w.a.1, pr.id(), reviewed.id(), true, 300, 1).unwrap();
    approve(&mut w.store, &w.b.0.did(), &w.b.1, pr.id(), reviewed.id(), true, 301, 1).unwrap();
    merge(&mut w.store, &w.c.0.did(), &w.c.1, &w.proj, &w.proj, pr.id(), 400, 1).unwrap();

    assert_eq!(resolve_project(&w.store, &w.oracle(), &w.proj).unwrap().entries, 0);
}

#[test]
fn the_maintainer_set_amends_itself_under_its_own_policy() {
    let mut w = world();
    let d = identity_root(170);

    // Amendment: remove A, add D (policy still 2-of-N), recorded by B.
    let new_policy = Policy {
        min_approvals: 2,
        maintainers: vec![w.b.0.did(), w.c.0.did(), d.0.did()],
    };
    let entry = amend(&mut w.store, &w.b.0.did(), &w.b.1, &w.proj, &w.proj, &new_policy, 200, 1)
        .unwrap();
    // Approved under the CURRENT policy by two current maintainers (A and C).
    approve(&mut w.store, &w.a.0.did(), &w.a.1, entry.id(), entry.id(), true, 300, 1).unwrap();
    approve(&mut w.store, &w.c.0.did(), &w.c.1, entry.id(), entry.id(), true, 301, 1).unwrap();

    let mut o = w.oracle();
    vouch(&mut o, &[&d.0, &d.1]);
    let state = resolve_project(&w.store, &o, &w.proj).unwrap();
    assert_eq!(state.entries, 1);
    assert!(state.policy.maintainers.iter().any(|m| m.as_str() == d.0.did().as_str()));
    assert!(!state.policy.maintainers.iter().any(|m| m.as_str() == w.a.0.did().as_str()));

    // Forward-only power: a PR approved by A (removed) + B fails quorum;
    // approved by D (added) + B succeeds.
    let head = a_commit(&mut w.store, &w.contrib.0.did(), &w.contrib.1, b"after", 1);
    let pr = propose(
        &mut w.store, &w.contrib.0.did(), &w.contrib.1, &w.proj, "main", "after",
        head.id(), &state.tip, 400, 1,
    )
    .unwrap();
    approve(&mut w.store, &w.a.0.did(), &w.a.1, pr.id(), head.id(), true, 500, 2).unwrap();
    approve(&mut w.store, &w.b.0.did(), &w.b.1, pr.id(), head.id(), true, 501, 2).unwrap();
    merge(&mut w.store, &w.c.0.did(), &w.c.1, &w.proj, &state.tip, pr.id(), 600, 1).unwrap();
    assert_eq!(resolve_project(&w.store, &o, &w.proj).unwrap().entries, 1); // still just the amendment

    approve(&mut w.store, &d.0.did(), &d.1, pr.id(), head.id(), true, 700, 1).unwrap();
    assert_eq!(resolve_project(&w.store, &o, &w.proj).unwrap().entries, 2);
}

#[test]
fn competing_valid_merges_resolve_deterministically_and_are_flagged() {
    let mut w = world();
    let h1 = a_commit(&mut w.store, &w.contrib.0.did(), &w.contrib.1, b"one", 1);
    let h2 = a_commit(&mut w.store, &w.contrib.0.did(), &w.contrib.1, b"two", 2);
    for (h, seq) in [(&h1, 1u64), (&h2, 2u64)] {
        let pr = propose(
            &mut w.store, &w.contrib.0.did(), &w.contrib.1, &w.proj, "main", "race",
            h.id(), &w.proj, 200, seq,
        )
        .unwrap();
        approve(&mut w.store, &w.a.0.did(), &w.a.1, pr.id(), h.id(), true, 300, seq).unwrap();
        approve(&mut w.store, &w.b.0.did(), &w.b.1, pr.id(), h.id(), true, 301, seq).unwrap();
        merge(&mut w.store, &w.c.0.did(), &w.c.1, &w.proj, &w.proj, pr.id(), 400, seq).unwrap();
    }
    let s1 = resolve_project(&w.store, &w.oracle(), &w.proj).unwrap();
    let s2 = resolve_project(&w.store, &w.oracle(), &w.proj).unwrap();
    assert_eq!(s1, s2); // deterministic
    assert!(s1.forks_detected); // and honest about it
    assert_eq!(s1.entries, 1); // the loser does not chain (its prev was taken)
}


#[test]
fn malformed_genesis_project_name_is_rejected_on_decode() {
    let a = identity_root(10);
    let mut store = Store::new(MemoryBackend::new());
    let policy = Policy { min_approvals: 1, maintainers: vec![a.0.did()] };
    let mut payload = Vec::new();
    put_str_for_test(&mut payload, "bad/name");
    encode_policy_for_test(&mut payload, &policy);
    let obj = ObjectBuilder::new(ObjectType::Custom(PROJECT_TYPE.to_string()))
        .payload(Payload::Public(payload))
        .sign(&a.0.did(), &a.1)
        .unwrap();
    store.insert(&obj).unwrap();
    let mut oracle = KelDirectory::new();
    vouch(&mut oracle, &[&a.0, &a.1]);
    assert_eq!(resolve_project(&store, &oracle, obj.id()), Err(ForgeError::BadObject));
}

#[test]
fn chain_entries_with_trailing_payload_are_ignored() {
    let mut w = world();
    let head = a_commit(&mut w.store, &w.contrib.0.did(), &w.contrib.1, b"fix", 1);
    let pr = propose(
        &mut w.store, &w.contrib.0.did(), &w.contrib.1, &w.proj, "main", "fix",
        head.id(), &w.proj, 200, 1,
    ).unwrap();
    approve(&mut w.store, &w.a.0.did(), &w.a.1, pr.id(), head.id(), true, 300, 1).unwrap();
    approve(&mut w.store, &w.b.0.did(), &w.b.1, pr.id(), head.id(), true, 301, 1).unwrap();

    let bad = ObjectBuilder::new(ObjectType::Custom(CHAIN_TYPE.to_string()))
        .timestamp_ms(400)
        .sequence(1)
        .payload(Payload::Public(vec![1, 0])) // ENTRY_MERGE plus forbidden trailing byte
        .link("project", w.proj.clone())
        .link("prev", w.proj.clone())
        .link("pr", pr.id().clone())
        .sign(&w.c.0.did(), &w.c.1)
        .unwrap();
    w.store.insert(&bad).unwrap();

    assert_eq!(resolve_project(&w.store, &w.oracle(), &w.proj).unwrap().entries, 0);
}

#[test]
fn pr_discussion_rides_the_crdt() {
    let mut w = world();
    let head = a_commit(&mut w.store, &w.contrib.0.did(), &w.contrib.1, b"talk", 1);
    let pr = propose(
        &mut w.store, &w.contrib.0.did(), &w.contrib.1, &w.proj, "main", "talk",
        head.id(), &w.proj, 200, 1,
    )
    .unwrap();

    // Review conversation: ops with the PR object as doc root.
    let c1 = op_add(pr.id(), pr.id(), b"why this approach?", 300, 1, &w.a.0.did(), &w.a.1).unwrap();
    let c2 = op_add(pr.id(), c1.id(), b"benchmarks attached", 400, 1, &w.contrib.0.did(), &w.contrib.1)
        .unwrap();
    let state = replay(pr.id(), &[c2.clone(), c1.clone()]);
    assert_eq!(state.children(pr.id()).len(), 1);
    assert_eq!(state.children(c1.id())[0].body, b"benchmarks attached".to_vec());
}

#[test]
fn approvals_from_unvouched_authors_do_not_count() {
    // Same objects, but the oracle omits maintainer C. Under the re-binding rule,
    // C's approval cannot count, so a 2-of-N merge that relied on it fails to apply.
    let mut w = world();
    let head = a_commit(&mut w.store, &w.contrib.0.did(), &w.contrib.1, b"fix", 1);
    let pr = propose(
        &mut w.store, &w.contrib.0.did(), &w.contrib.1, &w.proj, "main", "fix",
        head.id(), &w.proj, 200, 1,
    )
    .unwrap();
    approve(&mut w.store, &w.a.0.did(), &w.a.1, pr.id(), head.id(), true, 300, 1).unwrap();
    approve(&mut w.store, &w.c.0.did(), &w.c.1, pr.id(), head.id(), true, 301, 1).unwrap();
    merge(&mut w.store, &w.b.0.did(), &w.b.1, &w.proj, &w.proj, pr.id(), 400, 1).unwrap();

    // Full oracle: both approvals count → merge applies.
    assert_eq!(resolve_project(&w.store, &w.oracle(), &w.proj).unwrap().entries, 1);

    // Oracle missing C (and the contrib author): C's approval can't count → quorum 1 < 2.
    let mut partial = KelDirectory::new();
    vouch(&mut partial, &[&w.a.0, &w.a.1, &w.b.0, &w.b.1, &w.contrib.0, &w.contrib.1]);
    assert_eq!(resolve_project(&w.store, &partial, &w.proj).unwrap().entries, 0);
}

#[test]
fn the_full_build_from_inside_mini_loop_is_machine_enforced() {
    // PR → identity-root-quorum merge → canonical head → release from that head →
    // independent attestations → governed verification. One validity chain.
    let mut w = world();
    let head = a_commit(&mut w.store, &w.contrib.0.did(), &w.contrib.1, b"the change", 1);
    let pr = propose(
        &mut w.store, &w.contrib.0.did(), &w.contrib.1, &w.proj, "main", "ship it",
        head.id(), &w.proj, 200, 1,
    )
    .unwrap();
    approve(&mut w.store, &w.a.0.did(), &w.a.1, pr.id(), head.id(), true, 300, 1).unwrap();
    approve(&mut w.store, &w.b.0.did(), &w.b.1, pr.id(), head.id(), true, 301, 1).unwrap();
    merge(&mut w.store, &w.c.0.did(), &w.c.1, &w.proj, &w.proj, pr.id(), 400, 1).unwrap();

    // Build artifact "from" the canonical commit; release it.
    let artifact = b"reproducible binary".to_vec();
    let digest = HashAlgorithm::Blake3.digest(&artifact);
    let manifest = publish_media(
        &mut w.store, &w.a.0.did(), &w.a.1, "application/octet-stream", &artifact, 500, 1,
    )
    .unwrap();
    let rel = release(
        &mut w.store, &w.a.0.did(), &w.a.1, "1.0.0", &w.proj, "main", head.id(), &manifest.id,
        digest, [9u8; 32], 600, 2,
    )
    .unwrap();

    // Two independent verified identity roots attest the reproducible build.
    let mut oracle = w.oracle();
    for seed in [170u8, 210] {
        let (r, d) = identity_root(seed);
        attest(&mut w.store, &r.did(), &d, rel.id(), digest, 700, 1).unwrap();
        vouch(&mut oracle, &[&r, &d]);
    }

    let policy = ReleasePolicy {
        min_attestations: 2,
        timelock_ms: 3_600_000,
        now_ms: 600 + 3_600_001,
    };
    let v = verify_governed_release(&w.store, &oracle, rel.id(), &w.proj, "main", &policy)
        .unwrap();
    assert_eq!(v.version, "1.0.0");
    assert_eq!(v.attesters, 2);

    // A release from a commit that was NEVER merged through governance is
    // refused, even with valid attestations.
    let rogue_head = a_commit(&mut w.store, &w.a.0.did(), &w.a.1, b"rogue", 9);
    let rogue = release(
        &mut w.store, &w.a.0.did(), &w.a.1, "1.0.1", &w.proj, "main", rogue_head.id(),
        &manifest.id, digest, [9u8; 32], 600, 3,
    )
    .unwrap();
    for seed in [170u8, 210] {
        let (r, d) = identity_root(seed);
        attest(&mut w.store, &r.did(), &d, rogue.id(), digest, 700, 2).unwrap();
    }
    assert_eq!(
        verify_governed_release(&w.store, &oracle, rogue.id(), &w.proj, "main", &policy),
        Err(ForgeError::NotCanonical)
    );
}

#[test]
fn adoption_refuses_on_governance_forks() {
    // Same fork setup as the display-level test — but ADOPTION must refuse.
    let mut w = world();
    let h1 = a_commit(&mut w.store, &w.contrib.0.did(), &w.contrib.1, b"one", 1);
    let h2 = a_commit(&mut w.store, &w.contrib.0.did(), &w.contrib.1, b"two", 2);
    let mut heads = Vec::new();
    for (h, seq) in [(&h1, 1u64), (&h2, 2u64)] {
        let pr = propose(
            &mut w.store, &w.contrib.0.did(), &w.contrib.1, &w.proj, "main", "race",
            h.id(), &w.proj, 200, seq,
        )
        .unwrap();
        approve(&mut w.store, &w.a.0.did(), &w.a.1, pr.id(), h.id(), true, 300, seq).unwrap();
        approve(&mut w.store, &w.b.0.did(), &w.b.1, pr.id(), h.id(), true, 301, seq).unwrap();
        merge(&mut w.store, &w.c.0.did(), &w.c.1, &w.proj, &w.proj, pr.id(), 400, seq).unwrap();
        heads.push(h.id().clone());
    }
    let artifact = b"bin".to_vec();
    let digest = HashAlgorithm::Blake3.digest(&artifact);
    let manifest = publish_media(
        &mut w.store, &w.a.0.did(), &w.a.1, "application/octet-stream", &artifact, 500, 1,
    )
    .unwrap();
    // Release from whichever head won the provisional tiebreak — adoption must
    // STILL refuse, because a fork exists at all.
    let winner = resolve_project(&w.store, &w.oracle(), &w.proj).unwrap().branches[0].1.clone();
    let rel = release(
        &mut w.store, &w.a.0.did(), &w.a.1, "1.0.0", &w.proj, "main", &winner, &manifest.id,
        digest, [9u8; 32], 600, 2,
    )
    .unwrap();
    let mut oracle = w.oracle();
    for seed in [170u8, 210] {
        let (r, d) = identity_root(seed);
        attest(&mut w.store, &r.did(), &d, rel.id(), digest, 700, 1).unwrap();
        vouch(&mut oracle, &[&r, &d]);
    }
    let policy = ReleasePolicy {
        min_attestations: 2,
        timelock_ms: 3_600_000,
        now_ms: 600 + 3_600_001,
    };
    // Floor-valid policy, so the refusal is specifically the governance fork.
    assert!(matches!(
        verify_governed_release(&w.store, &oracle, rel.id(), &w.proj, "main", &policy),
        Err(ForgeError::ForkDetected)
    ));
}
