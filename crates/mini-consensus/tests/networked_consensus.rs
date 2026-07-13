//! The point of this whole crate, exercised end to end: four validator nodes,
//! in four real OS threads, each on its own TCP listener, with **completely
//! independent** ledgers that share no memory and no filesystem, run the
//! consensus protocol over a real socket mesh and converge — height by height
//! — on bit-identical finalized state.
//!
//! This is the first time in the tree that `mini_chain`'s finality and
//! `mini_execution`'s state machine cross a process boundary. Everything the
//! nodes agree on travels as bytes over a wire: proposals, signed
//! `did:mini` votes, quorum certificates. Nothing is shared but the public
//! validator KELs every node would have anyway.
//!
//! Honest caveat, matching the crate docs: these are threads over loopback,
//! not machines over the internet, and the round-0 driver assumes every
//! proposer is online. That is a real network transport exercising the real
//! protocol, not yet a deployment.

use std::collections::BTreeMap;
use std::net::{SocketAddr, TcpListener};
use std::thread;
use std::time::Duration;

use did_mini::{Capabilities, Controller, Did, Kel};
use mini_chain::{ValidatorOracle, ValidatorSet};
use mini_consensus::net::{run_to_height, TcpMesh};
use mini_consensus::{ConsensusNode, EquivocatorRegistry, NodeConfig};
use mini_crypto::SigningKey;
use mini_execution::SettlementBlockBody;
use mini_settlement::sign_claim;

/// A validator: an identity root plus a `VOTE`-capable delegated device.
fn validator(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

/// A clonable KEL directory — every node holds its own identical copy of the
/// public validator KELs (exactly what it would obtain from the network),
/// never anyone else's secret keys.
#[derive(Default, Clone)]
struct Directory(BTreeMap<String, Kel>);
impl Directory {
    fn insert(&mut self, kel: Kel) {
        self.0.insert(kel.scid().to_string(), kel);
    }
}
impl ValidatorOracle for Directory {
    fn kel(&self, did: &Did) -> Option<&Kel> {
        self.0.get(did.scid())
    }
}

/// The block every proposer builds for `height`. A pure function of the
/// height, so whichever validator is the designated proposer produces the
/// *same* block — a distinct payer per height (deterministic key), one claim,
/// sequence 0, so every height applies cleanly with no cross-height conflict.
fn block_body(height: u64) -> SettlementBlockBody {
    let payer = SigningKey::from_seed(&[height as u8; 32]);
    let claim = sign_claim(
        &payer,
        b"merchant",
        height * 100, // amount, micro-MINI; > 0 for every height >= 1
        0,
        1_000_000, // valid_until_ms, comfortably in the future of now_ms=0
        b"genesis",
        0,
    )
    .unwrap();
    SettlementBlockBody::new(vec![claim])
}

#[test]
fn four_nodes_over_a_real_tcp_mesh_finalize_and_converge() {
    const N: usize = 4;
    const TARGET_HEIGHT: u64 = 3;

    let signers: Vec<(Controller, Controller)> =
        (0..N as u8).map(|i| validator(10 + i * 10)).collect();

    let mut oracle = Directory::default();
    for (root, device) in &signers {
        oracle.insert(root.kel());
        oracle.insert(device.kel());
    }
    let validators = ValidatorSet::new(signers.iter().map(|(r, _)| r.did()).collect()).unwrap();

    // Bind every listener *before* any node dials, so the mesh setup cannot
    // race on connection-refused (see TcpMesh::establish's contract).
    let listeners: Vec<TcpListener> = (0..N)
        .map(|_| TcpListener::bind("127.0.0.1:0").unwrap())
        .collect();
    let addrs: Vec<SocketAddr> = listeners.iter().map(|l| l.local_addr().unwrap()).collect();

    let mut handles = Vec::new();
    for (index, (listener, (root, device))) in
        listeners.into_iter().zip(signers.into_iter()).enumerate()
    {
        let addrs = addrs.clone();
        let validators = validators.clone();
        let oracle = oracle.clone();
        let root_did = root.did();

        handles.push(thread::spawn(move || {
            let mut mesh = TcpMesh::establish(index, &addrs, &listener).unwrap();
            let mut node = ConsensusNode::new(NodeConfig {
                root: root_did,
                device,
                validators,
                oracle,
                body_source: Box::new(block_body),
            });
            let mut equivocators = EquivocatorRegistry::new();
            run_to_height(
                &mut node,
                &mut mesh,
                TARGET_HEIGHT,
                Duration::from_secs(30),
                &mut equivocators,
            )
            .expect("every honest node online should finalize the target height");
            assert_eq!(
                equivocators.flagged_count(),
                0,
                "no validator equivocated in this run"
            );

            (node.finalized_height(), node.commitment())
        }));
    }

    let results: Vec<(u64, [u8; 32])> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Every node reached the target height.
    for (height, _) in &results {
        assert_eq!(
            *height, TARGET_HEIGHT,
            "a node stopped short of the target height"
        );
    }

    // And — the whole reason the crate exists — every node agrees, bit for
    // bit, on the finalized state (Directive 4: honest nodes never disagree).
    let reference = results[0].1;
    for (i, (_, commitment)) in results.iter().enumerate() {
        assert_eq!(
            *commitment, reference,
            "node {i} disagreed on finalized state after running consensus over the wire"
        );
    }

    // Sanity: consensus actually did something — the settled state is not
    // still genesis-empty.
    let genesis = mini_execution::LedgerChain::genesis().state().commitment();
    assert_ne!(
        reference, genesis,
        "state never advanced past genesis — no block was really applied"
    );
}

/// View-change under a crashed proposer: a four-validator set (quorum 3) where
/// one validator is entirely **offline** — its KEL is known, it is a legitimate
/// proposer for some heights, but it never runs and the three online nodes are
/// not even meshed to it. Whenever the offline validator is a height's round-0
/// proposer, the three online nodes get no proposal, time out, prevote/precommit
/// `nil`, and roll to round 1 with a fresh (online) proposer — Tendermint
/// view-change over a real socket mesh. They must still finalize every height,
/// in lockstep, to identical state.
#[test]
fn a_crashed_proposer_is_survived_by_view_change_and_the_cluster_still_converges() {
    const N_VALIDATORS: usize = 4; // quorum = 3
    const N_ONLINE: usize = 3;
    const TARGET_HEIGHT: u64 = 4; // heights 1..=4 cover every proposer slot

    let signers: Vec<(Controller, Controller)> = (0..N_VALIDATORS as u8)
        .map(|i| validator(10 + i * 10))
        .collect();

    let mut oracle = Directory::default();
    for (root, device) in &signers {
        oracle.insert(root.kel());
        oracle.insert(device.kel());
    }
    // The validator set is all four; only the first three ever run. The fourth
    // (signers[3]) is the permanently-offline validator.
    let validators = ValidatorSet::new(signers.iter().map(|(r, _)| r.did()).collect()).unwrap();

    // Mesh only among the three online nodes — they never dial the offline one.
    let listeners: Vec<TcpListener> = (0..N_ONLINE)
        .map(|_| TcpListener::bind("127.0.0.1:0").unwrap())
        .collect();
    let addrs: Vec<SocketAddr> = listeners.iter().map(|l| l.local_addr().unwrap()).collect();

    let mut online = signers;
    online.truncate(N_ONLINE); // drop the offline validator's controllers

    let mut handles = Vec::new();
    for (index, (listener, (root, device))) in
        listeners.into_iter().zip(online.into_iter()).enumerate()
    {
        let addrs = addrs.clone();
        let validators = validators.clone();
        let oracle = oracle.clone();
        let root_did = root.did();

        handles.push(thread::spawn(move || {
            let mut mesh = TcpMesh::establish(index, &addrs, &listener).unwrap();
            let mut node = ConsensusNode::new(NodeConfig {
                root: root_did,
                device,
                validators,
                oracle,
                body_source: Box::new(block_body),
            });
            // A generous budget: view-change adds a few widening timeouts per
            // skipped proposer, but the height count is small.
            let mut equivocators = EquivocatorRegistry::new();
            run_to_height(
                &mut node,
                &mut mesh,
                TARGET_HEIGHT,
                Duration::from_secs(90),
                &mut equivocators,
            )
            .expect("three online validators (== quorum) must finalize via view-change");
            assert_eq!(equivocators.flagged_count(), 0);
            (node.finalized_height(), node.commitment())
        }));
    }

    let results: Vec<(u64, [u8; 32])> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    for (height, _) in &results {
        assert_eq!(*height, TARGET_HEIGHT, "an online node stopped short");
    }
    let reference = results[0].1;
    for (i, (_, commitment)) in results.iter().enumerate() {
        assert_eq!(
            *commitment, reference,
            "online node {i} diverged despite view-change recovery"
        );
    }
    let genesis = mini_execution::LedgerChain::genesis().state().commitment();
    assert_ne!(reference, genesis, "no block was really applied");
}

/// Consensus over a **partial** mesh: a four-node *line* 0—1—2—3, where the
/// endpoints (0 and 3) share no direct link. A vote from node 0 reaches node 3
/// only because every node dedup-floods (re-gossips) each message it has not
/// seen across its own edges. Without that re-gossip the endpoints would never
/// gather a quorum and the height would stall; with it, all four converge —
/// proving consensus no longer needs a *complete* graph, only a connected one.
#[test]
fn four_nodes_over_a_partial_line_mesh_finalize_via_re_gossip() {
    const N: usize = 4; // quorum = 3
    const TARGET_HEIGHT: u64 = 3;

    // Undirected line edges: 0-1, 1-2, 2-3. Each node's neighbor set.
    fn neighbors(i: usize) -> Vec<usize> {
        match i {
            0 => vec![1],
            3 => vec![2],
            k => vec![k - 1, k + 1],
        }
    }

    let signers: Vec<(Controller, Controller)> =
        (0..N as u8).map(|i| validator(10 + i * 10)).collect();
    let mut oracle = Directory::default();
    for (root, device) in &signers {
        oracle.insert(root.kel());
        oracle.insert(device.kel());
    }
    let validators = ValidatorSet::new(signers.iter().map(|(r, _)| r.did()).collect()).unwrap();

    let listeners: Vec<TcpListener> = (0..N)
        .map(|_| TcpListener::bind("127.0.0.1:0").unwrap())
        .collect();
    let addrs: Vec<SocketAddr> = listeners.iter().map(|l| l.local_addr().unwrap()).collect();

    let mut handles = Vec::new();
    for (index, (listener, (root, device))) in
        listeners.into_iter().zip(signers.into_iter()).enumerate()
    {
        let addrs = addrs.clone();
        let validators = validators.clone();
        let oracle = oracle.clone();
        let root_did = root.did();
        handles.push(thread::spawn(move || {
            let mut mesh =
                TcpMesh::establish_topology(index, &addrs, &listener, &neighbors(index)).unwrap();
            let mut node = ConsensusNode::new(NodeConfig {
                root: root_did,
                device,
                validators,
                oracle,
                body_source: Box::new(block_body),
            });
            let mut equivocators = EquivocatorRegistry::new();
            run_to_height(
                &mut node,
                &mut mesh,
                TARGET_HEIGHT,
                Duration::from_secs(90),
                &mut equivocators,
            )
            .expect("a connected (if partial) mesh must finalize via re-gossip");
            assert_eq!(equivocators.flagged_count(), 0);
            (node.finalized_height(), node.commitment())
        }));
    }

    let results: Vec<(u64, [u8; 32])> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    for (height, _) in &results {
        assert_eq!(*height, TARGET_HEIGHT, "a node on the line stopped short");
    }
    let reference = results[0].1;
    for (i, (_, commitment)) in results.iter().enumerate() {
        assert_eq!(
            *commitment, reference,
            "line node {i} diverged — re-gossip failed to propagate a quorum"
        );
    }
    let genesis = mini_execution::LedgerChain::genesis().state().commitment();
    assert_ne!(reference, genesis, "no block was really applied");
}
