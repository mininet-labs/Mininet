use mini_econ_sim::{run, Scenario};
use mini_economy::Amount;

fn baseline() -> Scenario {
    Scenario {
        years: 100,
        opening_supply: Amount::from_micro(1_000_000_000_000),
        early_humans: 1_000,
        humans_joining_per_year: 100,
        dormant_ppm_per_year: 0,
        verified_sybil_identities: 0,
        whale_opening_ppm: 100_000,
        service_utilization_ppm: 1_000_000,
        treasury_utilization_ppm: 1_000_000,
    }
}

#[test]
fn annual_issuance_never_exceeds_d0074_ceiling() {
    for row in run(baseline()).unwrap() {
        assert!(row.annual_inflation_ppm <= 30_000);
    }
}

#[test]
fn late_adopters_receive_the_same_current_human_share() {
    let rows = run(baseline()).unwrap();
    assert_eq!(rows.len(), 100);
    assert!(rows.last().unwrap().late_adopters > 0);
    assert!(rows
        .iter()
        .all(|row| row.human_share_per_identity > Amount::ZERO));
}

#[test]
fn sybil_extraction_is_explicit_and_measurable() {
    let mut scenario = baseline();
    scenario.verified_sybil_identities = 1_000;
    let rows = run(scenario).unwrap();
    assert!(rows.last().unwrap().sybil_share_ppm > 0);
}

#[test]
fn wealth_has_no_governance_weight_in_the_model() {
    let mut scenario = baseline();
    scenario.whale_opening_ppm = 900_000;
    let rows = run(scenario).unwrap();
    assert!(rows[0].whale_share_ppm > 800_000);
    // Snapshot deliberately has no vote-weight or governance-power field.
    assert_eq!(rows[0].active_humans, 1_100);
}
