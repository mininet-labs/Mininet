use mini_crypto::{HashAlgorithm, Multihash};

use crate::{
    Amount, Channel, EconomyError, HumanSnapshot, IssuancePolicy, Result, ScalableEpochPlan,
    VestingGrant, YEAR_MS,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VestingSubject {
    HumanSnapshot(HumanSnapshot),
    Beneficiary(String),
}

/// One supply position created by finalized issuance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VestingPosition {
    pub epoch: u64,
    pub subject: VestingSubject,
    pub channel: Channel,
    pub amount: Amount,
    pub starts_at_policy_ms: u128,
    pub duration_ms: u64,
}

impl VestingPosition {
    pub fn vested_at(&self, policy_ms: u128) -> Result<Amount> {
        if self.duration_ms == 0 {
            return Ok(self.amount);
        }
        let elapsed = policy_ms.saturating_sub(self.starts_at_policy_ms);
        if elapsed >= self.duration_ms as u128 {
            return Ok(self.amount);
        }
        let denominator = self.duration_ms as u128;
        let amount = self.amount.as_micro();
        // Exact floor(amount * elapsed / duration) without overflowing an
        // intermediate product when amount is near u128::MAX.
        let whole = (amount / denominator)
            .checked_mul(elapsed)
            .ok_or(EconomyError::Overflow)?;
        let fractional = (amount % denominator)
            .checked_mul(elapsed)
            .ok_or(EconomyError::Overflow)?
            / denominator;
        let vested = whole
            .checked_add(fractional)
            .ok_or(EconomyError::Overflow)?;
        Ok(Amount::from_micro(vested))
    }
}

/// Deterministic aggregate MINI supply and vesting state.
///
/// Policy time is the sum of finalized epoch durations. It is deliberately
/// independent of proposer timestamps and local wall clocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonetaryLedger {
    genesis_circulating: Amount,
    total_issued: Amount,
    policy_time_ms: u128,
    last_epoch: Option<u64>,
    positions: Vec<VestingPosition>,
}

impl MonetaryLedger {
    pub fn new(genesis_circulating: Amount) -> Self {
        Self {
            genesis_circulating,
            total_issued: Amount::ZERO,
            policy_time_ms: 0,
            last_epoch: None,
            positions: Vec::new(),
        }
    }

    pub fn policy_time_ms(&self) -> u128 {
        self.policy_time_ms
    }

    pub fn last_epoch(&self) -> Option<u64> {
        self.last_epoch
    }

    pub fn total_issued(&self) -> Amount {
        self.total_issued
    }

    pub fn total_supply(&self) -> Result<Amount> {
        self.genesis_circulating.checked_add(self.total_issued)
    }

    pub fn circulating_supply(&self) -> Result<Amount> {
        let mut circulating = self.genesis_circulating;
        for position in &self.positions {
            circulating = circulating.checked_add(position.vested_at(self.policy_time_ms)?)?;
        }
        Ok(circulating)
    }

    pub fn locked_supply(&self) -> Result<Amount> {
        self.total_supply()?.checked_sub(self.circulating_supply()?)
    }

    pub fn positions(&self) -> &[VestingPosition] {
        &self.positions
    }

    /// Validate and apply one next epoch atomically.
    pub fn apply_epoch(&self, plan: &ScalableEpochPlan, policy: &IssuancePolicy) -> Result<Self> {
        let expected_epoch = self.last_epoch.map_or(0, |epoch| epoch.saturating_add(1));
        if plan.epoch != expected_epoch {
            return Err(EconomyError::UnexpectedEpoch);
        }
        if plan.duration_ms == 0 || plan.duration_ms > YEAR_MS {
            return Err(EconomyError::InvalidDuration);
        }
        if plan.opening_circulating != self.circulating_supply()? {
            return Err(EconomyError::OpeningSupplyMismatch);
        }
        validate_plan_shape(plan, policy)?;

        let end = self
            .policy_time_ms
            .checked_add(plan.duration_ms as u128)
            .ok_or(EconomyError::Overflow)?;
        let mut next = self.clone();
        next.total_issued = next.total_issued.checked_add(plan.total_issued)?;
        next.policy_time_ms = end;
        next.last_epoch = Some(plan.epoch);
        next.positions.push(VestingPosition {
            epoch: plan.epoch,
            subject: VestingSubject::HumanSnapshot(plan.human.snapshot),
            channel: Channel::HumanShare,
            amount: plan.human.issued,
            starts_at_policy_ms: end,
            duration_ms: plan.human.vesting_ms,
        });
        next.positions
            .extend(plan.optional_grants.iter().map(|grant| VestingPosition {
                epoch: plan.epoch,
                subject: VestingSubject::Beneficiary(grant.beneficiary.clone()),
                channel: grant.channel,
                amount: grant.amount,
                starts_at_policy_ms: end,
                duration_ms: grant.vesting_ms,
            }));
        Ok(next)
    }

    pub fn commitment(&self) -> Multihash {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"mini-economy/monetary-ledger/v1");
        bytes.extend_from_slice(&self.genesis_circulating.as_micro().to_be_bytes());
        bytes.extend_from_slice(&self.total_issued.as_micro().to_be_bytes());
        bytes.extend_from_slice(&self.policy_time_ms.to_be_bytes());
        match self.last_epoch {
            Some(epoch) => {
                bytes.push(1);
                bytes.extend_from_slice(&epoch.to_be_bytes());
            }
            None => bytes.push(0),
        }
        bytes.extend_from_slice(&(self.positions.len() as u64).to_be_bytes());
        for position in &self.positions {
            bytes.extend_from_slice(&position.epoch.to_be_bytes());
            match &position.subject {
                VestingSubject::HumanSnapshot(snapshot) => {
                    bytes.push(0);
                    bytes.extend_from_slice(&snapshot.root);
                    bytes.extend_from_slice(&snapshot.eligible_count.to_be_bytes());
                }
                VestingSubject::Beneficiary(beneficiary) => {
                    bytes.push(1);
                    put_bytes(&mut bytes, beneficiary.as_bytes());
                }
            }
            bytes.push(channel_tag(position.channel));
            bytes.extend_from_slice(&position.amount.as_micro().to_be_bytes());
            bytes.extend_from_slice(&position.starts_at_policy_ms.to_be_bytes());
            bytes.extend_from_slice(&position.duration_ms.to_be_bytes());
        }
        Multihash::of(HashAlgorithm::Blake3, &bytes)
    }
}

fn validate_plan_shape(plan: &ScalableEpochPlan, policy: &IssuancePolicy) -> Result<()> {
    let expected = crate::plan_scalable_epoch(
        &crate::ScalableEpochRequest {
            epoch: plan.epoch,
            duration_ms: plan.duration_ms,
            opening_circulating: plan.opening_circulating,
            human_snapshot: plan.human.snapshot,
            service: allocations(&plan.optional_grants, Channel::Service),
            treasury: allocations(&plan.optional_grants, Channel::TreasuryContribution),
        },
        policy,
    )?;
    if &expected != plan {
        return Err(EconomyError::InvalidEpochPlan);
    }
    Ok(())
}

fn allocations(grants: &[VestingGrant], channel: Channel) -> Vec<crate::Allocation> {
    grants
        .iter()
        .filter(|grant| grant.channel == channel)
        .map(|grant| crate::Allocation {
            beneficiary: grant.beneficiary.clone(),
            amount: grant.amount,
        })
        .collect()
}

fn channel_tag(channel: Channel) -> u8 {
    match channel {
        Channel::HumanShare => 0,
        Channel::Service => 1,
        Channel::TreasuryContribution => 2,
    }
}

fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}
