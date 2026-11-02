//! Escalating admission cost: spam and flooding get exponentially more
//! expensive, in work (hashing) not money, so ordinary internet access stays
//! free (founder direction, 2026-11-02; the idea is the load-scaled fee
//! escalation XRP uses, applied to compute so no account or token is needed).
//!
//! ## Mechanism
//!
//! - Every admitted request from a *source* inside a sliding window (default
//!   5 s) raises the work the *next* request from that source must prove by
//!   [`AdmissionPolicy::bits_per_request`] bits, i.e. the expected hashing
//!   doubles per extra request. Quiet for one window and the cost resets.
//! - A global term raises everyone's required bits when total admitted load in
//!   the window exceeds a soft limit (log2 steps, capped lower than the
//!   per-source cap), so a botnet spread over many sources still pays.
//! - Work is `BLAKE3(domain || source || request_digest || seq || nonce)` with
//!   the required number of leading zero bits. Binding the request digest means
//!   work cannot be reused for another message; binding `seq` (how many
//!   requests this source has already had admitted in the window) means a solved
//!   nonce cannot be replayed for the next request.
//! - Past [`AdmissionPolicy::max_bits`] the request is refused outright.
//!
//! ## Honest limits
//!
//! Hashing cost is not equal across attackers: an attacker with GPUs/ASICs or a
//! large botnet pays less per unit than a phone, so this raises cost and
//! rate-limits, it does not make flooding impossible. It must be paired with
//! the connection/bandwidth quotas and peer-selection diversity this crate
//! already has. `source` must be something an attacker cannot spoof onto a
//! victim (an authenticated session identity, or an IP prefix on an established
//! TCP connection); otherwise a spoofer could raise a victim's cost. Pure state
//! machine: the caller supplies time. Not audited.

use mini_crypto::hash::blake3_256;
use std::collections::{HashMap, VecDeque};

const DOMAIN: &[u8] = b"mininet/admission-work/v1";

/// Tunable parameters. Defaults: 5 s window, 8 base bits (about 256 hashes,
/// microseconds), +1 bit per extra request, refused past 40 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionPolicy {
    /// Sliding window length (ms).
    pub window_ms: u64,
    /// Work bits for a source's first request in the window.
    pub base_bits: u8,
    /// Extra bits per additional admitted request in the window.
    pub bits_per_request: u8,
    /// Per-source ceiling; a request needing more is refused.
    pub max_bits: u8,
    /// Admitted requests per window (all sources) before the global term starts.
    pub global_soft_limit: u32,
    /// Largest global extra (bits).
    pub global_max_extra_bits: u8,
    /// Most distinct sources tracked at once.
    pub max_sources: usize,
}

impl AdmissionPolicy {
    /// The starting policy.
    pub const fn starting_point() -> Self {
        AdmissionPolicy {
            window_ms: 5_000,
            base_bits: 8,
            bits_per_request: 1,
            max_bits: 40,
            global_soft_limit: 2_000,
            global_max_extra_bits: 12,
            max_sources: 65_536,
        }
    }
}

/// Why a request was not admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// The submitted work did not meet the required bits.
    InsufficientWork {
        /// What was required.
        required_bits: u8,
    },
    /// The source has exceeded the per-source ceiling in this window.
    SourceExhausted,
}

/// What a client must do for its next request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quote {
    /// Leading zero bits the work hash must have.
    pub required_bits: u8,
    /// Sequence number to bind into the work (requests already admitted in the
    /// window).
    pub seq: u32,
}

/// Per-source sliding-window meter plus a global load term.
#[derive(Debug)]
pub struct AdmissionMeter {
    policy: AdmissionPolicy,
    sources: HashMap<[u8; 32], VecDeque<u64>>,
    global: VecDeque<u64>,
}

impl AdmissionMeter {
    /// A new meter.
    pub fn new(policy: AdmissionPolicy) -> Self {
        AdmissionMeter {
            policy,
            sources: HashMap::new(),
            global: VecDeque::new(),
        }
    }

    fn key(source: &[u8]) -> [u8; 32] {
        let mut k = DOMAIN.to_vec();
        k.extend_from_slice(b"/source");
        k.extend_from_slice(source);
        blake3_256(&k)
    }

    fn prune_deque(q: &mut VecDeque<u64>, now: u64, window: u64) {
        while q.front().is_some_and(|t| now.saturating_sub(*t) >= window) {
            q.pop_front();
        }
    }

    fn global_extra(&self) -> u8 {
        let n = self.global.len() as u64;
        let limit = u64::from(self.policy.global_soft_limit.max(1));
        if n < limit {
            return 0;
        }
        // One extra bit per doubling of load above the soft limit.
        let steps = 1 + (n / limit).ilog2();
        steps.min(u32::from(self.policy.global_max_extra_bits)) as u8
    }

    /// What the next request from `source` must prove at `now_ms`.
    pub fn quote(&mut self, source: &[u8], now_ms: u64) -> Quote {
        Self::prune_deque(&mut self.global, now_ms, self.policy.window_ms);
        let seq = self.count_for(&Self::key(source), now_ms);
        Quote {
            required_bits: self.required_bits(seq),
            seq,
        }
    }

    fn count_for(&mut self, key: &[u8; 32], now_ms: u64) -> u32 {
        match self.sources.get_mut(key) {
            Some(q) => {
                Self::prune_deque(q, now_ms, self.policy.window_ms);
                q.len() as u32
            }
            None => 0,
        }
    }

    fn required_bits(&self, seq: u32) -> u8 {
        let p = &self.policy;
        let own = u32::from(p.base_bits) + seq.saturating_mul(u32::from(p.bits_per_request));
        (own + u32::from(self.global_extra())).min(255) as u8
    }

    /// Try to admit a request. On success the request is recorded and raises
    /// the cost of the next one.
    pub fn admit(
        &mut self,
        source: &[u8],
        request_digest: &[u8; 32],
        nonce: u64,
        now_ms: u64,
    ) -> Result<Quote, Refused> {
        let key = Self::key(source);
        Self::prune_deque(&mut self.global, now_ms, self.policy.window_ms);
        let seq = self.count_for(&key, now_ms);
        let own_bits = u32::from(self.policy.base_bits)
            + seq.saturating_mul(u32::from(self.policy.bits_per_request));
        if own_bits > u32::from(self.policy.max_bits) {
            return Err(Refused::SourceExhausted);
        }
        let required = self.required_bits(seq);
        let digest = work_hash(source, request_digest, seq, nonce);
        if leading_zero_bits(&digest) < u32::from(required) {
            return Err(Refused::InsufficientWork {
                required_bits: required,
            });
        }
        self.make_room(&key, now_ms);
        self.sources.entry(key).or_default().push_back(now_ms);
        self.global.push_back(now_ms);
        Ok(Quote {
            required_bits: required,
            seq,
        })
    }

    fn make_room(&mut self, key: &[u8; 32], now_ms: u64) {
        if self.sources.len() < self.policy.max_sources || self.sources.contains_key(key) {
            return;
        }
        let window = self.policy.window_ms;
        self.sources
            .retain(|_, q| q.back().is_some_and(|t| now_ms.saturating_sub(*t) < window));
        if self.sources.len() >= self.policy.max_sources {
            // Still full of live sources: drop the quietest sixteenth in one
            // pass (amortized) rather than refusing everyone new.
            let mut last: Vec<(u64, [u8; 32])> = self
                .sources
                .iter()
                .map(|(k, q)| (q.back().copied().unwrap_or(0), *k))
                .collect();
            last.sort_unstable();
            for (_, k) in last.into_iter().take(self.policy.max_sources / 16 + 1) {
                self.sources.remove(&k);
            }
        }
    }

    /// Distinct sources currently tracked (for tests and monitoring).
    pub fn tracked_sources(&self) -> usize {
        self.sources.len()
    }
}

/// The work hash a client must grind: `BLAKE3(domain || source ||
/// request_digest || seq || nonce)`.
pub fn work_hash(source: &[u8], request_digest: &[u8; 32], seq: u32, nonce: u64) -> [u8; 32] {
    let mut input = Vec::with_capacity(DOMAIN.len() + source.len() + 32 + 12);
    input.extend_from_slice(DOMAIN);
    input.extend_from_slice(&(source.len() as u32).to_be_bytes());
    input.extend_from_slice(source);
    input.extend_from_slice(request_digest);
    input.extend_from_slice(&seq.to_be_bytes());
    input.extend_from_slice(&nonce.to_be_bytes());
    blake3_256(&input)
}

fn leading_zero_bits(h: &[u8; 32]) -> u32 {
    let mut n = 0;
    for b in h {
        if *b == 0 {
            n += 8;
        } else {
            n += b.leading_zeros();
            break;
        }
    }
    n
}

/// Honest-client helper: find a nonce meeting `quote` for this request, trying
/// at most `max_tries` nonces (returns `None` if not found).
pub fn solve(
    source: &[u8],
    request_digest: &[u8; 32],
    quote: Quote,
    max_tries: u64,
) -> Option<u64> {
    (0..max_tries).find(|nonce| {
        leading_zero_bits(&work_hash(source, request_digest, quote.seq, *nonce))
            >= u32::from(quote.required_bits)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small() -> AdmissionPolicy {
        AdmissionPolicy {
            window_ms: 5_000,
            base_bits: 2,
            bits_per_request: 1,
            max_bits: 12,
            global_soft_limit: 1_000,
            global_max_extra_bits: 4,
            max_sources: 64,
        }
    }
    fn digest(n: u8) -> [u8; 32] {
        blake3_256(&[n])
    }
    fn go(m: &mut AdmissionMeter, src: &[u8], d: u8, now: u64) -> Result<Quote, Refused> {
        let q = m.quote(src, now);
        let nonce = solve(src, &digest(d), q, 1 << 22).expect("solvable");
        m.admit(src, &digest(d), nonce, now)
    }

    #[test]
    fn each_request_in_the_window_raises_the_next_cost_by_one_bit() {
        let mut m = AdmissionMeter::new(small());
        let mut bits = vec![];
        for i in 0..6u8 {
            bits.push(
                go(&mut m, b"a", i, 1_000 + u64::from(i))
                    .unwrap()
                    .required_bits,
            );
        }
        assert_eq!(bits, vec![2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn cost_resets_after_a_quiet_window_and_sources_are_independent() {
        let mut m = AdmissionMeter::new(small());
        for i in 0..5u8 {
            go(&mut m, b"a", i, 1_000).unwrap();
        }
        assert_eq!(m.quote(b"a", 1_001).required_bits, 7);
        assert_eq!(m.quote(b"b", 1_001).required_bits, 2);
        assert_eq!(m.quote(b"a", 6_001).required_bits, 2);
    }

    #[test]
    fn insufficient_work_is_refused_and_does_not_raise_the_cost() {
        let mut m = AdmissionMeter::new(small());
        go(&mut m, b"a", 1, 0).unwrap();
        let q = m.quote(b"a", 1);
        // A nonce that fails the target (search for one that does not meet it).
        let bad = (0u64..)
            .find(|n| {
                leading_zero_bits(&work_hash(b"a", &digest(2), q.seq, *n))
                    < u32::from(q.required_bits)
            })
            .unwrap();
        assert!(matches!(
            m.admit(b"a", &digest(2), bad, 1),
            Err(Refused::InsufficientWork { .. })
        ));
        assert_eq!(m.quote(b"a", 2).seq, 1);
    }

    #[test]
    fn work_is_bound_to_the_request_and_cannot_be_replayed() {
        let mut m = AdmissionMeter::new(small());
        // A nonce good enough for seq 0 (2 bits) but not for seq 1 (3 bits).
        let nonce = (0u64..)
            .find(|n| {
                leading_zero_bits(&work_hash(b"a", &digest(1), 0, *n)) == 2
                    && leading_zero_bits(&work_hash(b"a", &digest(1), 1, *n)) < 3
            })
            .unwrap();
        assert!(m.admit(b"a", &digest(1), nonce, 0).is_ok());
        // The very same (request, nonce) can not be replayed: seq advanced.
        assert!(matches!(
            m.admit(b"a", &digest(1), nonce, 1),
            Err(Refused::InsufficientWork { required_bits: 3 })
        ));
        // And work is bound to the request digest.
        assert_ne!(
            work_hash(b"a", &digest(1), 0, nonce),
            work_hash(b"a", &digest(2), 0, nonce)
        );
    }

    #[test]
    fn a_flooding_source_is_refused_past_the_ceiling() {
        let mut m = AdmissionMeter::new(small());
        let mut ok = 0;
        for i in 0..40u8 {
            let q = m.quote(b"bot", 100);
            match solve(b"bot", &digest(i), q, 1 << 18) {
                Some(n) => match m.admit(b"bot", &digest(i), n, 100) {
                    Ok(_) => ok += 1,
                    Err(Refused::SourceExhausted) => break,
                    Err(_) => {}
                },
                None => break,
            }
        }
        // base 2 + one bit each: admitted while own bits <= 12 => seq 0..=10.
        assert_eq!(ok, 11);
        assert_eq!(
            m.admit(b"bot", &digest(99), 0, 100),
            Err(Refused::SourceExhausted)
        );
    }

    #[test]
    fn heavy_global_load_raises_everyones_cost_up_to_a_cap() {
        let mut p = small();
        p.global_soft_limit = 4;
        p.global_max_extra_bits = 3;
        p.max_sources = 4096;
        let mut m = AdmissionMeter::new(p);
        assert_eq!(m.quote(b"honest", 0).required_bits, 2);
        for i in 0..64u32 {
            let src = i.to_be_bytes();
            let q = m.quote(&src, 10);
            let n = solve(&src, &digest(0), q, 1 << 20).unwrap();
            m.admit(&src, &digest(0), n, 10).unwrap();
        }
        let q = m.quote(b"honest", 11);
        assert_eq!(q.required_bits, 2 + 3, "capped global extra");
        assert_eq!(m.quote(b"honest", 6_000).required_bits, 2, "load decays");
    }

    #[test]
    fn tracked_sources_stay_bounded() {
        let mut m = AdmissionMeter::new(small());
        for i in 0..500u32 {
            let src = i.to_be_bytes();
            let q = m.quote(&src, 10);
            let n = solve(&src, &digest(0), q, 1 << 20).unwrap();
            m.admit(&src, &digest(0), n, 10).unwrap();
            assert!(m.tracked_sources() <= 64 + 1);
        }
    }
}
