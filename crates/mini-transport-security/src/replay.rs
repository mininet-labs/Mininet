//! Bounded replay cache for session and discovery nonces.

use std::collections::{HashSet, VecDeque};

use crate::{Result, TransportSecurityError};

/// Maximum replay ids held by one cache.
pub const MAX_REPLAY_CACHE_ENTRIES: usize = 65_536;

/// Bounded first-seen cache. Oldest ids are evicted at capacity; the cache can
/// be persisted by a host if replay protection must survive process restart.
#[derive(Debug, Clone)]
pub struct ReplayCache {
    capacity: usize,
    seen: HashSet<[u8; 32]>,
    order: VecDeque<[u8; 32]>,
}

impl ReplayCache {
    pub fn new(capacity: usize) -> Result<Self> {
        if capacity == 0 || capacity > MAX_REPLAY_CACHE_ENTRIES {
            return Err(TransportSecurityError::LimitExceeded);
        }
        Ok(Self {
            capacity,
            seen: HashSet::with_capacity(capacity.min(1024)),
            order: VecDeque::with_capacity(capacity.min(1024)),
        })
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    /// Record `id`, rejecting a duplicate. Eviction affects only replay
    /// availability; signed expiry windows remain the outer bound.
    pub fn check_and_record(&mut self, id: [u8; 32]) -> Result<()> {
        if !self.seen.insert(id) {
            return Err(TransportSecurityError::Replay);
        }
        self.order.push_back(id);
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.seen.remove(&oldest);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_is_rejected_and_oldest_is_evicted() {
        let mut cache = ReplayCache::new(2).unwrap();
        cache.check_and_record([1; 32]).unwrap();
        assert_eq!(
            cache.check_and_record([1; 32]),
            Err(TransportSecurityError::Replay)
        );
        cache.check_and_record([2; 32]).unwrap();
        cache.check_and_record([3; 32]).unwrap();
        cache.check_and_record([1; 32]).unwrap();
        assert_eq!(cache.len(), 2);
    }
}
