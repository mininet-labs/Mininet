#!/usr/bin/env python3
"""Make transport and onion replay caches fail closed across wall-clock rollback."""

from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:100]!r}")
    p.write_text(text.replace(old, new), encoding="utf-8")


replace_once(
    "crates/mini-transport-security/src/replay.rs",
    """#[derive(Debug, Clone)]
pub struct ReplayCache {
    capacity: usize,
    seen: HashMap<[u8; 32], u64>,
}
""",
    """#[derive(Debug, Clone)]
pub struct ReplayCache {
    capacity: usize,
    seen: HashMap<[u8; 32], u64>,
    // Security time is monotonic within this cache even when the host wall clock
    // moves backwards. Once a validity window has expired locally, a later clock
    // rollback must not make its replay token admissible again.
    highest_now_ms: u64,
}
""",
)
replace_once(
    "crates/mini-transport-security/src/replay.rs",
    """        Ok(Self {
            capacity,
            seen: HashMap::with_capacity(capacity.min(1024)),
        })
""",
    """        Ok(Self {
            capacity,
            seen: HashMap::with_capacity(capacity.min(1024)),
            highest_now_ms: 0,
        })
""",
)
replace_once(
    "crates/mini-transport-security/src/replay.rs",
    """    /// Remove ids whose signed validity window has ended before `now_ms`.
    pub fn prune_expired(&mut self, now_ms: u64) {
        self.seen
            .retain(|_, expires_at_ms| *expires_at_ms >= now_ms);
    }

    /// Record `id` until `expires_at_ms`, rejecting a duplicate. Capacity is a
    /// fail-closed admission bound: no unexpired id is evicted to make room.
    pub fn check_and_record(
        &mut self,
        id: [u8; 32],
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<()> {
        if expires_at_ms < now_ms {
            return Err(TransportSecurityError::Expired);
        }
        self.prune_expired(now_ms);
        if self.seen.contains_key(&id) {
            return Err(TransportSecurityError::Replay);
        }
        if self.seen.len() >= self.capacity {
            return Err(TransportSecurityError::LimitExceeded);
        }
        self.seen.insert(id, expires_at_ms);
        Ok(())
    }
""",
    """    fn advance_time(&mut self, now_ms: u64) -> u64 {
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
""",
)
replace_once(
    "crates/mini-transport-security/src/replay.rs",
    """    fn expired_entries_are_pruned_before_admission() {
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
""",
    """    fn expired_entries_are_pruned_before_admission() {
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
""",
)

replace_once(
    "crates/mini-relay/src/onion.rs",
    """#[derive(Debug, Clone)]
pub struct OnionReplayCache {
    capacity: usize,
    seen: HashMap<[u8; 32], u64>,
}
""",
    """#[derive(Debug, Clone)]
pub struct OnionReplayCache {
    capacity: usize,
    seen: HashMap<[u8; 32], u64>,
    // Security time is monotonic within this cache even when the host wall clock
    // moves backwards. Once a validity window has expired locally, a later clock
    // rollback must not make its replay token admissible again.
    highest_now_ms: u64,
}
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """        Ok(Self {
            capacity,
            seen: HashMap::with_capacity(capacity.min(1024)),
        })
""",
    """        Ok(Self {
            capacity,
            seen: HashMap::with_capacity(capacity.min(1024)),
            highest_now_ms: 0,
        })
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """    pub fn prune_expired(&mut self, now_ms: u64) {
        self.seen.retain(|_, expires_at_ms| *expires_at_ms > now_ms);
    }

    pub fn check_and_record(
        &mut self,
        token: [u8; 32],
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<()> {
        validate_onion_window(now_ms, expires_at_ms)?;
        self.prune_expired(now_ms);
        if self.seen.contains_key(&token) {
            return Err(RelayError::OnionReplay);
        }
        if self.seen.len() >= self.capacity {
            return Err(RelayError::LimitExceeded);
        }
        self.seen.insert(token, expires_at_ms);
        Ok(())
    }
""",
    """    fn advance_time(&mut self, now_ms: u64) -> u64 {
        self.highest_now_ms = self.highest_now_ms.max(now_ms);
        self.highest_now_ms
    }

    pub fn prune_expired(&mut self, now_ms: u64) {
        let effective_now_ms = self.advance_time(now_ms);
        self.seen
            .retain(|_, expires_at_ms| *expires_at_ms > effective_now_ms);
    }

    pub fn check_and_record(
        &mut self,
        token: [u8; 32],
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<()> {
        let effective_now_ms = self.advance_time(now_ms);
        validate_onion_window(effective_now_ms, expires_at_ms)?;
        self.seen
            .retain(|_, stored_expires_at_ms| *stored_expires_at_ms > effective_now_ms);
        if self.seen.contains_key(&token) {
            return Err(RelayError::OnionReplay);
        }
        if self.seen.len() >= self.capacity {
            return Err(RelayError::LimitExceeded);
        }
        self.seen.insert(token, expires_at_ms);
        Ok(())
    }
""",
)
replace_once(
    "crates/mini-relay/src/onion.rs",
    """    fn replay_capacity_fails_closed_until_entries_expire() {
        let mut cache = OnionReplayCache::new(2).unwrap();
        cache.check_and_record([1; 32], 2_000, 1_000).unwrap();
        cache.check_and_record([2; 32], 2_000, 1_000).unwrap();
        assert_eq!(
            cache.check_and_record([3; 32], 2_000, 1_000),
            Err(RelayError::LimitExceeded)
        );
        assert_eq!(cache.len(), 2);
        cache.check_and_record([3; 32], 3_000, 2_001).unwrap();
        assert_eq!(cache.len(), 1);
    }
""",
    """    fn replay_capacity_fails_closed_until_entries_expire() {
        let mut cache = OnionReplayCache::new(2).unwrap();
        cache.check_and_record([1; 32], 2_000, 1_000).unwrap();
        cache.check_and_record([2; 32], 2_000, 1_000).unwrap();
        assert_eq!(
            cache.check_and_record([3; 32], 2_000, 1_000),
            Err(RelayError::LimitExceeded)
        );
        assert_eq!(cache.len(), 2);
        cache.check_and_record([3; 32], 3_000, 2_001).unwrap();
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn wall_clock_rollback_cannot_resurrect_an_expired_onion_token() {
        let mut cache = OnionReplayCache::new(2).unwrap();
        cache.check_and_record([1; 32], 2_000, 1_000).unwrap();
        cache.prune_expired(2_500);
        assert!(cache.is_empty());

        assert_eq!(
            cache.check_and_record([1; 32], 2_000, 1_500),
            Err(RelayError::OnionExpired)
        );
        cache.check_and_record([2; 32], 3_000, 1_500).unwrap();
    }
""",
)

replace_once(
    "docs/planning/privacy-transport-runtime-convergence.md",
    """| Relay and destination replay defense | **PASS in-process** | Onion v2 uses v2 key domains, encrypts expiry/replay tokens for every relay and destination, bounds lifetime with explicit clock-skew tolerance, never evicts live entries, records only after inner validation, and fails closed at capacity. | Hosts must persist equivalent state across restart; authenticated packet floods can still exhaust bounded capacity and require rate/resource controls. |
""",
    """| Relay and destination replay defense | **PASS in-process** | Onion v2 uses v2 key domains, encrypts expiry/replay tokens for every relay and destination, bounds lifetime with explicit clock-skew tolerance, retains a monotonic local time high-water mark so wall-clock rollback cannot resurrect expired tokens, never evicts live entries, records only after inner validation, and fails closed at capacity. | Hosts must persist equivalent replay state and its time high-water mark across restart; authenticated packet floods can still exhaust bounded capacity and require rate/resource controls. |
""",
)
replace_once(
    "docs/THREAT_MODEL.md",
    """| **Relay-cache eviction re-enabling replay** | Onion v2 uses separate v2 cryptographic domains, stores `(token, expiry)` through a clock-skew-bounded encrypted window, prunes only expired entries, records only after the whole local inner structure validates, and fails closed at capacity. | **Closed in-process.** Restart persistence and flood/rate controls remain host responsibilities. |
""",
    """| **Relay-cache eviction or clock rollback re-enabling replay** | Onion v2 uses separate v2 cryptographic domains, stores `(token, expiry)` through a clock-skew-bounded encrypted window, advances a monotonic local time high-water mark, prunes only expired entries, records only after the whole local inner structure validates, and fails closed at capacity. | **Closed in-process.** Restart persistence of both tokens and the time high-water mark, plus flood/rate controls, remain host responsibilities. |
""",
)
replace_once(
    "crates/mini-transport-security/README.md",
    """  lifetime with explicit clock-skew tolerance, and requires fail-closed
  relay/destination replay state. It is not Sphinx and does not
""",
    """  lifetime with explicit clock-skew tolerance, retains a monotonic local
  time high-water mark against wall-clock rollback, and requires fail-closed
  relay/destination replay state. It is not Sphinx and does not
""",
)

print("PR 296 monotonic replay hardening applied")
