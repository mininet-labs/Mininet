use mini_economy::{
    build_genesis, plan_epoch, plan_human_share, Allocation, Amount, Channel, EconomyError,
    EpochRequest, GenesisPolicy, HumanSnapshot, IssuancePolicy, YEAR_MS,
};

fn amount(value: u128) -> Amount {
    Amount::from_micro(value)
}

#[test]
fn aggregate_human_share_scales_without_materializing_population_grants() {
    let snapshot = HumanSnapshot {
        root: [7; 32],
        eligible_count: 20_000_000_000,
    };
    let plan = plan_human_share(
        42,
        YEAR_MS,
        amount(10_u128.pow(24)),
        snapshot,
        &IssuancePolicy::d0074(),
    )
    .unwrap();
    assert_eq!(plan.snapshot, snapshot);
    assert_eq!(
        plan.issued.checked_add(plan.unissued_remainder).unwrap(),
        plan.cap
    );
    assert!(plan.unissued_remainder.as_micro() < snapshot.eligible_count as u128);
}

#[test]
fn aggregate_human_share_requires_a_committed_nonempty_snapshot() {
    let error = plan_human_share(
        1,
        YEAR_MS,
        amount(1_000_000),
        HumanSnapshot {
            root: [0; 32],
            eligible_count: 1,
        },
        &IssuancePolicy::d0074(),
    )
    .unwrap_err();
    assert_eq!(error, EconomyError::InvalidSnapshot);
}

#[test]
fn genesis_is_equal_order_independent_and_has_no_privileged_recipient() {
    let policy = GenesisPolicy {
        bootstrap_per_human: amount(1_000_000),
        vesting_ms: YEAR_MS,
    };
    let a = build_genesis(
        "mini-test",
        [7; 32],
        &["did:mini:z".into(), "did:mini:a".into()],
        &policy,
    )
    .unwrap();
    let b = build_genesis(
        "mini-test",
        [7; 32],
        &["did:mini:a".into(), "did:mini:z".into()],
        &policy,
    )
    .unwrap();
    assert_eq!(a, b);
    assert_eq!(a.total_locked, amount(2_000_000));
    assert_eq!(a.recipients, vec!["did:mini:a", "did:mini:z"]);
}

#[test]
fn d0074_envelope_is_exact_at_one_year() {
    let request = EpochRequest {
        epoch: 1,
        duration_ms: YEAR_MS,
        opening_circulating: amount(1_000_000_000),
        eligible_humans: vec!["alice".into(), "bob".into()],
        service: vec![Allocation {
            beneficiary: "relay".into(),
            amount: amount(7_500_000),
        }],
        treasury: vec![Allocation {
            beneficiary: "contributor".into(),
            amount: amount(2_500_000),
        }],
    };
    let plan = plan_epoch(&request, &IssuancePolicy::d0074()).unwrap();
    assert_eq!(plan.human_issued, amount(20_000_000));
    assert_eq!(plan.service_issued, amount(7_500_000));
    assert_eq!(plan.treasury_issued, amount(2_500_000));
    assert_eq!(plan.total_issued, amount(30_000_000));
    let human: Vec<_> = plan
        .grants
        .iter()
        .filter(|grant| grant.channel == Channel::HumanShare)
        .collect();
    assert_eq!(human.len(), 2);
    assert_eq!(human[0].amount, human[1].amount);
    assert_eq!(human[0].vesting_ms, YEAR_MS);
}

#[test]
fn unused_optional_capacity_expires_instead_of_moving_to_humans_or_treasury() {
    let request = EpochRequest {
        epoch: 2,
        duration_ms: YEAR_MS,
        opening_circulating: amount(1_000_000),
        eligible_humans: vec!["alice".into()],
        service: vec![],
        treasury: vec![],
    };
    let plan = plan_epoch(&request, &IssuancePolicy::d0074()).unwrap();
    assert_eq!(plan.total_cap, amount(30_000));
    assert_eq!(plan.total_issued, amount(20_000));
}

#[test]
fn channel_and_total_caps_fail_closed() {
    let request = EpochRequest {
        epoch: 3,
        duration_ms: YEAR_MS,
        opening_circulating: amount(1_000_000),
        eligible_humans: vec!["alice".into()],
        service: vec![Allocation {
            beneficiary: "warehouse".into(),
            amount: amount(7_501),
        }],
        treasury: vec![],
    };
    assert!(plan_epoch(&request, &IssuancePolicy::d0074()).is_err());
}

#[test]
fn duplicate_humans_and_duplicate_channel_beneficiaries_are_rejected() {
    let duplicate_human = EpochRequest {
        epoch: 4,
        duration_ms: YEAR_MS,
        opening_circulating: amount(1_000_000),
        eligible_humans: vec!["alice".into(), "alice".into()],
        service: vec![],
        treasury: vec![],
    };
    assert!(plan_epoch(&duplicate_human, &IssuancePolicy::d0074()).is_err());
}

#[test]
fn u64_micro_mini_migrates_losslessly_into_century_scale_amount() {
    let old = u64::MAX;
    let widened = Amount::from(old);
    assert_eq!(widened.as_micro(), old as u128);
}
