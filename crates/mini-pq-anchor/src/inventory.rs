//! Wallet-facing PQ anchor inventory -- the "surface wallet-side UI/
//! inventory" half of roadmap issue #231. A UI renders from
//! [`InventorySummary`], never from a raw `Vec<PqAnchorRecord>` directly,
//! so the "do I actually have a pre-provisioned anchor yet" question has
//! exactly one place a client can ask it, the same discipline
//! `mini_settlement::SettlementState::wallet_label` uses for settlement
//! state.

use did_mini::Did;

use crate::anchor::PqAnchorRecord;
use crate::error::{PqAnchorError, Result};

/// Hard limit on how many PQ anchors this inventory holds for a single
/// owner. Not a protocol limit -- a defensive bound against unbounded
/// growth in wallet-local storage, matching the workspace-wide
/// defensive-decoding discipline (ID5) applied to counts as well as bytes.
pub const MAX_ANCHORS_PER_OWNER: usize = 16;

/// A wallet's local collection of [`PqAnchorRecord`]s across however many
/// identity roots it manages. Purely local bookkeeping -- never gossiped,
/// never synced by this crate, never a source of truth for anything but
/// itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PqAnchorInventory {
    records: Vec<PqAnchorRecord>,
}

impl PqAnchorInventory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a record, rejecting it if it is malformed or would push the
    /// owning identity's anchor count past [`MAX_ANCHORS_PER_OWNER`].
    pub fn add(&mut self, record: PqAnchorRecord) -> Result<()> {
        record.check_wellformed()?;
        let existing_for_owner = self
            .records
            .iter()
            .filter(|r| r.owner == record.owner)
            .count();
        if existing_for_owner >= MAX_ANCHORS_PER_OWNER {
            return Err(PqAnchorError::TooManyAnchorsForOwner);
        }
        self.records.push(record);
        Ok(())
    }

    /// Every anchor provisioned for `owner`, oldest first.
    pub fn for_owner(&self, owner: &Did) -> Vec<&PqAnchorRecord> {
        let mut out: Vec<&PqAnchorRecord> =
            self.records.iter().filter(|r| &r.owner == owner).collect();
        out.sort_by_key(|r| r.generated_at_ms);
        out
    }

    /// All records, regardless of owner.
    pub fn all(&self) -> &[PqAnchorRecord] {
        &self.records
    }

    /// The one-question wallet UI summary for `owner`: does this identity
    /// have a pre-provisioned PQ anchor on record, and if so, how many /
    /// how recently. Mirrors `mini_settlement::SettlementState`'s
    /// "collapse the detail into the one question a UI actually needs to
    /// render" pattern.
    pub fn summary_for_owner(&self, owner: &Did) -> InventorySummary {
        let owned = self.for_owner(owner);
        if owned.is_empty() {
            return InventorySummary::NoAnchorProvisioned;
        }
        let most_recent_ms = owned
            .iter()
            .map(|r| r.generated_at_ms)
            .max()
            .unwrap_or_default();
        InventorySummary::AnchorsProvisioned {
            count: owned.len(),
            most_recent_generated_at_ms: most_recent_ms,
        }
    }
}

/// The wallet-facing label a client should actually render for one
/// identity's PQ-anchor readiness. Deliberately makes no claim about
/// recovery -- this crate provisions anchors, it does not implement or
/// promise the emergency migration procedure (roadmap issue #230) that
/// would ever actually use one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InventorySummary {
    /// This identity has never pre-provisioned a PQ anchor. If a live
    /// classical-signature break happened right now, this identity has no
    /// unbroken anchor to build a recovery on top of (PQ recovery Class C,
    /// PR #220 §4) -- exactly the state pre-provisioning exists to avoid.
    NoAnchorProvisioned,
    /// At least one anchor is on record.
    AnchorsProvisioned {
        count: usize,
        most_recent_generated_at_ms: u64,
    },
}

impl InventorySummary {
    /// The single question a wallet's "you're PQ-ready" indicator should
    /// ask. `true` only means an anchor is *provisioned* -- it is never a
    /// claim that recovery using it has been built or would succeed.
    pub const fn has_any_anchor(&self) -> bool {
        matches!(self, InventorySummary::AnchorsProvisioned { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::provision_anchor;
    use did_mini::Controller;

    fn owner() -> Did {
        Controller::incept_single().unwrap().did()
    }

    #[test]
    fn a_fresh_inventory_reports_no_anchor_for_anyone() {
        let inventory = PqAnchorInventory::new();
        let summary = inventory.summary_for_owner(&owner());
        assert_eq!(summary, InventorySummary::NoAnchorProvisioned);
        assert!(!summary.has_any_anchor());
    }

    #[test]
    fn adding_a_record_makes_it_visible_in_that_owners_summary() {
        let owner = owner();
        let (_, record) = provision_anchor(owner.clone(), "Primary", 1_000).unwrap();
        let mut inventory = PqAnchorInventory::new();
        inventory.add(record).unwrap();

        let summary = inventory.summary_for_owner(&owner);
        assert!(summary.has_any_anchor());
        assert_eq!(
            summary,
            InventorySummary::AnchorsProvisioned {
                count: 1,
                most_recent_generated_at_ms: 1_000
            }
        );
    }

    #[test]
    fn anchors_belonging_to_a_different_owner_never_appear_in_this_owners_summary() {
        let owner_a = owner();
        let owner_b = owner();
        let (_, record) = provision_anchor(owner_a, "Primary", 1_000).unwrap();
        let mut inventory = PqAnchorInventory::new();
        inventory.add(record).unwrap();

        assert_eq!(
            inventory.summary_for_owner(&owner_b),
            InventorySummary::NoAnchorProvisioned
        );
    }

    #[test]
    fn most_recent_generated_at_reflects_the_latest_of_several_anchors() {
        let owner = owner();
        let (_, r1) = provision_anchor(owner.clone(), "old", 100).unwrap();
        let (_, r2) = provision_anchor(owner.clone(), "new", 500).unwrap();
        let mut inventory = PqAnchorInventory::new();
        inventory.add(r1).unwrap();
        inventory.add(r2).unwrap();

        assert_eq!(
            inventory.summary_for_owner(&owner),
            InventorySummary::AnchorsProvisioned {
                count: 2,
                most_recent_generated_at_ms: 500
            }
        );
    }

    #[test]
    fn exceeding_the_per_owner_cap_is_rejected() {
        let owner = owner();
        let mut inventory = PqAnchorInventory::new();
        for i in 0..MAX_ANCHORS_PER_OWNER {
            let (_, record) =
                provision_anchor(owner.clone(), format!("anchor-{i}"), i as u64).unwrap();
            inventory.add(record).unwrap();
        }
        let (_, one_too_many) = provision_anchor(owner, "overflow", 9_999).unwrap();
        assert_eq!(
            inventory.add(one_too_many).unwrap_err(),
            PqAnchorError::TooManyAnchorsForOwner
        );
    }

    #[test]
    fn for_owner_returns_records_oldest_first() {
        let owner = owner();
        let (_, newer) = provision_anchor(owner.clone(), "newer", 500).unwrap();
        let (_, older) = provision_anchor(owner.clone(), "older", 100).unwrap();
        let mut inventory = PqAnchorInventory::new();
        inventory.add(newer).unwrap();
        inventory.add(older).unwrap();

        let ordered = inventory.for_owner(&owner);
        assert_eq!(ordered[0].label, "older");
        assert_eq!(ordered[1].label, "newer");
    }
}
