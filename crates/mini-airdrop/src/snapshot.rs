//! The eligibility snapshot: who gets how much, decided once, off-chain,
//! by whoever runs the campaign -- this crate has no opinion on *why* an
//! identity root is included (that's a policy decision, deliberately kept
//! out of this crate, the same way `mini-provider`'s protocol never
//! judges whether a declaration is honest). What this crate enforces
//! structurally is the shape every campaign needs regardless of policy:
//! at most one entry per identity root, bounded sizes, and a
//! content-derived snapshot digest a claim can bind itself to so a claim
//! valid against one snapshot can never be replayed against another.

use std::collections::HashMap;

use did_mini::Did;
use mini_uniqueness::HumanStatus;

use crate::error::{AirdropError, Result};

/// Hard limit on [`AllocationEntry::reason`].
pub const MAX_REASON_BYTES: usize = 512;
/// Hard limit on how many entries a single snapshot may hold -- a
/// defensive bound against unbounded memory growth building a snapshot
/// from an untrusted or merely huge source list, the same
/// defensive-decoding discipline (ID5) every bounded collection in this
/// workspace applies.
pub const MAX_ENTRIES: usize = 1_000_000;
/// Hard limit on [`AirdropSnapshot::campaign_id`].
pub const MAX_CAMPAIGN_ID_BYTES: usize = 128;

/// One identity root's allocation within a campaign.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllocationEntry {
    pub identity_root: Did,
    /// In micro-MINI, the same convention `mini-settlement`/`mini-bounty`/
    /// `mini-reward` already use.
    pub amount_micro: u64,
    /// Advisory only -- the personhood signal this identity root had
    /// accumulated in `mini-uniqueness` at snapshot time, if the campaign
    /// operator chose to attach one. **Never an enforcement gate**:
    /// `mini-uniqueness`'s own docs are explicit that identity root !=
    /// verified human (Sybil resistance is unsolved), so a `None` here or
    /// even `Some(HumanStatus::Unverified)` does not by itself disqualify
    /// an entry the campaign operator chose to include, and this crate
    /// never reads this field to decide eligibility -- only
    /// [`AllocationEntry::identity_root`]'s presence in the snapshot does.
    pub human_status: Option<HumanStatus>,
    /// Free-form, human-readable justification (e.g. "testnet validator",
    /// "contributor, PR #219"). No protocol meaning.
    pub reason: String,
}

impl AllocationEntry {
    fn check_wellformed(&self) -> Result<()> {
        if self.amount_micro == 0 {
            return Err(AirdropError::ZeroAmount);
        }
        if self.reason.len() > MAX_REASON_BYTES {
            return Err(AirdropError::ReasonTooLong);
        }
        Ok(())
    }
}

/// Builds an [`AirdropSnapshot`], rejecting duplicate identity roots at
/// insertion time rather than leaving "did I already add this root" to a
/// caller's own bookkeeping.
#[derive(Debug, Clone)]
pub struct SnapshotBuilder {
    campaign_id: Vec<u8>,
    entries: HashMap<Did, AllocationEntry>,
}

impl SnapshotBuilder {
    /// Start a new snapshot for `campaign_id` -- an opaque, campaign-
    /// operator-chosen identifier (e.g. `b"testnet-genesis-2026"`) that
    /// every claim against this snapshot must echo back, so a claim
    /// signed for one campaign can never be replayed against another.
    pub fn new(campaign_id: impl Into<Vec<u8>>) -> Result<Self> {
        let campaign_id = campaign_id.into();
        if campaign_id.len() > MAX_CAMPAIGN_ID_BYTES {
            return Err(AirdropError::CampaignIdTooLong);
        }
        Ok(SnapshotBuilder {
            campaign_id,
            entries: HashMap::new(),
        })
    }

    /// Add one entry. Rejects a second entry for an identity root already
    /// present, a zero amount, an oversized reason, or exceeding
    /// [`MAX_ENTRIES`].
    pub fn insert(&mut self, entry: AllocationEntry) -> Result<()> {
        entry.check_wellformed()?;
        if self.entries.contains_key(&entry.identity_root) {
            return Err(AirdropError::DuplicateIdentityRoot);
        }
        if self.entries.len() >= MAX_ENTRIES {
            return Err(AirdropError::TooManyEntries);
        }
        self.entries.insert(entry.identity_root.clone(), entry);
        Ok(())
    }

    /// How many entries have been added so far.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Finish building. Entries are sorted by identity-root string so two
    /// builders fed the same entries in different orders produce
    /// bit-identical snapshots (and therefore the same
    /// [`AirdropSnapshot::digest`]).
    pub fn build(self) -> AirdropSnapshot {
        let mut entries: Vec<AllocationEntry> = self.entries.into_values().collect();
        entries.sort_by(|a, b| a.identity_root.as_str().cmp(b.identity_root.as_str()));
        AirdropSnapshot {
            campaign_id: self.campaign_id,
            entries,
        }
    }
}

/// A finished, immutable eligibility snapshot: at most one entry per
/// identity root, deterministically ordered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AirdropSnapshot {
    campaign_id: Vec<u8>,
    entries: Vec<AllocationEntry>,
}

impl AirdropSnapshot {
    pub fn campaign_id(&self) -> &[u8] {
        &self.campaign_id
    }

    pub fn entries(&self) -> &[AllocationEntry] {
        &self.entries
    }

    /// The entry for `identity_root`, if it has one.
    pub fn entry_for(&self, identity_root: &Did) -> Option<&AllocationEntry> {
        // Deliberately linear -- `entries` is the immutable, already-sorted
        // public representation; an index keyed by `Did` would duplicate
        // storage for a lookup pattern this crate's own verification path
        // performs once per claim, not in a hot loop. A caller building a
        // service around this crate is free to index `entries()` however
        // it needs.
        self.entries
            .iter()
            .find(|e| &e.identity_root == identity_root)
    }

    /// A BLAKE3-256 digest over the campaign id and every entry, in the
    /// snapshot's canonical (sorted) order -- content-addresses the
    /// snapshot itself, independent of this crate's in-memory
    /// representation. Two snapshots with the same digest are guaranteed
    /// to have the same campaign id and the exact same allocations.
    pub fn digest(&self) -> [u8; 32] {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"mini-airdrop/snapshot/v1");
        buf.extend_from_slice(&(self.campaign_id.len() as u32).to_be_bytes());
        buf.extend_from_slice(&self.campaign_id);
        buf.extend_from_slice(&(self.entries.len() as u64).to_be_bytes());
        for e in &self.entries {
            let root_bytes = e.identity_root.as_str().as_bytes();
            buf.extend_from_slice(&(root_bytes.len() as u32).to_be_bytes());
            buf.extend_from_slice(root_bytes);
            buf.extend_from_slice(&e.amount_micro.to_be_bytes());
        }
        mini_crypto::hash::blake3_256(&buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;

    fn root() -> Did {
        Controller::incept_single().unwrap().did()
    }

    fn entry(identity_root: Did, amount_micro: u64) -> AllocationEntry {
        AllocationEntry {
            identity_root,
            amount_micro,
            human_status: None,
            reason: "test".to_string(),
        }
    }

    #[test]
    fn a_duplicate_identity_root_is_rejected() {
        let mut b = SnapshotBuilder::new(b"campaign-1".to_vec()).unwrap();
        let r = root();
        b.insert(entry(r.clone(), 100)).unwrap();
        assert_eq!(
            b.insert(entry(r, 50)).unwrap_err(),
            AirdropError::DuplicateIdentityRoot
        );
    }

    #[test]
    fn a_zero_amount_entry_is_rejected() {
        let mut b = SnapshotBuilder::new(b"campaign-1".to_vec()).unwrap();
        assert_eq!(
            b.insert(entry(root(), 0)).unwrap_err(),
            AirdropError::ZeroAmount
        );
    }

    #[test]
    fn an_oversized_campaign_id_is_rejected() {
        let too_long = vec![0u8; MAX_CAMPAIGN_ID_BYTES + 1];
        assert_eq!(
            SnapshotBuilder::new(too_long).unwrap_err(),
            AirdropError::CampaignIdTooLong
        );
    }

    #[test]
    fn snapshot_lookup_finds_exactly_its_own_entries() {
        let mut b = SnapshotBuilder::new(b"campaign-1".to_vec()).unwrap();
        let in_snapshot = root();
        let not_in_snapshot = root();
        b.insert(entry(in_snapshot.clone(), 100)).unwrap();
        let snap = b.build();

        assert!(snap.entry_for(&in_snapshot).is_some());
        assert!(snap.entry_for(&not_in_snapshot).is_none());
    }

    #[test]
    fn digest_is_stable_regardless_of_insertion_order() {
        let a = root();
        let b_root = root();

        let mut builder1 = SnapshotBuilder::new(b"campaign-1".to_vec()).unwrap();
        builder1.insert(entry(a.clone(), 100)).unwrap();
        builder1.insert(entry(b_root.clone(), 200)).unwrap();

        let mut builder2 = SnapshotBuilder::new(b"campaign-1".to_vec()).unwrap();
        builder2.insert(entry(b_root, 200)).unwrap();
        builder2.insert(entry(a, 100)).unwrap();

        assert_eq!(builder1.build().digest(), builder2.build().digest());
    }

    #[test]
    fn digest_changes_if_an_amount_changes() {
        let r = root();
        let mut b1 = SnapshotBuilder::new(b"campaign-1".to_vec()).unwrap();
        b1.insert(entry(r.clone(), 100)).unwrap();
        let mut b2 = SnapshotBuilder::new(b"campaign-1".to_vec()).unwrap();
        b2.insert(entry(r, 200)).unwrap();

        assert_ne!(b1.build().digest(), b2.build().digest());
    }

    #[test]
    fn digest_changes_if_the_campaign_id_changes() {
        let r = root();
        let mut b1 = SnapshotBuilder::new(b"campaign-1".to_vec()).unwrap();
        b1.insert(entry(r.clone(), 100)).unwrap();
        let mut b2 = SnapshotBuilder::new(b"campaign-2".to_vec()).unwrap();
        b2.insert(entry(r, 100)).unwrap();

        assert_ne!(b1.build().digest(), b2.build().digest());
    }
}
