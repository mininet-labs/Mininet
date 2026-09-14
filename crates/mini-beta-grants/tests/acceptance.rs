use did_mini::Controller;
use mini_beta::{
    create_campaign, create_contribution_receipt, create_grant_authorization, BetaAccountId,
    BetaEpochId, BetaError, BetaMiniPolicy, ClaimTag, ContributionKind, GrantClass,
};
use mini_beta_grants::{
    create_grant_approval, create_grant_policy, detect_authorizer_equivocations,
    validate_grant_acceptance, GrantAcceptanceError, SharedBetaLedger,
};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{MemoryBackend, Store};

fn signer(seed: u8) -> Controller {
    Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap()
}

fn target(store: &mut Store<MemoryBackend>, signer: &Controller, label: &[u8]) -> Object {
    let object = ObjectBuilder::new(ObjectType::Custom(
        "mininet.beta-grants/test-target/v1".to_string(),
    ))
    .timestamp_ms(1)
    .sequence(1)
    .payload(Payload::Public(label.to_vec()))
    .sign(&signer.did(), signer)
    .unwrap();
    store.insert(&object).unwrap();
    object
}

struct Fixture {
    campaign_author: Controller,
    authorizers: Vec<Controller>,
    outsider: Controller,
    store: Store<MemoryBackend>,
    target: Object,
    campaign: Object,
    policy: Object,
    epoch: BetaEpochId,
}

impl Fixture {
    fn new() -> Self {
        let campaign_author = signer(10);
        let authorizers = vec![signer(20), signer(30), signer(40), signer(50)];
        let outsider = signer(60);
        let mut store = Store::new(MemoryBackend::new());
        let target = target(&mut store, &campaign_author, b"grant acceptance fixture");
        let epoch = BetaEpochId::new([7; 32]).unwrap();
        let campaign = create_campaign(
            &mut store,
            &campaign_author.did(),
            &campaign_author,
            target.id(),
            epoch,
            "threshold beta campaign",
            "test decentralized Beta MINI grant acceptance",
            &["beta-mini".to_string()],
            100,
            10_000,
            1_000,
            50,
            1,
        )
        .unwrap();
        let members = authorizers.iter().map(Controller::did).collect::<Vec<_>>();
        let policy = create_grant_policy(
            &mut store,
            &campaign_author.did(),
            &campaign_author,
            campaign.id(),
            &members,
            2,
            3,
            100,
            &[10, 25, 50],
            200,
            9_000,
            150,
            2,
        )
        .unwrap();

        Self {
            campaign_author,
            authorizers,
            outsider,
            store,
            target,
            campaign,
            policy,
            epoch,
        }
    }

    fn testing_grant(&mut self, account: BetaAccountId, memo: &str, sequence: u64) -> Object {
        create_grant_authorization(
            &mut self.store,
            &self.campaign_author.did(),
            &self.campaign_author,
            self.campaign.id(),
            None,
            self.epoch,
            account,
            GrantClass::Testing,
            100,
            memo,
            500 + sequence,
            sequence,
        )
        .unwrap()
    }

    fn contribution(&mut self, tag: u8, sequence: u64) -> Object {
        create_contribution_receipt(
            &mut self.store,
            &self.campaign_author.did(),
            &self.campaign_author,
            self.target.id(),
            Some(self.campaign.id()),
            ClaimTag::new([tag; 32]).unwrap(),
            ContributionKind::Testing,
            "accepted contribution",
            &["evidence:accepted".to_string()],
            400 + sequence,
            sequence,
        )
        .unwrap()
    }

    fn participation_grant(
        &mut self,
        account: BetaAccountId,
        contribution_id: &ObjectId,
        amount: u64,
        memo: &str,
        sequence: u64,
    ) -> Object {
        create_grant_authorization(
            &mut self.store,
            &self.campaign_author.did(),
            &self.campaign_author,
            self.campaign.id(),
            Some(contribution_id),
            self.epoch,
            account,
            GrantClass::Participation,
            amount,
            memo,
            600 + sequence,
            sequence,
        )
        .unwrap()
    }

    fn approve(&mut self, member_index: usize, grant_id: &ObjectId, sequence: u64) -> Object {
        let member = &self.authorizers[member_index];
        create_grant_approval(
            &mut self.store,
            &member.did(),
            member,
            self.policy.id(),
            grant_id,
            1_000 + sequence,
            sequence,
        )
        .unwrap()
    }
}

#[test]
fn one_authorizer_cannot_mint_testing_grant() {
    let mut f = Fixture::new();
    let grant = f.testing_grant(BetaAccountId::new([1; 32]).unwrap(), "test", 10);
    let approval = f.approve(0, grant.id(), 20);

    assert!(matches!(
        validate_grant_acceptance(
            &f.store,
            f.policy.id(),
            grant.id(),
            &[approval.id().clone()]
        ),
        Err(GrantAcceptanceError::ThresholdNotMet {
            required: 2,
            observed: 1
        })
    ));
}

#[test]
fn duplicate_approvals_from_one_did_never_increase_weight() {
    let mut f = Fixture::new();
    let grant = f.testing_grant(BetaAccountId::new([2; 32]).unwrap(), "test", 10);
    let first = f.approve(0, grant.id(), 20);
    let second = f.approve(0, grant.id(), 21);

    assert_ne!(first.id(), second.id());
    assert!(matches!(
        validate_grant_acceptance(
            &f.store,
            f.policy.id(),
            grant.id(),
            &[first.id().clone(), second.id().clone()]
        ),
        Err(GrantAcceptanceError::ThresholdNotMet {
            required: 2,
            observed: 1
        })
    ));
}

#[test]
fn distinct_members_reach_testing_threshold_and_order_does_not_matter() {
    let mut f = Fixture::new();
    let grant = f.testing_grant(BetaAccountId::new([3; 32]).unwrap(), "test", 10);
    let a = f.approve(0, grant.id(), 20);
    let b = f.approve(1, grant.id(), 21);

    let forward = validate_grant_acceptance(
        &f.store,
        f.policy.id(),
        grant.id(),
        &[a.id().clone(), b.id().clone()],
    )
    .unwrap();
    let reverse = validate_grant_acceptance(
        &f.store,
        f.policy.id(),
        grant.id(),
        &[b.id().clone(), a.id().clone()],
    )
    .unwrap();

    assert_eq!(forward, reverse);
    assert_eq!(forward.required_approvals, 2);
    assert_eq!(forward.distinct_approvers, 2);
}

#[test]
fn participation_uses_the_stronger_threshold() {
    let mut f = Fixture::new();
    let contribution = f.contribution(8, 10);
    let grant = f.participation_grant(
        BetaAccountId::new([4; 32]).unwrap(),
        contribution.id(),
        25,
        "participation",
        20,
    );
    let a = f.approve(0, grant.id(), 30);
    let b = f.approve(1, grant.id(), 31);

    assert!(matches!(
        validate_grant_acceptance(
            &f.store,
            f.policy.id(),
            grant.id(),
            &[a.id().clone(), b.id().clone()]
        ),
        Err(GrantAcceptanceError::ThresholdNotMet {
            required: 3,
            observed: 2
        })
    ));

    let c = f.approve(2, grant.id(), 32);
    let accepted = validate_grant_acceptance(
        &f.store,
        f.policy.id(),
        grant.id(),
        &[a.id().clone(), b.id().clone(), c.id().clone()],
    )
    .unwrap();
    assert_eq!(accepted.required_approvals, 3);
}

#[test]
fn nonmember_cannot_create_valid_approval() {
    let mut f = Fixture::new();
    let grant = f.testing_grant(BetaAccountId::new([5; 32]).unwrap(), "test", 10);
    let result = create_grant_approval(
        &mut f.store,
        &f.outsider.did(),
        &f.outsider,
        f.policy.id(),
        grant.id(),
        1_000,
        20,
    );
    assert!(matches!(result, Err(GrantAcceptanceError::InvalidApproval)));
}

#[test]
fn approval_for_another_grant_cannot_be_reused() {
    let mut f = Fixture::new();
    let account = BetaAccountId::new([6; 32]).unwrap();
    let grant_a = f.testing_grant(account, "grant a", 10);
    let grant_b = f.testing_grant(account, "grant b", 11);
    let a = f.approve(0, grant_a.id(), 20);
    let b = f.approve(1, grant_b.id(), 21);

    assert!(matches!(
        validate_grant_acceptance(
            &f.store,
            f.policy.id(),
            grant_a.id(),
            &[a.id().clone(), b.id().clone()]
        ),
        Err(GrantAcceptanceError::InvalidApproval)
    ));
}

#[test]
fn campaign_author_cannot_delegate_policy_authorship_to_arbitrary_key() {
    let mut f = Fixture::new();
    let members = f
        .authorizers
        .iter()
        .map(Controller::did)
        .collect::<Vec<_>>();
    let result = create_grant_policy(
        &mut f.store,
        &f.outsider.did(),
        &f.outsider,
        f.campaign.id(),
        &members,
        2,
        3,
        100,
        &[10, 25, 50],
        200,
        9_000,
        150,
        40,
    );
    assert!(matches!(
        result,
        Err(GrantAcceptanceError::PolicyAuthorMismatch)
    ));
}

#[test]
fn testing_amount_is_deterministic_not_authorizer_discretion() {
    let mut f = Fixture::new();
    let grant = create_grant_authorization(
        &mut f.store,
        &f.campaign_author.did(),
        &f.campaign_author,
        f.campaign.id(),
        None,
        f.epoch,
        BetaAccountId::new([7; 32]).unwrap(),
        GrantClass::Testing,
        99,
        "not the policy amount",
        500,
        10,
    )
    .unwrap();

    assert!(matches!(
        validate_grant_acceptance(&f.store, f.policy.id(), grant.id(), &[]),
        Err(GrantAcceptanceError::GrantRuleViolation)
    ));
}

#[test]
fn participation_amount_must_be_an_explicit_reward_band() {
    let mut f = Fixture::new();
    let contribution = f.contribution(9, 10);
    let grant = f.participation_grant(
        BetaAccountId::new([8; 32]).unwrap(),
        contribution.id(),
        24,
        "not a reward band",
        20,
    );

    assert!(matches!(
        validate_grant_acceptance(&f.store, f.policy.id(), grant.id(), &[]),
        Err(GrantAcceptanceError::GrantRuleViolation)
    ));
}

#[test]
fn expired_or_future_approval_is_rejected() {
    let mut f = Fixture::new();
    let grant = f.testing_grant(BetaAccountId::new([9; 32]).unwrap(), "test", 10);
    let member = &f.authorizers[0];
    let late = create_grant_approval(
        &mut f.store,
        &member.did(),
        member,
        f.policy.id(),
        grant.id(),
        9_001,
        20,
    );
    assert!(matches!(late, Err(GrantAcceptanceError::InvalidApproval)));
}

#[test]
fn threshold_approval_still_cannot_reward_one_contribution_twice() {
    let mut f = Fixture::new();
    let contribution = f.contribution(10, 10);
    let account = BetaAccountId::new([10; 32]).unwrap();
    let first = f.participation_grant(account, contribution.id(), 25, "first", 20);
    let second = f.participation_grant(account, contribution.id(), 25, "second", 21);

    let first_approvals = [
        f.approve(0, first.id(), 30).id().clone(),
        f.approve(1, first.id(), 31).id().clone(),
        f.approve(2, first.id(), 32).id().clone(),
    ];
    let second_approvals = [
        f.approve(0, second.id(), 40).id().clone(),
        f.approve(1, second.id(), 41).id().clone(),
        f.approve(2, second.id(), 42).id().clone(),
    ];

    let mut ledger = SharedBetaLedger::new(f.epoch, BetaMiniPolicy::open_beta_default()).unwrap();
    ledger
        .apply_accepted_grant(&f.store, f.policy.id(), first.id(), &first_approvals)
        .unwrap();
    assert!(matches!(
        ledger.apply_accepted_grant(&f.store, f.policy.id(), second.id(), &second_approvals),
        Err(GrantAcceptanceError::Beta(BetaError::DuplicateGrant))
    ));
    assert_eq!(ledger.balance(&account), 25);
}

#[test]
fn authorizer_equivocation_is_detected_but_not_given_hidden_blacklist_power() {
    let mut f = Fixture::new();
    let account = BetaAccountId::new([11; 32]).unwrap();
    let first = f.testing_grant(account, "first", 10);
    let second = f.testing_grant(account, "second", 11);
    let a = f.approve(0, first.id(), 20);
    let b = f.approve(0, second.id(), 21);

    let conflicts =
        detect_authorizer_equivocations(&f.store, f.policy.id(), &[a.id().clone(), b.id().clone()])
            .unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].authorizer, f.authorizers[0].did());
}

#[test]
fn three_way_equivocation_evidence_does_not_depend_on_arrival_order() {
    // Recording conflicts only against whichever grant was seen first made
    // the *set* of reported pairs depend on approval_ids' order -- exactly
    // the kind of non-determinism this function's own doc comment says
    // cannot happen once the same immutable objects have replicated. With
    // three mutually conflicting approvals from one authorizer, every
    // ordering must report the same three pairs.
    let mut f = Fixture::new();
    let account = BetaAccountId::new([13; 32]).unwrap();
    let first = f.testing_grant(account, "first", 10);
    let second = f.testing_grant(account, "second", 11);
    let third = f.testing_grant(account, "third", 12);
    let a = f.approve(0, first.id(), 20);
    let b = f.approve(0, second.id(), 21);
    let c = f.approve(0, third.id(), 22);

    let mut forward = detect_authorizer_equivocations(
        &f.store,
        f.policy.id(),
        &[a.id().clone(), b.id().clone(), c.id().clone()],
    )
    .unwrap();
    let mut reordered = detect_authorizer_equivocations(
        &f.store,
        f.policy.id(),
        &[c.id().clone(), a.id().clone(), b.id().clone()],
    )
    .unwrap();

    assert_eq!(forward.len(), 3, "all three pairs must be reported");
    let sort_key = |e: &mini_beta_grants::AuthorizerEquivocation| {
        (
            e.first_grant_id.as_str().to_string(),
            e.second_grant_id.as_str().to_string(),
        )
    };
    forward.sort_by_key(sort_key);
    reordered.sort_by_key(sort_key);
    assert_eq!(forward, reordered);
}

#[test]
fn offline_store_converges_after_the_same_immutable_evidence_arrives() {
    let mut f = Fixture::new();
    let grant = f.testing_grant(BetaAccountId::new([12; 32]).unwrap(), "test", 10);
    let a = f.approve(0, grant.id(), 20);
    let b = f.approve(1, grant.id(), 21);

    let mut offline = Store::new(MemoryBackend::new());
    offline.insert(&f.campaign).unwrap();
    offline.insert(&f.policy).unwrap();
    offline.insert(&grant).unwrap();
    offline.insert(&a).unwrap();

    assert!(matches!(
        validate_grant_acceptance(&offline, f.policy.id(), grant.id(), &[a.id().clone()]),
        Err(GrantAcceptanceError::ThresholdNotMet {
            required: 2,
            observed: 1
        })
    ));

    offline.insert(&b).unwrap();
    let online = validate_grant_acceptance(
        &f.store,
        f.policy.id(),
        grant.id(),
        &[a.id().clone(), b.id().clone()],
    )
    .unwrap();
    let after_sync = validate_grant_acceptance(
        &offline,
        f.policy.id(),
        grant.id(),
        &[b.id().clone(), a.id().clone()],
    )
    .unwrap();
    assert_eq!(online, after_sync);
}

#[test]
fn epoch_rollover_carries_no_beta_balance() {
    let mut f = Fixture::new();
    let account = BetaAccountId::new([13; 32]).unwrap();
    let grant = f.testing_grant(account, "test", 10);
    let approvals = [
        f.approve(0, grant.id(), 20).id().clone(),
        f.approve(1, grant.id(), 21).id().clone(),
    ];
    let mut ledger = SharedBetaLedger::new(f.epoch, BetaMiniPolicy::open_beta_default()).unwrap();
    ledger
        .apply_accepted_grant(&f.store, f.policy.id(), grant.id(), &approvals)
        .unwrap();
    assert_eq!(ledger.balance(&account), 100);

    let next_epoch = BetaEpochId::new([14; 32]).unwrap();
    let next = ledger.rollover(next_epoch).unwrap();
    assert_eq!(next.balance(&account), 0);
    assert_eq!(next.total_issued(), 0);
}

#[test]
fn competing_valid_campaign_policies_fail_closed_instead_of_local_tie_breaking() {
    let mut f = Fixture::new();
    let grant = f.testing_grant(BetaAccountId::new([15; 32]).unwrap(), "test", 10);
    let a = f.approve(0, grant.id(), 20);
    let b = f.approve(1, grant.id(), 21);
    let members = f
        .authorizers
        .iter()
        .map(Controller::did)
        .collect::<Vec<_>>();
    let competing = create_grant_policy(
        &mut f.store,
        &f.campaign_author.did(),
        &f.campaign_author,
        f.campaign.id(),
        &members,
        2,
        3,
        100,
        &[10, 25, 50],
        200,
        9_000,
        151,
        99,
    )
    .unwrap();
    assert_ne!(competing.id(), f.policy.id());

    assert!(matches!(
        validate_grant_acceptance(
            &f.store,
            f.policy.id(),
            grant.id(),
            &[a.id().clone(), b.id().clone()]
        ),
        Err(GrantAcceptanceError::PolicyConflict)
    ));
}

#[test]
fn outsider_policy_shaped_objects_cannot_veto_the_campaign_policy() {
    let mut f = Fixture::new();
    let grant = f.testing_grant(BetaAccountId::new([16; 32]).unwrap(), "test", 10);
    let a = f.approve(0, grant.id(), 20);
    let b = f.approve(1, grant.id(), 21);

    let outsider_copy = ObjectBuilder::new(ObjectType::Custom(
        mini_beta_grants::BETA_GRANT_POLICY_TYPE.to_string(),
    ))
    .timestamp_ms(f.policy.timestamp_ms + 1)
    .sequence(999)
    .payload(f.policy.payload.clone())
    .link("campaign", f.campaign.id().clone())
    .sign(&f.outsider.did(), &f.outsider)
    .unwrap();
    f.store.insert(&outsider_copy).unwrap();

    let malformed = ObjectBuilder::new(ObjectType::Custom(
        mini_beta_grants::BETA_GRANT_POLICY_TYPE.to_string(),
    ))
    .timestamp_ms(152)
    .sequence(1_000)
    .payload(Payload::Public(vec![0xff]))
    .link("campaign", f.campaign.id().clone())
    .sign(&f.outsider.did(), &f.outsider)
    .unwrap();
    f.store.insert(&malformed).unwrap();

    let accepted = validate_grant_acceptance(
        &f.store,
        f.policy.id(),
        grant.id(),
        &[a.id().clone(), b.id().clone()],
    )
    .unwrap();
    assert_eq!(accepted.distinct_approvers, 2);
}
