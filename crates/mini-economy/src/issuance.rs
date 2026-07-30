use std::collections::BTreeSet;

use mini_crypto::{HashAlgorithm, Multihash};

use crate::{Amount, EconomyError, Result};

pub const MILLION: u128 = 1_000_000;
pub const YEAR_MS: u64 = 365 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    HumanShare,
    Service,
    TreasuryContribution,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allocation {
    pub beneficiary: String,
    pub amount: Amount,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VestingGrant {
    pub beneficiary: String,
    pub channel: Channel,
    pub amount: Amount,
    pub vesting_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuancePolicy {
    pub total_ceiling_ppm: u32,
    pub human_floor_ppm: u32,
    pub service_ceiling_ppm: u32,
    pub treasury_ceiling_ppm: u32,
    pub human_vesting_ms: u64,
    pub treasury_vesting_ms: u64,
}

impl IssuancePolicy {
    pub const fn d0074() -> Self {
        Self {
            total_ceiling_ppm: 30_000,
            human_floor_ppm: 20_000,
            service_ceiling_ppm: 7_500,
            treasury_ceiling_ppm: 2_500,
            human_vesting_ms: YEAR_MS,
            treasury_vesting_ms: 90 * 24 * 60 * 60 * 1_000,
        }
    }

    fn validate(&self) -> Result<()> {
        let channels = self
            .human_floor_ppm
            .checked_add(self.service_ceiling_ppm)
            .and_then(|v| v.checked_add(self.treasury_ceiling_ppm))
            .ok_or(EconomyError::InvalidPolicy)?;
        if channels > self.total_ceiling_ppm
            || self.total_ceiling_ppm > MILLION as u32
            || self.human_vesting_ms == 0
            || self.treasury_vesting_ms == 0
        {
            return Err(EconomyError::InvalidPolicy);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpochRequest {
    pub epoch: u64,
    pub duration_ms: u64,
    pub opening_circulating: Amount,
    pub eligible_humans: Vec<String>,
    pub service: Vec<Allocation>,
    pub treasury: Vec<Allocation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpochPlan {
    pub epoch: u64,
    pub duration_ms: u64,
    pub opening_circulating: Amount,
    pub human_cap: Amount,
    pub service_cap: Amount,
    pub treasury_cap: Amount,
    pub total_cap: Amount,
    pub human_issued: Amount,
    pub service_issued: Amount,
    pub treasury_issued: Amount,
    pub total_issued: Amount,
    pub grants: Vec<VestingGrant>,
}

impl EpochPlan {
    /// Versioned commitment to the exact transition proposal.
    pub fn commitment(&self) -> Multihash {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"mini-economy/epoch-plan/v1");
        bytes.extend_from_slice(&self.epoch.to_be_bytes());
        bytes.extend_from_slice(&self.duration_ms.to_be_bytes());
        for amount in [
            self.opening_circulating,
            self.human_cap,
            self.service_cap,
            self.treasury_cap,
            self.total_cap,
            self.human_issued,
            self.service_issued,
            self.treasury_issued,
            self.total_issued,
        ] {
            bytes.extend_from_slice(&amount.as_micro().to_be_bytes());
        }
        bytes.extend_from_slice(&(self.grants.len() as u64).to_be_bytes());
        put_grants(&mut bytes, &self.grants);
        Multihash::of(HashAlgorithm::Blake3, &bytes)
    }
}

/// Finalized personhood-set commitment consumed by the scalable Human Share
/// path. Membership proof semantics belong to the personhood/chain layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HumanSnapshot {
    pub root: [u8; 32],
    pub eligible_count: u64,
}

/// One aggregate equal-share instruction. A chain can validate membership
/// claims against `snapshot.root` without materializing the population here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HumanSharePlan {
    pub epoch: u64,
    pub snapshot: HumanSnapshot,
    pub cap: Amount,
    pub per_human: Amount,
    pub issued: Amount,
    pub unissued_remainder: Amount,
    pub vesting_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScalableEpochRequest {
    pub epoch: u64,
    pub duration_ms: u64,
    pub opening_circulating: Amount,
    pub human_snapshot: HumanSnapshot,
    pub service: Vec<Allocation>,
    pub treasury: Vec<Allocation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScalableEpochPlan {
    pub epoch: u64,
    pub duration_ms: u64,
    pub opening_circulating: Amount,
    pub human: HumanSharePlan,
    pub service_cap: Amount,
    pub treasury_cap: Amount,
    pub total_cap: Amount,
    pub service_issued: Amount,
    pub treasury_issued: Amount,
    pub total_issued: Amount,
    /// Service and treasury grants only. Human Share remains aggregate.
    pub optional_grants: Vec<VestingGrant>,
}

impl ScalableEpochPlan {
    pub fn commitment(&self) -> Multihash {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"mini-economy/scalable-epoch-plan/v1");
        bytes.extend_from_slice(&self.epoch.to_be_bytes());
        bytes.extend_from_slice(&self.duration_ms.to_be_bytes());
        bytes.extend_from_slice(&self.opening_circulating.as_micro().to_be_bytes());
        bytes.extend_from_slice(&self.human.snapshot.root);
        bytes.extend_from_slice(&self.human.snapshot.eligible_count.to_be_bytes());
        for amount in [
            self.human.cap,
            self.human.per_human,
            self.human.issued,
            self.human.unissued_remainder,
            self.service_cap,
            self.treasury_cap,
            self.total_cap,
            self.service_issued,
            self.treasury_issued,
            self.total_issued,
        ] {
            bytes.extend_from_slice(&amount.as_micro().to_be_bytes());
        }
        bytes.extend_from_slice(&(self.optional_grants.len() as u64).to_be_bytes());
        put_grants(&mut bytes, &self.optional_grants);
        Multihash::of(HashAlgorithm::Blake3, &bytes)
    }
}

pub fn plan_human_share(
    epoch: u64,
    duration_ms: u64,
    opening_circulating: Amount,
    snapshot: HumanSnapshot,
    policy: &IssuancePolicy,
) -> Result<HumanSharePlan> {
    policy.validate()?;
    if duration_ms == 0 || duration_ms > YEAR_MS {
        return Err(EconomyError::InvalidDuration);
    }
    if snapshot.eligible_count == 0 || snapshot.root == [0; 32] {
        return Err(EconomyError::InvalidSnapshot);
    }
    let cap = prorated(opening_circulating, policy.human_floor_ppm, duration_ms)?;
    let per_human = Amount::from_micro(cap.as_micro() / snapshot.eligible_count as u128);
    let issued = Amount::from_micro(
        per_human
            .as_micro()
            .checked_mul(snapshot.eligible_count as u128)
            .ok_or(EconomyError::Overflow)?,
    );
    Ok(HumanSharePlan {
        epoch,
        snapshot,
        cap,
        per_human,
        issued,
        unissued_remainder: cap.checked_sub(issued)?,
        vesting_ms: policy.human_vesting_ms,
    })
}

pub fn plan_scalable_epoch(
    request: &ScalableEpochRequest,
    policy: &IssuancePolicy,
) -> Result<ScalableEpochPlan> {
    policy.validate()?;
    let human = plan_human_share(
        request.epoch,
        request.duration_ms,
        request.opening_circulating,
        request.human_snapshot,
        policy,
    )?;
    let service_cap = prorated(
        request.opening_circulating,
        policy.service_ceiling_ppm,
        request.duration_ms,
    )?;
    let treasury_cap = prorated(
        request.opening_circulating,
        policy.treasury_ceiling_ppm,
        request.duration_ms,
    )?;
    let total_cap = prorated(
        request.opening_circulating,
        policy.total_ceiling_ppm,
        request.duration_ms,
    )?;
    let service_issued = validate_allocations(&request.service, service_cap)?;
    let treasury_issued = validate_allocations(&request.treasury, treasury_cap)?;
    let total_issued = human
        .issued
        .checked_add(service_issued)?
        .checked_add(treasury_issued)?;
    if total_issued > total_cap {
        return Err(EconomyError::TotalExceeded);
    }
    let mut optional_grants = Vec::with_capacity(request.service.len() + request.treasury.len());
    optional_grants.extend(request.service.iter().map(|allocation| VestingGrant {
        beneficiary: allocation.beneficiary.clone(),
        channel: Channel::Service,
        amount: allocation.amount,
        vesting_ms: 0,
    }));
    optional_grants.extend(request.treasury.iter().map(|allocation| VestingGrant {
        beneficiary: allocation.beneficiary.clone(),
        channel: Channel::TreasuryContribution,
        amount: allocation.amount,
        vesting_ms: policy.treasury_vesting_ms,
    }));
    Ok(ScalableEpochPlan {
        epoch: request.epoch,
        duration_ms: request.duration_ms,
        opening_circulating: request.opening_circulating,
        human,
        service_cap,
        treasury_cap,
        total_cap,
        service_issued,
        treasury_issued,
        total_issued,
        optional_grants,
    })
}

pub fn plan_epoch(request: &EpochRequest, policy: &IssuancePolicy) -> Result<EpochPlan> {
    policy.validate()?;
    if request.duration_ms == 0 || request.duration_ms > YEAR_MS {
        return Err(EconomyError::InvalidDuration);
    }
    let humans = canonical_humans(&request.eligible_humans)?;
    let human_cap = prorated(
        request.opening_circulating,
        policy.human_floor_ppm,
        request.duration_ms,
    )?;
    let service_cap = prorated(
        request.opening_circulating,
        policy.service_ceiling_ppm,
        request.duration_ms,
    )?;
    let treasury_cap = prorated(
        request.opening_circulating,
        policy.treasury_ceiling_ppm,
        request.duration_ms,
    )?;
    let total_cap = prorated(
        request.opening_circulating,
        policy.total_ceiling_ppm,
        request.duration_ms,
    )?;

    let equal_human = Amount::from_micro(human_cap.as_micro() / humans.len() as u128);
    let human_issued = Amount::from_micro(
        equal_human
            .as_micro()
            .checked_mul(humans.len() as u128)
            .ok_or(EconomyError::Overflow)?,
    );
    let service_issued = validate_allocations(&request.service, service_cap)?;
    let treasury_issued = validate_allocations(&request.treasury, treasury_cap)?;
    let total_issued = human_issued
        .checked_add(service_issued)?
        .checked_add(treasury_issued)?;
    if total_issued > total_cap {
        return Err(EconomyError::TotalExceeded);
    }

    let mut grants =
        Vec::with_capacity(humans.len() + request.service.len() + request.treasury.len());
    grants.extend(humans.into_iter().map(|beneficiary| VestingGrant {
        beneficiary,
        channel: Channel::HumanShare,
        amount: equal_human,
        vesting_ms: policy.human_vesting_ms,
    }));
    grants.extend(request.service.iter().map(|allocation| VestingGrant {
        beneficiary: allocation.beneficiary.clone(),
        channel: Channel::Service,
        amount: allocation.amount,
        // Service maturity remains controlled by the evidence-specific reward
        // policy. Zero here means this envelope does not invent a new delay.
        vesting_ms: 0,
    }));
    grants.extend(request.treasury.iter().map(|allocation| VestingGrant {
        beneficiary: allocation.beneficiary.clone(),
        channel: Channel::TreasuryContribution,
        amount: allocation.amount,
        vesting_ms: policy.treasury_vesting_ms,
    }));
    Ok(EpochPlan {
        epoch: request.epoch,
        duration_ms: request.duration_ms,
        opening_circulating: request.opening_circulating,
        human_cap,
        service_cap,
        treasury_cap,
        total_cap,
        human_issued,
        service_issued,
        treasury_issued,
        total_issued,
        grants,
    })
}

fn canonical_humans(humans: &[String]) -> Result<Vec<String>> {
    if humans.is_empty() {
        return Err(EconomyError::EmptyEligibleSet);
    }
    let unique: BTreeSet<String> = humans.iter().cloned().collect();
    if unique.len() != humans.len() || unique.iter().any(|human| human.is_empty()) {
        return Err(EconomyError::DuplicateBeneficiary);
    }
    Ok(unique.into_iter().collect())
}

fn validate_allocations(allocations: &[Allocation], cap: Amount) -> Result<Amount> {
    let mut seen = BTreeSet::new();
    let mut total = Amount::ZERO;
    for allocation in allocations {
        if allocation.beneficiary.is_empty() || !seen.insert(&allocation.beneficiary) {
            return Err(EconomyError::DuplicateBeneficiary);
        }
        total = total.checked_add(allocation.amount)?;
    }
    if total > cap {
        return Err(EconomyError::ChannelExceeded);
    }
    Ok(total)
}

fn put_grants(bytes: &mut Vec<u8>, grants: &[VestingGrant]) {
    for grant in grants {
        bytes.extend_from_slice(&(grant.beneficiary.len() as u64).to_be_bytes());
        bytes.extend_from_slice(grant.beneficiary.as_bytes());
        bytes.push(match grant.channel {
            Channel::HumanShare => 0,
            Channel::Service => 1,
            Channel::TreasuryContribution => 2,
        });
        bytes.extend_from_slice(&grant.amount.as_micro().to_be_bytes());
        bytes.extend_from_slice(&grant.vesting_ms.to_be_bytes());
    }
}

fn prorated(supply: Amount, ppm: u32, duration_ms: u64) -> Result<Amount> {
    let mut factors = [supply.as_micro(), ppm as u128, duration_ms as u128];
    let mut denominator = MILLION * YEAR_MS as u128;
    for factor in &mut factors {
        let divisor = gcd(*factor, denominator);
        *factor /= divisor;
        denominator /= divisor;
    }
    let numerator = factors[0]
        .checked_mul(factors[1])
        .and_then(|value| value.checked_mul(factors[2]))
        .ok_or(EconomyError::Overflow)?;
    Ok(Amount::from_micro(numerator / denominator))
}

fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}
