//! The vertical, end to end: **publish → view → pay → reconcile**, with a
//! real post in a real store, a real stealth output, a real ring signature,
//! a real Bulletproof, and the real M1/M2/M3 state machine.
//!
//! This is the artifact that proves the layers compose. It lives in a test
//! rather than in the crate because `mini-private-payment` deliberately does
//! not depend on the social layer — a payment layer that knew what a post
//! was would be a payment layer that could be made to treat some posts
//! differently. The dependency exists here, in `dev-dependencies`, where it
//! demonstrates the composition without making it permanent.
//!
//! What each step must preserve is asserted, not narrated:
//!
//! | step | property |
//! |---|---|
//! | publish | a post needs `POST`, never `VOTE` (V3) |
//! | view | the store learns no viewer identity (PR2) |
//! | pay | the network learns no payer, payee, amount, or post |
//! | reconcile | only canonical inclusion is final (M2), conflicts never merge (M1/M3) |

mod support;

use did_mini::{BaseDeviceRole, Capabilities, Controller, Did};
use mini_private_payment::{
    build, reconcile, verify, InMemoryPrivateLedger, KeyImageSet, PaymentPurpose, PaymentRequest,
    SpendOutcome, MIN_RING_SIZE,
};
use mini_settlement::{SettlementState, WalletLabel};
use mini_social::publish_post;
use mini_store::{CacheTier, MemoryBackend, Store, ViewConditions};
use mini_value::StealthKeypair;
use support::{one_time_key, ring_containing, NETWORK};

/// A human root plus one delegated device.
fn human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

/// A human whose device holds only the secondary capability set — no
/// `VOTE`, no `MANAGE_DEVICES`.
fn human_without_vote(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::secondary())
        .unwrap();
    (root, device)
}

fn viewing_conditions() -> ViewConditions {
    ViewConditions {
        battery_percent: 100,
        on_battery: false,
        minute_of_day: 12 * 60,
        metered_connection: false,
        storage_budget_remaining: true,
    }
}

/// A reader's spendable one-time output plus a ring to hide it in.
fn reader_funds() -> (Vec<Vec<u8>>, usize, [u8; 32]) {
    let (public, secret) = one_time_key();
    let (ring, index) = ring_containing(&public, MIN_RING_SIZE);
    (ring, index, secret)
}

#[test]
fn a_reader_privately_pays_a_creator_for_a_specific_post() {
    // ---- 1. A creator publishes -------------------------------------
    let (creator_root, creator_device) = human_without_vote(60);
    let mut store = Store::new(MemoryBackend::new());
    let post = publish_post(
        &mut store,
        &creator_root.did(),
        &creator_device,
        "the thing worth paying for",
        1_000,
        1,
    )
    .unwrap();
    // V3: publishing needed only POST. A creator who can be paid has not
    // thereby acquired any governance capability.
    assert!(!Capabilities::secondary().contains(Capabilities::VOTE));

    // The creator publishes stealth keys alongside their identity. These
    // are public and stable -- and that is fine, because no payment ever
    // names them.
    let creator_wallet = StealthKeypair::generate().unwrap();

    // ---- 2. A reader views it ---------------------------------------
    let tier = store
        .note_view(
            post.id(),
            &BaseDeviceRole::always_on_default(),
            viewing_conditions(),
        )
        .unwrap();
    assert_eq!(tier, CacheTier::SeedCache);
    // PR2: note_view takes no viewer identity at all. There is no argument
    // to pass one through, which is the point -- and the payment below must
    // not reintroduce what this refuses to record.

    // ---- 3. The reader pays, privately ------------------------------
    let (ring, secret_index, secret_key) = reader_funds();
    let (claim, shared) = build(&PaymentRequest {
        network_id: NETWORK,
        recipient_spend_public: creator_wallet.spend_public_bytes().to_vec(),
        recipient_view_public: creator_wallet.view_public_bytes().to_vec(),
        amount_micro: 25_000,
        // The post id travels sealed. Putting it in a cleartext field would
        // publish exactly the engagement graph note_view refuses to record.
        purpose: PaymentPurpose::new(post.id().as_str().as_bytes().to_vec()),
        valid_until_ms: 100_000,
        last_known_chain: b"height:7".to_vec(),
        ring,
        secret_index,
        secret_key: secret_key.to_vec(),
        blinding: mini_crypto::random_32().unwrap(),
    })
    .unwrap();

    let verified = verify(&claim, &NETWORK).unwrap();

    // What an observer sees on the wire: not the amount, not the post, not
    // the creator's address, not the reader.
    let wire = claim.encode();
    assert!(!wire
        .windows(8)
        .any(|w| w == 25_000u64.to_be_bytes() || w == 25_000u64.to_le_bytes()));
    let post_id_bytes = post.id().as_str().as_bytes();
    assert!(!wire
        .windows(post_id_bytes.len())
        .any(|w| w == post_id_bytes));
    let creator_address = creator_wallet.spend_public_bytes();
    assert!(!wire.windows(32).any(|w| w == creator_address));

    // ---- 4. The creator finds it, with the view key alone ------------
    let found = mini_private_payment::scan_one(
        &creator_wallet.view_secret_bytes(),
        &creator_wallet.spend_public_bytes(),
        &verified,
    )
    .unwrap()
    .expect("the creator recognizes their own payment");
    assert_eq!(
        found.purpose,
        PaymentPurpose::new(post.id().as_str().as_bytes().to_vec()),
        "the creator learns which post was paid for"
    );
    // The shared secret both sides derived is the same one.
    assert_eq!(verified.open_memo(&shared).unwrap(), found.purpose);

    // ---- 5. Settlement discipline is unchanged ----------------------
    let mut ledger = InMemoryPrivateLedger::new();
    let pending = reconcile(&verified, &ledger, 0).unwrap();
    assert_eq!(pending, SettlementState::PendingCanonical);
    assert!(!pending.is_final(), "M2: a signed promise is not ownership");
    assert_eq!(pending.wallet_label(), WalletLabel::Pending);

    ledger.finalize(&verified);
    let settled = reconcile(&verified, &ledger, 0).unwrap();
    assert!(settled.is_final());
    assert_eq!(settled.wallet_label(), WalletLabel::Finalized);
}

#[test]
fn nobody_else_learns_which_post_was_paid_for() {
    // The engagement-graph leak, tested from the observer's side: another
    // participant holding a full copy of the claim learns nothing.
    let (creator_root, creator_device) = human(70);
    let mut store = Store::new(MemoryBackend::new());
    let post = publish_post(
        &mut store,
        &creator_root.did(),
        &creator_device,
        "sensitive",
        1,
        1,
    )
    .unwrap();

    let creator_wallet = StealthKeypair::generate().unwrap();
    let nosy = StealthKeypair::generate().unwrap();
    let (ring, secret_index, secret_key) = reader_funds();

    let (claim, _) = build(&PaymentRequest {
        network_id: NETWORK,
        recipient_spend_public: creator_wallet.spend_public_bytes().to_vec(),
        recipient_view_public: creator_wallet.view_public_bytes().to_vec(),
        amount_micro: 1,
        purpose: PaymentPurpose::new(post.id().as_str().as_bytes().to_vec()),
        valid_until_ms: 10,
        last_known_chain: Vec::new(),
        ring,
        secret_index,
        secret_key: secret_key.to_vec(),
        blinding: mini_crypto::random_32().unwrap(),
    })
    .unwrap();
    let verified = verify(&claim, &NETWORK).unwrap();

    // An observer with the complete claim, running a full scan, gets nothing.
    assert!(mini_private_payment::scan_one(
        &nosy.view_secret_bytes(),
        &nosy.spend_public_bytes(),
        &verified
    )
    .unwrap()
    .is_none());
}

#[test]
fn one_creator_paid_by_many_readers_has_no_public_income_ledger() {
    // The property that decides whether this is worth anything: a creator's
    // total income, and the number of people who paid, must not be readable
    // off the chain. Every payment goes to a different one-time address.
    let creator = StealthKeypair::generate().unwrap();
    let mut claims = Vec::new();
    for reader in 0..5u64 {
        let (ring, secret_index, secret_key) = reader_funds();
        let (claim, _) = build(&PaymentRequest {
            network_id: NETWORK,
            recipient_spend_public: creator.spend_public_bytes().to_vec(),
            recipient_view_public: creator.view_public_bytes().to_vec(),
            amount_micro: 1_000 * (reader + 1),
            purpose: PaymentPurpose::new(format!("post:{reader}").into_bytes()),
            valid_until_ms: 10_000,
            last_known_chain: Vec::new(),
            ring,
            secret_index,
            secret_key: secret_key.to_vec(),
            blinding: mini_crypto::random_32().unwrap(),
        })
        .unwrap();
        claims.push(verify(&claim, &NETWORK).unwrap());
    }

    // No two payments share an address, so nothing groups them by creator.
    let mut addresses: Vec<_> = claims
        .iter()
        .map(|c| c.claim().output.one_time_address.clone())
        .collect();
    addresses.sort();
    addresses.dedup();
    assert_eq!(addresses.len(), 5, "every payment has its own address");

    // ...and no two share a commitment, so equal amounts would not group
    // them either.
    let mut commitments: Vec<_> = claims
        .iter()
        .map(|c| c.claim().amount_commitment.clone())
        .collect();
    commitments.sort();
    commitments.dedup();
    assert_eq!(commitments.len(), 5);

    // The creator, and only the creator, can enumerate their own income.
    let mine = mini_private_payment::scan(
        &creator.view_secret_bytes(),
        &creator.spend_public_bytes(),
        claims.iter(),
    )
    .unwrap();
    assert_eq!(mine.len(), 5);
}

#[test]
fn paying_for_a_post_grants_the_payer_no_capability_over_it() {
    // Directive 16, in the concrete case that matters most for a social
    // network: money buys attention, storage, bandwidth -- never voice.
    // A paid-for post is byte-identical to an unpaid one, and the payer
    // gains nothing they can exercise.
    let (creator_root, creator_device) = human(80);
    let mut store = Store::new(MemoryBackend::new());
    let post = publish_post(
        &mut store,
        &creator_root.did(),
        &creator_device,
        "unchanged by payment",
        5,
        1,
    )
    .unwrap();
    let before = store.get(post.id()).unwrap();

    let creator_wallet = StealthKeypair::generate().unwrap();
    let (ring, secret_index, secret_key) = reader_funds();
    let (claim, _) = build(&PaymentRequest {
        network_id: NETWORK,
        recipient_spend_public: creator_wallet.spend_public_bytes().to_vec(),
        recipient_view_public: creator_wallet.view_public_bytes().to_vec(),
        amount_micro: 1_000_000_000,
        purpose: PaymentPurpose::new(post.id().as_str().as_bytes().to_vec()),
        valid_until_ms: 10_000,
        last_known_chain: Vec::new(),
        ring,
        secret_index,
        secret_key: secret_key.to_vec(),
        blinding: mini_crypto::random_32().unwrap(),
    })
    .unwrap();
    let verified = verify(&claim, &NETWORK).unwrap();
    let mut ledger = InMemoryPrivateLedger::new();
    ledger.finalize(&verified);
    assert!(reconcile(&verified, &ledger, 0).unwrap().is_final());

    // An enormous, finalized payment changed nothing about the post.
    let after = store.get(post.id()).unwrap();
    assert_eq!(before.id(), after.id());
    assert_eq!(before.payload, after.payload);
    // And the verified payment exposes nothing capability-shaped: its
    // entire public surface is a key image, two digests, and the claim.
    let rendered = format!("{verified:?}");
    for forbidden in ["Capabilit", "VOTE", "weight", "quorum"] {
        assert!(
            !rendered.contains(forbidden),
            "a verified payment exposed something capability-shaped: {forbidden}"
        );
    }
}

#[test]
fn a_reader_cannot_pay_the_same_funds_to_two_creators() {
    // The double-spend, in the social case: pay creator A, then try to pay
    // creator B with the same output. M1 -- the second is refused, never
    // merged, netted, or split between them.
    let first_creator = StealthKeypair::generate().unwrap();
    let second_creator = StealthKeypair::generate().unwrap();
    let (ring, secret_index, secret_key) = reader_funds();

    let pay_to = |to: &StealthKeypair| {
        let (claim, _) = build(&PaymentRequest {
            network_id: NETWORK,
            recipient_spend_public: to.spend_public_bytes().to_vec(),
            recipient_view_public: to.view_public_bytes().to_vec(),
            amount_micro: 500,
            purpose: PaymentPurpose::none(),
            valid_until_ms: 10_000,
            last_known_chain: Vec::new(),
            ring: ring.clone(),
            secret_index,
            secret_key: secret_key.to_vec(),
            blinding: mini_crypto::random_32().unwrap(),
        })
        .unwrap();
        verify(&claim, &NETWORK).unwrap()
    };

    let to_first = pay_to(&first_creator);
    let to_second = pay_to(&second_creator);

    let mut spent = KeyImageSet::new();
    assert_eq!(spent.observe(&to_first), SpendOutcome::Accepted);
    assert!(matches!(
        spent.observe(&to_second),
        SpendOutcome::Conflict { .. }
    ));

    // Canonical ordering, not arrival order or generosity, decides.
    let mut ledger = InMemoryPrivateLedger::new();
    ledger.finalize(&to_second);
    assert_eq!(
        reconcile(&to_first, &ledger, 0).unwrap(),
        SettlementState::RejectedConflict
    );
    assert_eq!(
        reconcile(&to_second, &ledger, 0).unwrap(),
        SettlementState::Finalized
    );
    // Exactly one finalized. Nothing was split between the two creators.
    assert_eq!(
        [&to_first, &to_second]
            .iter()
            .filter(|c| reconcile(c, &ledger, 0).unwrap().is_final())
            .count(),
        1
    );
}

#[test]
fn the_post_and_the_payment_share_no_identifier_on_the_wire() {
    // Belt and braces on the central privacy claim: take a post id, build a
    // payment for it, and assert no substring of the id of any meaningful
    // length appears in the claim's bytes.
    let (root, device) = human(90);
    let mut store = Store::new(MemoryBackend::new());
    let post = publish_post(&mut store, &root.did(), &device, "linkage", 1, 1).unwrap();
    let creator = StealthKeypair::generate().unwrap();
    let (ring, secret_index, secret_key) = reader_funds();

    let (claim, _) = build(&PaymentRequest {
        network_id: NETWORK,
        recipient_spend_public: creator.spend_public_bytes().to_vec(),
        recipient_view_public: creator.view_public_bytes().to_vec(),
        amount_micro: 1,
        purpose: PaymentPurpose::new(post.id().as_str().as_bytes().to_vec()),
        valid_until_ms: 1,
        last_known_chain: Vec::new(),
        ring,
        secret_index,
        secret_key: secret_key.to_vec(),
        blinding: mini_crypto::random_32().unwrap(),
    })
    .unwrap();

    let wire = claim.encode();
    let id = post.id().as_str().as_bytes();
    for window in 8..=id.len() {
        for chunk in id.windows(window) {
            assert!(
                !wire.windows(chunk.len()).any(|w| w == chunk),
                "an {window}-byte run of the post id leaked onto the wire"
            );
        }
    }
}

#[test]
fn the_creators_identity_root_never_appears_in_a_payment() {
    // The creator's did:mini root is public -- it is on every post they
    // sign. If a payment named it, every payment would be attributable to a
    // person by anyone holding the post.
    let (root, device) = human(100);
    let mut store = Store::new(MemoryBackend::new());
    let post = publish_post(&mut store, &root.did(), &device, "identity", 1, 1).unwrap();
    let creator = StealthKeypair::generate().unwrap();
    let (ring, secret_index, secret_key) = reader_funds();

    let (claim, _) = build(&PaymentRequest {
        network_id: NETWORK,
        recipient_spend_public: creator.spend_public_bytes().to_vec(),
        recipient_view_public: creator.view_public_bytes().to_vec(),
        amount_micro: 1,
        purpose: PaymentPurpose::new(post.id().as_str().as_bytes().to_vec()),
        valid_until_ms: 1,
        last_known_chain: Vec::new(),
        ring,
        secret_index,
        secret_key: secret_key.to_vec(),
        blinding: mini_crypto::random_32().unwrap(),
    })
    .unwrap();

    let wire = claim.encode();
    let did: &Did = &root.did();
    let did_bytes = did.as_str().as_bytes();
    assert!(!wire.windows(did_bytes.len()).any(|w| w == did_bytes));
    // The SCID alone, without the did:mini: prefix, must not appear either.
    let scid = did_bytes
        .rsplit(|b| *b == b':')
        .next()
        .expect("a did has a scid");
    assert!(!wire.windows(scid.len()).any(|w| w == scid));
}
