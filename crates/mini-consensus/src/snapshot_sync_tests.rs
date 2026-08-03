//! End-to-end authenticated snapshot/state-sync tests kept inside the crate so
//! they can exercise the archive's verified-write seam without making that
//! seam a public trust API.

use std::collections::BTreeMap;
use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;

use did_mini::{Capabilities, Controller, Did, Kel};
use mini_chain::{
    sign_vote, BlockHeader, QuorumCertificate, ValidatorOracle, ValidatorSet, VoteKind,
};
use mini_execution::{apply_block, LedgerChain, SettlementBlockBody};

use crate::catchup::FinalizedBlock;
use crate::net::{serve_state_sync_over_tcp, state_sync_over_tcp};
use crate::{
    ConsensusArchive, ConsensusArchiveConfig, ConsensusError, ConsensusNode, ConsensusSnapshot,
    NodeConfig, StateSyncResponse,
};

#[derive(Debug, Default, Clone)]
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

struct Fixture {
    signers: Vec<(Controller, Controller)>,
    validators: ValidatorSet,
    oracle: Directory,
}

fn fixture() -> Fixture {
    let signers: Vec<_> = [10u8, 20, 30, 40]
        .into_iter()
        .map(|seed| {
            let mut root =
                Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
            let device = Controller::incept_device_single_from_seeds(
                &root.did(),
                &[seed + 2; 32],
                &[seed + 3; 32],
            )
            .unwrap();
            root.delegate_device(&device.did(), Capabilities::primary())
                .unwrap();
            (root, device)
        })
        .collect();
    let validators =
        ValidatorSet::new(signers.iter().map(|(root, _)| root.did()).collect()).unwrap();
    let mut oracle = Directory::default();
    for (root, device) in &signers {
        oracle.insert(root.kel());
        oracle.insert(device.kel());
    }
    Fixture {
        signers,
        validators,
        oracle,
    }
}

fn node_config(fixture: &Fixture) -> NodeConfig<Directory> {
    let root = fixture.signers[0].0.did();
    let device = Controller::incept_device_single_from_seeds(&root, &[12; 32], &[13; 32]).unwrap();
    NodeConfig {
        root,
        device,
        validators: fixture.validators.clone(),
        oracle: fixture.oracle.clone(),
        body_source: Box::new(|_| SettlementBlockBody::new(Vec::new())),
    }
}

fn finalized_block(chain: &LedgerChain, height: u64, fixture: &Fixture) -> FinalizedBlock {
    let body = SettlementBlockBody::new(Vec::new());
    let next = apply_block(chain.state(), &body).unwrap();
    let header = BlockHeader {
        height,
        prev_hash: chain.tip_hash(),
        state_root: next.commitment(),
        timestamp_ms: height,
        proposer: fixture.signers[0].0.did(),
    };
    let block_hash = header.hash();
    let votes = fixture
        .signers
        .iter()
        .map(|(root, device)| {
            sign_vote(
                VoteKind::Precommit,
                height,
                0,
                block_hash,
                &root.did(),
                device,
            )
        })
        .collect();
    FinalizedBlock {
        qc: QuorumCertificate {
            height,
            round: 0,
            block_hash,
            votes,
        },
        header,
        body,
    }
}

fn temp_root(tag: &str) -> PathBuf {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "mini-consensus-snapshot-sync-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    root
}

fn build_archive(
    root: &PathBuf,
    fixture: &Fixture,
) -> (ConsensusArchive, LedgerChain, Vec<FinalizedBlock>) {
    let config = ConsensusArchiveConfig {
        snapshot_interval: 2,
        max_suffix_blocks: 2,
        ..ConsensusArchiveConfig::default()
    };
    let archive = ConsensusArchive::open(root, config).unwrap();
    let mut chain = LedgerChain::genesis();
    let mut blocks = Vec::new();
    for height in 1..=5 {
        let block = finalized_block(&chain, height, fixture);
        chain
            .apply_finalized_block(
                &block.header,
                &block.body,
                &block.qc,
                &fixture.validators,
                &fixture.oracle,
            )
            .unwrap();
        archive
            .record_verified_batch(core::slice::from_ref(&block), chain.state())
            .unwrap();
        blocks.push(block);
    }
    (archive, chain, blocks)
}

#[test]
fn a_long_offline_node_reaches_the_exact_tip_via_real_tcp_and_reopens() {
    let fixture = fixture();
    let root = temp_root("tcp");
    let (archive, source_chain, _) = build_archive(&root, &fixture);

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let serving_archive = archive.clone();
    let server = thread::spawn(move || {
        serve_state_sync_over_tcp(&serving_archive, &listener).unwrap();
    });

    let mut late = ConsensusNode::new(node_config(&fixture));
    let applied = state_sync_over_tcp(&mut late, address).unwrap();
    server.join().unwrap();

    assert_eq!(applied, 5);
    assert_eq!(late.finalized_height(), source_chain.height());
    assert_eq!(late.commitment(), source_chain.state().commitment());

    let reopened = ConsensusArchive::open(
        &root,
        ConsensusArchiveConfig {
            snapshot_interval: 2,
            max_suffix_blocks: 2,
            ..ConsensusArchiveConfig::default()
        },
    )
    .unwrap();
    let restored = ConsensusNode::new_with_archive(node_config(&fixture), reopened).unwrap();
    assert_eq!(restored.finalized_height(), source_chain.height());
    assert_eq!(restored.commitment(), source_chain.state().commitment());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_bad_second_block_leaves_the_destination_completely_unchanged() {
    let fixture = fixture();
    let root = temp_root("atomic");
    let (_archive, _source_chain, blocks) = build_archive(&root, &fixture);
    let mut response_blocks = blocks[..2].to_vec();
    response_blocks[1].header.prev_hash = [9; 32];
    let response = StateSyncResponse::blocks(mini_settlement::MININET_NETWORK_ID, response_blocks);

    let mut destination = ConsensusNode::new(node_config(&fixture));
    let before = destination.commitment();
    assert!(destination.apply_state_sync(response).is_err());
    assert_eq!(destination.finalized_height(), 0);
    assert_eq!(destination.commitment(), before);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn peer_block_state_sync_rejects_gap_duplicate_and_reordering_all_or_nothing() {
    let fixture = fixture();
    let root = temp_root("peer-ordering");
    let (_archive, _source_chain, blocks) = build_archive(&root, &fixture);
    let cases = [
        (vec![blocks[0].clone(), blocks[2].clone()], 2, 3),
        (vec![blocks[0].clone(), blocks[0].clone()], 2, 1),
        (vec![blocks[1].clone(), blocks[0].clone()], 1, 2),
    ];

    for (malformed_suffix, expected, got) in cases {
        let encoded =
            StateSyncResponse::blocks(mini_settlement::MININET_NETWORK_ID, malformed_suffix)
                .to_wire_bytes()
                .unwrap();
        let response = StateSyncResponse::from_wire_bytes(&encoded).unwrap();
        let mut destination = ConsensusNode::new(node_config(&fixture));
        let before = destination.commitment();
        assert_eq!(
            destination.apply_state_sync(response).unwrap_err(),
            ConsensusError::CatchupOutOfOrder { expected, got }
        );
        assert_eq!(destination.finalized_height(), 0);
        assert_eq!(destination.commitment(), before);
    }

    let _ = fs::remove_dir_all(root);
}

#[test]
fn peer_snapshot_state_sync_rejects_a_gapped_suffix_all_or_nothing() {
    let fixture = fixture();
    let root = temp_root("peer-snapshot-gap");
    let (_archive, _source_chain, blocks) = build_archive(&root, &fixture);
    let snapshot = ConsensusSnapshot::new(
        blocks[0].header.clone(),
        blocks[0].qc.clone(),
        LedgerChain::genesis().state().clone(),
    )
    .unwrap();
    let encoded = StateSyncResponse::snapshot(
        mini_settlement::MININET_NETWORK_ID,
        snapshot,
        vec![blocks[2].clone()],
    )
    .to_wire_bytes()
    .unwrap();
    let response = StateSyncResponse::from_wire_bytes(&encoded).unwrap();

    let mut destination = ConsensusNode::new(node_config(&fixture));
    let before = destination.commitment();
    assert_eq!(
        destination.apply_state_sync(response).unwrap_err(),
        ConsensusError::CatchupOutOfOrder {
            expected: 2,
            got: 3,
        }
    );
    assert_eq!(destination.finalized_height(), 0);
    assert_eq!(destination.commitment(), before);

    let _ = fs::remove_dir_all(root);
}
