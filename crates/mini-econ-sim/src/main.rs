use mini_econ_sim::{run, Scenario};
use mini_economy::Amount;

fn main() {
    let scenario = Scenario {
        years: 200,
        opening_supply: Amount::from_micro(8_000_000_000_u128 * Amount::MICRO_PER_MINI),
        early_humans: 8_000_000_000,
        humans_joining_per_year: 80_000_000,
        dormant_ppm_per_year: 5_000,
        verified_sybil_identities: 0,
        whale_opening_ppm: 10_000,
        service_utilization_ppm: 500_000,
        treasury_utilization_ppm: 500_000,
    };
    println!(
        "year,total_supply_micro,active_humans,late_adopters,dormant_humans,sybil_share_ppm,whale_share_ppm,annual_inflation_ppm,human_share_per_identity_micro"
    );
    for row in run(scenario).expect("built-in scenario must be valid") {
        println!(
            "{},{},{},{},{},{},{},{},{}",
            row.year,
            row.total_supply.as_micro(),
            row.active_humans,
            row.late_adopters,
            row.dormant_humans,
            row.sybil_share_ppm,
            row.whale_share_ppm,
            row.annual_inflation_ppm,
            row.human_share_per_identity.as_micro()
        );
    }
}
