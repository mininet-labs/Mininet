//! Validator accountability (roadmap R8): a validator that votes two ways
//! at once convicts itself, and the protocol can act on that.
//!
//! The tests worth reading are not "a real fault is provable" — that is the
//! easy half. They are the ones where a **false accusation** must fail, and
//! the one that pins the constitutional constraint: the penalty is exclusion
//! from counting, never anything denominated in value. Mininet has no stake,
//! and a penalty that needed one would be a value-to-voice edge (P1,
//! Directive 16).

use std::collections::BTreeMap;

use did_mini::{Capabilities, Controller, Did};
use mini_chain::{
    sign_vote, verify_finality, ChainError, EquivocationProof, EquivocationRegistry,
    QuorumCertificate, ValidatorOracle, ValidatorSet, VoteKind,
};

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

fn validator(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

/// Four validators, all resolvable through one directory.
fn a_network() -> (Vec<(Controller, Controller)>, Directory, ValidatorSet) {
    let validators: Vec<_> = (0..4).map(|i| validator(10 + i * 10)).collect();
    let mut directory = Directory::default();
    for (root, device) in &validators {
        directory.insert(root.kel());
        directory.insert(device.kel());
    }
    let set = ValidatorSet::new(validators.iter().map(|(r, _)| r.did()).collect()).unwrap();
    (validators, directory, set)
}

#[test]
fn a_validator_that_precommits_two_blocks_at_one_slot_convicts_itself() {
    // The fault BFT safety assumes nobody commits, made provable. Both
    // signatures are the offender's own, so no trusted reporter is
    // involved -- a third party who was offline the whole time can check
    // this years later.
    let (validators, directory, _) = a_network();
    let (root, device) = &validators[0];

    let one = sign_vote(VoteKind::Precommit, 7, 0, [0xaa; 32], &root.did(), device);
    let other = sign_vote(VoteKind::Precommit, 7, 0, [0xbb; 32], &root.did(), device);

    let proof = EquivocationProof::assemble(one, other, &directory).unwrap();
    assert_eq!(proof.offender().scid(), root.did().scid());
    assert_eq!(proof.height(), 7);
    assert_eq!(proof.round(), 0);
    assert_eq!(proof.kind(), VoteKind::Precommit);
}

#[test]
fn the_same_fault_seen_in_either_order_is_one_proof() {
    // Two observers who saw the conflicting votes in opposite orders must
    // produce the same proof, or one fault would have two identities and a
    // registry keyed on the digest would hold it twice.
    let (validators, directory, _) = a_network();
    let (root, device) = &validators[0];
    let one = sign_vote(VoteKind::Precommit, 3, 1, [0x11; 32], &root.did(), device);
    let other = sign_vote(VoteKind::Precommit, 3, 1, [0x99; 32], &root.did(), device);

    let forward = EquivocationProof::assemble(one.clone(), other.clone(), &directory).unwrap();
    let backward = EquivocationProof::assemble(other, one, &directory).unwrap();

    assert_eq!(forward.digest(), backward.digest());
    assert_eq!(forward, backward);

    let mut registry = EquivocationRegistry::new();
    registry.record(forward);
    registry.record(backward);
    assert_eq!(registry.len(), 1, "one fault, recorded once");
}

#[test]
fn voting_for_the_same_block_twice_is_not_a_fault() {
    // Networks re-deliver constantly. Treating a duplicate as misbehaviour
    // would make ordinary gossip look like an attack -- the same reasoning
    // that makes a re-broadcast idempotent in the nullifier set rather than
    // a double-spend.
    let (validators, directory, _) = a_network();
    let (root, device) = &validators[0];
    let one = sign_vote(VoteKind::Precommit, 5, 0, [0xcc; 32], &root.did(), device);
    let again = sign_vote(VoteKind::Precommit, 5, 0, [0xcc; 32], &root.did(), device);

    assert_eq!(
        EquivocationProof::assemble(one, again, &directory),
        Err(ChainError::NotAnEquivocation)
    );
}

#[test]
fn voting_at_different_slots_is_what_a_validator_is_supposed_to_do() {
    // A validator votes at every height and every round. Different blocks
    // at different slots is the job, not a fault -- and an accusation built
    // from it would let anyone "convict" any honest validator by collecting
    // two of its ordinary votes.
    let (validators, directory, _) = a_network();
    let (root, device) = &validators[0];

    for (a, b) in [
        // different height
        (
            sign_vote(VoteKind::Precommit, 1, 0, [0x01; 32], &root.did(), device),
            sign_vote(VoteKind::Precommit, 2, 0, [0x02; 32], &root.did(), device),
        ),
        // different round
        (
            sign_vote(VoteKind::Precommit, 1, 0, [0x01; 32], &root.did(), device),
            sign_vote(VoteKind::Precommit, 1, 1, [0x02; 32], &root.did(), device),
        ),
        // different phase
        (
            sign_vote(VoteKind::Prevote, 1, 0, [0x01; 32], &root.did(), device),
            sign_vote(VoteKind::Precommit, 1, 0, [0x02; 32], &root.did(), device),
        ),
    ] {
        assert_eq!(
            EquivocationProof::assemble(a, b, &directory),
            Err(ChainError::NotAnEquivocation)
        );
    }
}

#[test]
fn two_different_validators_disagreeing_is_consensus_working() {
    // The whole point of a vote is that validators may differ. An
    // accusation assembled from two honest validators voting differently
    // must fail, or disagreement itself becomes a punishable offence.
    let (validators, directory, _) = a_network();
    let (root_a, device_a) = &validators[0];
    let (root_b, device_b) = &validators[1];

    let from_a = sign_vote(
        VoteKind::Precommit,
        4,
        0,
        [0xaa; 32],
        &root_a.did(),
        device_a,
    );
    let from_b = sign_vote(
        VoteKind::Precommit,
        4,
        0,
        [0xbb; 32],
        &root_b.did(),
        device_b,
    );

    assert_eq!(
        EquivocationProof::assemble(from_a, from_b, &directory),
        Err(ChainError::NotAnEquivocation)
    );
}

#[test]
fn a_forged_vote_cannot_convict_anyone() {
    // The accusation is only as good as the signatures in it. Attributing
    // someone else's device's vote to a validator root must fail
    // verification rather than produce a proof -- otherwise framing an
    // honest validator costs nothing.
    let (validators, directory, _) = a_network();
    let (victim, _) = &validators[0];
    let (_, attacker_device) = &validators[1];

    // The attacker signs with their own device but claims the victim's root.
    let honest = sign_vote(
        VoteKind::Precommit,
        9,
        0,
        [0x01; 32],
        &victim.did(),
        &validators[0].1,
    );
    let forged = sign_vote(
        VoteKind::Precommit,
        9,
        0,
        [0x02; 32],
        &victim.did(),
        attacker_device,
    );

    assert!(
        EquivocationProof::assemble(honest, forged, &directory).is_err(),
        "a vote the victim never signed must not convict them"
    );
}

#[test]
fn a_vote_from_an_unresolvable_validator_is_refused_rather_than_assumed() {
    let (validators, _, _) = a_network();
    let (root, device) = &validators[0];
    let empty = Directory::default();

    let one = sign_vote(VoteKind::Precommit, 2, 0, [0x01; 32], &root.did(), device);
    let other = sign_vote(VoteKind::Precommit, 2, 0, [0x02; 32], &root.did(), device);

    assert_eq!(
        EquivocationProof::assemble(one, other, &empty),
        Err(ChainError::UnknownValidator)
    );
}

#[test]
fn a_proven_equivocator_can_be_excluded_and_then_stops_counting() {
    // Exclusion is the whole sanction, and this is what it buys: the
    // offender's votes no longer contribute to quorum. Nothing here is
    // denominated in value, because there is nothing to denominate it in.
    let (validators, directory, set) = a_network();
    let (offender, offender_device) = &validators[0];

    let mut registry = EquivocationRegistry::new();
    registry.record(
        EquivocationProof::assemble(
            sign_vote(
                VoteKind::Precommit,
                1,
                0,
                [0xaa; 32],
                &offender.did(),
                offender_device,
            ),
            sign_vote(
                VoteKind::Precommit,
                1,
                0,
                [0xbb; 32],
                &offender.did(),
                offender_device,
            ),
            &directory,
        )
        .unwrap(),
    );
    assert!(registry.is_proven_faulty(&offender.did()));
    assert!(!registry.is_proven_faulty(&validators[1].0.did()));

    let reduced = set.excluding(&registry.offenders()).unwrap();
    assert_eq!(reduced.len(), 3);
    assert!(!reduced.contains(&offender.did()));
    assert!(reduced.contains(&validators[1].0.did()));

    // A quorum certificate carrying the offender's vote plus two honest
    // ones met the old set's threshold (3 of 4) and does not meet the new
    // one's (3 of 3): the excluded vote simply stops being counted.
    let block = [0xaa; 32];
    let votes = vec![
        sign_vote(
            VoteKind::Precommit,
            1,
            0,
            block,
            &offender.did(),
            offender_device,
        ),
        sign_vote(
            VoteKind::Precommit,
            1,
            0,
            block,
            &validators[1].0.did(),
            &validators[1].1,
        ),
        sign_vote(
            VoteKind::Precommit,
            1,
            0,
            block,
            &validators[2].0.did(),
            &validators[2].1,
        ),
    ];
    let qc = QuorumCertificate {
        height: 1,
        round: 0,
        block_hash: block,
        votes,
    };

    assert!(
        verify_finality(&qc, &set, &directory).is_ok(),
        "three of four is a quorum in the original set"
    );
    assert!(
        matches!(
            verify_finality(&qc, &reduced, &directory),
            Err(ChainError::QuorumNotMet { .. })
        ),
        "the excluded validator's vote no longer counts toward the reduced set"
    );
}

#[test]
fn excluding_everyone_is_refused_rather_than_producing_a_dead_chain() {
    // If enough validators are provably faulty that removing them leaves
    // nobody, that is not a set to adopt -- it is a network to stop and
    // look at. Silently reaching an empty set through exclusions would be a
    // liveness failure dressed up as accountability.
    let (validators, _, set) = a_network();
    let everyone: Vec<Did> = validators.iter().map(|(r, _)| r.did()).collect();
    assert_eq!(set.excluding(&everyone), Err(ChainError::EmptyValidatorSet));
}

#[test]
fn excluding_a_validator_that_is_not_in_the_set_changes_nothing() {
    let (validators, _, set) = a_network();
    let stranger = validator(200).0.did();
    let unchanged = set.excluding(&[stranger]).unwrap();
    assert_eq!(unchanged, set);
    let _ = validators;
}

#[test]
fn nothing_in_the_accountability_path_is_denominated_in_value() {
    // P1/P2 and Directive 16, asserted rather than only documented. Most
    // chains slash a stake; Mininet has no stake, by construction, and a
    // penalty that needed one would make validator behaviour a function of
    // wealth -- rich validators could afford to equivocate and poor ones
    // could not afford to validate.
    //
    // This is a structural test: if someone later adds an amount, a
    // balance, or a stake to any of these types, their Debug output starts
    // saying so and this fails.
    let (validators, directory, set) = a_network();
    let (offender, device) = &validators[0];
    let proof = EquivocationProof::assemble(
        sign_vote(
            VoteKind::Precommit,
            1,
            0,
            [0xaa; 32],
            &offender.did(),
            device,
        ),
        sign_vote(
            VoteKind::Precommit,
            1,
            0,
            [0xbb; 32],
            &offender.did(),
            device,
        ),
        &directory,
    )
    .unwrap();
    let mut registry = EquivocationRegistry::new();
    registry.record(proof.clone());

    for rendered in [
        format!("{proof:?}"),
        format!("{registry:?}"),
        format!("{set:?}"),
    ] {
        let lowered = rendered.to_lowercase();
        for forbidden in [
            "stake", "balance", "amount", "micro", "bond", "deposit", "slash",
        ] {
            assert!(
                !lowered.contains(forbidden),
                "the accountability path acquired something value-shaped: {forbidden}"
            );
        }
    }
}
