//! Import-direction tests for the git SHA-256 bridge (D-0418).
//!
//! The core property under test: a commit chain built by one identity,
//! exported via `export_commit_chain`, and imported by a **different**
//! identity reconstructs byte-identical content (re-exporting the
//! imported tree gives the exact same blob/tree git ids) while the
//! imported commit's signed author is the importer, never the original
//! author -- and the original git commit id/author/committer survive only
//! as a separate, explicitly-labeled `GitImportProvenance` record.

use std::collections::BTreeMap;

use did_mini::{Capabilities, Controller};
use mini_forge::{
    checkout, commit, export_commit_chain, import_commit_chain, put_file, put_tree,
    read_git_import_provenance, GitObject, GitObjectKind, TreeEntry,
};
use mini_store::{MemoryBackend, Store};

fn human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

fn objects_map(objects: Vec<GitObject>) -> BTreeMap<String, GitObject> {
    objects.into_iter().map(|o| (o.id.clone(), o)).collect()
}

#[test]
fn a_two_commit_chain_round_trips_content_identically_under_a_different_importer() {
    let (author, author_dev) = human(10);
    let mut store = Store::new(MemoryBackend::new());

    let readme = b"# demo".to_vec();
    let readme_blob = put_file(&mut store, &author.did(), &author_dev, &readme).unwrap();
    let tree1 = put_tree(
        &mut store,
        &author.did(),
        &author_dev,
        &[TreeEntry {
            name: "README.md".into(),
            is_dir: false,
            target: readme_blob.clone(),
        }],
    )
    .unwrap();
    let c1 = commit(
        &mut store,
        &author.did(),
        &author_dev,
        "init",
        &tree1,
        &[],
        100_000,
        1,
    )
    .unwrap();

    let nested = b"mod inner;".to_vec();
    let nested_blob = put_file(&mut store, &author.did(), &author_dev, &nested).unwrap();
    let subtree = put_tree(
        &mut store,
        &author.did(),
        &author_dev,
        &[TreeEntry {
            name: "inner.rs".into(),
            is_dir: false,
            target: nested_blob,
        }],
    )
    .unwrap();
    let tree2 = put_tree(
        &mut store,
        &author.did(),
        &author_dev,
        &[
            TreeEntry {
                name: "README.md".into(),
                is_dir: false,
                target: readme_blob,
            },
            TreeEntry {
                name: "src".into(),
                is_dir: true,
                target: subtree,
            },
        ],
    )
    .unwrap();
    let c2 = commit(
        &mut store,
        &author.did(),
        &author_dev,
        "add src/inner.rs",
        &tree2,
        &[c1.id().clone()],
        200_000,
        2,
    )
    .unwrap();

    let (git_id, exported) = export_commit_chain(&store, c2.id()).unwrap();
    let objects = objects_map(exported.clone());

    // A different identity imports the exported chain.
    let (importer, importer_dev) = human(50);
    let mut import_store = Store::new(MemoryBackend::new());
    let imported = import_commit_chain(
        &mut import_store,
        &importer.did(),
        &importer_dev,
        &git_id,
        &objects,
        999_000,
    )
    .unwrap();
    assert_eq!(imported.len(), 2, "both commits in the chain were imported");

    let imported_head = &imported[&git_id];

    // Content fidelity: checking out the imported head reproduces the
    // exact same files as the original.
    let mut original_files = checkout(&store, c2.id()).unwrap();
    let mut imported_files = checkout(&import_store, &imported_head.commit_id).unwrap();
    original_files.sort();
    imported_files.sort();
    assert_eq!(
        original_files, imported_files,
        "imported checkout must reproduce the original content exactly"
    );

    // Re-exporting the imported chain gives the exact same blob/tree git
    // ids as the original export -- content is byte-identical -- but a
    // DIFFERENT commit id, since the signed author changed from the
    // original author to the importer.
    let (reexported_git_id, reexported) =
        export_commit_chain(&import_store, &imported_head.commit_id).unwrap();
    assert_ne!(
        reexported_git_id, git_id,
        "the imported commit must not claim the original author's identity"
    );
    let blob_and_tree_ids = |objs: &[GitObject]| -> Vec<String> {
        let mut ids: Vec<String> = objs
            .iter()
            .filter(|o| o.kind != GitObjectKind::Commit)
            .map(|o| o.id.clone())
            .collect();
        ids.sort_unstable();
        ids
    };
    assert_eq!(
        blob_and_tree_ids(&exported),
        blob_and_tree_ids(&reexported),
        "blob/tree content must round-trip byte-identically"
    );

    // The imported commit's own signed author is the importer, never the
    // original author.
    let commit_obj = import_store.get(&imported_head.commit_id).unwrap();
    assert_eq!(commit_obj.author_human, importer.did());
    assert_ne!(commit_obj.author_human, author.did());

    // The original git commit's identity survives only in the separate
    // provenance record.
    let provenance_obj = import_store.get(&imported_head.provenance_id).unwrap();
    assert_eq!(provenance_obj.author_human, importer.did());
    let provenance = read_git_import_provenance(&provenance_obj).unwrap();
    assert_eq!(provenance.original_git_commit_id, git_id);
    let scid = author.did().scid().to_string();
    assert_eq!(provenance.author_name, format!("mini:{scid}"));
    assert_eq!(provenance.author_email, format!("{scid}@mininet.invalid"));
    assert_eq!(provenance.author_ts_secs, 200);
    assert_eq!(provenance.committer_name, format!("mini:{scid}"));

    // The first commit was imported too and carries its own provenance.
    let (c1_git_id, _) = export_commit_chain(&store, c1.id()).unwrap();
    let imported_c1 = &imported[&c1_git_id];
    let c1_provenance =
        read_git_import_provenance(&import_store.get(&imported_c1.provenance_id).unwrap()).unwrap();
    assert_eq!(c1_provenance.original_git_commit_id, c1_git_id);
    assert_eq!(c1_provenance.author_ts_secs, 100);
}

#[test]
fn a_tampered_object_whose_bytes_do_not_match_its_claimed_id_is_rejected() {
    let (author, author_dev) = human(20);
    let mut store = Store::new(MemoryBackend::new());
    let file = put_file(&mut store, &author.did(), &author_dev, b"hello").unwrap();
    let tree = put_tree(
        &mut store,
        &author.did(),
        &author_dev,
        &[TreeEntry {
            name: "a.txt".into(),
            is_dir: false,
            target: file,
        }],
    )
    .unwrap();
    let c = commit(
        &mut store,
        &author.did(),
        &author_dev,
        "msg",
        &tree,
        &[],
        1_000,
        1,
    )
    .unwrap();
    let (git_id, exported) = export_commit_chain(&store, c.id()).unwrap();
    let mut objects = objects_map(exported);

    // Tamper with the blob's bytes without updating its claimed id.
    for obj in objects.values_mut() {
        if obj.kind == GitObjectKind::Blob {
            obj.bytes = b"blob 5\0adieu".to_vec();
        }
    }

    let (importer, importer_dev) = human(60);
    let mut import_store = Store::new(MemoryBackend::new());
    let err = import_commit_chain(
        &mut import_store,
        &importer.did(),
        &importer_dev,
        &git_id,
        &objects,
        1_000,
    )
    .unwrap_err();
    assert_eq!(err, mini_forge::ForgeError::BadObject);
}

#[test]
fn a_commit_with_an_unrecognized_header_line_is_rejected_not_silently_dropped() {
    let (author, author_dev) = human(30);
    let mut store = Store::new(MemoryBackend::new());
    let file = put_file(&mut store, &author.did(), &author_dev, b"hi").unwrap();
    let tree = put_tree(
        &mut store,
        &author.did(),
        &author_dev,
        &[TreeEntry {
            name: "a.txt".into(),
            is_dir: false,
            target: file,
        }],
    )
    .unwrap();
    let c = commit(
        &mut store,
        &author.did(),
        &author_dev,
        "msg",
        &tree,
        &[],
        1_000,
        1,
    )
    .unwrap();
    let (git_id, exported) = export_commit_chain(&store, c.id()).unwrap();
    let mut objects = objects_map(exported);

    // Splice a `gpgsig` header into the commit's real framed bytes,
    // recomputing the outer "<kind> <len>\0" prefix so the frame stays
    // internally consistent -- only the id verification (against the
    // *original* content) would ever have caught a length mismatch, so
    // this specifically exercises the header-parser's own rejection.
    let commit_obj = objects.get(&git_id).unwrap().clone();
    let body_start = commit_obj.bytes.iter().position(|&b| b == 0).unwrap() + 1;
    let mut body = commit_obj.bytes[body_start..].to_vec();
    let tree_line_end = body.iter().position(|&b| b == b'\n').unwrap() + 1;
    let mut new_body = body[..tree_line_end].to_vec();
    new_body.extend_from_slice(b"gpgsig -----BEGIN PGP SIGNATURE-----\n");
    new_body.extend_from_slice(&body[tree_line_end..]);
    body = new_body;
    let mut new_bytes = format!("commit {}\0", body.len()).into_bytes();
    new_bytes.extend_from_slice(&body);
    // Recompute the claimed id to match the (now-modified) bytes exactly,
    // isolating this test to the header-shape rejection rather than the
    // separate id-verification check.
    let digest = mini_crypto::HashAlgorithm::Sha256.digest(&new_bytes);
    let new_id = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    objects.remove(&git_id);
    objects.insert(
        new_id.clone(),
        GitObject {
            id: new_id.clone(),
            kind: GitObjectKind::Commit,
            bytes: new_bytes,
        },
    );

    let (importer, importer_dev) = human(70);
    let mut import_store = Store::new(MemoryBackend::new());
    let err = import_commit_chain(
        &mut import_store,
        &importer.did(),
        &importer_dev,
        &new_id,
        &objects,
        1_000,
    )
    .unwrap_err();
    assert_eq!(err, mini_forge::ForgeError::BadObject);
}
