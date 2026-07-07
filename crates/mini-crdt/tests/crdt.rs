//! Integration tests: a two-human thread converging under every arrival order,
//! one-human edit authority (own second device may edit; strangers may not),
//! tombstones, late parents, and hostile-op exclusion.

use did_mini::{Capabilities, Controller, Did};
use mini_crdt::{op_add, op_edit, op_tombstone, replay};
use mini_objects::{Object, ObjectBuilder, ObjectType, Payload};

fn human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary()).unwrap();
    (root, device)
}

fn second_device(root: &mut Controller, seed: u8) -> Controller {
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed; 32], &[seed + 1; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary()).unwrap();
    device
}

/// A thread root: any object works as a doc root; use a POST.
fn thread_root(human: &Did, device: &Controller) -> Object {
    ObjectBuilder::new(ObjectType::POST)
        .payload(Payload::Public(b"thread root".to_vec()))
        .sign(human, device)
        .unwrap()
}

#[test]
fn two_human_thread_converges_under_every_permutation() {
    let (a_root, a_dev) = human(10);
    let (b_root, b_dev) = human(50);
    let root = thread_root(&a_root.did(), &a_dev);
    let doc = root.id();

    let c1 = op_add(doc, doc, b"first!", 100, 1, &a_root.did(), &a_dev).unwrap();
    let c2 = op_add(doc, c1.id(), b"reply to first", 200, 1, &b_root.did(), &b_dev).unwrap();
    let c3 = op_add(doc, doc, b"another top-level", 150, 2, &a_root.did(), &a_dev).unwrap();
    let e1 = op_edit(doc, c1.id(), b"first! (edited)", 300, 3, &a_root.did(), &a_dev).unwrap();

    let ops = [c1.clone(), c2.clone(), c3.clone(), e1.clone()];
    // All 24 permutations of 4 ops must produce identical state.
    let baseline = replay(doc, &ops);
    let idx = [
        [0, 1, 2, 3], [0, 1, 3, 2], [0, 2, 1, 3], [0, 2, 3, 1], [0, 3, 1, 2], [0, 3, 2, 1],
        [1, 0, 2, 3], [1, 0, 3, 2], [1, 2, 0, 3], [1, 2, 3, 0], [1, 3, 0, 2], [1, 3, 2, 0],
        [2, 0, 1, 3], [2, 0, 3, 1], [2, 1, 0, 3], [2, 1, 3, 0], [2, 3, 0, 1], [2, 3, 1, 0],
        [3, 0, 1, 2], [3, 0, 2, 1], [3, 1, 0, 2], [3, 1, 2, 0], [3, 2, 0, 1], [3, 2, 1, 0],
    ];
    for perm in idx {
        let shuffled: Vec<Object> = perm.iter().map(|&i| ops[i].clone()).collect();
        assert_eq!(replay(doc, &shuffled), baseline);
    }

    // Structure: two top-level nodes (by timestamp then id), one child, edit applied.
    let top = baseline.children(doc);
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].body, b"first! (edited)".to_vec());
    assert_eq!(top[1].body, b"another top-level".to_vec());
    assert_eq!(baseline.children(c1.id())[0].body, b"reply to first".to_vec());
    assert!(baseline.rejected.is_empty());
    assert!(baseline.pending.is_empty());
}

#[test]
fn one_human_edit_authority() {
    let (mut a_root, a_dev) = human(10);
    let (b_root, b_dev) = human(50);
    let root = thread_root(&a_root.did(), &a_dev);
    let doc = root.id();
    let c = op_add(doc, doc, b"mine", 100, 1, &a_root.did(), &a_dev).unwrap();

    // A's OTHER device may edit A's node (same human).
    let a_phone = second_device(&mut a_root, 30);
    let good_edit = op_edit(doc, c.id(), b"mine v2", 200, 1, &a_root.did(), &a_phone).unwrap();

    // B's edit of A's node is deterministically rejected, never applied.
    let bad_edit = op_edit(doc, c.id(), b"vandalized", 999, 9, &b_root.did(), &b_dev).unwrap();

    let state = replay(doc, &[c.clone(), good_edit, bad_edit.clone()]);
    assert_eq!(state.node(c.id()).unwrap().body, b"mine v2".to_vec());
    assert!(state.rejected.contains(bad_edit.id()));
}

#[test]
fn concurrent_edits_resolve_lww_identically_everywhere() {
    let (mut a_root, a_dev) = human(10);
    let a_phone = second_device(&mut a_root, 30);
    let root = thread_root(&a_root.did(), &a_dev);
    let doc = root.id();
    let c = op_add(doc, doc, b"v0", 100, 1, &a_root.did(), &a_dev).unwrap();

    // Two devices edit concurrently with the same sequence: id tiebreak decides,
    // identically on every replica and in every order.
    let e_laptop = op_edit(doc, c.id(), b"laptop", 200, 5, &a_root.did(), &a_dev).unwrap();
    let e_phone = op_edit(doc, c.id(), b"phone", 201, 5, &a_root.did(), &a_phone).unwrap();
    let winner = if e_laptop.id().as_str() > e_phone.id().as_str() {
        b"laptop".to_vec()
    } else {
        b"phone".to_vec()
    };

    let s1 = replay(doc, &[c.clone(), e_laptop.clone(), e_phone.clone()]);
    let s2 = replay(doc, &[e_phone, c.clone(), e_laptop]);
    assert_eq!(s1, s2);
    assert_eq!(s1.node(c.id()).unwrap().body, winner);
}

#[test]
fn tombstone_hides_from_children_but_state_is_honest() {
    let (a_root, a_dev) = human(10);
    let root = thread_root(&a_root.did(), &a_dev);
    let doc = root.id();
    let c = op_add(doc, doc, b"regret", 100, 1, &a_root.did(), &a_dev).unwrap();
    let t = op_tombstone(doc, c.id(), 200, 2, &a_root.did(), &a_dev).unwrap();

    let state = replay(doc, &[c.clone(), t]);
    assert!(state.children(doc).is_empty()); // hidden from display
    assert!(state.node(c.id()).unwrap().tombstoned); // but honestly present
}

#[test]
fn orphan_attaches_when_parent_arrives() {
    let (a_root, a_dev) = human(10);
    let (b_root, b_dev) = human(50);
    let root = thread_root(&a_root.did(), &a_dev);
    let doc = root.id();
    let parent = op_add(doc, doc, b"parent", 100, 1, &a_root.did(), &a_dev).unwrap();
    let child = op_add(doc, parent.id(), b"child", 200, 1, &b_root.did(), &b_dev).unwrap();

    // Child synced first: pending, not lost, not rejected.
    let early = replay(doc, std::slice::from_ref(&child));
    assert_eq!(early.pending, vec![child.id().clone()]);
    assert!(early.rejected.is_empty());

    // Parent arrives: the same set now attaches both.
    let full = replay(doc, &[child.clone(), parent.clone()]);
    assert!(full.pending.is_empty());
    assert_eq!(full.children(parent.id())[0].body, b"child".to_vec());
}

#[test]
fn hostile_ops_are_excluded_not_fatal() {
    let (a_root, a_dev) = human(10);
    let (b_root, b_dev) = human(50);
    let root = thread_root(&a_root.did(), &a_dev);
    let other_root = thread_root(&b_root.did(), &b_dev);
    let doc = root.id();

    let good = op_add(doc, doc, b"good", 100, 1, &a_root.did(), &a_dev).unwrap();
    // Op for a DIFFERENT document.
    let wrong_doc = op_add(other_root.id(), other_root.id(), b"x", 1, 1, &b_root.did(), &b_dev).unwrap();
    // A CRDT_OP with garbage payload.
    let garbage = ObjectBuilder::new(ObjectType::CRDT_OP)
        .link("doc", doc.clone())
        .payload(Payload::Public(vec![0xEE, 1, 2, 3]))
        .sign(&b_root.did(), &b_dev)
        .unwrap();
    // A tombstone by a stranger.
    let stranger_tomb = op_tombstone(doc, good.id(), 300, 1, &b_root.did(), &b_dev).unwrap();

    let state = replay(doc, &[good.clone(), wrong_doc.clone(), garbage.clone(), stranger_tomb.clone()]);
    assert_eq!(state.len(), 1);
    assert!(!state.node(good.id()).unwrap().tombstoned);
    for bad in [wrong_doc.id(), garbage.id(), stranger_tomb.id()] {
        assert!(state.rejected.contains(bad));
    }
}
