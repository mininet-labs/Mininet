//! The treasury signer set: **who** is authorized to move treasury funds,
//! and whether enough of them have approved a given action — not the real
//! signing cryptography itself.
//!
//! Whitepaper §10: the community treasury "is held under a rotating
//! threshold signature that requires the agreement of many independent,
//! geographically and socially diverse human signers, and that signer set
//! is itself governed by the flat vote and rotated over time." Membership
//! in this set is a custody role decided by ordinary one-human-one-vote
//! governance — it is **not** itself a source of voting power, the same
//! discipline `mini_chain::ValidatorSet` states for its own equal-weight
//! set: no weight field, no path to extra influence for being a signer.
//!
//! ## Honest limit — this is not a threshold-signature scheme
//!
//! [`meets_threshold`] is ordinary distinct-identity counting, the same
//! pattern `mini-forge`'s governance approval counting already uses safely.
//! It is deliberately **not** a real threshold/multisig cryptographic
//! scheme (e.g. FROST): generating and combining partial signatures over
//! real treasury funds is exactly the "permanent honeypot" component the
//! whitepaper (§11) and D-0035 point 5 require human authorship and
//! external audit for. This module answers "did enough authorized people
//! agree," not "here is a valid signature the treasury chain would accept."

use std::collections::HashSet;

use did_mini::Did;

use crate::error::{Result, TreasuryError};

/// Hard cap on signer-set size — a small rotating committee, not a full
/// validator set, so capped tighter than `mini_chain::MAX_VALIDATORS`.
pub const MAX_SIGNERS: usize = 1_000;

/// A set of identity roots authorized to approve treasury actions, plus the
/// threshold of them required to agree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreasurySignerSet {
    signers: Vec<Did>,
    threshold: usize,
}

impl TreasurySignerSet {
    /// Build a signer set. Rejects an empty, oversized, or duplicate-
    /// containing set, and a threshold of zero or more than the set's size
    /// — the same construction-time discipline `mini_chain::ValidatorSet`
    /// applies, so no caller can silently build an unsafe set.
    pub fn new(mut signers: Vec<Did>, threshold: usize) -> Result<Self> {
        if signers.is_empty() || signers.len() > MAX_SIGNERS {
            return Err(TreasuryError::InvalidSignerSet);
        }
        signers.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        let mut seen: HashSet<&str> = HashSet::with_capacity(signers.len());
        for s in &signers {
            if !seen.insert(s.scid()) {
                return Err(TreasuryError::InvalidSignerSet);
            }
        }
        if threshold == 0 || threshold > signers.len() {
            return Err(TreasuryError::InvalidThreshold);
        }
        Ok(TreasurySignerSet { signers, threshold })
    }

    /// Number of authorized signers.
    pub fn len(&self) -> usize {
        self.signers.len()
    }

    /// Whether the set is empty (construction forbids this; kept for API
    /// completeness).
    pub fn is_empty(&self) -> bool {
        self.signers.is_empty()
    }

    /// Whether `signer` is currently authorized.
    pub fn contains(&self, signer: &Did) -> bool {
        self.signers.iter().any(|s| s.scid() == signer.scid())
    }

    /// The authorized signer roots, canonically sorted.
    pub fn signers(&self) -> &[Did] {
        &self.signers
    }

    /// How many distinct authorized approvals a treasury action needs.
    pub fn threshold(&self) -> usize {
        self.threshold
    }
}

/// How many distinct authorized signers appear in `approvers`. Duplicate
/// entries and non-members count once and not at all, respectively — the
/// same "one identity, one count" discipline as everywhere else in this
/// tree that counts approvals.
pub fn count_valid_approvals(set: &TreasurySignerSet, approvers: &[Did]) -> usize {
    let mut counted: HashSet<&str> = HashSet::new();
    for approver in approvers {
        if set.contains(approver) {
            counted.insert(approver.scid());
        }
    }
    counted.len()
}

/// Whether `approvers` meets `set`'s threshold.
pub fn meets_threshold(set: &TreasurySignerSet, approvers: &[Did]) -> bool {
    count_valid_approvals(set, approvers) >= set.threshold()
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;

    fn did(seed: u8) -> Did {
        Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32])
            .unwrap()
            .did()
    }

    #[test]
    fn empty_signer_set_is_rejected() {
        assert_eq!(
            TreasurySignerSet::new(vec![], 1),
            Err(TreasuryError::InvalidSignerSet)
        );
    }

    #[test]
    fn duplicate_signer_is_rejected() {
        let a = did(1);
        assert_eq!(
            TreasurySignerSet::new(vec![a.clone(), a], 1),
            Err(TreasuryError::InvalidSignerSet)
        );
    }

    #[test]
    fn zero_or_oversized_threshold_is_rejected() {
        let signers = vec![did(1), did(2)];
        assert_eq!(
            TreasurySignerSet::new(signers.clone(), 0),
            Err(TreasuryError::InvalidThreshold)
        );
        assert_eq!(
            TreasurySignerSet::new(signers, 3),
            Err(TreasuryError::InvalidThreshold)
        );
    }

    #[test]
    fn threshold_met_only_by_enough_distinct_authorized_approvers() {
        let signers = vec![did(1), did(2), did(3), did(4), did(5)];
        let set = TreasurySignerSet::new(signers, 3).unwrap();

        // Two approvers, threshold 3: not enough.
        assert!(!meets_threshold(&set, &[did(1), did(2)]));

        // Three distinct authorized approvers: enough.
        assert!(meets_threshold(&set, &[did(1), did(2), did(3)]));

        // Duplicates of the same approver don't count multiple times.
        assert!(!meets_threshold(&set, &[did(1), did(1), did(1)]));

        // A non-member approving contributes nothing.
        let outsider = did(99);
        assert!(!meets_threshold(&set, &[did(1), did(2), outsider]));
    }
}
