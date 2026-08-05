//! Bounded replay cache for session and discovery nonces.

use std::collections::HashMap;

use crate::{Result, TransportSecurityError};

/// Maximum replay ids held by one cache.
pub const MAX_REPLAY_CACHE_ENTRIES: usize = 65_536;

/// Bounded first-seen cache whose entries remain until their signed validity
/// window ends. A full cache fails closed instead of evicting a still-valid id:
/// bounded memory must never silently turn into replay acceptance.
///
/// A host that needs protection across process restart must persist an
/// equivalent `(id, expires_at_ms)` set. This in-memory type does not claim
/// crash-persistent replay defense.
#[derive(Debug, Clone)]
pub struct ReplayCache {
    capacity: usize,
    seen: HashMap<[u8; 32], u64>,
    // Security time is monotonic within this cache even when the host wall clock
    // moves backwards. Once a validity window has expired locally, a later clock
    // rollback must not make its replay token admissible again.
    highest_now_ms: u64,
}

impl ReplayCache {
    pub fn new(capacity: usize) -> Result<Self> {
        if capacity == 0 || capacity > MAX_REPLAY_CACHE_ENTRIES {
            return Err(TransportSecurityError::LimitExceeded);
        }
        Ok(Self {
            capacity,
            seen: HashMap::with_capacity(capacity.min(1024)),
            highest_now_ms: 0,
        })
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    fn advance_time(&mut self, now_ms: u64) -> u64 {
        self.highest_now_ms = self.highest_now_ms.max(now_ms);
        self.highest_now_ms
    }

    /// Remove ids whose signed validity window has ended before `now_ms`.
    /// The cache retains a monotonic time high-water mark, so a later wall-clock
    /// rollback cannot resurrect a token that was already considered expired.
    pub fn prune_expired(&mut self, now_ms: u64) {
        let effective_now_ms = self.advance_time(now_ms);
        self.seen
            .retain(|_, expires_at_ms| *expires_at_ms >= effective_now_ms);
    }

    /// Record `id` until `expires_at_ms`, rejecting a duplicate. Capacity is a
    /// fail-closed admission bound: no unexpired id is evicted to make room.
    pub fn check_and_record(
        &mut self,
        id: [u8; 32],
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<()> {
        let effective_now_ms = self.advance_time(now_ms);
        if expires_at_ms < effective_now_ms {
            return Err(TransportSecurityError::Expired);
        }
        self.seen
            .retain(|_, stored_expires_at_ms| *stored_expires_at_ms >= effective_now_ms);
        if self.seen.contains_key(&id) {
            return Err(TransportSecurityError::Replay);
        }
        if self.seen.len() >= self.capacity {
            return Err(TransportSecurityError::LimitExceeded);
        }
        self.seen.insert(id, expires_at_ms);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_is_rejected_and_capacity_fails_closed() {
        let mut cache = ReplayCache::new(2).unwrap();
        cache.check_and_record([1; 32], 2_000, 1_000).unwrap();
        assert_eq!(
            cache.check_and_record([1; 32], 2_000, 1_000),
            Err(TransportSecurityError::Replay)
        );
        cache.check_and_record([2; 32], 2_000, 1_000).unwrap();
        assert_eq!(
            cache.check_and_record([3; 32], 2_000, 1_000),
            Err(TransportSecurityError::LimitExceeded)
        );
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn expired_entries_are_pruned_before_admission() {
        let mut cache = ReplayCache::new(2).unwrap();
        cache.check_and_record([1; 32], 1_500, 1_000).unwrap();
        cache.check_and_record([2; 32], 1_500, 1_000).unwrap();
        cache.check_and_record([3; 32], 3_000, 1_501).unwrap();
        assert_eq!(cache.len(), 1);
        assert_eq!(
            cache.check_and_record([4; 32], 1_500, 1_501),
            Err(TransportSecurityError::Expired)
        );
    }

    #[test]
    fn wall_clock_rollback_cannot_resurrect_an_expired_replay_id() {
        let mut cache = ReplayCache::new(2).unwrap();
        cache.check_and_record([1; 32], 2_000, 1_000).unwrap();
        cache.prune_expired(2_500);
        assert!(cache.is_empty());

        // The supplied wall clock moved backwards, but the cache's security
        // time may not. The old token stays expired and a fresh later window is
        // still admissible against the retained high-water mark.
        assert_eq!(
            cache.check_and_record([1; 32], 2_000, 1_500),
            Err(TransportSecurityError::Expired)
        );
        cache.check_and_record([2; 32], 3_000, 1_500).unwrap();
    }
}
