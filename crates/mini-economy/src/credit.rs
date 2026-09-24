//! Experimental Human Share credit limits and repayment accounting.
//!
//! A signed online permit authorizes a finite TOTAL of offline IOUs, not a
//! guarantee or a mint. Every still-live allowance and every unpaid debt counts
//! against the same human's cap, across devices and permit renewals. Recipients
//! have a receivable until future released Human Share actually repays it.
//!
//! The host must verify issuer authority, unique-human eligibility and subject
//! binding, supply canonical heights and actual released share, persist state,
//! and atomically execute the returned private transfers. This bounded in-memory
//! prototype does none of those jobs and cannot be activated by constructing it.

use crate::{Amount, MILLION};
use mini_settlement::credit_permit::{VerifiedCreditIou, VerifiedCreditPermit};
use std::collections::BTreeMap;

pub type CreditSubject = [u8; 32];
pub type PermitId = [u8; 32];

/// Collectively chosen policy, deliberately without a production default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreditPolicy {
    pub network_id: [u8; 32],
    pub revision: [u8; 32],
    pub max_debt_per_human: u64,
    /// Fraction of actually released share available for repayment (0 < ppm <= 1m).
    pub repayment_ppm: u32,
    pub max_permit_height_span: u64,
    pub max_accounts: usize,
    pub max_permits: usize,
    pub max_ious: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreditError {
    InvalidPolicy,
    InvalidTerms,
    UnknownAccount,
    UnknownPermit,
    LimitExceeded,
    PermitConflict,
    IouConflict,
    OutsideWindow,
    NotExpired,
    ReplayMismatch,
    InvalidEpoch,
    Arithmetic,
}
impl core::fmt::Display for CreditError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "credit accounting refused: {self:?}")
    }
}
impl std::error::Error for CreditError {}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Position {
    cap: u64,
    debt: u64,
    unused_authorized: u64,
    repayment_remainder: u128,
    last_share: Option<(u64, ShareRepayment)>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Permit {
    subject: CreditSubject,
    verified: VerifiedCreditPermit,
    used: u64,
    expired: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Iou {
    subject: CreditSubject,
    digest: [u8; 32],
    payee: [u8; 32],
    remaining: u64,
}

/// One private transfer instruction, never a public recipient/amount record.
#[derive(Clone, PartialEq, Eq)]
pub struct CreditRepayment {
    pub iou_digest: [u8; 32],
    pub payee_commitment: [u8; 32],
    pub amount_micro: u64,
}
impl core::fmt::Debug for CreditRepayment {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("CreditRepayment(<private>)")
    }
}

/// Authorized release split. No new issuance: gross = repayments + remainder.
#[derive(Clone, PartialEq, Eq)]
pub struct ShareRepayment {
    pub gross_released: Amount,
    pub recipient_available: Amount,
    pub remaining_debt_micro: u64,
    pub repayments: Vec<CreditRepayment>,
}
impl core::fmt::Debug for ShareRepayment {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ShareRepayment")
            .field("payments", &self.repayments.len())
            .finish_non_exhaustive()
    }
}

/// Finite state with no pruning/reuse of old IDs. A host must checkpoint/archive
/// safely before configured capacity is reached; exhaustion fails closed.
#[derive(Clone, PartialEq, Eq)]
pub struct HumanShareCredit {
    policy: CreditPolicy,
    accounts: BTreeMap<CreditSubject, Position>,
    permits: BTreeMap<PermitId, Permit>,
    ious: BTreeMap<(PermitId, u64), Iou>,
    canonical_order: Vec<(PermitId, u64)>,
}
impl core::fmt::Debug for HumanShareCredit {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HumanShareCredit")
            .field("accounts", &self.accounts.len())
            .field("permits", &self.permits.len())
            .field("ious", &self.ious.len())
            .finish_non_exhaustive()
    }
}

impl HumanShareCredit {
    pub fn new(policy: CreditPolicy) -> Result<Self, CreditError> {
        if policy.revision == [0; 32]
            || policy.max_debt_per_human == 0
            || policy.repayment_ppm == 0
            || u128::from(policy.repayment_ppm) > MILLION
            || policy.max_permit_height_span == 0
            || policy.max_accounts == 0
            || policy.max_permits == 0
            || policy.max_ious == 0
        {
            return Err(CreditError::InvalidPolicy);
        }
        Ok(Self {
            policy,
            accounts: BTreeMap::new(),
            permits: BTreeMap::new(),
            ious: BTreeMap::new(),
            canonical_order: Vec::new(),
        })
    }

    /// Host-authorized eligibility admission. One unique person maps to one
    /// subject across ALL roots/devices; this API cannot prove that mapping.
    /// Re-enrollment cannot erase debts or obtain another allowance.
    pub fn enroll(
        &mut self,
        subject: CreditSubject,
        approved_cap_micro: u64,
    ) -> Result<(), CreditError> {
        if subject == [0; 32]
            || approved_cap_micro == 0
            || approved_cap_micro > self.policy.max_debt_per_human
        {
            return Err(CreditError::InvalidTerms);
        }
        if self.accounts.contains_key(&subject) || self.accounts.len() >= self.policy.max_accounts {
            return Err(CreditError::LimitExceeded);
        }
        self.accounts.insert(
            subject,
            Position {
                cap: approved_cap_micro,
                debt: 0,
                unused_authorized: 0,
                repayment_remainder: 0,
                last_share: None,
            },
        );
        Ok(())
    }

    pub fn debt_micro(&self, subject: &CreditSubject) -> Option<u64> {
        self.accounts.get(subject).map(|p| p.debt)
    }
    pub fn new_allowance_capacity(&self, subject: &CreditSubject) -> Result<u64, CreditError> {
        let p = self
            .accounts
            .get(subject)
            .ok_or(CreditError::UnknownAccount)?;
        p.cap
            .checked_sub(p.debt)
            .and_then(|v| v.checked_sub(p.unused_authorized))
            .ok_or(CreditError::Arithmetic)
    }
    pub fn permit_remaining(&self, id: &PermitId) -> Option<u64> {
        self.permits.get(id).map(|p| {
            if p.expired {
                0
            } else {
                p.verified.terms().limit_micro - p.used
            }
        })
    }

    /// Register the exact online-issued signature before offline use. Refresh is
    /// another permit; it does not reset old budgets, erase debt, or invalidate
    /// still-valid promises. Unused limits across devices consume one shared cap.
    pub fn register_permit(
        &mut self,
        subject: CreditSubject,
        permit: VerifiedCreditPermit,
        canonical_height: u64,
    ) -> Result<bool, CreditError> {
        let terms = permit.terms();
        if terms.network_id != self.policy.network_id
            || terms.policy_revision != self.policy.revision
            || terms.valid_through_height - terms.issued_height > self.policy.max_permit_height_span
        {
            return Err(CreditError::InvalidTerms);
        }
        if let Some(old) = self.permits.get(permit.id()) {
            return if old.subject == subject && old.verified == permit {
                Ok(false)
            } else {
                Err(CreditError::PermitConflict)
            };
        }
        // Registration is an online canonical event, not a backdated signature.
        if canonical_height != terms.issued_height {
            return Err(CreditError::OutsideWindow);
        }
        if self.permits.len() >= self.policy.max_permits
            || terms.limit_micro > self.new_allowance_capacity(&subject)?
        {
            return Err(CreditError::LimitExceeded);
        }
        let position = self
            .accounts
            .get_mut(&subject)
            .ok_or(CreditError::UnknownAccount)?;
        position.unused_authorized = position
            .unused_authorized
            .checked_add(terms.limit_micro)
            .ok_or(CreditError::Arithmetic)?;
        self.permits.insert(
            *permit.id(),
            Permit {
                subject,
                verified: permit,
                used: 0,
                expired: false,
            },
        );
        Ok(true)
    }

    /// Record a debt in canonical arrival/order, NOT a settled payment. A copied
    /// permit cannot admit aggregate IOUs above its total allowance. Sequence
    /// reuse with different signed contents is a conflict; exact replay is inert.
    pub fn record_iou(
        &mut self,
        iou: &VerifiedCreditIou,
        canonical_height: u64,
    ) -> Result<bool, CreditError> {
        let terms = iou.terms();
        let slot = (terms.permit_id, terms.sequence);
        if let Some(prior) = self.ious.get(&slot) {
            return if &prior.digest == iou.digest() {
                Ok(false)
            } else {
                Err(CreditError::IouConflict)
            };
        }
        let permit = self
            .permits
            .get(&terms.permit_id)
            .ok_or(CreditError::UnknownPermit)?;
        let authorized = permit.verified.terms();
        if permit.expired
            || canonical_height < authorized.issued_height
            || canonical_height > authorized.valid_through_height
        {
            return Err(CreditError::OutsideWindow);
        }
        if self.ious.len() >= self.policy.max_ious {
            return Err(CreditError::LimitExceeded);
        }
        let used = permit
            .used
            .checked_add(terms.amount_micro)
            .ok_or(CreditError::Arithmetic)?;
        if used > authorized.limit_micro {
            return Err(CreditError::LimitExceeded);
        }
        let subject = permit.subject;
        let p = self
            .accounts
            .get(&subject)
            .ok_or(CreditError::UnknownAccount)?;
        let debt = p
            .debt
            .checked_add(terms.amount_micro)
            .ok_or(CreditError::Arithmetic)?;
        let unused = p
            .unused_authorized
            .checked_sub(terms.amount_micro)
            .ok_or(CreditError::Arithmetic)?;
        self.permits
            .get_mut(&terms.permit_id)
            .expect("checked")
            .used = used;
        let p = self.accounts.get_mut(&subject).expect("checked");
        p.debt = debt;
        p.unused_authorized = unused;
        self.ious.insert(
            slot,
            Iou {
                subject,
                digest: *iou.digest(),
                payee: terms.payee_commitment,
                remaining: terms.amount_micro,
            },
        );
        self.canonical_order.push(slot);
        Ok(true)
    }

    /// Expire only against canonical height. Already-recorded debt survives.
    pub fn expire_permit(
        &mut self,
        id: &PermitId,
        canonical_height: u64,
    ) -> Result<(), CreditError> {
        let permit = self.permits.get(id).ok_or(CreditError::UnknownPermit)?;
        if permit.expired {
            return Ok(());
        }
        if canonical_height <= permit.verified.terms().valid_through_height {
            return Err(CreditError::NotExpired);
        }
        let remaining = permit.verified.terms().limit_micro - permit.used;
        let subject = permit.subject;
        let unused = self.accounts[&subject]
            .unused_authorized
            .checked_sub(remaining)
            .ok_or(CreditError::Arithmetic)?;
        self.accounts
            .get_mut(&subject)
            .expect("checked")
            .unused_authorized = unused;
        self.permits.get_mut(id).expect("checked").expired = true;
        Ok(())
    }

    /// Repay oldest canonically accepted IOUs from actual released/vested share.
    /// Returns (split, newly_applied). An exact replay MUST NOT transfer twice.
    /// The host must commit this state and the corresponding private payments in
    /// ONE atomic canonical transition. No projected or unvested income is money.
    pub fn release_share(
        &mut self,
        subject: &CreditSubject,
        release_epoch: u64,
        gross: Amount,
    ) -> Result<(ShareRepayment, bool), CreditError> {
        let position = self
            .accounts
            .get(subject)
            .ok_or(CreditError::UnknownAccount)?;
        if let Some((last, split)) = &position.last_share {
            if release_epoch == *last {
                return if gross == split.gross_released {
                    Ok((split.clone(), false))
                } else {
                    Err(CreditError::ReplayMismatch)
                };
            }
            if release_epoch < *last {
                return Err(CreditError::InvalidEpoch);
            }
        }
        let n = gross.as_micro();
        let ppm = u128::from(self.policy.repayment_ppm);
        let fractional = (n % MILLION) * ppm + position.repayment_remainder;
        let permitted = (n / MILLION) * ppm + fractional / MILLION;
        let repay = permitted.min(u128::from(position.debt)) as u64;
        let remaining_debt = position.debt - repay;
        let mut budget = repay;
        // Stage first: even an unexpected invariant failure cannot partially pay.
        let mut next = self.clone();
        let mut repayments = Vec::new();
        for slot in &self.canonical_order {
            if budget == 0 {
                break;
            }
            let iou = next.ious.get_mut(slot).expect("retained order");
            if &iou.subject != subject || iou.remaining == 0 {
                continue;
            }
            let amount = budget.min(iou.remaining);
            iou.remaining -= amount;
            budget -= amount;
            repayments.push(CreditRepayment {
                iou_digest: iou.digest,
                payee_commitment: iou.payee,
                amount_micro: amount,
            });
        }
        if budget != 0 {
            return Err(CreditError::Arithmetic);
        }
        let split = ShareRepayment {
            gross_released: gross,
            recipient_available: Amount::from_micro(n - u128::from(repay)),
            remaining_debt_micro: remaining_debt,
            repayments,
        };
        let p = next.accounts.get_mut(subject).expect("checked");
        p.debt = remaining_debt;
        // Fractional repayment survives tiny releases, but not a paid-off loan.
        p.repayment_remainder = if remaining_debt == 0 {
            0
        } else {
            fractional % MILLION
        };
        p.last_share = Some((release_epoch, split.clone()));
        *self = next;
        Ok((split, true))
    }
}
