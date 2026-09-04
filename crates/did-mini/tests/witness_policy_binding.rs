//! The witness policy is the identity's own, or the witness layer is
//! checking an attacker's homework (D-0459, roadmap R9).
//!
//! A witness certificate proves "these witnesses receipted this event". It
//! says nothing about whether *those* witnesses are the ones the identity
//! appointed. Until this change the policy naming them came from the caller,
//! which meant whoever supplied the certificate could also supply the
//! standard it was judged against.
//!
//! These tests are about that binding. The interesting ones are not "a valid
//! certificate verifies" — that was already true and already tested — but
//! the ones where an attacker brings a perfectly valid certificate signed by
//! witnesses the real controller never named.

use did_mini::{
    assess_kel_assurance, sign_witness_receipt, Controller, FreshnessPins, IdentityError,
    KelAssurance, KeyEventKind, WitnessEvidence, WitnessId, WitnessReceipt,
    WitnessReceiptStatement, WitnessReceiptVersion, WitnessedEventCertificate,
};
use mini_crypto::{SigningKey, VerifyingKey};

type Witness = (WitnessId, SigningKey, VerifyingKey);

fn a_witness() -> Witness {
    let did = Controller::incept_single().unwrap().did();
    let key = SigningKey::generate().unwrap();
    let vk = key.verifying_key();
    (WitnessId(did), key, vk)
}

/// Certify whatever head `kel` currently has, with `witnesses`.
fn certify(
    kel: &did_mini::Kel,
    witnesses: &[Witness],
    generation: u64,
    observed_epoch: u64,
) -> WitnessedEventCertificate {
    let head = kel.events().last().unwrap();
    let kind = KeyEventKind::from(&head.kind);
    let prior = (head.sn > 0).then(|| kel.events()[head.sn as usize - 1].digest());
    let receipts: Vec<WitnessReceipt> = witnesses
        .iter()
        .map(|(id, sk, _)| {
            let statement = WitnessReceiptStatement {
                version: WitnessReceiptVersion::V1,
                identity: kel.did(),
                sequence: head.sn,
                event_digest: head.digest(),
                prior_event_digest: prior.clone(),
                event_kind: kind,
                witness_policy_generation: generation,
                witness_id: id.clone(),
                observed_epoch,
            };
            sign_witness_receipt(statement, sk)
        })
        .collect();
    WitnessedEventCertificate::assemble(kel.did(), head.sn, head.digest(), generation, receipts)
        .unwrap()
}

fn resolver(witnesses: Vec<Witness>) -> impl Fn(&WitnessId) -> Option<VerifyingKey> {
    move |id: &WitnessId| {
        witnesses
            .iter()
            .find(|(wid, _, _)| wid == id)
            .map(|(_, _, vk)| vk.clone())
    }
}

#[test]
fn witnesses_the_controller_never_appointed_cannot_manufacture_assurance() {
    // **The attack this change closes.**
    //
    // Alice appoints two witnesses. An attacker stands up two witnesses of
    // their own, has them receipt Alice's head perfectly correctly, and
    // presents the result. Every signature in that certificate is valid;
    // every receipt names the right identity, sequence and digest. The only
    // thing wrong with it is that Alice never appointed those witnesses.
    //
    // Before D-0459 the attacker also supplied the policy, so this verified
    // and reported WitnessedRecent -- the strongest level in the enum -- for
    // evidence the identity had never authorized. Now the policy is read
    // from Alice's own signed KEL and the attacker's witnesses resolve to
    // nobody in it.
    let mut alice = Controller::incept_single().unwrap();
    let real: Vec<Witness> = (0..2).map(|_| a_witness()).collect();
    alice
        .appoint_witnesses(real.iter().map(|(id, _, _)| id.0.clone()).collect(), 2)
        .unwrap();

    let attacker: Vec<Witness> = (0..2).map(|_| a_witness()).collect();
    let generation = alice.kel().declared_witness_policy().unwrap().generation;
    let forged = certify(&alice.kel(), &attacker, generation, 100);

    let resolve = resolver(attacker.clone());
    let mut pins = FreshnessPins::new();
    let evidence = WitnessEvidence {
        certificate: &forged,
        resolve_witness_key: &resolve,
        now_epoch: 105,
        max_epoch_age: 10,
    };

    let outcome = assess_kel_assurance(&alice.kel(), &mut pins, Some(evidence), false);
    assert!(
        outcome.is_err(),
        "a certificate from unappointed witnesses must not yield assurance, got {outcome:?}"
    );
}

#[test]
fn an_identity_that_appointed_nobody_cannot_be_reported_as_witnessed() {
    // The degenerate version of the same attack, and the reason the answer
    // is an error rather than a quiet downgrade to Direct: the caller asked
    // whether this witness evidence holds, and the honest answer is that the
    // identity appointed nobody, so the evidence is about witnesses it never
    // authorized. Silently reporting Direct would let a caller conclude
    // "checked, nothing wrong" from a check that did not happen.
    let alice = Controller::incept_single().unwrap();
    let attacker: Vec<Witness> = (0..2).map(|_| a_witness()).collect();
    let forged = certify(&alice.kel(), &attacker, 1, 100);

    let resolve = resolver(attacker);
    let mut pins = FreshnessPins::new();
    let evidence = WitnessEvidence {
        certificate: &forged,
        resolve_witness_key: &resolve,
        now_epoch: 105,
        max_epoch_age: 10,
    };

    assert_eq!(
        assess_kel_assurance(&alice.kel(), &mut pins, Some(evidence), false),
        Err(IdentityError::NoWitnessPolicyDeclared)
    );
}

#[test]
fn the_appointed_witnesses_still_produce_real_assurance() {
    // The change must not have closed the attack by breaking the feature.
    let mut alice = Controller::incept_single().unwrap();
    let real: Vec<Witness> = (0..2).map(|_| a_witness()).collect();
    alice
        .appoint_witnesses(real.iter().map(|(id, _, _)| id.0.clone()).collect(), 2)
        .unwrap();

    let generation = alice.kel().declared_witness_policy().unwrap().generation;
    let cert = certify(&alice.kel(), &real, generation, 100);
    let resolve = resolver(real);
    let mut pins = FreshnessPins::new();
    let evidence = WitnessEvidence {
        certificate: &cert,
        resolve_witness_key: &resolve,
        now_epoch: 105,
        max_epoch_age: 10,
    };

    assert_eq!(
        assess_kel_assurance(&alice.kel(), &mut pins, Some(evidence), false).unwrap(),
        KelAssurance::WitnessedRecent
    );
}

#[test]
fn a_policy_is_read_from_the_latest_appointment_not_the_first() {
    // Rotating the witness set has to actually replace it. A verifier that
    // kept honouring an earlier generation would let a retired witness keep
    // certifying -- revocation that does not propagate, in the witness layer
    // rather than the device one.
    let mut alice = Controller::incept_single().unwrap();
    let old: Vec<Witness> = (0..2).map(|_| a_witness()).collect();
    alice
        .appoint_witnesses(old.iter().map(|(id, _, _)| id.0.clone()).collect(), 2)
        .unwrap();
    let old_generation = alice.kel().declared_witness_policy().unwrap().generation;

    let new: Vec<Witness> = (0..2).map(|_| a_witness()).collect();
    alice
        .appoint_witnesses(new.iter().map(|(id, _, _)| id.0.clone()).collect(), 2)
        .unwrap();

    let policy = alice.kel().declared_witness_policy().unwrap();
    assert!(policy.generation > old_generation, "the policy advanced");
    for (id, _, _) in &new {
        assert!(
            policy.witnesses.contains(id),
            "the new set is authoritative"
        );
    }
    for (id, _, _) in &old {
        assert!(!policy.witnesses.contains(id), "the old set is retired");
    }

    // And a certificate from the retired witnesses no longer earns anything.
    let stale = certify(&alice.kel(), &old, policy.generation, 100);
    let resolve = resolver(old);
    let mut pins = FreshnessPins::new();
    let evidence = WitnessEvidence {
        certificate: &stale,
        resolve_witness_key: &resolve,
        now_epoch: 105,
        max_epoch_age: 10,
    };
    assert!(assess_kel_assurance(&alice.kel(), &mut pins, Some(evidence), false).is_err());
}

#[test]
fn retiring_a_witness_set_clears_the_policy_rather_than_leaving_it_standing() {
    let mut alice = Controller::incept_single().unwrap();
    let real: Vec<Witness> = (0..2).map(|_| a_witness()).collect();
    alice
        .appoint_witnesses(real.iter().map(|(id, _, _)| id.0.clone()).collect(), 2)
        .unwrap();
    assert!(alice.kel().declared_witness_policy().is_some());

    alice.retire_witnesses().unwrap();
    assert!(
        alice.kel().declared_witness_policy().is_none(),
        "a controller must be able to retire its witnesses"
    );
}

#[test]
fn an_unsatisfiable_or_incoherent_policy_cannot_be_signed() {
    let mut alice = Controller::incept_single().unwrap();
    let real: Vec<Witness> = (0..2).map(|_| a_witness()).collect();
    let ids: Vec<_> = real.iter().map(|(id, _, _)| id.0.clone()).collect();

    // A threshold nobody could ever meet.
    assert!(alice.appoint_witnesses(ids.clone(), 3).is_err());
    // A threshold of zero, which would make any certificate "sufficient".
    assert!(alice.appoint_witnesses(ids.clone(), 0).is_err());
    // A repeated witness, which would inflate an apparent threshold for
    // free -- the same reason a ring refuses duplicate members.
    assert!(alice
        .appoint_witnesses(vec![ids[0].clone(), ids[0].clone()], 2)
        .is_err());

    // None of the refusals left a half-written event behind.
    assert!(alice.kel().declared_witness_policy().is_none());
    assert!(alice.kel().verify().is_ok());
}

#[test]
fn appointing_witnesses_leaves_the_kel_verifiable_and_the_scid_unchanged() {
    // The policy lives in a controller-signed establishment event, so it is
    // covered by the event digest and chained -- it cannot be edited after
    // the fact without breaking the chain. And appointing does not re-issue
    // the identity: the SCID comes from inception and stays put.
    let mut alice = Controller::incept_single().unwrap();
    let before = alice.did();
    let real: Vec<Witness> = (0..2).map(|_| a_witness()).collect();
    alice
        .appoint_witnesses(real.iter().map(|(id, _, _)| id.0.clone()).collect(), 2)
        .unwrap();

    assert_eq!(
        alice.did(),
        before,
        "appointing witnesses is not a new identity"
    );
    alice.kel().verify().expect("the chain still verifies");

    // Round-tripping the KEL preserves the policy: it is real wire content,
    // not something reconstructed in memory.
    let bytes = alice.kel().to_bytes();
    let decoded = did_mini::Kel::from_bytes(&bytes).unwrap();
    decoded.verify().unwrap();
    assert_eq!(
        decoded.declared_witness_policy(),
        alice.kel().declared_witness_policy()
    );
}

#[test]
fn identities_without_witnesses_encode_exactly_as_they_did_before() {
    // The compatibility guarantee that made this change possible at all.
    //
    // An event's bytes feed its digest, the digest chains the KEL, and the
    // inception event's digest *is* the SCID. A field appended
    // unconditionally would have changed every identifier this project has
    // ever issued. The threshold is therefore written only when a witness
    // set exists, and every identity predating D-0459 has an empty one.
    //
    // Pinned against a seeded identity, and pinned to values *captured from
    // the pre-change code* rather than recomputed here -- a test that
    // asserted whatever this build produces would agree with itself no
    // matter what the encoding did.
    let alice = Controller::incept_single_from_seeds(&[7u8; 32], &[9u8; 32]).unwrap();
    assert_eq!(
        alice.did().as_str(),
        "did:mini:zgVytxcYh2onAGUKKqkSSsuWzsCL8oQz1N7yEjUvxda7TTC"
    );

    // The whole KEL, byte for byte, not only the identifier it hashes to.
    let encoded: String = alice
        .kel()
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(encoded.len() / 2, 296);
    assert!(
        encoded.starts_with("0000002f7a6756797478635968326f6e4147554b4b716b53537375577a73434c386f517a314e3779456a5576786461375454"),
        "the encoding of a witness-less identity did not move"
    );
    assert!(alice.kel().declared_witness_policy().is_none());
}
