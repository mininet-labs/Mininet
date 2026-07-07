//! Integration tests: quorum certificates over a real validator set, proving
//! the P1/P2 guarantees hold mechanically — no stake anywhere, one root
//! counts once however many devices vote, only VOTE-capable devices count,
//! and votes for the wrong height/round/block never contribute.

use did_mini::{Capabilities, Controller, Did};
use mini_chain::{
    sign_vote, verify_finality, verify_vote, BlockHeader, ChainError, QuorumCertificate,
    ValidatorOracle, ValidatorSet, VoteKind,
};
use std::collections::BTreeMap;

fn validator(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

/// A validator whose only device holds `secondary()` capabilities — no
/// VOTE — used to prove such a device's vote never counts.
fn validator_no_vote(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::secondary())
        .unwrap();
    (root, device)
}

#[derive(Default)]
struct Directory(BTreeMap<String, did_mini::Kel>);
impl Directory {
    fn insert(&mut self, kel: did_mini::Kel) {
        self.0.insert(kel.scid().to_string(), kel);
    }
}
impl ValidatorOracle for Directory {
    fn kel(&self, did: &Did) -> Option<&did_mini::Kel> {
        self.0.get(did.scid())
    }
}

fn genesis_header(proposer: &Did) -> BlockHeader {
    BlockHeader {
        height: 1,
        prev_hash: [0u8; 32],
        state_root: [1u8; 32],
        timestamp_ms: 1_000,
        proposer: proposer.clone(),
    }
}

#[test]
fn block_hash_is_deterministic_and_field_sensitive() {
    let (a, _) = validator(10);
    let h1 = genesis_header(&a.did());
    let h2 = genesis_header(&a.did());
    assert_eq!(h1.hash(), h2.hash());

    let mut h3 = h1.clone();
    h3.timestamp_ms += 1;
    assert_ne!(h1.hash(), h3.hash());
}

#[test]
fn validator_set_rejects_empty_and_duplicates() {
    assert_eq!(
        ValidatorSet::new(vec![]).unwrap_err(),
        ChainError::EmptyValidatorSet
    );

    let (a, _) = validator(10);
    assert_eq!(
        ValidatorSet::new(vec![a.did(), a.did()]).unwrap_err(),
        ChainError::DuplicateValidator
    );
}

#[test]
fn quorum_threshold_is_strictly_more_than_two_thirds() {
    let mut roots = Vec::new();
    for seed in [10u8, 20, 30, 40] {
        roots.push(validator(seed).0.did());
    }
    let set = ValidatorSet::new(roots).unwrap();
    // n=4: floor(8/3)+1 = 2+1 = 3.
    assert_eq!(set.quorum_threshold(), 3);
}

#[test]
fn a_precommit_quorum_certificate_verifies() {
    let (a_root, a_dev) = validator(10);
    let (b_root, b_dev) = validator(20);
    let (c_root, c_dev) = validator(30);
    let (d_root, d_dev) = validator(40);
    let validators =
        ValidatorSet::new(vec![a_root.did(), b_root.did(), c_root.did(), d_root.did()]).unwrap();

    let header = genesis_header(&a_root.did());
    let hash = header.hash();

    let mut oracle = Directory::default();
    for c in [
        &a_root, &a_dev, &b_root, &b_dev, &c_root, &c_dev, &d_root, &d_dev,
    ] {
        oracle.insert(c.kel());
    }

    // 3 of 4 precommit — meets the threshold (3).
    let votes = vec![
        sign_vote(VoteKind::Precommit, 1, 0, hash, &a_root.did(), &a_dev),
        sign_vote(VoteKind::Precommit, 1, 0, hash, &b_root.did(), &b_dev),
        sign_vote(VoteKind::Precommit, 1, 0, hash, &c_root.did(), &c_dev),
    ];
    let qc = QuorumCertificate {
        height: 1,
        round: 0,
        block_hash: hash,
        votes,
    };
    let count = verify_finality(&qc, &validators, &oracle).unwrap();
    assert_eq!(count, 3);
}

#[test]
fn two_of_four_precommits_do_not_reach_quorum() {
    let (a_root, a_dev) = validator(10);
    let (b_root, b_dev) = validator(20);
    let (c_root, _) = validator(30);
    let (d_root, _) = validator(40);
    let validators =
        ValidatorSet::new(vec![a_root.did(), b_root.did(), c_root.did(), d_root.did()]).unwrap();

    let header = genesis_header(&a_root.did());
    let hash = header.hash();
    let mut oracle = Directory::default();
    for c in [&a_root, &a_dev, &b_root, &b_dev] {
        oracle.insert(c.kel());
    }

    let votes = vec![
        sign_vote(VoteKind::Precommit, 1, 0, hash, &a_root.did(), &a_dev),
        sign_vote(VoteKind::Precommit, 1, 0, hash, &b_root.did(), &b_dev),
    ];
    let qc = QuorumCertificate {
        height: 1,
        round: 0,
        block_hash: hash,
        votes,
    };
    assert_eq!(
        verify_finality(&qc, &validators, &oracle).unwrap_err(),
        ChainError::QuorumNotMet { needed: 3, got: 2 }
    );
}

#[test]
fn one_validator_voting_from_many_devices_still_counts_once() {
    let (mut a_root, a_dev1) = validator(10);
    let a_dev2 =
        Controller::incept_device_single_from_seeds(&a_root.did(), &[70u8; 32], &[71u8; 32])
            .unwrap();
    a_root
        .delegate_device(&a_dev2.did(), Capabilities::primary())
        .unwrap();
    let (b_root, b_dev) = validator(20);
    let (c_root, c_dev) = validator(30);
    let (d_root, _) = validator(40);
    let validators =
        ValidatorSet::new(vec![a_root.did(), b_root.did(), c_root.did(), d_root.did()]).unwrap();

    let header = genesis_header(&a_root.did());
    let hash = header.hash();
    let mut oracle = Directory::default();
    for c in [&a_root, &a_dev1, &a_dev2, &b_root, &b_dev, &c_root, &c_dev] {
        oracle.insert(c.kel());
    }

    // Root A votes from TWO devices; B and C each vote once. Distinct
    // validator roots represented: {A, B, C} = 3, meeting the threshold —
    // NOT 4 "votes", which would wrongly exceed the validator set's size.
    let votes = vec![
        sign_vote(VoteKind::Precommit, 1, 0, hash, &a_root.did(), &a_dev1),
        sign_vote(VoteKind::Precommit, 1, 0, hash, &a_root.did(), &a_dev2),
        sign_vote(VoteKind::Precommit, 1, 0, hash, &b_root.did(), &b_dev),
        sign_vote(VoteKind::Precommit, 1, 0, hash, &c_root.did(), &c_dev),
    ];
    let qc = QuorumCertificate {
        height: 1,
        round: 0,
        block_hash: hash,
        votes,
    };
    let count = verify_finality(&qc, &validators, &oracle).unwrap();
    assert_eq!(count, 3);
}

#[test]
fn prevotes_never_finalize() {
    let (a_root, a_dev) = validator(10);
    let (b_root, b_dev) = validator(20);
    let (c_root, c_dev) = validator(30);
    let validators = ValidatorSet::new(vec![a_root.did(), b_root.did(), c_root.did()]).unwrap();
    let header = genesis_header(&a_root.did());
    let hash = header.hash();
    let mut oracle = Directory::default();
    for c in [&a_root, &a_dev, &b_root, &b_dev, &c_root, &c_dev] {
        oracle.insert(c.kel());
    }

    // All three validators Prevote (not Precommit) — never counts toward
    // finality, however many there are.
    let votes = vec![
        sign_vote(VoteKind::Prevote, 1, 0, hash, &a_root.did(), &a_dev),
        sign_vote(VoteKind::Prevote, 1, 0, hash, &b_root.did(), &b_dev),
        sign_vote(VoteKind::Prevote, 1, 0, hash, &c_root.did(), &c_dev),
    ];
    let qc = QuorumCertificate {
        height: 1,
        round: 0,
        block_hash: hash,
        votes,
    };
    assert!(matches!(
        verify_finality(&qc, &validators, &oracle),
        Err(ChainError::QuorumNotMet { .. })
    ));
}

#[test]
fn votes_for_a_different_block_height_or_round_never_count() {
    let (a_root, a_dev) = validator(10);
    let (b_root, b_dev) = validator(20);
    let (c_root, c_dev) = validator(30);
    let validators = ValidatorSet::new(vec![a_root.did(), b_root.did(), c_root.did()]).unwrap();
    let header = genesis_header(&a_root.did());
    let hash = header.hash();
    let other_hash = [9u8; 32];
    let mut oracle = Directory::default();
    for c in [&a_root, &a_dev, &b_root, &b_dev, &c_root, &c_dev] {
        oracle.insert(c.kel());
    }

    let votes = vec![
        sign_vote(VoteKind::Precommit, 1, 0, hash, &a_root.did(), &a_dev), // matches
        sign_vote(VoteKind::Precommit, 1, 0, other_hash, &b_root.did(), &b_dev), // wrong block
        sign_vote(VoteKind::Precommit, 2, 0, hash, &c_root.did(), &c_dev), // wrong height
    ];
    let qc = QuorumCertificate {
        height: 1,
        round: 0,
        block_hash: hash,
        votes,
    };
    assert_eq!(
        verify_finality(&qc, &validators, &oracle).unwrap_err(),
        ChainError::QuorumNotMet { needed: 3, got: 1 }
    );
}

#[test]
fn a_device_without_vote_capability_never_counts() {
    let (a_root, a_dev) = validator(10);
    let (b_root, b_dev) = validator(20);
    let (c_root, c_dev) = validator_no_vote(30); // secondary caps only
    let validators = ValidatorSet::new(vec![a_root.did(), b_root.did(), c_root.did()]).unwrap();
    let header = genesis_header(&a_root.did());
    let hash = header.hash();
    let mut oracle = Directory::default();
    for c in [&a_root, &a_dev, &b_root, &b_dev, &c_root, &c_dev] {
        oracle.insert(c.kel());
    }

    // C's device holds only `secondary()` (no VOTE) — its vote never counts,
    // even though C is a member of the validator set.
    assert!(matches!(
        verify_vote(
            &sign_vote(VoteKind::Precommit, 1, 0, hash, &c_root.did(), &c_dev),
            &c_root.kel(),
            &c_dev.kel(),
        ),
        Err(ChainError::MissingVoteCapability)
    ));

    let votes = vec![
        sign_vote(VoteKind::Precommit, 1, 0, hash, &a_root.did(), &a_dev),
        sign_vote(VoteKind::Precommit, 1, 0, hash, &b_root.did(), &b_dev),
        sign_vote(VoteKind::Precommit, 1, 0, hash, &c_root.did(), &c_dev),
    ];
    let qc = QuorumCertificate {
        height: 1,
        round: 0,
        block_hash: hash,
        votes,
    };
    // Only A and B actually count; 2 < threshold(3).
    assert_eq!(
        verify_finality(&qc, &validators, &oracle).unwrap_err(),
        ChainError::QuorumNotMet { needed: 3, got: 2 }
    );
}

#[test]
fn a_non_validator_vote_never_counts_even_if_perfectly_signed() {
    let (a_root, a_dev) = validator(10);
    let (b_root, b_dev) = validator(20);
    let (outsider_root, outsider_dev) = validator(90); // not in the set
    let validators = ValidatorSet::new(vec![a_root.did(), b_root.did()]).unwrap();
    let header = genesis_header(&a_root.did());
    let hash = header.hash();
    let mut oracle = Directory::default();
    for c in [
        &a_root,
        &a_dev,
        &b_root,
        &b_dev,
        &outsider_root,
        &outsider_dev,
    ] {
        oracle.insert(c.kel());
    }

    let votes = vec![
        sign_vote(VoteKind::Precommit, 1, 0, hash, &a_root.did(), &a_dev),
        sign_vote(
            VoteKind::Precommit,
            1,
            0,
            hash,
            &outsider_root.did(),
            &outsider_dev,
        ),
    ];
    // n=2: threshold = floor(4/3)+1 = 1+1 = 2. Only A counts (outsider is
    // not a validator), so this stays below threshold.
    let qc = QuorumCertificate {
        height: 1,
        round: 0,
        block_hash: hash,
        votes,
    };
    assert_eq!(
        verify_finality(&qc, &validators, &oracle).unwrap_err(),
        ChainError::QuorumNotMet { needed: 2, got: 1 }
    );
}

#[test]
fn a_forged_or_altered_signature_is_rejected() {
    let (a_root, a_dev) = validator(10);
    let (b_root, _) = validator(20);
    let header = genesis_header(&a_root.did());
    let hash = header.hash();

    // A vote claiming to be from B's root but signed by A's device.
    let forged = sign_vote(VoteKind::Precommit, 1, 0, hash, &b_root.did(), &a_dev);
    assert!(matches!(
        verify_vote(&forged, &b_root.kel(), &a_dev.kel()),
        Err(ChainError::Identity(_))
    ));
}
