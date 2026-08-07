//! The vertical slice, exercised end to end: seal → audited registration →
//! ongoing proof windows → lapse → suspension, with capacity that follows
//! proof rather than assertion.
//!
//! Every possession proof here goes through the real primitives —
//! `mini_porep::respond` produces the response and
//! `mini_spacetime::verify_storage_challenge` checks it against the replica
//! root the audited claim carries. Nothing is simulated.

mod support;

use mini_spacetime::verify_storage_challenge;
use mini_storage_fraud::{
    capacity_units_of, ProviderStanding, ReplicaLifecycle, ReplicaState, StorageUnitPolicy,
    WindowPolicy,
};
use support::{context, data, directory_of, policy, registered_claim, Party};

const GENESIS: u64 = 1_700_000_000_000;

/// One unit per 128 bytes, so `support`'s 8-node (256-byte) replicas are worth
/// exactly 2 units and the arithmetic is checkable by hand.
fn units() -> StorageUnitPolicy {
    StorageUnitPolicy::new(128).unwrap()
}

fn windows() -> WindowPolicy {
    WindowPolicy::new(1_000, 4, 2).unwrap()
}

/// Answer every challenge for a window against the real sealed replica, and
/// verify each response the way a verifier would.
fn prove_window(
    lifecycle: &mut ReplicaLifecycle,
    replica: &mini_porep::SealedReplica,
    window: u64,
    beacon: &[u8],
    policy: &WindowPolicy,
) -> bool {
    let commitment = mini_porep::replica_commitment(replica);
    for challenge in lifecycle.challenges_for(window, beacon, policy) {
        let Some(response) = mini_porep::respond(replica, &challenge) else {
            return false;
        };
        if response.leaf_index != challenge.leaf_index {
            return false;
        }
        if !verify_storage_challenge(&commitment, &challenge, &response) {
            return false;
        }
    }
    lifecycle.record_proven_window(window, policy).is_ok()
}

fn tracked() -> (ReplicaLifecycle, mini_porep::SealedReplica) {
    let provider = Party::provider(140);
    let (first, second) = (Party::auditor(141), Party::auditor(142));
    let directory = directory_of(&[&provider, &first, &second]);
    let (claim, replica) = registered_claim(&provider, &[&first, &second], &context(40), &data(40));
    let verified = claim.verify(&directory, &policy()).unwrap();
    let lifecycle = ReplicaLifecycle::begin(verified, GENESIS, GENESIS, &windows());
    (lifecycle, replica)
}

// ---------------------------------------------------------------------------
// Capacity follows what was audited, never what was asserted
// ---------------------------------------------------------------------------

#[test]
fn capacity_is_derived_from_the_audited_seal_not_supplied() {
    // The gap this slice closes: mini_spacetime::MerkleStorageProof takes
    // capacity_units from its caller, so a provider could seal one node and
    // declare a million units into a weighting layer that "trusts its input
    // completely". There is no constructor here that accepts a number.
    let (lifecycle, replica) = tracked();
    let capacity = capacity_units_of(lifecycle.claim(), &units());

    let sealed_bytes = replica.node_count() as u64 * mini_porep::NODE_SIZE as u64;
    assert_eq!(capacity.sealed_bytes(), sealed_bytes);
    assert_eq!(capacity.units(), sealed_bytes / 128);
    assert_eq!(capacity.units(), 2);
}

#[test]
fn a_replica_smaller_than_one_unit_counts_as_zero_not_one() {
    // Truncating division on purpose: rounding up would mint capacity nobody
    // sealed, which is the same failure in miniature.
    let (lifecycle, _) = tracked();
    let coarse = StorageUnitPolicy::new(1024 * 1024).unwrap();
    assert_eq!(capacity_units_of(lifecycle.claim(), &coarse).units(), 0);
}

#[test]
fn a_zero_byte_unit_policy_is_refused() {
    assert!(StorageUnitPolicy::new(0).is_err());
}

// ---------------------------------------------------------------------------
// Proof over time
// ---------------------------------------------------------------------------

#[test]
fn registration_alone_does_not_count_as_capacity() {
    // Registration proves the replica was sealed once. Whether it is still
    // held is a different question, and until it is answered the provider
    // contributes nothing.
    let (lifecycle, _) = tracked();
    assert_eq!(
        lifecycle.state(),
        ReplicaState::Degraded { missed_windows: 0 }
    );
    assert_eq!(lifecycle.proven_capacity(&units()).units(), 0);
}

#[test]
fn answering_a_window_activates_and_counts_capacity() {
    let (mut lifecycle, replica) = tracked();
    assert!(prove_window(
        &mut lifecycle,
        &replica,
        1,
        b"beacon-1",
        &windows()
    ));
    assert_eq!(lifecycle.state(), ReplicaState::Active);
    assert_eq!(lifecycle.proven_capacity(&units()).units(), 2);
    assert_eq!(lifecycle.last_proven_window(), Some(1));
}

#[test]
fn continuous_proving_stays_active_across_many_windows() {
    let (mut lifecycle, replica) = tracked();
    for window in 1..=10 {
        let beacon = format!("beacon-{window}");
        assert!(
            prove_window(
                &mut lifecycle,
                &replica,
                window,
                beacon.as_bytes(),
                &windows()
            ),
            "window {window} failed to prove"
        );
        assert_eq!(lifecycle.state(), ReplicaState::Active);
    }
    assert_eq!(lifecycle.proven_capacity(&units()).units(), 2);
}

#[test]
fn a_missed_window_degrades_and_stops_counting_but_recovers() {
    // Lapse is reversible on purpose: a missed window and an unreachable peer
    // are indistinguishable from here, so the response is to stop counting
    // capacity, not to punish.
    let (mut lifecycle, replica) = tracked();
    assert!(prove_window(&mut lifecycle, &replica, 1, b"b1", &windows()));

    lifecycle.advance_to(3, &windows()); // window 2 missed
    assert_eq!(
        lifecycle.state(),
        ReplicaState::Degraded { missed_windows: 1 }
    );
    assert_eq!(lifecycle.proven_capacity(&units()).units(), 0);

    assert!(prove_window(&mut lifecycle, &replica, 3, b"b3", &windows()));
    assert_eq!(lifecycle.state(), ReplicaState::Active);
    assert_eq!(lifecycle.proven_capacity(&units()).units(), 2);
}

#[test]
fn missing_beyond_grace_suspends_and_does_not_self_recover() {
    let (mut lifecycle, replica) = tracked();
    assert!(prove_window(&mut lifecycle, &replica, 1, b"b1", &windows()));

    // grace_windows is 2; windows 2, 3 and 4 all missed.
    lifecycle.advance_to(5, &windows());
    assert_eq!(lifecycle.state(), ReplicaState::Suspended);
    assert_eq!(lifecycle.proven_capacity(&units()).units(), 0);

    // A late proof does not resurrect it — a replica nobody has seen that long
    // is not distinguishable from one that is gone.
    assert!(!prove_window(
        &mut lifecycle,
        &replica,
        5,
        b"b5",
        &windows()
    ));
    assert_eq!(lifecycle.state(), ReplicaState::Suspended);
}

#[test]
fn a_window_cannot_be_credited_twice() {
    // Replaying a credited window must not extend a streak or reverse a lapse.
    let (mut lifecycle, replica) = tracked();
    assert!(prove_window(&mut lifecycle, &replica, 2, b"b2", &windows()));
    assert!(!prove_window(
        &mut lifecycle,
        &replica,
        2,
        b"b2",
        &windows()
    ));
    assert!(!prove_window(
        &mut lifecycle,
        &replica,
        1,
        b"b1",
        &windows()
    ));
    assert_eq!(lifecycle.last_proven_window(), Some(2));
}

#[test]
fn advancing_repeatedly_inside_one_window_accrues_no_phantom_misses() {
    let (mut lifecycle, replica) = tracked();
    assert!(prove_window(&mut lifecycle, &replica, 1, b"b1", &windows()));
    for _ in 0..5 {
        lifecycle.advance_to(1, &windows());
    }
    assert_eq!(lifecycle.state(), ReplicaState::Active);
}

#[test]
fn retiring_is_voluntary_and_terminal() {
    let (mut lifecycle, replica) = tracked();
    assert!(prove_window(&mut lifecycle, &replica, 1, b"b1", &windows()));
    lifecycle.retire();
    assert_eq!(lifecycle.state(), ReplicaState::Retired);
    assert_eq!(lifecycle.proven_capacity(&units()).units(), 0);
    assert!(!prove_window(
        &mut lifecycle,
        &replica,
        2,
        b"b2",
        &windows()
    ));
}

// ---------------------------------------------------------------------------
// Challenge derivation the provider cannot steer
// ---------------------------------------------------------------------------

#[test]
fn challenges_depend_on_the_verifiers_beacon_and_the_window() {
    // A provider that could predict its challenges could keep only the nodes
    // it will be asked for. The derivation takes the seal digest, the window,
    // and a beacon the verifier supplies; the provider contributes nothing.
    let (lifecycle, _) = tracked();
    let policy = WindowPolicy::new(1_000, 16, 2).unwrap();

    let a = lifecycle.challenges_for(1, b"beacon-a", &policy);
    let b = lifecycle.challenges_for(1, b"beacon-b", &policy);
    let later = lifecycle.challenges_for(2, b"beacon-a", &policy);

    assert_eq!(a.len(), 16);
    assert_ne!(a, b, "a different beacon must draw different challenges");
    assert_ne!(
        a, later,
        "a different window must draw different challenges"
    );
    // Deterministic for a fixed (window, beacon), so two verifiers agree.
    assert_eq!(a, lifecycle.challenges_for(1, b"beacon-a", &policy));
}

#[test]
fn every_drawn_challenge_is_in_range_and_answerable() {
    let (lifecycle, replica) = tracked();
    let policy = WindowPolicy::new(1_000, 64, 2).unwrap();
    for challenge in lifecycle.challenges_for(7, b"beacon", &policy) {
        assert!(challenge.leaf_index < replica.node_count());
        assert!(mini_porep::respond(&replica, &challenge).is_some());
    }
}

#[test]
fn a_response_from_a_different_replica_does_not_verify() {
    // The possession check is real: answering with someone else's replica
    // fails against this claim's committed root.
    let (lifecycle, replica) = tracked();
    let other = Party::provider(150);
    let (_, other_replica) = registered_claim(
        &other,
        &[&Party::auditor(151), &Party::auditor(152)],
        &context(41),
        &data(41),
    );

    let commitment = mini_porep::replica_commitment(&replica);
    let challenge = lifecycle.challenges_for(1, b"beacon", &windows())[0];
    let foreign = mini_porep::respond(&other_replica, &challenge).unwrap();
    assert!(!verify_storage_challenge(&commitment, &challenge, &foreign));
}

#[test]
fn an_unusable_window_policy_is_refused() {
    assert!(WindowPolicy::new(0, 4, 2).is_err());
    assert!(WindowPolicy::new(1_000, 0, 2).is_err());
    assert!(
        WindowPolicy::new(1_000, mini_storage_fraud::MAX_CHALLENGES_PER_WINDOW + 1, 2).is_err()
    );
}

// ---------------------------------------------------------------------------
// A provider's total standing
// ---------------------------------------------------------------------------

#[test]
fn provider_capacity_is_the_sum_of_actively_proving_replicas() {
    let provider = Party::provider(160);
    let (first, second) = (Party::auditor(161), Party::auditor(162));
    let directory = directory_of(&[&provider, &first, &second]);

    let mut standing = ProviderStanding::new();
    let mut replicas = Vec::new();
    for ordinal in 0..3u8 {
        let mut ctx = context(50);
        ctx.replica_ordinal = ordinal as u32;
        let (claim, replica) = registered_claim(&provider, &[&first, &second], &ctx, &data(50));
        let verified = claim.verify(&directory, &policy()).unwrap();
        let root = verified.replica_root();
        standing.track(ReplicaLifecycle::begin(
            verified,
            GENESIS,
            GENESIS,
            &windows(),
        ));
        replicas.push((root, replica));
    }
    assert_eq!(standing.len(), 3);
    // Nothing proven yet.
    assert_eq!(standing.proven_capacity(&units()).units(), 0);

    // Prove two of the three.
    for (root, replica) in replicas.iter().take(2) {
        let lifecycle = standing.get_mut(root).unwrap();
        assert!(prove_window(lifecycle, replica, 1, b"beacon", &windows()));
    }
    assert_eq!(standing.proven_capacity(&units()).units(), 4);

    // The two provers keep up with each window as it opens; the third answers
    // nothing and lapses past grace. Grace is per replica, not per provider: a
    // provider in good standing on two replicas gets no cover for a third.
    for window in 2..=5u64 {
        standing.advance_to(window, &windows());
        for (root, replica) in replicas.iter().take(2) {
            let lifecycle = standing.get_mut(root).unwrap();
            let beacon = format!("beacon-{window}");
            assert!(prove_window(
                lifecycle,
                replica,
                window,
                beacon.as_bytes(),
                &windows()
            ));
        }
    }
    assert_eq!(
        standing.get(&replicas[2].0).unwrap().state(),
        ReplicaState::Suspended
    );
    assert_eq!(standing.proven_capacity(&units()).units(), 4);
}

#[test]
fn three_ordinals_under_one_provider_are_three_distinct_replicas() {
    // The replica ordinal exists so a provider keeping several independent
    // copies seals several times rather than counting one copy repeatedly.
    let provider = Party::provider(170);
    let (first, second) = (Party::auditor(171), Party::auditor(172));
    let directory = directory_of(&[&provider, &first, &second]);

    let mut roots = Vec::new();
    for ordinal in 0..3u32 {
        let mut ctx = context(60);
        ctx.replica_ordinal = ordinal;
        let (claim, _) = registered_claim(&provider, &[&first, &second], &ctx, &data(60));
        roots.push(claim.verify(&directory, &policy()).unwrap().replica_root());
    }
    roots.sort();
    roots.dedup();
    assert_eq!(roots.len(), 3, "each ordinal must seal to its own replica");
}
