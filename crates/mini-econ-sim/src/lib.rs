//! Deterministic, cohort-based stress simulation for MINI economics.
//!
//! This is an engineering calibration harness, not an oracle or proof of
//! economic safety. In particular, a simulated "verified" Sybil remains a
//! personhood-system failure supplied as an explicit scenario assumption.

#![forbid(unsafe_code)]

use mini_economy::{Amount, IssuancePolicy, MILLION};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Actor {
    HonestUser,
    Whale,
    StorageFarmer,
    SybilCluster,
    DormantHuman,
    Contributor,
    RelayOperator,
    SearchIndexer,
    EarlyAdopter,
    LateAdopter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scenario {
    pub years: u16,
    pub opening_supply: Amount,
    pub early_humans: u64,
    pub humans_joining_per_year: u64,
    pub dormant_ppm_per_year: u32,
    pub verified_sybil_identities: u64,
    pub whale_opening_ppm: u32,
    pub service_utilization_ppm: u32,
    pub treasury_utilization_ppm: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub year: u16,
    pub total_supply: Amount,
    pub active_humans: u64,
    pub late_adopters: u64,
    pub dormant_humans: u64,
    pub sybil_share_ppm: u32,
    pub whale_share_ppm: u32,
    pub annual_inflation_ppm: u32,
    pub human_share_per_identity: Amount,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimulationError {
    InvalidScenario,
    Overflow,
}

pub fn run(scenario: Scenario) -> Result<Vec<Snapshot>, SimulationError> {
    validate(scenario)?;
    let policy = IssuancePolicy::d0074();
    let mut supply = scenario.opening_supply.as_micro();
    let mut early = scenario.early_humans;
    let mut late = 0_u64;
    let mut dormant = 0_u64;
    let sybils = scenario.verified_sybil_identities;
    let mut sybil_balance = 0_u128;
    let mut whale_balance = mul_ppm(supply, scenario.whale_opening_ppm)?;
    let mut snapshots = Vec::with_capacity(scenario.years as usize);

    for year in 1..=scenario.years {
        late = late
            .checked_add(scenario.humans_joining_per_year)
            .ok_or(SimulationError::Overflow)?;
        let newly_dormant = mul_ppm(
            (early as u128)
                .checked_add(late as u128)
                .ok_or(SimulationError::Overflow)?,
            scenario.dormant_ppm_per_year,
        )? as u64;
        dormant = dormant
            .checked_add(newly_dormant)
            .ok_or(SimulationError::Overflow)?
            .min(early.saturating_add(late));

        let active_real = early
            .checked_add(late)
            .and_then(|v| v.checked_sub(dormant))
            .ok_or(SimulationError::Overflow)?;
        let active = active_real
            .checked_add(sybils)
            .ok_or(SimulationError::Overflow)?;
        if active == 0 {
            return Err(SimulationError::InvalidScenario);
        }

        let human_cap = mul_ppm(supply, policy.human_floor_ppm)?;
        let per_identity = human_cap / active as u128;
        let human_issued = per_identity
            .checked_mul(active as u128)
            .ok_or(SimulationError::Overflow)?;
        sybil_balance = sybil_balance
            .checked_add(
                per_identity
                    .checked_mul(sybils as u128)
                    .ok_or(SimulationError::Overflow)?,
            )
            .ok_or(SimulationError::Overflow)?;

        let service_cap = mul_ppm(supply, policy.service_ceiling_ppm)?;
        let treasury_cap = mul_ppm(supply, policy.treasury_ceiling_ppm)?;
        let service_issued = mul_ppm(service_cap, scenario.service_utilization_ppm)?;
        let treasury_issued = mul_ppm(treasury_cap, scenario.treasury_utilization_ppm)?;
        let issued = human_issued
            .checked_add(service_issued)
            .and_then(|v| v.checked_add(treasury_issued))
            .ok_or(SimulationError::Overflow)?;
        let opening = supply;
        supply = supply
            .checked_add(issued)
            .ok_or(SimulationError::Overflow)?;

        // Wealth cannot buy protocol voice. This balance tracks only economic
        // concentration; no governance-weight field exists in the model.
        whale_balance = whale_balance
            .checked_add(service_issued / 2)
            .ok_or(SimulationError::Overflow)?;
        snapshots.push(Snapshot {
            year,
            total_supply: Amount::from_micro(supply),
            active_humans: active,
            late_adopters: late,
            dormant_humans: dormant,
            sybil_share_ppm: ratio_ppm(sybil_balance, supply),
            whale_share_ppm: ratio_ppm(whale_balance, supply),
            annual_inflation_ppm: ratio_ppm(issued, opening),
            human_share_per_identity: Amount::from_micro(per_identity),
        });
        early = early.checked_add(0).ok_or(SimulationError::Overflow)?;
    }
    Ok(snapshots)
}

fn validate(s: Scenario) -> Result<(), SimulationError> {
    if s.years == 0
        || s.opening_supply == Amount::ZERO
        || s.early_humans == 0
        || s.dormant_ppm_per_year > MILLION as u32
        || s.whale_opening_ppm > MILLION as u32
        || s.service_utilization_ppm > MILLION as u32
        || s.treasury_utilization_ppm > MILLION as u32
    {
        return Err(SimulationError::InvalidScenario);
    }
    Ok(())
}

fn mul_ppm(value: u128, ppm: u32) -> Result<u128, SimulationError> {
    value
        .checked_mul(ppm as u128)
        .map(|v| v / MILLION)
        .ok_or(SimulationError::Overflow)
}

fn ratio_ppm(part: u128, whole: u128) -> u32 {
    if whole == 0 {
        return 0;
    }
    part.saturating_mul(MILLION)
        .checked_div(whole)
        .unwrap_or_default()
        .min(MILLION) as u32
}
