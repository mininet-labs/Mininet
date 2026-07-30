//! Integration tests: claim -> proposed block -> real quorum certificate ->
//! `LedgerChain::apply_finalized_block` -> `mini_settlement::reconcile`
//! reading this crate's own `LedgerState` as its `CanonicalLedgerView`.
//! This is the loop D-0055's required follow-up and roadmap #40 named as
//! still missing: `mini_settlement`'s reconciliation logic, proven against
//! a real (if minimal) chain-backed ledger instead of the test-only
//! `InMemoryLedgerView`.

use std::collections::BTreeMap;

use did_mini::{Capabilities, Controller, Did, Kel};
use mini_chain::{
    sign_vote, BlockHeader, QuorumCertificate, ValidatorOracle, ValidatorSet, VoteKind,
};
use mini_crypto::SigningKey;
use mini_economy::{
    plan_scalable_epoch, Allocation, Amount, HumanSnapshot, IssuancePolicy, ScalableEpochRequest,
    YEAR_MS,
};
use mini_execution::{ExecutionError, LedgerChain, SettlementBlockBody};
use mini_settlement::{reconcile, sign_claim, SettlementState};

fn validator(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

#[derive(Default)]
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
    validators: ValidatorSet,
    oracle: Directory,
    signers: Vec<(Controller, Controller)>, // (root, device), 4 validators, 3-of-4 quorum
}

fn fixture() -> Fixture {
    let signers: Vec<(Controller, Controller)> =
        [10u8, 20, 30, 40].into_iter().map(validator).collect();
    let mut oracle = Directory::default();
    for (root, device) in &signers {
        oracle.insert(root.kel());
        oracle.insert(device.kel());
    }
    let validators = ValidatorSet::new(signers.iter().map(|(r, _)| r.did()).collect()).unwrap();
    Fixture {
        validators,
        oracle,
        signers,
    }
}

fn account(key: &SigningKey) -> Vec<u8> {
    key.verifying_key().to_bytes().to_vec()
}

fn funded_chain(key: &SigningKey, amount_micro: u64) -> LedgerChain {
    let amount = Amount::from(amount_micro);
    LedgerChain::genesis_with_balances(amount, vec![(account(key), amount)]).unwrap()
}

fn recipient(seed: u8) -> Vec<u8> {
    SigningKey::from_seed(&[seed; 32])
        .verifying_key()
        .to_bytes()
        .to_vec()
}

fn monetary_plan(epoch: u64, opening: Amount) -> mini_economy::ScalableEpochPlan {
    plan_scalable_epoch(
        &ScalableEpochRequest {
            epoch,
            duration_ms: YEAR_MS,
            opening_circulating: opening,
            human_snapshot: HumanSnapshot {
                root: [42; 32],
                eligible_count: 2,
            },
            service: vec![Allocation {
                beneficiary: "did:mini:relay".into(),
                amount: Amount::from_micro(opening.as_micro() * 7_500 / 1_000_000),
            }],
            treasury: vec![Allocation {
                beneficiary: "did:mini:treasury".into(),
                amount: Amount::from_micro(opening.as_micro() * 2_500 / 1_000_000),
            }],
        },
        &IssuancePolicy::d0074(),
    )
    .unwrap()
}

#[test]
fn finalized_monetary_epoch_updates_supply_and_replay_fails() {
    let fx = fixture();
    let genesis = Amount::from_micro(1_000_000_000);
    let mut chain = LedgerChain::genesis_with_supply(genesis);
    let plan = monetary_plan(0, genesis);
    let body = SettlementBlockBody::new(vec![]).with_monetary_epochs(vec![plan.clone()]);
    let next_state = mini_execution::apply_block(chain.state(), &body).unwrap();
    let header = BlockHeader {
        height: 1,
        prev_hash: chain.tip_hash(),
        state_root: next_state.commitment(),
        timestamp_ms: 1,
        proposer: fx.signers[0].0.did(),
    };
    let hash = header.hash();
    let qc = QuorumCertificate {
        height: 1,
        round: 0,
        block_hash: hash,
        votes: fx.signers[..3]
            .iter()
            .map(|(root, device)| sign_vote(VoteKind::Precommit, 1, 0, hash, &root.did(), device))
            .collect(),
    };
    chain
        .apply_finalized_block(&header, &body, &qc, &fx.validators, &fx.oracle)
        .unwrap();
    assert_eq!(chain.state().monetary().last_epoch(), Some(0));
    assert_eq!(chain.state().monetary().total_issued(), plan.total_issued);
    assert_eq!(
        chain.state().unallocated_circulating(),
        chain.state().monetary().circulating_supply().unwrap()
    );
    chain.state().verify_supply_conservation().unwrap();

    let replay = SettlementBlockBody::new(vec![]).with_monetary_epochs(vec![plan]);
    assert_eq!(
        mini_execution::apply_block(chain.state(), &replay).unwrap_err(),
        ExecutionError::InvalidMonetaryEpoch(mini_economy::EconomyError::UnexpectedEpoch)
    );
}

#[test]
fn a_single_claim_finalizes_end_to_end_and_reconcile_reports_it() {
    let fx = fixture();
    let payer = SigningKey::from_seed(&[0x55; 32]);
    let mut chain = funded_chain(&payer, 500);
    let claim = sign_claim(&payer, &recipient(0x51), 500, 0, 10_000, b"chain-1", 0).unwrap();
    let body = SettlementBlockBody::new(vec![claim.clone()]);

    let next_state = mini_execution::apply_block(chain.state(), &body).unwrap();
    let header = BlockHeader {
        height: 1,
        prev_hash: chain.tip_hash(),
        state_root: next_state.commitment(),
        timestamp_ms: 1,
        proposer: fx.signers[0].0.did(),
    };
    let hash = header.hash();
    let votes = fx.signers[..3]
        .iter()
        .map(|(root, device)| sign_vote(VoteKind::Precommit, 1, 0, hash, &root.did(), device))
        .collect();
    let qc = QuorumCertificate {
        height: 1,
        round: 0,
        block_hash: hash,
        votes,
    };

    chain
        .apply_finalized_block(&header, &body, &qc, &fx.validators, &fx.oracle)
        .unwrap();
    assert_eq!(chain.height(), 1);
    assert_eq!(chain.state().balance(&claim.payer), Amount::ZERO);
    assert_eq!(chain.state().balance(&claim.payee), Amount::from(500));
    chain.state().verify_supply_conservation().unwrap();

    let outcome = reconcile(&claim, chain.state(), 100).unwrap();
    assert_eq!(outcome, SettlementState::Finalized);
    assert!(outcome.is_final());
}

#[test]
fn a_finalized_overspend_returns_a_canonical_wallet_rejection() {
    let fx = fixture();
    let payer = SigningKey::from_seed(&[0x56; 32]);
    let mut chain = funded_chain(&payer, 500);
    let claim = sign_claim(&payer, &recipient(0x51), 501, 0, 10_000, b"chain-1", 0).unwrap();
    let body = SettlementBlockBody::new(vec![claim.clone()]);
    let next_state = mini_execution::apply_block(chain.state(), &body).unwrap();
    let header = BlockHeader {
        height: 1,
        prev_hash: chain.tip_hash(),
        state_root: next_state.commitment(),
        timestamp_ms: 1,
        proposer: fx.signers[0].0.did(),
    };
    let hash = header.hash();
    let qc = QuorumCertificate {
        height: 1,
        round: 0,
        block_hash: hash,
        votes: fx.signers[..3]
            .iter()
            .map(|(root, device)| sign_vote(VoteKind::Precommit, 1, 0, hash, &root.did(), device))
            .collect(),
    };
    chain
        .apply_finalized_block(&header, &body, &qc, &fx.validators, &fx.oracle)
        .unwrap();

    assert_eq!(chain.state().balance(&claim.payer), Amount::from(500));
    assert_eq!(
        reconcile(&claim, chain.state(), 100).unwrap(),
        SettlementState::RejectedCanonical(mini_settlement::CanonicalRejection::InsufficientFunds)
    );
}

#[test]
fn a_double_spend_across_two_competing_proposals_resolves_to_exactly_one_winner() {
    let fx = fixture();
    let payer = SigningKey::from_seed(&[0x66; 32]);
    let mut chain = funded_chain(&payer, 500);
    let claim_a = sign_claim(&payer, &recipient(0x51), 500, 0, 10_000, b"chain-1", 0).unwrap();
    let claim_b = sign_claim(&payer, &recipient(0x52), 500, 0, 10_000, b"chain-1", 0).unwrap();
    assert_ne!(
        mini_settlement::claim_digest(&claim_a),
        mini_settlement::claim_digest(&claim_b)
    );

    // Two competing block-body proposals at height 1 -- only one can ever
    // actually be finalized on a real BFT chain (a real network would never
    // produce quorum certificates for both at the same height/round; this
    // test proves the state machine's own logic holds even if it did).
    let body_a = SettlementBlockBody::new(vec![claim_a.clone()]);
    let next_state_a = mini_execution::apply_block(chain.state(), &body_a).unwrap();
    let header_a = BlockHeader {
        height: 1,
        prev_hash: chain.tip_hash(),
        state_root: next_state_a.commitment(),
        timestamp_ms: 1,
        proposer: fx.signers[0].0.did(),
    };
    let hash_a = header_a.hash();
    let votes_a = fx.signers[..3]
        .iter()
        .map(|(root, device)| sign_vote(VoteKind::Precommit, 1, 0, hash_a, &root.did(), device))
        .collect();
    let qc_a = QuorumCertificate {
        height: 1,
        round: 0,
        block_hash: hash_a,
        votes: votes_a,
    };

    chain
        .apply_finalized_block(&header_a, &body_a, &qc_a, &fx.validators, &fx.oracle)
        .unwrap();

    // claim_b never gets its own finalized block (as in reality, only one
    // block per height ever finalizes) -- reconcile it against the SAME
    // resulting state and confirm it lost.
    let outcome_a = reconcile(&claim_a, chain.state(), 100).unwrap();
    let outcome_b = reconcile(&claim_b, chain.state(), 100).unwrap();
    assert_eq!(outcome_a, SettlementState::Finalized);
    assert_eq!(outcome_b, SettlementState::RejectedConflict);
    assert!(outcome_a.is_final() ^ outcome_b.is_final());
    assert_eq!(chain.state().balance(&claim_a.payee), Amount::from(500));
    assert_eq!(chain.state().balance(&claim_b.payee), Amount::ZERO);
}

#[test]
fn two_independent_chains_fed_the_same_finalized_blocks_converge_to_identical_state() {
    let fx = fixture();
    let payer_1 = SigningKey::from_seed(&[0x11; 32]);
    let payer_2 = SigningKey::from_seed(&[0x22; 32]);
    let allocations = vec![
        (account(&payer_1), Amount::from(100)),
        (account(&payer_2), Amount::from(200)),
    ];
    let mut chain_1 =
        LedgerChain::genesis_with_balances(Amount::from(300), allocations.clone()).unwrap();
    let mut chain_2 = LedgerChain::genesis_with_balances(Amount::from(300), allocations).unwrap();
    let claim_1 = sign_claim(&payer_1, &recipient(0x51), 100, 0, 10_000, b"chain-1", 0).unwrap();
    let claim_2 = sign_claim(&payer_2, &recipient(0x52), 200, 0, 10_000, b"chain-1", 0).unwrap();

    for (height, claim) in [(1u64, claim_1), (2u64, claim_2)] {
        let body = SettlementBlockBody::new(vec![claim]);
        let prev_hash = chain_1.tip_hash();
        assert_eq!(prev_hash, chain_2.tip_hash());
        let next_state = mini_execution::apply_block(chain_1.state(), &body).unwrap();
        let header = BlockHeader {
            height,
            prev_hash,
            state_root: next_state.commitment(),
            timestamp_ms: height,
            proposer: fx.signers[0].0.did(),
        };
        let hash = header.hash();
        let votes = fx.signers[..3]
            .iter()
            .map(|(root, device)| {
                sign_vote(VoteKind::Precommit, height, 0, hash, &root.did(), device)
            })
            .collect();
        let qc = QuorumCertificate {
            height,
            round: 0,
            block_hash: hash,
            votes,
        };

        chain_1
            .apply_finalized_block(&header, &body, &qc, &fx.validators, &fx.oracle)
            .unwrap();
        chain_2
            .apply_finalized_block(&header, &body, &qc, &fx.validators, &fx.oracle)
            .unwrap();
    }

    assert_eq!(chain_1.height(), chain_2.height());
    assert_eq!(chain_1.tip_hash(), chain_2.tip_hash());
    assert_eq!(
        chain_1.state().commitment(),
        chain_2.state().commitment(),
        "two honest nodes given the same finalized blocks must never disagree (Directive 4)"
    );
}

#[test]
fn an_unfinalized_block_is_never_applied() {
    let fx = fixture();
    let mut chain = LedgerChain::genesis();

    let payer = SigningKey::from_seed(&[0x77; 32]);
    let claim = sign_claim(&payer, &recipient(0x51), 500, 0, 10_000, b"chain-1", 0).unwrap();
    let body = SettlementBlockBody::new(vec![claim]);
    let next_state = mini_execution::apply_block(chain.state(), &body).unwrap();
    let header = BlockHeader {
        height: 1,
        prev_hash: chain.tip_hash(),
        state_root: next_state.commitment(),
        timestamp_ms: 1_000,
        proposer: fx.signers[0].0.did(),
    };
    let hash = header.hash();

    // Only 2 of 4 validators precommit -- below the 3-of-4 threshold.
    let votes = fx.signers[..2]
        .iter()
        .map(|(root, device)| sign_vote(VoteKind::Precommit, 1, 0, hash, &root.did(), device))
        .collect();
    let qc = QuorumCertificate {
        height: 1,
        round: 0,
        block_hash: hash,
        votes,
    };

    let err = chain
        .apply_finalized_block(&header, &body, &qc, &fx.validators, &fx.oracle)
        .unwrap_err();
    assert!(matches!(err, ExecutionError::NotFinalized(_)));
    assert_eq!(
        chain.height(),
        0,
        "state must not advance on an unfinalized block"
    );
}

#[test]
fn a_dishonest_state_root_is_rejected() {
    let fx = fixture();
    let mut chain = LedgerChain::genesis();

    let payer = SigningKey::from_seed(&[0x88; 32]);
    let claim = sign_claim(&payer, &recipient(0x51), 500, 0, 10_000, b"chain-1", 0).unwrap();
    let body = SettlementBlockBody::new(vec![claim]);
    let header = BlockHeader {
        height: 1,
        prev_hash: chain.tip_hash(),
        state_root: [0xFFu8; 32], // does not match what `body` actually produces
        timestamp_ms: 1,
        proposer: fx.signers[0].0.did(),
    };
    let hash = header.hash();
    let votes = fx.signers[..3]
        .iter()
        .map(|(root, device)| sign_vote(VoteKind::Precommit, 1, 0, hash, &root.did(), device))
        .collect();
    let qc = QuorumCertificate {
        height: 1,
        round: 0,
        block_hash: hash,
        votes,
    };

    let err = chain
        .apply_finalized_block(&header, &body, &qc, &fx.validators, &fx.oracle)
        .unwrap_err();
    assert_eq!(err, ExecutionError::StateRootMismatch);
    assert_eq!(chain.height(), 0);
}

#[test]
fn wrong_height_and_wrong_parent_are_both_rejected() {
    let fx = fixture();
    let chain = LedgerChain::genesis();

    let body = SettlementBlockBody::new(vec![]);
    let next_state = mini_execution::apply_block(chain.state(), &body).unwrap();

    let wrong_height_header = BlockHeader {
        height: 2, // should be 1
        prev_hash: chain.tip_hash(),
        state_root: next_state.commitment(),
        timestamp_ms: 1_000,
        proposer: fx.signers[0].0.did(),
    };
    let hash = wrong_height_header.hash();
    let votes = fx.signers[..3]
        .iter()
        .map(|(root, device)| sign_vote(VoteKind::Precommit, 2, 0, hash, &root.did(), device))
        .collect();
    let qc = QuorumCertificate {
        height: 2,
        round: 0,
        block_hash: hash,
        votes,
    };
    let mut chain_copy = chain.clone();
    let err = chain_copy
        .apply_finalized_block(&wrong_height_header, &body, &qc, &fx.validators, &fx.oracle)
        .unwrap_err();
    assert_eq!(
        err,
        ExecutionError::WrongHeight {
            expected: 1,
            got: 2
        }
    );

    let wrong_parent_header = BlockHeader {
        height: 1,
        prev_hash: [0xAAu8; 32], // not the real genesis tip hash
        state_root: next_state.commitment(),
        timestamp_ms: 1_000,
        proposer: fx.signers[0].0.did(),
    };
    let hash2 = wrong_parent_header.hash();
    let votes2 = fx.signers[..3]
        .iter()
        .map(|(root, device)| sign_vote(VoteKind::Precommit, 1, 0, hash2, &root.did(), device))
        .collect();
    let qc2 = QuorumCertificate {
        height: 1,
        round: 0,
        block_hash: hash2,
        votes: votes2,
    };
    let mut chain_copy2 = chain.clone();
    let err2 = chain_copy2
        .apply_finalized_block(
            &wrong_parent_header,
            &body,
            &qc2,
            &fx.validators,
            &fx.oracle,
        )
        .unwrap_err();
    assert_eq!(err2, ExecutionError::WrongParent);
}

#[test]
fn a_timestamp_that_does_not_equal_the_block_height_is_rejected() {
    // Timestamp-attack finding (roadmap #44): `BlockHeader::timestamp_ms`
    // is proposer-controlled and a signature only proves who proposed a
    // value, never that it reflects real time. Consensus therefore uses
    // deterministic logical time -- at height N, `timestamp_ms` must equal
    // N exactly, giving the proposer no discretion over it at all, not
    // merely a monotonicity bound a malicious proposer could still exploit
    // (e.g. jumping straight to `u64::MAX`).
    let fx = fixture();
    let payer = SigningKey::from_seed(&[0x99; 32]);
    let mut chain = funded_chain(&payer, 200);
    let claim_1 = sign_claim(&payer, &recipient(0x51), 100, 0, 10_000, b"chain-1", 0).unwrap();
    let body_1 = SettlementBlockBody::new(vec![claim_1]);
    let next_state_1 = mini_execution::apply_block(chain.state(), &body_1).unwrap();
    let header_1 = BlockHeader {
        height: 1,
        prev_hash: chain.tip_hash(),
        state_root: next_state_1.commitment(),
        timestamp_ms: 1,
        proposer: fx.signers[0].0.did(),
    };
    let hash_1 = header_1.hash();
    let votes_1 = fx.signers[..3]
        .iter()
        .map(|(root, device)| sign_vote(VoteKind::Precommit, 1, 0, hash_1, &root.did(), device))
        .collect();
    let qc_1 = QuorumCertificate {
        height: 1,
        round: 0,
        block_hash: hash_1,
        votes: votes_1,
    };
    chain
        .apply_finalized_block(&header_1, &body_1, &qc_1, &fx.validators, &fx.oracle)
        .unwrap();
    assert_eq!(chain.height(), 1);

    // Height 2, claiming a wildly advanced timestamp instead of exactly 2 --
    // still "increasing," so a merely-monotonic check would have let this
    // through. It must be rejected even though everything else about it
    // (height, parent, state root, quorum) is perfectly valid.
    let claim_2 = sign_claim(&payer, &recipient(0x52), 100, 1, 10_000, b"chain-1", 0).unwrap();
    let body_2 = SettlementBlockBody::new(vec![claim_2]);
    let next_state_2 = mini_execution::apply_block(chain.state(), &body_2).unwrap();
    let header_2 = BlockHeader {
        height: 2,
        prev_hash: chain.tip_hash(),
        state_root: next_state_2.commitment(),
        timestamp_ms: u64::MAX, // increasing, but not the required value
        proposer: fx.signers[0].0.did(),
    };
    let hash_2 = header_2.hash();
    let votes_2 = fx.signers[..3]
        .iter()
        .map(|(root, device)| sign_vote(VoteKind::Precommit, 2, 0, hash_2, &root.did(), device))
        .collect();
    let qc_2 = QuorumCertificate {
        height: 2,
        round: 0,
        block_hash: hash_2,
        votes: votes_2,
    };
    let err = chain
        .apply_finalized_block(&header_2, &body_2, &qc_2, &fx.validators, &fx.oracle)
        .unwrap_err();
    assert_eq!(
        err,
        ExecutionError::TimestampNotDeterministic {
            expected: 2,
            got: u64::MAX,
        }
    );
    assert_eq!(
        chain.height(),
        1,
        "the chain must not advance on a non-deterministic timestamp"
    );
}
