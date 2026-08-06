//! What an attacker can and cannot do, written as tests.
//!
//! Each test names the attack it rules out. Where an attack is *not* ruled out,
//! there is a test showing exactly how far it gets and what the protocol says
//! about the result — an honest boundary is worth more than a comfortable one.

mod support;

use did_mini::Capabilities;
use mini_porep::SealCommitment;
use mini_storage_fraud::{
    seal_commitment_digest, storage_commitment_of, verify_conflict, Admission, ConflictAttribution,
    ConflictKind, DecodeFailure, FraudError, RegisteredReplicaClaim, RegistrationPolicy,
    RegistrationReceipt, ReplicaConflictEvidence, ReplicaRegistry,
};
use support::{
    attest, context, data, directory_of, policy, receipt, registered_claim, seal_for, Party,
};

// ---------------------------------------------------------------------------
// The honest path works at all
// ---------------------------------------------------------------------------

#[test]
fn an_audited_registered_claim_verifies_end_to_end() {
    let provider = Party::provider(10);
    let (first_auditor, second_auditor) = (Party::auditor(20), Party::auditor(30));
    let directory = directory_of(&[&provider, &first_auditor, &second_auditor]);

    let (claim, replica) = registered_claim(
        &provider,
        &[&first_auditor, &second_auditor],
        &context(1),
        &data(0),
    );
    let verified = claim.verify(&directory, &policy()).unwrap();

    assert_eq!(verified.distinct_auditor_roots(), 2);
    assert_eq!(verified.replica_root(), replica.replica_root());
    // The PDP commitment is derived from the audited seal, so there is no
    // second, independently-supplied copy of it that could disagree.
    assert_eq!(
        verified.storage_commitment(),
        storage_commitment_of(&replica.commitment())
    );
    assert_eq!(
        verified.storage_commitment().block_count,
        replica.node_count()
    );
}

// ---------------------------------------------------------------------------
// 1. Copying an honest provider's published commitment
// ---------------------------------------------------------------------------

#[test]
fn copying_an_honest_providers_replica_root_does_not_survive_an_audit() {
    // The attack the previous design admitted: watch an honest provider publish
    // a replica root, then claim it under your own perfectly valid identity.
    //
    // The attacker must now present a seal commitment whose replica_id is the
    // one *its own* identity derives to. Sealing is deterministic in that id,
    // so the attacker cannot produce labels consistent with both its own id and
    // the victim's replica root -- and an auditor recomputes the labeling
    // itself rather than taking the commitment's word for it.
    let victim = Party::provider(11);
    let attacker = Party::provider(12);
    let auditor = Party::auditor(21);

    let victim_replica = seal_for(&victim, &context(2), &data(1));
    let victim_seal = victim_replica.commitment();

    let attacker_context = context(2);
    let attacker_replica = seal_for(&attacker, &attacker_context, &data(1));
    let forged = SealCommitment {
        // The attacker's own required replica id, so the identity binding check
        // passes...
        replica_id: attacker_replica.commitment().replica_id,
        // ...but the victim's replica root, which is the whole point of the
        // theft.
        replica_root: victim_seal.replica_root,
        ..attacker_replica.commitment()
    };

    // An honest auditor answering from the only replica that exists for that
    // root -- the victim's -- rejects it.
    assert_eq!(
        attest(&auditor, &forged, &victim_replica, 0x51, 8),
        Err(FraudError::AuditFailed)
    );
    // And answering from the attacker's own replica fails too, because the
    // forged commitment's replica root does not match what that replica seals
    // to.
    assert_eq!(
        attest(&auditor, &forged, &attacker_replica, 0x52, 8),
        Err(FraudError::AuditFailed)
    );
}

#[test]
fn a_stolen_registration_receipt_does_not_transfer_to_another_identity() {
    // Receipts are public. An attacker that copies a victim's whole receipt
    // gets attestations naming the victim's seal digest, which cannot match the
    // attacker's own identity-bound seal.
    let victim = Party::provider(13);
    let attacker = Party::provider(14);
    let (first_auditor, second_auditor) = (Party::auditor(22), Party::auditor(23));
    let directory = directory_of(&[&victim, &attacker, &first_auditor, &second_auditor]);

    let (victim_claim, _) = registered_claim(
        &victim,
        &[&first_auditor, &second_auditor],
        &context(3),
        &data(2),
    );
    let stolen = victim_claim.registration().clone();

    let attacker_replica = seal_for(&attacker, &context(3), &data(2));
    let attacker_claim = RegisteredReplicaClaim::issue(
        &attacker.root_did(),
        &attacker.device,
        context(3),
        attacker_replica.commitment(),
        stolen,
        1_700_000_000_002,
    )
    .unwrap();

    assert_eq!(
        attacker_claim.verify(&directory, &policy()),
        Err(FraudError::AttestationTargetMismatch)
    );
}

// ---------------------------------------------------------------------------
// 2. Breaking the identity binding
// ---------------------------------------------------------------------------

#[test]
fn a_replica_id_that_is_not_identity_derived_is_refused_at_issue_and_at_verify() {
    let provider = Party::provider(15);
    let (first_auditor, second_auditor) = (Party::auditor(24), Party::auditor(25));
    let directory = directory_of(&[&provider, &first_auditor, &second_auditor]);

    let replica = seal_for(&provider, &context(4), &data(3));
    let honest_seal = replica.commitment();
    let quorum = receipt(&[&first_auditor, &second_auditor], &honest_seal, &replica);

    let mut unbound = honest_seal.clone();
    unbound.replica_id[0] ^= 0xFF;
    assert_eq!(
        RegisteredReplicaClaim::issue(
            &provider.root_did(),
            &provider.device,
            context(4),
            unbound,
            quorum.clone(),
            1_700_000_000_003,
        ),
        Err(FraudError::ReplicaIdNotIdentityBound)
    );

    // And a claim tampered with on the wire, after signing, is refused before
    // any signature work happens.
    let honest = RegisteredReplicaClaim::issue(
        &provider.root_did(),
        &provider.device,
        context(4),
        honest_seal.clone(),
        quorum,
        1_700_000_000_003,
    )
    .unwrap();
    let mut bytes = honest.to_bytes();
    let offset = bytes
        .windows(32)
        .position(|window| window == honest_seal.replica_id)
        .expect("the claim carries its replica id verbatim");
    bytes[offset] ^= 0xFF;
    let tampered = RegisteredReplicaClaim::from_bytes(&bytes).unwrap();
    assert_eq!(
        tampered.verify(&directory, &policy()),
        Err(FraudError::ReplicaIdNotIdentityBound)
    );
}

#[test]
fn a_claim_for_a_different_context_does_not_verify() {
    // The context is inside the replica id, so re-labelling a real replica as
    // being of some other assignment breaks the binding.
    let provider = Party::provider(16);
    let (first_auditor, second_auditor) = (Party::auditor(26), Party::auditor(27));
    let directory = directory_of(&[&provider, &first_auditor, &second_auditor]);

    let replica = seal_for(&provider, &context(5), &data(4));
    let seal = replica.commitment();
    let quorum = receipt(&[&first_auditor, &second_auditor], &seal, &replica);

    let mut other_network = context(5);
    other_network.network_id[0] ^= 1;
    assert_eq!(
        RegisteredReplicaClaim::issue(
            &provider.root_did(),
            &provider.device,
            other_network,
            seal,
            quorum,
            1_700_000_000_004,
        ),
        Err(FraudError::ReplicaIdNotIdentityBound)
    );
    let _ = directory;
}

// ---------------------------------------------------------------------------
// 3. Registration quorum rules
// ---------------------------------------------------------------------------

#[test]
fn a_provider_cannot_audit_its_own_registration() {
    let provider = Party::provider(17);
    let auditor = Party::auditor(28);
    let directory = directory_of(&[&provider, &auditor]);

    let replica = seal_for(&provider, &context(6), &data(5));
    let seal = replica.commitment();
    // The provider's own root signs one of the two attestations, through a
    // device of its own.
    let self_attestation = attest(&provider, &seal, &replica, 0x60, 8).unwrap();
    let honest = attest(&auditor, &seal, &replica, 0x61, 8).unwrap();
    let quorum = RegistrationReceipt::new(vec![self_attestation, honest]).unwrap();

    let claim = RegisteredReplicaClaim::issue(
        &provider.root_did(),
        &provider.device,
        context(6),
        seal,
        quorum,
        1_700_000_000_005,
    )
    .unwrap();
    assert_eq!(
        claim.verify(&directory, &policy()),
        Err(FraudError::SelfAttestation)
    );
}

#[test]
fn two_devices_of_one_auditor_root_are_one_auditor() {
    // The one-root-one-voice rule: an auditor cannot reach quorum alone by
    // delegating a second device to itself.
    let provider = Party::provider(18);
    let mut auditor = Party::auditor(29);
    let second_device = did_mini::Controller::incept_device_single_from_seeds(
        &auditor.root_did(),
        &[0x77; 32],
        &[0x78; 32],
    )
    .unwrap();
    auditor
        .root
        .delegate_device(&second_device.did(), Capabilities::primary())
        .unwrap();

    let mut directory = directory_of(&[&provider, &auditor]);
    directory.refresh(&second_device);

    let replica = seal_for(&provider, &context(7), &data(6));
    let seal = replica.commitment();
    let first = attest(&auditor, &seal, &replica, 0x62, 8).unwrap();
    let second = mini_storage_fraud::audit_and_attest(
        &auditor.root_did(),
        &second_device,
        &seal,
        [0x63; 32],
        8,
        1_700_000_000_000,
        |challenge| mini_porep::answer_challenge(&replica, challenge).ok(),
    )
    .unwrap();

    let quorum = RegistrationReceipt::new(vec![first, second]).unwrap();
    assert_eq!(quorum.attestations().len(), 2);
    assert_eq!(quorum.distinct_auditor_roots(), 1);

    let claim = RegisteredReplicaClaim::issue(
        &provider.root_did(),
        &provider.device,
        context(7),
        seal,
        quorum,
        1_700_000_000_006,
    )
    .unwrap();
    assert_eq!(
        claim.verify(&directory, &policy()),
        Err(FraudError::InsufficientAuditQuorum { needed: 2, got: 1 })
    );
}

#[test]
fn a_quorum_that_reused_one_challenge_seed_did_not_sample_independently() {
    let provider = Party::provider(19);
    let (first_auditor, second_auditor) = (Party::auditor(31), Party::auditor(32));
    let directory = directory_of(&[&provider, &first_auditor, &second_auditor]);

    let replica = seal_for(&provider, &context(8), &data(7));
    let seal = replica.commitment();
    let first = attest(&first_auditor, &seal, &replica, 0x64, 8).unwrap();
    let second = attest(&second_auditor, &seal, &replica, 0x64, 8).unwrap();
    let quorum = RegistrationReceipt::new(vec![first, second]).unwrap();

    let claim = RegisteredReplicaClaim::issue(
        &provider.root_did(),
        &provider.device,
        context(8),
        seal,
        quorum,
        1_700_000_000_007,
    )
    .unwrap();
    assert_eq!(
        claim.verify(&directory, &policy()),
        Err(FraudError::RepeatedChallengeSeed)
    );
}

#[test]
fn an_audit_that_sampled_too_few_challenges_is_refused() {
    let provider = Party::provider(40);
    let (first_auditor, second_auditor) = (Party::auditor(41), Party::auditor(42));
    let directory = directory_of(&[&provider, &first_auditor, &second_auditor]);

    let replica = seal_for(&provider, &context(9), &data(8));
    let seal = replica.commitment();
    let thorough = attest(&first_auditor, &seal, &replica, 0x65, 8).unwrap();
    let cursory = attest(&second_auditor, &seal, &replica, 0x66, 2).unwrap();
    let quorum = RegistrationReceipt::new(vec![thorough, cursory]).unwrap();

    let claim = RegisteredReplicaClaim::issue(
        &provider.root_did(),
        &provider.device,
        context(9),
        seal,
        quorum,
        1_700_000_000_008,
    )
    .unwrap();
    assert_eq!(
        claim.verify(&directory, &policy()),
        Err(FraudError::InsufficientAuditSampling { needed: 8, got: 2 })
    );
}

#[test]
fn an_unanswered_challenge_fails_the_audit_rather_than_being_skipped() {
    let provider = Party::provider(43);
    let auditor = Party::auditor(44);
    let replica = seal_for(&provider, &context(10), &data(9));
    let seal = replica.commitment();

    assert_eq!(
        mini_storage_fraud::audit_and_attest(
            &auditor.root_did(),
            &auditor.device,
            &seal,
            [0x67; 32],
            8,
            1_700_000_000_000,
            |_| None,
        ),
        Err(FraudError::AuditUnanswered)
    );
}

#[test]
fn a_policy_that_would_accept_self_assertion_cannot_be_constructed() {
    assert_eq!(
        RegistrationPolicy::new(0, 8),
        Err(FraudError::InvalidPolicy)
    );
    assert_eq!(
        RegistrationPolicy::new(2, 0),
        Err(FraudError::InvalidPolicy)
    );
    assert_eq!(
        RegistrationPolicy::new(2, mini_storage_fraud::MAX_AUDIT_CHALLENGES + 1),
        Err(FraudError::InvalidPolicy)
    );
    assert_eq!(RegistrationPolicy::baseline().min_distinct_auditors(), 2);
}

// ---------------------------------------------------------------------------
// 4. Identity: roots, devices, capabilities, rotation
// ---------------------------------------------------------------------------

#[test]
fn a_claim_survives_the_providers_ordinary_key_rotation() {
    // Durable evidence must not decay the moment its signer rotates keys, which
    // is exactly what verifying against the *current* key state would cause.
    let mut provider = Party::provider(45);
    let (first_auditor, second_auditor) = (Party::auditor(46), Party::auditor(47));

    let (claim, _) = registered_claim(
        &provider,
        &[&first_auditor, &second_auditor],
        &context(11),
        &data(10),
    );

    provider.device.rotate().unwrap();
    provider.device.rotate().unwrap();
    let directory = directory_of(&[&provider, &first_auditor, &second_auditor]);

    let verified = claim.clone().verify(&directory, &policy()).unwrap();
    assert_eq!(verified.provider_root(), &provider.root_did());
    // The claim was signed two rotations ago, and says so.
    assert_eq!(claim.signing_kel_sn(), 0);
}

#[test]
fn an_auditors_attestation_also_survives_that_auditors_rotation() {
    let provider = Party::provider(48);
    let (mut first_auditor, second_auditor) = (Party::auditor(49), Party::auditor(50));

    let (claim, _) = registered_claim(
        &provider,
        &[&first_auditor, &second_auditor],
        &context(12),
        &data(11),
    );

    first_auditor.device.rotate().unwrap();
    let directory = directory_of(&[&provider, &first_auditor, &second_auditor]);
    assert!(claim.verify(&directory, &policy()).is_ok());
}

#[test]
fn a_device_without_the_store_capability_cannot_commit_its_root_to_a_replica() {
    let provider = Party::new(51, Capabilities::primary()); // no STORE
    let (first_auditor, second_auditor) = (Party::auditor(52), Party::auditor(53));
    let directory = directory_of(&[&provider, &first_auditor, &second_auditor]);

    let (claim, _) = registered_claim(
        &provider,
        &[&first_auditor, &second_auditor],
        &context(13),
        &data(12),
    );
    assert_eq!(
        claim.verify(&directory, &policy()),
        Err(FraudError::MissingCapability)
    );
}

#[test]
fn a_device_the_root_never_delegated_cannot_speak_for_it() {
    let provider = Party::provider(54);
    let stranger = Party::provider(55);
    let (first_auditor, second_auditor) = (Party::auditor(56), Party::auditor(57));

    let mut directory = directory_of(&[&provider, &stranger, &first_auditor, &second_auditor]);

    // A claim naming the provider's root but signed by a device of another
    // root entirely. The replica id must bind to the signing device for issue
    // to succeed, so this is the strongest form the attack can take.
    let replica = {
        let params = mini_storage_fraud::seal_params_for(
            &provider.root_did(),
            &stranger.device_did(),
            &context(14),
            support::LAYERS,
        )
        .unwrap();
        mini_porep::seal(&params, &data(13)).unwrap()
    };
    let seal = replica.commitment();
    let quorum = receipt(&[&first_auditor, &second_auditor], &seal, &replica);
    let claim = RegisteredReplicaClaim::issue(
        &provider.root_did(),
        &stranger.device,
        context(14),
        seal,
        quorum,
        1_700_000_000_009,
    )
    .unwrap();

    assert_eq!(
        claim.verify(&directory, &policy()),
        Err(FraudError::DelegationRejected)
    );

    directory.forget(&provider.root_did());
    let (orphan, _) = registered_claim(
        &provider,
        &[&first_auditor, &second_auditor],
        &context(14),
        &data(13),
    );
    assert_eq!(
        orphan.verify(&directory, &policy()),
        Err(FraudError::UnknownIdentity)
    );
}

#[test]
fn a_revoked_storage_device_stops_verifying_once_the_root_kel_is_current() {
    // The freshness gap, stated as a test rather than a footnote: revocation
    // only takes effect for a verifier holding the root's post-revocation KEL.
    let mut provider = Party::provider(58);
    let (first_auditor, second_auditor) = (Party::auditor(59), Party::auditor(60));

    let (claim, _) = registered_claim(
        &provider,
        &[&first_auditor, &second_auditor],
        &context(15),
        &data(14),
    );
    let stale = directory_of(&[&provider, &first_auditor, &second_auditor]);
    assert!(claim.clone().verify(&stale, &policy()).is_ok());

    provider.root.revoke_device(&provider.device_did()).unwrap();
    let current = directory_of(&[&provider, &first_auditor, &second_auditor]);
    assert_eq!(
        claim.verify(&current, &policy()),
        Err(FraudError::DelegationRejected)
    );
}

// ---------------------------------------------------------------------------
// 5. The registry: where uniqueness is actually enforced
// ---------------------------------------------------------------------------

#[test]
fn a_registry_accepts_a_replica_root_once_and_recognises_a_replay() {
    let provider = Party::provider(61);
    let (first_auditor, second_auditor) = (Party::auditor(62), Party::auditor(63));
    let directory = directory_of(&[&provider, &first_auditor, &second_auditor]);
    let mut registry = ReplicaRegistry::new();

    let (claim, _) = registered_claim(
        &provider,
        &[&first_auditor, &second_auditor],
        &context(16),
        &data(15),
    );

    assert!(matches!(
        registry
            .admit(claim.clone(), &directory, &policy())
            .unwrap(),
        Admission::Accepted(_)
    ));
    assert_eq!(registry.len(), 1);
    assert!(matches!(
        registry.admit(claim, &directory, &policy()).unwrap(),
        Admission::AlreadyRegistered
    ));
    assert_eq!(registry.len(), 1);
}

#[test]
fn a_registry_never_stores_a_claim_it_could_not_verify() {
    let provider = Party::new(64, Capabilities::primary()); // no STORE
    let (first_auditor, second_auditor) = (Party::auditor(65), Party::auditor(66));
    let directory = directory_of(&[&provider, &first_auditor, &second_auditor]);
    let mut registry = ReplicaRegistry::new();

    let (claim, replica) = registered_claim(
        &provider,
        &[&first_auditor, &second_auditor],
        &context(17),
        &data(16),
    );
    assert_eq!(
        registry.admit(claim, &directory, &policy()),
        Err(FraudError::MissingCapability)
    );
    assert!(registry.is_empty());
    assert!(registry.registered(&replica.replica_root()).is_none());
}

// ---------------------------------------------------------------------------
// 6. Conflict: the residual attack, and what the output refuses to say
// ---------------------------------------------------------------------------

/// The only way to reach a conflict is a quorum that signed without auditing.
/// This builds exactly that, so the conflict path is tested against the real
/// residual threat rather than a hypothetical one.
fn conflicting_claim_via_corrupt_quorum(
    attacker: &Party,
    corrupt: &[&Party],
    stolen_root: [u8; 32],
    divergent_shape: bool,
) -> RegisteredReplicaClaim {
    let attacker_replica = seal_for(attacker, &context(20), &data(20));
    let mut forged = attacker_replica.commitment();
    forged.replica_root = stolen_root;
    if divergent_shape {
        forged.data_root[0] ^= 0xFF;
    }

    let attestations = corrupt
        .iter()
        .enumerate()
        .map(|(index, auditor)| {
            mini_storage_fraud::AuditAttestation::issue(
                &auditor.root_did(),
                &auditor.device,
                &forged,
                [0x90 + index as u8; 32],
                8,
                1_700_000_000_000,
            )
            .unwrap()
        })
        .collect();

    RegisteredReplicaClaim::issue(
        &attacker.root_did(),
        &attacker.device,
        context(20),
        forged,
        RegistrationReceipt::new(attestations).unwrap(),
        1_700_000_000_010,
    )
    .unwrap()
}

#[test]
fn a_duplicate_replica_root_is_reported_as_an_unattributed_conflict() {
    let honest = Party::provider(70);
    let attacker = Party::provider(71);
    let (honest_first, honest_second) = (Party::auditor(72), Party::auditor(73));
    let (corrupt_first, corrupt_second) = (Party::auditor(74), Party::auditor(75));
    let directory = directory_of(&[
        &honest,
        &attacker,
        &honest_first,
        &honest_second,
        &corrupt_first,
        &corrupt_second,
    ]);

    let (honest_claim, honest_replica) = registered_claim(
        &honest,
        &[&honest_first, &honest_second],
        &context(20),
        &data(20),
    );
    let attacker_claim = conflicting_claim_via_corrupt_quorum(
        &attacker,
        &[&corrupt_first, &corrupt_second],
        honest_replica.replica_root(),
        false,
    );

    let evidence = ReplicaConflictEvidence::new(honest_claim, attacker_claim);
    let conflict = verify_conflict(evidence, &directory, &policy()).unwrap();

    assert_eq!(conflict.kind(), ConflictKind::DuplicateReplicaRoot);
    // The load-bearing assertion of this whole crate: nobody is blamed.
    assert_eq!(conflict.attribution(), ConflictAttribution::Unattributed);
    assert_eq!(conflict.replica_root(), honest_replica.replica_root());
    let (first, second) = conflict.involved_roots();
    assert_ne!(first.scid(), second.scid());
    assert!(conflict.required_follow_up().contains("re-audit"));
}

#[test]
fn a_duplicate_root_over_a_differently_shaped_replica_is_reported_separately() {
    let honest = Party::provider(76);
    let attacker = Party::provider(77);
    let (honest_first, honest_second) = (Party::auditor(78), Party::auditor(79));
    let (corrupt_first, corrupt_second) = (Party::auditor(80), Party::auditor(81));
    let directory = directory_of(&[
        &honest,
        &attacker,
        &honest_first,
        &honest_second,
        &corrupt_first,
        &corrupt_second,
    ]);

    let (honest_claim, honest_replica) = registered_claim(
        &honest,
        &[&honest_first, &honest_second],
        &context(20),
        &data(20),
    );
    let attacker_claim = conflicting_claim_via_corrupt_quorum(
        &attacker,
        &[&corrupt_first, &corrupt_second],
        honest_replica.replica_root(),
        true,
    );

    let conflict = verify_conflict(
        ReplicaConflictEvidence::new(honest_claim, attacker_claim),
        &directory,
        &policy(),
    )
    .unwrap();
    assert_eq!(
        conflict.kind(),
        ConflictKind::DuplicateReplicaRootWithDivergentShape
    );
    assert_eq!(conflict.attribution(), ConflictAttribution::Unattributed);
}

#[test]
fn a_registry_that_meets_the_second_claim_reports_a_conflict_and_keeps_the_first() {
    let honest = Party::provider(82);
    let attacker = Party::provider(83);
    let (honest_first, honest_second) = (Party::auditor(84), Party::auditor(85));
    let (corrupt_first, corrupt_second) = (Party::auditor(86), Party::auditor(87));
    let directory = directory_of(&[
        &honest,
        &attacker,
        &honest_first,
        &honest_second,
        &corrupt_first,
        &corrupt_second,
    ]);
    let mut registry = ReplicaRegistry::new();

    let (honest_claim, honest_replica) = registered_claim(
        &honest,
        &[&honest_first, &honest_second],
        &context(20),
        &data(20),
    );
    let honest_id = honest_claim.claim_id();
    assert!(matches!(
        registry.admit(honest_claim, &directory, &policy()).unwrap(),
        Admission::Accepted(_)
    ));

    let attacker_claim = conflicting_claim_via_corrupt_quorum(
        &attacker,
        &[&corrupt_first, &corrupt_second],
        honest_replica.replica_root(),
        false,
    );
    let admission = registry
        .admit(attacker_claim, &directory, &policy())
        .unwrap();
    assert!(matches!(admission, Admission::Conflict(_)));

    // The registry keeps what it already had; a conflicting arrival never
    // evicts an accepted registration.
    assert_eq!(registry.len(), 1);
    assert_eq!(
        registry
            .registered(&honest_replica.replica_root())
            .unwrap()
            .claim_id(),
        honest_id
    );
}

#[test]
fn evidence_is_the_same_object_whichever_order_it_is_assembled_in() {
    let honest = Party::provider(88);
    let attacker = Party::provider(89);
    let (honest_first, honest_second) = (Party::auditor(90), Party::auditor(91));
    let (corrupt_first, corrupt_second) = (Party::auditor(92), Party::auditor(93));

    let (honest_claim, honest_replica) = registered_claim(
        &honest,
        &[&honest_first, &honest_second],
        &context(20),
        &data(20),
    );
    let attacker_claim = conflicting_claim_via_corrupt_quorum(
        &attacker,
        &[&corrupt_first, &corrupt_second],
        honest_replica.replica_root(),
        false,
    );

    let forwards = ReplicaConflictEvidence::new(honest_claim.clone(), attacker_claim.clone());
    let backwards = ReplicaConflictEvidence::new(attacker_claim, honest_claim);
    assert_eq!(forwards.evidence_id(), backwards.evidence_id());
    assert_eq!(forwards.to_bytes(), backwards.to_bytes());
}

#[test]
fn unrelated_claims_are_not_a_conflict() {
    let first_provider = Party::provider(94);
    let second_provider = Party::provider(95);
    let (first_auditor, second_auditor) = (Party::auditor(96), Party::auditor(97));
    let directory = directory_of(&[
        &first_provider,
        &second_provider,
        &first_auditor,
        &second_auditor,
    ]);

    let (first, _) = registered_claim(
        &first_provider,
        &[&first_auditor, &second_auditor],
        &context(21),
        &data(21),
    );
    let (second, _) = registered_claim(
        &second_provider,
        &[&first_auditor, &second_auditor],
        &context(21),
        &data(21),
    );

    // Two honest providers sealing the *same source data* land on different
    // replica roots, because their identity-bound replica ids differ. That is
    // the property the whole scheme rests on.
    assert_ne!(first.seal().replica_root, second.seal().replica_root);
    assert_eq!(
        verify_conflict(
            ReplicaConflictEvidence::new(first, second),
            &directory,
            &policy()
        ),
        Err(FraudError::NotAConflict)
    );
}

#[test]
fn one_root_cannot_manufacture_a_conflict_against_itself() {
    let provider = Party::provider(98);
    let (first_auditor, second_auditor) = (Party::auditor(99), Party::auditor(100));
    let directory = directory_of(&[&provider, &first_auditor, &second_auditor]);

    let (first, _) = registered_claim(
        &provider,
        &[&first_auditor, &second_auditor],
        &context(22),
        &data(22),
    );
    let second = first.clone();
    assert_eq!(
        verify_conflict(
            ReplicaConflictEvidence::new(first, second),
            &directory,
            &policy()
        ),
        Err(FraudError::NotAConflict)
    );
}

#[test]
fn a_conflict_between_claims_that_do_not_individually_verify_is_not_evidence() {
    let honest = Party::provider(101);
    let attacker = Party::provider(102);
    let (honest_first, honest_second) = (Party::auditor(103), Party::auditor(104));
    let (corrupt_first, corrupt_second) = (Party::auditor(105), Party::auditor(106));
    // The corrupt auditors' KELs are simply not known to this verifier.
    let directory = directory_of(&[&honest, &attacker, &honest_first, &honest_second]);

    let (honest_claim, honest_replica) = registered_claim(
        &honest,
        &[&honest_first, &honest_second],
        &context(20),
        &data(20),
    );
    let attacker_claim = conflicting_claim_via_corrupt_quorum(
        &attacker,
        &[&corrupt_first, &corrupt_second],
        honest_replica.replica_root(),
        false,
    );

    assert_eq!(
        verify_conflict(
            ReplicaConflictEvidence::new(honest_claim, attacker_claim),
            &directory,
            &policy()
        ),
        Err(FraudError::UnknownIdentity)
    );
}

// ---------------------------------------------------------------------------
// 7. Wire format
// ---------------------------------------------------------------------------

#[test]
fn every_object_round_trips_and_rejects_trailing_bytes() {
    let provider = Party::provider(107);
    let (first_auditor, second_auditor) = (Party::auditor(108), Party::auditor(109));

    let (claim, replica) = registered_claim(
        &provider,
        &[&first_auditor, &second_auditor],
        &context(23),
        &data(23),
    );

    let attestation = &claim.registration().attestations()[0];
    assert_eq!(
        mini_storage_fraud::AuditAttestation::from_bytes(&attestation.to_bytes()).unwrap(),
        *attestation
    );
    assert_eq!(
        RegistrationReceipt::from_bytes(&claim.registration().to_bytes()).unwrap(),
        *claim.registration()
    );
    assert_eq!(
        RegisteredReplicaClaim::from_bytes(&claim.to_bytes()).unwrap(),
        claim
    );

    for bytes in [
        attestation.to_bytes(),
        claim.registration().to_bytes(),
        claim.to_bytes(),
    ] {
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(matches!(
            RegisteredReplicaClaim::from_bytes(&trailing),
            Err(FraudError::Decode(_))
        ));

        for truncate_to in [0, 1, bytes.len() / 2, bytes.len() - 1] {
            assert!(matches!(
                RegisteredReplicaClaim::from_bytes(&bytes[..truncate_to]),
                Err(FraudError::Decode(_))
            ));
        }
    }

    let _ = replica;
}

#[test]
fn a_threshold_identity_with_more_keys_than_the_old_codec_allowed_round_trips() {
    // The previous codec capped signatures at 16 while did-mini permits 32 keys
    // and 64 signatures, so a large threshold identity could sign a claim,
    // verify it in memory, and then fail to decode its own encoding.
    use mini_crypto::SigningKey;

    let keys: Vec<SigningKey> = (0..17u8).map(|i| SigningKey::from_seed(&[i; 32])).collect();
    let next: Vec<SigningKey> = (0..17u8)
        .map(|i| SigningKey::from_seed(&[i.wrapping_add(100); 32]))
        .collect();
    let mut root = did_mini::Controller::incept(keys, 17, next, 17).unwrap();

    let device = did_mini::Controller::incept_device_single_from_seeds(
        &root.did(),
        &[0xC1; 32],
        &[0xC2; 32],
    )
    .unwrap();
    root.delegate_device(
        &device.did(),
        Capabilities::secondary().with(Capabilities::STORE),
    )
    .unwrap();

    // The 17-key identity is the auditor here, so its 17 signatures ride inside
    // the claim's registration receipt.
    let big_auditor = Party {
        root,
        device: did_mini::Controller::incept(
            (0..17u8)
                .map(|i| SigningKey::from_seed(&[i.wrapping_add(50); 32]))
                .collect(),
            17,
            (0..17u8)
                .map(|i| SigningKey::from_seed(&[i.wrapping_add(150); 32]))
                .collect(),
            17,
        )
        .unwrap(),
    };
    let auditor_root_did = big_auditor.root.did();

    let provider = Party::provider(110);
    let other_auditor = Party::auditor(111);
    let replica = seal_for(&provider, &context(24), &data(24));
    let seal = replica.commitment();

    // The big identity attests as a root signing directly for itself.
    let big_attestation = mini_storage_fraud::audit_and_attest(
        &auditor_root_did,
        &big_auditor.root,
        &seal,
        [0xD1; 32],
        8,
        1_700_000_000_000,
        |challenge| mini_porep::answer_challenge(&replica, challenge).ok(),
    )
    .unwrap();
    let small_attestation = attest(&other_auditor, &seal, &replica, 0xD2, 8).unwrap();
    let quorum =
        RegistrationReceipt::new(vec![big_attestation.clone(), small_attestation]).unwrap();

    let claim = RegisteredReplicaClaim::issue(
        &provider.root_did(),
        &provider.device,
        context(24),
        seal,
        quorum,
        1_700_000_000_011,
    )
    .unwrap();

    let decoded = RegisteredReplicaClaim::from_bytes(&claim.to_bytes()).unwrap();
    assert_eq!(decoded, claim);

    let mut directory = directory_of(&[&provider, &other_auditor]);
    directory.refresh(&big_auditor.root);
    let verified = decoded.verify(&directory, &policy()).unwrap();
    assert_eq!(verified.distinct_auditor_roots(), 2);
    assert_eq!(
        mini_storage_fraud::AuditAttestation::from_bytes(&big_attestation.to_bytes()).unwrap(),
        big_attestation
    );
}

#[test]
fn a_receipt_whose_attestations_are_out_of_canonical_order_is_rejected() {
    let provider = Party::provider(112);
    let (first_auditor, second_auditor) = (Party::auditor(113), Party::auditor(114));
    let replica = seal_for(&provider, &context(25), &data(25));
    let seal = replica.commitment();

    let first = attest(&first_auditor, &seal, &replica, 0xE1, 8).unwrap();
    let second = attest(&second_auditor, &seal, &replica, 0xE2, 8).unwrap();

    // RegistrationReceipt::new sorts, so both orderings produce identical bytes.
    let forwards = RegistrationReceipt::new(vec![first.clone(), second.clone()]).unwrap();
    let backwards = RegistrationReceipt::new(vec![second.clone(), first.clone()]).unwrap();
    assert_eq!(forwards.to_bytes(), backwards.to_bytes());

    // A hand-built encoding in the wrong order is refused rather than silently
    // normalised, so one receipt never has two valid wire forms.
    let ordered = if first.attestation_id() < second.attestation_id() {
        (second, first)
    } else {
        (first, second)
    };
    let mut bytes = vec![mini_storage_fraud::REGISTRATION_RECEIPT_VERSION];
    bytes.extend_from_slice(&2u64.to_be_bytes());
    bytes.extend_from_slice(&ordered.0.to_bytes()[1..]);
    bytes.extend_from_slice(&ordered.1.to_bytes()[1..]);
    assert_eq!(
        RegistrationReceipt::from_bytes(&bytes),
        Err(DecodeFailure::NoncanonicalAttestationOrder.into())
    );
}

#[test]
fn an_empty_quorum_cannot_be_built_or_decoded() {
    assert!(matches!(
        RegistrationReceipt::new(Vec::new()),
        Err(FraudError::InsufficientAuditQuorum { .. })
    ));
    let bytes = {
        let mut bytes = vec![mini_storage_fraud::REGISTRATION_RECEIPT_VERSION];
        bytes.extend_from_slice(&0u64.to_be_bytes());
        bytes
    };
    assert!(matches!(
        RegistrationReceipt::from_bytes(&bytes),
        Err(FraudError::InsufficientAuditQuorum { .. })
    ));
}

#[test]
fn a_seal_digest_is_what_ties_a_quorum_to_one_replica() {
    let provider = Party::provider(115);
    let replica = seal_for(&provider, &context(26), &data(26));
    let seal = replica.commitment();
    let mut altered = seal.clone();
    altered.replica_root[0] ^= 1;
    assert_ne!(
        seal_commitment_digest(&seal),
        seal_commitment_digest(&altered)
    );
}
