//! Replay prevention: has this identity root already redeemed this
//! campaign? Mirrors the same seam `mini_settlement::ClaimWatcher` and
//! `mini_settlement::CanonicalLedgerView` already use in this workspace --
//! a trait this crate's verification logic is fully specified and tested
//! against today (via [`InMemoryClaimedRegistry`]), that a real persisted
//! backend implements later with no change to the verification rules
//! themselves.

use did_mini::Did;

use crate::error::Result;

/// A record of which identity roots have already claimed a campaign.
///
/// `mark_claimed` is fallible on purpose: a real backend that persists to
/// disk or a canonical ledger can genuinely fail to write (full disk, I/O
/// error), and [`crate::claim::verify_and_resolve_claim`] must not report
/// success back to a caller unless the mark is durably recorded --
/// otherwise a crash between "verified" and "actually persisted" could
/// let the same identity root claim twice.
pub trait ClaimedRegistry {
    /// `true` if `identity_root` has already claimed this campaign.
    fn already_claimed(&self, identity_root: &Did) -> bool;

    /// Record that `identity_root` claimed at `at_ms`. Called only after
    /// every other check in [`crate::claim::verify_and_resolve_claim`]
    /// has already passed -- a failed verification never marks anything
    /// claimed. An `Err` here propagates straight out of
    /// `verify_and_resolve_claim` as [`crate::error::AirdropError::
    /// RegistryWriteFailed`], so a caller never receives a `ClaimOutcome`
    /// for a claim this registry did not actually manage to record.
    fn mark_claimed(&mut self, identity_root: &Did, at_ms: u64) -> Result<()>;
}

/// A trivial in-memory [`ClaimedRegistry`] -- test-only, and for any real
/// deployment where losing all claim history on process restart is
/// actually acceptable. See [`crate::file_registry::FileClaimedRegistry`]
/// for a real persisted implementation.
#[derive(Debug, Default)]
pub struct InMemoryClaimedRegistry {
    claimed: std::collections::HashMap<Did, u64>,
}

impl InMemoryClaimedRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// When `identity_root` claimed, if it has.
    pub fn claimed_at(&self, identity_root: &Did) -> Option<u64> {
        self.claimed.get(identity_root).copied()
    }
}

impl ClaimedRegistry for InMemoryClaimedRegistry {
    fn already_claimed(&self, identity_root: &Did) -> bool {
        self.claimed.contains_key(identity_root)
    }

    fn mark_claimed(&mut self, identity_root: &Did, at_ms: u64) -> Result<()> {
        self.claimed.insert(identity_root.clone(), at_ms);
        Ok(())
    }
}
