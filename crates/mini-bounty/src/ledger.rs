//! Double-claim prevention: tracks which grants (by key image) have
//! already paid out, the same shape `mini_presence::ReplayGuard` uses for
//! nonce tracking.
//!
//! **This trait is the persistence interface**: a production deployment
//! must back it with the chain's own durable state (or at minimum a
//! device/service store), because a claim that pays out twice is a real
//! loss of funds, not just a replayed message. [`InMemoryKeyImageLedger`]
//! is for tests and prototyping only.

use std::collections::HashSet;

/// Tracks which `(pool_id, key_image)` pairs have already been paid out.
pub trait KeyImageLedger {
    /// Whether `(pool_id, key_image)` has already been recorded as paid.
    fn is_claimed(&self, pool_id: &[u8], key_image: &[u8]) -> bool;

    /// Record `(pool_id, key_image)` as paid. Returns `true` if this was
    /// the first time (the claim should proceed), `false` if it had
    /// already been recorded (the claim must be rejected).
    fn check_and_record(&mut self, pool_id: &[u8], key_image: &[u8]) -> bool;
}

/// A simple in-memory [`KeyImageLedger`].
#[derive(Debug, Default)]
pub struct InMemoryKeyImageLedger {
    claimed: HashSet<(Vec<u8>, Vec<u8>)>,
}

impl InMemoryKeyImageLedger {
    /// A new, empty ledger.
    pub fn new() -> Self {
        Self::default()
    }
}

impl KeyImageLedger for InMemoryKeyImageLedger {
    fn is_claimed(&self, pool_id: &[u8], key_image: &[u8]) -> bool {
        self.claimed
            .contains(&(pool_id.to_vec(), key_image.to_vec()))
    }

    fn check_and_record(&mut self, pool_id: &[u8], key_image: &[u8]) -> bool {
        self.claimed.insert((pool_id.to_vec(), key_image.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_key_image_is_not_claimed() {
        let ledger = InMemoryKeyImageLedger::new();
        assert!(!ledger.is_claimed(b"pool-1", b"image-a"));
    }

    #[test]
    fn recording_a_key_image_makes_it_claimed() {
        let mut ledger = InMemoryKeyImageLedger::new();
        assert!(ledger.check_and_record(b"pool-1", b"image-a"));
        assert!(ledger.is_claimed(b"pool-1", b"image-a"));
    }

    #[test]
    fn recording_the_same_key_image_twice_is_rejected_the_second_time() {
        let mut ledger = InMemoryKeyImageLedger::new();
        assert!(ledger.check_and_record(b"pool-1", b"image-a"));
        assert!(!ledger.check_and_record(b"pool-1", b"image-a"));
    }

    #[test]
    fn the_same_key_image_in_a_different_pool_is_independent() {
        let mut ledger = InMemoryKeyImageLedger::new();
        assert!(ledger.check_and_record(b"pool-1", b"image-a"));
        assert!(ledger.check_and_record(b"pool-2", b"image-a"));
    }
}
