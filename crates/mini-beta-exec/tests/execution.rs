use did_mini::Controller;
use mini_beta::{
    create_campaign, create_grant_authorization, BetaAccountId, BetaEpochId, BetaMiniPolicy,
    GrantClass,
};
use mini_beta_exec::{
    create_account_registration, create_transfer, derive_account_id,
    parse_account_registration_object, resolve_snapshot, BetaExecError, BetaOutputRef,
    BetaTransferOutput,
};
use mini_beta_grants::{create_grant_approval, create_grant_policy};
use mini_objects::{Object, ObjectBuilder, ObjectType, Payload};
use mini_store::{FsBackend, MemoryBackend, Store};

fn signer(seed: u8) -> Controller {
    Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap()
}

fn limits(max_supply: u64) -> BetaMiniPolicy {
    BetaMiniPolicy {
        max_testing_grant: 1_000,
        max_participation_grant: 1_000,
        max_epoch_supply: max_supply,
    }
}

struct Fixture {
    store: Store<MemoryBackend>,
    objects: Vec<Object>,
    epoch: BetaEpochId,
    campaign_author: Controller,
    authorizers: Vec<Controller>,
    policy: Object,
    campaign: Object,
    alice: Controller,
    bob: Controller,
    alice_account: BetaAccountId,
    bob_account: BetaAccountId,
    carol_account: BetaAccountId,
    grant: Object,
}

impl Fixture {
    fn new() -> Self {
        let campaign_author = signer(10);
        let authorizers = vec![signer(20), signer(30), signer(40)];
        let alice = signer(50);
        let bob = signer(60);
        let carol = signer(70);
        let epoch = BetaEpochId::new([7; 32]).unwrap();
        let mut store = Store::new(MemoryBackend::new());
        let mut objects = Vec::new();

        let target = ObjectBuilder::new(ObjectType::Custom(
            "mininet.beta-exec/test-release/v1".to_string(),
        ))
        .timestamp_ms(1)
        .sequence(1)
        .payload(Payload::Public(b"beta-exec-fixture".to_vec()))
        .sign(&campaign_author.did(), &campaign_author)
        .unwrap();
        store.insert(&target).unwrap();
        objects.push(target.clone());

        let campaign = create_campaign(
            &mut store,
            &campaign_author.did(),
            &campaign_author,
            target.id(),
            epoch,
            "durable beta",
            "test authenticated durable execution",
            &["wallet".to_string()],
            100,
            10_000,
            1_000,
            100,
            2,
        )
        .unwrap();
        objects.push(campaign.clone());

        let members = authorizers.iter().map(Controller::did).collect::<Vec<_>>();
        let policy = create_grant_policy(
            &mut store,
            &campaign_author.did(),
            &campaign_author,
            campaign.id(),
            &members,
            2,
            2,
            100,
            &[25, 50, 100],
            200,
            9_000,
            150,
            3,
        )
        .unwrap();
        objects.push(policy.clone());

        let alice_reg =
            create_account_registration(&mut store, &alice.did(), &alice, epoch, [1; 32], 250, 1)
                .unwrap();
        let bob_reg =
            create_account_registration(&mut store, &bob.did(), &bob, epoch, [2; 32], 251, 1)
                .unwrap();
        let carol_reg =
            create_account_registration(&mut store, &carol.did(), &carol, epoch, [3; 32], 252, 1)
                .unwrap();
        objects.extend([alice_reg.clone(), bob_reg.clone(), carol_reg.clone()]);
        let alice_account = parse_account_registration_object(&alice_reg)
            .unwrap()
            .account;
        let bob_account = parse_account_registration_object(&bob_reg).unwrap().account;
        let carol_account = parse_account_registration_object(&carol_reg)
            .unwrap()
            .account;

        let grant = create_grant_authorization(
            &mut store,
            &campaign_author.did(),
            &campaign_author,
            campaign.id(),
            None,
            epoch,
            alice_account,
            GrantClass::Testing,
            100,
            "testing balance",
            500,
            4,
        )
        .unwrap();
        objects.push(grant.clone());
        for (index, member) in authorizers.iter().take(2).enumerate() {
            let approval = create_grant_approval(
                &mut store,
                &member.did(),
                member,
                policy.id(),
                grant.id(),
                600 + index as u64,
                1,
            )
            .unwrap();
            objects.push(approval);
        }

        Self {
            store,
            objects,
            epoch,
            campaign_author,
            authorizers,
            policy,
            campaign,
            alice,
            bob,
            alice_account,
            bob_account,
            carol_account,
            grant,
        }
    }

    fn grant_ref(&self) -> BetaOutputRef {
        BetaOutputRef {
            source_id: self.grant.id().clone(),
            index: 0,
        }
    }

    fn add_second_testing_grant(&mut self) -> Object {
        let grant = create_grant_authorization(
            &mut self.store,
            &self.campaign_author.did(),
            &self.campaign_author,
            self.campaign.id(),
            None,
            self.epoch,
            self.alice_account,
            GrantClass::Testing,
            100,
            "second testing balance",
            700,
            5,
        )
        .unwrap();
        self.objects.push(grant.clone());
        for (index, member) in self.authorizers.iter().take(2).enumerate() {
            let approval = create_grant_approval(
                &mut self.store,
                &member.did(),
                member,
                self.policy.id(),
                grant.id(),
                800 + index as u64,
                2,
            )
            .unwrap();
            self.objects.push(approval);
        }
        grant
    }
}

#[test]
fn account_id_binds_epoch_owner_and_nonce() {
    let owner = signer(90);
    let other = signer(91);
    let epoch_a = BetaEpochId::new([1; 32]).unwrap();
    let epoch_b = BetaEpochId::new([2; 32]).unwrap();
    let nonce = [9; 32];
    assert_ne!(
        derive_account_id(epoch_a, &owner.did(), nonce).unwrap(),
        derive_account_id(epoch_b, &owner.did(), nonce).unwrap()
    );
    assert_ne!(
        derive_account_id(epoch_a, &owner.did(), nonce).unwrap(),
        derive_account_id(epoch_a, &other.did(), nonce).unwrap()
    );
}

#[test]
fn another_author_cannot_claim_an_observed_account_id() {
    let owner = signer(92);
    let attacker = signer(93);
    let epoch = BetaEpochId::new([3; 32]).unwrap();
    let nonce = [4; 32];
    let victim_account = derive_account_id(epoch, &owner.did(), nonce).unwrap();
    let mut payload = vec![1];
    payload.extend_from_slice(epoch.as_bytes());
    payload.extend_from_slice(&nonce);
    payload.extend_from_slice(victim_account.as_bytes());
    let forged = ObjectBuilder::new(ObjectType::Custom(
        mini_beta_exec::BETA_ACCOUNT_TYPE.to_string(),
    ))
    .timestamp_ms(1)
    .sequence(1)
    .payload(Payload::Public(payload))
    .sign(&attacker.did(), &attacker)
    .unwrap();
    assert!(matches!(
        parse_account_registration_object(&forged),
        Err(BetaExecError::InvalidAccount)
    ));
}

#[test]
fn accepted_grant_is_one_durable_unspent_output() {
    let f = Fixture::new();
    let snapshot = resolve_snapshot(&f.store, f.epoch, limits(1_000)).unwrap();
    assert!(!snapshot.issuance_conflict);
    assert_eq!(snapshot.total_issued, 100);
    assert_eq!(snapshot.balance(&f.alice_account), 100);
    assert_eq!(snapshot.balance(&f.bob_account), 0);
    assert_eq!(snapshot.outputs.len(), 1);
    assert_eq!(snapshot.outputs[0].output_ref, f.grant_ref());
    assert!(!snapshot.outputs[0].spent);
    assert!(snapshot.is_provisional());
}

#[test]
fn signed_transfer_conserves_value_and_moves_only_owned_inputs() {
    let mut f = Fixture::new();
    let grant_ref = f.grant_ref();
    let transfer = create_transfer(
        &mut f.store,
        &f.alice.did(),
        &f.alice,
        f.epoch,
        f.alice_account,
        &[grant_ref],
        &[
            BetaTransferOutput {
                account: f.bob_account,
                amount: 60,
            },
            BetaTransferOutput {
                account: f.alice_account,
                amount: 40,
            },
        ],
        "beta payment",
        900,
        1,
    )
    .unwrap();
    let snapshot = resolve_snapshot(&f.store, f.epoch, limits(1_000)).unwrap();
    assert_eq!(snapshot.balance(&f.alice_account), 40);
    assert_eq!(snapshot.balance(&f.bob_account), 60);
    assert!(snapshot
        .outputs
        .iter()
        .any(|output| output.output_ref == f.grant_ref() && output.spent));
    assert!(snapshot
        .outputs
        .iter()
        .any(|output| output.output_ref.source_id == *transfer.id() && !output.spent));
}

#[test]
fn non_owner_authored_transfer_is_invalid() {
    let mut f = Fixture::new();
    let grant_ref = f.grant_ref();
    let transfer = create_transfer(
        &mut f.store,
        &f.bob.did(),
        &f.bob,
        f.epoch,
        f.alice_account,
        &[grant_ref],
        &[BetaTransferOutput {
            account: f.bob_account,
            amount: 100,
        }],
        "attempted theft",
        900,
        1,
    )
    .unwrap();
    let snapshot = resolve_snapshot(&f.store, f.epoch, limits(1_000)).unwrap();
    assert_eq!(snapshot.balance(&f.alice_account), 100);
    assert_eq!(snapshot.balance(&f.bob_account), 0);
    assert!(snapshot.invalid_transfers.contains(transfer.id()));
}

#[test]
fn two_spends_of_one_output_invalidate_all_consumers_without_a_local_winner() {
    let mut f = Fixture::new();
    let grant_ref = f.grant_ref();
    let first = create_transfer(
        &mut f.store,
        &f.alice.did(),
        &f.alice,
        f.epoch,
        f.alice_account,
        std::slice::from_ref(&grant_ref),
        &[BetaTransferOutput {
            account: f.bob_account,
            amount: 100,
        }],
        "first offline spend",
        900,
        1,
    )
    .unwrap();
    let second = create_transfer(
        &mut f.store,
        &f.alice.did(),
        &f.alice,
        f.epoch,
        f.alice_account,
        &[grant_ref],
        &[BetaTransferOutput {
            account: f.carol_account,
            amount: 100,
        }],
        "second offline spend",
        901,
        2,
    )
    .unwrap();
    let downstream = create_transfer(
        &mut f.store,
        &f.bob.did(),
        &f.bob,
        f.epoch,
        f.bob_account,
        &[BetaOutputRef {
            source_id: first.id().clone(),
            index: 0,
        }],
        &[BetaTransferOutput {
            account: f.carol_account,
            amount: 100,
        }],
        "depends on conflicted producer",
        902,
        1,
    )
    .unwrap();

    let snapshot = resolve_snapshot(&f.store, f.epoch, limits(1_000)).unwrap();
    assert_eq!(snapshot.balance(&f.alice_account), 100);
    assert_eq!(snapshot.balance(&f.bob_account), 0);
    assert_eq!(snapshot.balance(&f.carol_account), 0);
    assert_eq!(snapshot.transfer_conflicts.len(), 2);
    assert!(snapshot.transfer_conflicts.contains(first.id()));
    assert!(snapshot.transfer_conflicts.contains(second.id()));
    assert!(snapshot.invalid_transfers.contains(downstream.id()));
    assert!(!snapshot.outputs.iter().any(|output| {
        output.output_ref.source_id == *first.id()
            || output.output_ref.source_id == *second.id()
            || output.output_ref.source_id == *downstream.id()
    }));
}

#[test]
fn non_conserving_transfer_is_invalid_not_a_mint_or_burn() {
    let mut f = Fixture::new();
    let grant_ref = f.grant_ref();
    let transfer = create_transfer(
        &mut f.store,
        &f.alice.did(),
        &f.alice,
        f.epoch,
        f.alice_account,
        &[grant_ref],
        &[BetaTransferOutput {
            account: f.bob_account,
            amount: 101,
        }],
        "invalid mint",
        900,
        1,
    )
    .unwrap();
    let snapshot = resolve_snapshot(&f.store, f.epoch, limits(1_000)).unwrap();
    assert_eq!(snapshot.balance(&f.alice_account), 100);
    assert!(snapshot.invalid_transfers.contains(transfer.id()));
}

#[test]
fn accepted_issuance_above_epoch_cap_fails_closed_instead_of_hash_ordering_winners() {
    let mut f = Fixture::new();
    f.add_second_testing_grant();
    let snapshot = resolve_snapshot(&f.store, f.epoch, limits(150)).unwrap();
    assert!(snapshot.issuance_conflict);
    assert_eq!(snapshot.total_issued, 0);
    assert_eq!(snapshot.balance(&f.alice_account), 0);
    assert!(snapshot.outputs.is_empty());
}

#[test]
fn identical_object_sets_converge_independent_of_insertion_order() {
    let mut f = Fixture::new();
    let grant_ref = f.grant_ref();
    let transfer = create_transfer(
        &mut f.store,
        &f.alice.did(),
        &f.alice,
        f.epoch,
        f.alice_account,
        &[grant_ref],
        &[
            BetaTransferOutput {
                account: f.bob_account,
                amount: 75,
            },
            BetaTransferOutput {
                account: f.alice_account,
                amount: 25,
            },
        ],
        "deterministic order test",
        900,
        1,
    )
    .unwrap();
    f.objects.push(transfer);
    let expected = resolve_snapshot(&f.store, f.epoch, limits(1_000)).unwrap();

    let mut reverse = Store::new(MemoryBackend::new());
    for object in f.objects.iter().rev() {
        reverse.insert(object).unwrap();
    }
    let observed = resolve_snapshot(&reverse, f.epoch, limits(1_000)).unwrap();
    assert_eq!(expected, observed);
    assert_eq!(expected.digest, observed.digest);
}

#[test]
fn filesystem_restart_rederives_the_same_snapshot_without_a_balance_database() {
    let f = Fixture::new();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store");
    let first = {
        let backend = FsBackend::open(&path).unwrap();
        let mut store = Store::new(backend);
        for object in &f.objects {
            store.insert(object).unwrap();
        }
        resolve_snapshot(&store, f.epoch, limits(1_000)).unwrap()
    };
    let second = {
        let backend = FsBackend::open(&path).unwrap();
        let store = Store::new(backend);
        resolve_snapshot(&store, f.epoch, limits(1_000)).unwrap()
    };
    assert_eq!(first, second);
    assert_eq!(first.digest, second.digest);
}
