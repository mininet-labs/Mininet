//! Canonical, bounded encoding for finalized scalable monetary epoch plans.
//!
//! Consensus must carry the exact plan it finalizes. A digest-only encoding is
//! insufficient for catch-up, while ad-hoc field duplication in the consensus
//! crate would let issuance and transport disagree about what was committed.

use crate::{
    Amount, Channel, EconomyError, HumanSharePlan, HumanSnapshot, Result, ScalableEpochPlan,
    VestingGrant,
};

const DOMAIN: &[u8] = b"mini-economy/scalable-epoch-plan-wire/v1";

/// Hard allocation/CPU ceiling for optional service and treasury grants in one
/// epoch transition.
pub const MAX_SCALABLE_EPOCH_GRANTS: usize = 1_024;

/// Hard UTF-8 byte ceiling for one grant beneficiary identifier.
pub const MAX_EPOCH_BENEFICIARY_BYTES: usize = 1_024;

/// Maximum standalone encoded epoch plan. This stays well below the bearer
/// frame ceiling so a block retains room for claims, headers, and finality.
pub const MAX_SCALABLE_EPOCH_PLAN_BYTES: usize = 2 * 1024 * 1024;

impl ScalableEpochPlan {
    /// Exact versioned bytes used by both [`Self::commitment`] and settlement
    /// block-body hashing. Every field is represented, including the nested
    /// human epoch and vesting duration.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(DOMAIN);
        put_u64(&mut out, self.epoch);
        put_u64(&mut out, self.duration_ms);
        put_amount(&mut out, self.opening_circulating);
        put_u64(&mut out, self.human.epoch);
        out.extend_from_slice(&self.human.snapshot.root);
        put_u64(&mut out, self.human.snapshot.eligible_count);
        for amount in [
            self.human.cap,
            self.human.per_human,
            self.human.issued,
            self.human.unissued_remainder,
        ] {
            put_amount(&mut out, amount);
        }
        put_u64(&mut out, self.human.vesting_ms);
        for amount in [
            self.service_cap,
            self.treasury_cap,
            self.total_cap,
            self.service_issued,
            self.treasury_issued,
            self.total_issued,
        ] {
            put_amount(&mut out, amount);
        }
        put_u64(&mut out, self.optional_grants.len() as u64);
        for grant in &self.optional_grants {
            put_u64(&mut out, grant.beneficiary.len() as u64);
            out.extend_from_slice(grant.beneficiary.as_bytes());
            out.push(match grant.channel {
                Channel::HumanShare => 0,
                Channel::Service => 1,
                Channel::TreasuryContribution => 2,
            });
            put_amount(&mut out, grant.amount);
            put_u64(&mut out, grant.vesting_ms);
        }
        out
    }

    /// Bounded canonical bytes suitable for an untrusted wire boundary.
    pub fn to_wire_bytes(&self) -> Result<Vec<u8>> {
        if self.optional_grants.len() > MAX_SCALABLE_EPOCH_GRANTS
            || self.optional_grants.iter().any(|grant| {
                grant.beneficiary.is_empty()
                    || grant.beneficiary.len() > MAX_EPOCH_BENEFICIARY_BYTES
            })
        {
            return Err(EconomyError::LimitExceeded);
        }
        let bytes = self.canonical_bytes();
        if bytes.len() > MAX_SCALABLE_EPOCH_PLAN_BYTES {
            return Err(EconomyError::LimitExceeded);
        }
        Ok(bytes)
    }

    /// Decode one canonical plan with all counts and byte lengths bounded
    /// before allocation. Economic validity is still re-derived by
    /// `MonetaryLedger::apply_epoch`; decoding grants no issuance authority.
    pub fn from_wire_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_SCALABLE_EPOCH_PLAN_BYTES {
            return Err(EconomyError::LimitExceeded);
        }
        let mut reader = Reader::new(bytes);
        if reader.take(DOMAIN.len())? != DOMAIN {
            return Err(EconomyError::MalformedEncoding);
        }
        let epoch = reader.u64()?;
        let duration_ms = reader.u64()?;
        let opening_circulating = reader.amount()?;
        let human_epoch = reader.u64()?;
        let snapshot = HumanSnapshot {
            root: reader.array_32()?,
            eligible_count: reader.u64()?,
        };
        let human = HumanSharePlan {
            epoch: human_epoch,
            snapshot,
            cap: reader.amount()?,
            per_human: reader.amount()?,
            issued: reader.amount()?,
            unissued_remainder: reader.amount()?,
            vesting_ms: reader.u64()?,
        };
        let service_cap = reader.amount()?;
        let treasury_cap = reader.amount()?;
        let total_cap = reader.amount()?;
        let service_issued = reader.amount()?;
        let treasury_issued = reader.amount()?;
        let total_issued = reader.amount()?;
        let count = usize::try_from(reader.u64()?).map_err(|_| EconomyError::LimitExceeded)?;
        if count > MAX_SCALABLE_EPOCH_GRANTS {
            return Err(EconomyError::LimitExceeded);
        }
        let mut optional_grants = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            let beneficiary_len =
                usize::try_from(reader.u64()?).map_err(|_| EconomyError::LimitExceeded)?;
            if beneficiary_len == 0 || beneficiary_len > MAX_EPOCH_BENEFICIARY_BYTES {
                return Err(EconomyError::LimitExceeded);
            }
            let beneficiary = String::from_utf8(reader.take(beneficiary_len)?.to_vec())
                .map_err(|_| EconomyError::MalformedEncoding)?;
            let channel = match reader.u8()? {
                0 => Channel::HumanShare,
                1 => Channel::Service,
                2 => Channel::TreasuryContribution,
                _ => return Err(EconomyError::MalformedEncoding),
            };
            optional_grants.push(VestingGrant {
                beneficiary,
                channel,
                amount: reader.amount()?,
                vesting_ms: reader.u64()?,
            });
        }
        if !reader.finished() {
            return Err(EconomyError::MalformedEncoding);
        }
        let plan = Self {
            epoch,
            duration_ms,
            opening_circulating,
            human,
            service_cap,
            treasury_cap,
            total_cap,
            service_issued,
            treasury_issued,
            total_issued,
            optional_grants,
        };
        if plan.to_wire_bytes()?.as_slice() != bytes {
            return Err(EconomyError::MalformedEncoding);
        }
        Ok(plan)
    }
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn put_amount(out: &mut Vec<u8>, amount: Amount) {
    out.extend_from_slice(&amount.as_micro().to_be_bytes());
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(EconomyError::MalformedEncoding)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(EconomyError::MalformedEncoding)?;
        self.position = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u64(&mut self) -> Result<u64> {
        let mut bytes = [0; 8];
        bytes.copy_from_slice(self.take(8)?);
        Ok(u64::from_be_bytes(bytes))
    }

    fn amount(&mut self) -> Result<Amount> {
        let mut bytes = [0; 16];
        bytes.copy_from_slice(self.take(16)?);
        Ok(Amount::from_micro(u128::from_be_bytes(bytes)))
    }

    fn array_32(&mut self) -> Result<[u8; 32]> {
        let mut bytes = [0; 32];
        bytes.copy_from_slice(self.take(32)?);
        Ok(bytes)
    }

    fn finished(&self) -> bool {
        self.position == self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> ScalableEpochPlan {
        ScalableEpochPlan {
            epoch: 7,
            duration_ms: 100,
            opening_circulating: Amount::from_micro(1_000),
            human: HumanSharePlan {
                epoch: 7,
                snapshot: HumanSnapshot {
                    root: [3; 32],
                    eligible_count: 10,
                },
                cap: Amount::from_micro(20),
                per_human: Amount::from_micro(2),
                issued: Amount::from_micro(20),
                unissued_remainder: Amount::ZERO,
                vesting_ms: 50,
            },
            service_cap: Amount::from_micro(7),
            treasury_cap: Amount::from_micro(3),
            total_cap: Amount::from_micro(30),
            service_issued: Amount::from_micro(7),
            treasury_issued: Amount::from_micro(3),
            total_issued: Amount::from_micro(30),
            optional_grants: vec![VestingGrant {
                beneficiary: "service-a".to_string(),
                channel: Channel::Service,
                amount: Amount::from_micro(7),
                vesting_ms: 0,
            }],
        }
    }

    #[test]
    fn exact_plan_round_trips() {
        let plan = plan();
        let bytes = plan.to_wire_bytes().unwrap();
        assert_eq!(ScalableEpochPlan::from_wire_bytes(&bytes).unwrap(), plan);
    }

    #[test]
    fn nested_human_fields_are_commitment_bearing() {
        let plan = plan();
        let mut changed = plan.clone();
        changed.human.vesting_ms += 1;
        assert_ne!(plan.commitment(), changed.commitment());
        changed = plan.clone();
        changed.human.epoch += 1;
        assert_ne!(plan.commitment(), changed.commitment());
    }

    #[test]
    fn malformed_and_oversized_inputs_fail_closed() {
        let plan = plan();
        let bytes = plan.to_wire_bytes().unwrap();
        for cut in 0..bytes.len() {
            assert!(ScalableEpochPlan::from_wire_bytes(&bytes[..cut]).is_err());
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert_eq!(
            ScalableEpochPlan::from_wire_bytes(&trailing),
            Err(EconomyError::MalformedEncoding)
        );

        let mut oversized = plan;
        oversized.optional_grants = vec![
            VestingGrant {
                beneficiary: "a".to_string(),
                channel: Channel::Service,
                amount: Amount::ZERO,
                vesting_ms: 0,
            };
            MAX_SCALABLE_EPOCH_GRANTS + 1
        ];
        assert_eq!(oversized.to_wire_bytes(), Err(EconomyError::LimitExceeded));
    }
}
