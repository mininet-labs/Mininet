//! Local conflict detection — the same shape as `mini_presence::ReplayGuard`
//! and `mini_bounty::KeyImageLedger`, applied to `(payer, sequence)` instead of
//! `(device, sequence)`/a key image.
//!
//! A merchant or peer who has never talked to a canonical ledger can still
//! catch the *cheapest* double-spend attempt locally: a payer signing two
//! different claims at the same sequence and showing each to a different
//! recipient. Neither recipient alone can know which (if either) will
//! canonically win — that answer only ever comes from
//! [`crate::reconcile::reconcile`] — but each recipient can refuse to
//! locally accept a claim that already contradicts one they've seen.

use std::collections::HashMap;

/// Tracks the first claim digest seen per `(payer, sequence)`, so a second,
/// *different* claim at the same slot is detectable before ever reaching a
/// canonical ledger. **This is a local heuristic, not a security
/// boundary** — it only sees what it's shown, and a payer who shows
/// different recipients different claims defeats it trivially unless they
/// compare notes. The only real defense is canonical finality
/// ([`crate::reconcile::reconcile`]); this exists purely to let an honest
/// recipient refuse the cheapest, most obvious attempt before wasting any
/// trust on it.
pub trait ClaimWatcher {
    /// The digest already recorded for `(payer, sequence)`, if any.
    fn first_seen(&self, payer: &[u8], sequence: u64) -> Option<[u8; 32]>;

    /// Record `digest` as seen for `(payer, sequence)` if nothing was recorded
    /// yet. Returns `true` if `digest` is consistent with everything this
    /// watcher has seen for this slot (either it's the first, or it
    /// matches exactly), `false` if it conflicts with a different
    /// already-recorded digest.
    fn observe(&mut self, payer: &[u8], sequence: u64, digest: [u8; 32]) -> bool;
}

/// A simple in-memory [`ClaimWatcher`]. Production needs durable storage —
/// the same requirement `InMemoryReplayGuard`/`InMemoryKeyImageLedger`
/// document for their own in-memory implementations, for the same reason:
/// this state must survive process restarts to mean anything.
#[derive(Debug, Default)]
pub struct InMemoryClaimWatcher {
    seen: HashMap<(Vec<u8>, u64), [u8; 32]>,
}

impl InMemoryClaimWatcher {
    /// A new, empty watcher.
    pub fn new() -> Self {
        Self::default()
    }
}

impl ClaimWatcher for InMemoryClaimWatcher {
    fn first_seen(&self, payer: &[u8], sequence: u64) -> Option<[u8; 32]> {
        self.seen.get(&(payer.to_vec(), sequence)).copied()
    }

    fn observe(&mut self, payer: &[u8], sequence: u64, digest: [u8; 32]) -> bool {
        match self.seen.entry((payer.to_vec(), sequence)) {
            std::collections::hash_map::Entry::Occupied(existing) => *existing.get() == digest,
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(digest);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_claim_at_a_slot_is_always_consistent() {
        let mut w = InMemoryClaimWatcher::new();
        assert!(w.observe(b"payer-a", 0, [1u8; 32]));
        assert_eq!(w.first_seen(b"payer-a", 0), Some([1u8; 32]));
    }

    #[test]
    fn the_same_digest_observed_twice_stays_consistent() {
        let mut w = InMemoryClaimWatcher::new();
        assert!(w.observe(b"payer-a", 0, [1u8; 32]));
        assert!(w.observe(b"payer-a", 0, [1u8; 32]));
    }

    #[test]
    fn a_different_digest_at_the_same_slot_is_flagged_inconsistent() {
        let mut w = InMemoryClaimWatcher::new();
        assert!(w.observe(b"payer-a", 0, [1u8; 32]));
        assert!(!w.observe(b"payer-a", 0, [2u8; 32]));
    }

    #[test]
    fn different_payers_or_different_sequences_never_conflict() {
        let mut w = InMemoryClaimWatcher::new();
        assert!(w.observe(b"payer-a", 0, [1u8; 32]));
        assert!(w.observe(b"payer-b", 0, [2u8; 32])); // different payer
        assert!(w.observe(b"payer-a", 1, [3u8; 32])); // different sequence
    }
}
