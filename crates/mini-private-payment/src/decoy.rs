//! Choosing who you hide among — one rule, the same for everyone.
//!
//! # Why this is a protocol rule and not a wallet setting
//!
//! Before this, `PaymentRequest` took a caller-supplied `ring`, and the crate
//! said outright that decoy quality was the caller's problem. That is two
//! separate failures wearing one coat:
//!
//! 1. **Bad decoys break the payment that uses them.** A ring of sixteen
//!    whose other fifteen members are visibly long-spent outputs hides
//!    nobody, and every signature check still passes. The failure is silent.
//! 2. **Different decoys break payments that don't.** If two wallets sample
//!    differently, an observer can tell *which wallet* made a payment from
//!    the shape of its ring. Wallet fingerprinting is a deanonymization
//!    vector in its own right, and it harms users who did nothing wrong —
//!    including the users of the *better* wallet, because "unusual" is what
//!    stands out. A per-wallet choice makes every wallet's users a smaller
//!    anonymity set.
//!
//! (2) is why this cannot be left to implementations even in principle, and
//! it is why the founder's direction was that the anonymity set be enforced
//! by protocol for everyone. So there is one sampling rule, it lives here,
//! and [`select_ring`] is how a ring is built.
//!
//! # Why not a mixer
//!
//! A mixer is a service: value in, different value out. That reintroduces a
//! coordinator, a pool, an operator, and something that can be seized or can
//! simply vanish — precisely what Directive 2 says to assume about every
//! service. It also requires trusting the mixer's logging policy, which is a
//! promise rather than mathematics (Directive 9).
//!
//! A ring signature **is already the mixing**, performed locally, with no
//! pool and nothing to shut down. Nothing needed adding; what was missing
//! was only the rule for choosing who to mix with.
//!
//! # Why not ask a server for decoys
//!
//! Because asking is the leak. A peer that serves you decoy keys learns your
//! ring, and your ring contains your real output. There is no way to phrase
//! that request that does not hand over the answer. So [`OutputSet`] is a
//! **local** view, and a device that cannot hold one does not make private
//! payments from that device — see the crate docs' honest limits.
//!
//! # The rule: recent-weighted, integer-only
//!
//! Real spends skew recent — people mostly spend outputs they received
//! lately. Uniform decoy selection therefore fails immediately: in a ring
//! where one member is far newer than the rest, the newest is the real one
//! with high probability. This is not hypothetical; it is the attack that
//! forced the wider field to abandon uniform selection.
//!
//! So decoys are drawn with the same recency skew real spends have, from
//! [`AGE_WEIGHTS`] — a frozen table over logarithmic age buckets.
//!
//! **No floating point anywhere.** A protocol rule computed in `f64` is a
//! protocol rule two platforms can disagree about, and a wallet that samples
//! differently from its peers is a wallet whose users are identifiable. Every
//! step here is integer arithmetic over a fixed table, so the same
//! `(entropy, output_count)` yields the same ring on every machine forever.

use mini_crypto::HashAlgorithm;

use crate::error::{PrivatePaymentError, Result};

/// Domain separator for decoy index derivation.
pub const DECOY_DOMAIN: &[u8] = b"mininet/mini-private-payment/decoy/v1";

/// Cumulative weights over logarithmic age buckets, newest first.
///
/// Bucket `i` covers outputs whose age (in positions back from the newest)
/// falls in `[2^i - 1, 2^(i+1) - 1)`. The weights approximate the shape of a
/// real spend-age distribution — heavily recent, with a long tail that never
/// reaches zero, because a decoy set that could never include an old output
/// would mark every genuinely old spend as real.
///
/// **Frozen.** Changing these numbers changes which rings are typical, which
/// splits the anonymity set between old and new wallets — the exact harm
/// this table exists to prevent. A change is a version bump and a decision
/// entry, never a tuning commit.
///
/// These are a legible starting shape, **not** a distribution fitted to
/// measured traffic — no such traffic exists yet. The design document
/// records that as the open question it is.
pub const AGE_WEIGHTS: [u32; 16] = [
    // Newest first: roughly half of all spends land in the newest few
    // positions, and the tail decays but never vanishes.
    4096, 2048, 1024, 640, 400, 256, 160, 100, 64, 40, 25, 16, 10, 6, 4, 2,
];

/// A local, append-only view of spendable outputs.
///
/// Index order is age order: index 0 is the oldest output the wallet knows
/// about, `len() - 1` the newest. That ordering is the only thing the
/// sampler needs, and it deliberately needs nothing else — no timestamps, no
/// amounts, no owner information, because a sampler that consulted any of
/// those would leak them into the choice of ring.
pub trait OutputSet {
    /// How many outputs this view holds.
    fn len(&self) -> usize;

    /// Whether the view is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The one-time public key at `index`, or `None` if out of range.
    fn key_at(&self, index: usize) -> Option<Vec<u8>>;
}

/// An in-memory [`OutputSet`], for tests and small wallets.
#[derive(Debug, Clone, Default)]
pub struct InMemoryOutputSet {
    keys: Vec<Vec<u8>>,
}

impl InMemoryOutputSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an output. Callers must append in the order outputs appeared,
    /// oldest first — the sampler reads index order as age order and cannot
    /// detect a caller that shuffles.
    pub fn push(&mut self, key: impl Into<Vec<u8>>) {
        self.keys.push(key.into());
    }

    pub fn from_keys(keys: Vec<Vec<u8>>) -> Self {
        Self { keys }
    }
}

impl OutputSet for InMemoryOutputSet {
    fn len(&self) -> usize {
        self.keys.len()
    }

    fn key_at(&self, index: usize) -> Option<Vec<u8>> {
        self.keys.get(index).cloned()
    }
}

/// Deterministic integer randomness from a domain-separated transcript.
///
/// A counter-mode hash rather than a general PRNG: reproducible on every
/// platform, no floating point, no library-version dependence, and
/// inspectable by anyone re-deriving a ring from the same inputs.
struct Draw {
    entropy: [u8; 32],
    counter: u64,
}

impl Draw {
    fn new(entropy: &[u8; 32]) -> Self {
        Self {
            entropy: *entropy,
            counter: 0,
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut transcript = Vec::with_capacity(DECOY_DOMAIN.len() + 40);
        transcript.extend_from_slice(DECOY_DOMAIN);
        transcript.extend_from_slice(&self.entropy);
        transcript.extend_from_slice(&self.counter.to_be_bytes());
        self.counter += 1;
        let digest = HashAlgorithm::Blake3.digest(&transcript);
        u64::from_be_bytes(digest[0..8].try_into().expect("32-byte digest"))
    }

    /// A value in `[0, bound)`, rejection-sampled so the result is uniform.
    ///
    /// Plain modulo would bias toward small values whenever `bound` does not
    /// divide `2^64`. That bias is tiny for small bounds and still wrong:
    /// biased decoy indices are a distribution an observer can fit against,
    /// which is the whole game here.
    fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            return 0;
        }
        let limit = u64::MAX - (u64::MAX % bound);
        loop {
            let value = self.next_u64();
            if value < limit {
                return value % bound;
            }
        }
    }
}

/// Pick one age offset (positions back from the newest) under [`AGE_WEIGHTS`].
fn draw_age_offset(draw: &mut Draw, output_count: usize) -> usize {
    let total: u64 = AGE_WEIGHTS.iter().map(|w| u64::from(*w)).sum();
    let mut target = draw.below(total);

    let mut bucket = AGE_WEIGHTS.len() - 1;
    for (index, weight) in AGE_WEIGHTS.iter().enumerate() {
        let weight = u64::from(*weight);
        if target < weight {
            bucket = index;
            break;
        }
        target -= weight;
    }

    // Bucket `i` spans ages [2^i - 1, 2^(i+1) - 1).
    let low = (1u64 << bucket) - 1;
    let high = (1u64 << (bucket + 1)) - 1;
    let span = high - low;
    let offset = low + draw.below(span);

    // Clamp into the set. A young wallet whose whole history is shorter than
    // the bucket simply gets an older-than-intended output; refusing would
    // mean no private payments until the set is large, which is worse.
    (offset as usize).min(output_count.saturating_sub(1))
}

/// Build a ring containing the caller's real output plus protocol-chosen
/// decoys, and report where the real output landed.
///
/// `entropy` must be fresh per payment. Reusing it reproduces the same ring,
/// and two payments sharing a ring are visibly related.
///
/// Returns the canonically-sorted ring and the real output's index within
/// it. Sorting is what keeps one payment to one encoding — and it means the
/// real output's *position* carries no information, since position is
/// determined by key bytes rather than by anything the sender chose.
pub fn select_ring(
    outputs: &impl OutputSet,
    real_index: usize,
    ring_size: usize,
    entropy: &[u8; 32],
) -> Result<(Vec<Vec<u8>>, usize)> {
    if ring_size < crate::MIN_RING_SIZE {
        return Err(PrivatePaymentError::RingTooSmall {
            got: ring_size,
            min: crate::MIN_RING_SIZE,
        });
    }
    if ring_size > crate::MAX_RING_SIZE {
        return Err(PrivatePaymentError::RingTooLarge {
            got: ring_size,
            max: crate::MAX_RING_SIZE,
        });
    }
    let real_key = outputs
        .key_at(real_index)
        .ok_or(PrivatePaymentError::RealOutputNotInSet)?;
    if outputs.len() < ring_size {
        return Err(PrivatePaymentError::OutputSetTooSmall {
            got: outputs.len(),
            need: ring_size,
        });
    }

    let newest = outputs.len() - 1;
    let mut draw = Draw::new(entropy);
    let mut ring: Vec<Vec<u8>> = vec![real_key.clone()];

    // Bounded attempts: with a set barely larger than the ring, the age
    // distribution can keep proposing indices already taken. Falling back to
    // uniform fill is honest degradation -- a slightly worse distribution is
    // better than a wallet that cannot pay at all -- and it only happens on
    // sets small enough that the distribution was never protecting much.
    let mut attempts = 0usize;
    let max_attempts = ring_size * 64;
    while ring.len() < ring_size && attempts < max_attempts {
        attempts += 1;
        let offset = draw_age_offset(&mut draw, outputs.len());
        let index = newest - offset;
        let Some(key) = outputs.key_at(index) else {
            continue;
        };
        if !ring.contains(&key) {
            ring.push(key);
        }
    }
    while ring.len() < ring_size {
        let index = draw.below(outputs.len() as u64) as usize;
        if let Some(key) = outputs.key_at(index) {
            if !ring.contains(&key) {
                ring.push(key);
            }
        }
    }

    crate::canonicalize_ring(&mut ring);
    if ring.len() != ring_size {
        // Duplicate keys in the output set itself collapsed the ring below
        // its declared size. Refuse rather than silently sign a smaller
        // anonymity set than the caller asked for.
        return Err(PrivatePaymentError::OutputSetTooSmall {
            got: ring.len(),
            need: ring_size,
        });
    }

    let position = ring
        .iter()
        .position(|member| *member == real_key)
        .expect("the real key was inserted first and canonicalization only reorders");
    Ok((ring, position))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_of(n: usize) -> InMemoryOutputSet {
        InMemoryOutputSet::from_keys(
            (0..n)
                .map(|i| {
                    let mut key = vec![0u8; 32];
                    key[0..8].copy_from_slice(&(i as u64).to_be_bytes());
                    key
                })
                .collect(),
        )
    }

    #[test]
    fn the_real_output_is_always_in_the_ring_at_the_reported_index() {
        let outputs = set_of(1_000);
        for real in [0usize, 1, 499, 998, 999] {
            let (ring, position) = select_ring(&outputs, real, 16, &[7u8; 32]).unwrap();
            assert_eq!(ring.len(), 16);
            assert_eq!(ring[position], outputs.key_at(real).unwrap());
        }
    }

    #[test]
    fn the_same_entropy_reproduces_the_same_ring_exactly() {
        // Determinism is not a convenience: a rule two machines compute
        // differently is a rule that fingerprints whichever machine is in
        // the minority.
        let outputs = set_of(500);
        let a = select_ring(&outputs, 42, 16, &[3u8; 32]).unwrap();
        let b = select_ring(&outputs, 42, 16, &[3u8; 32]).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn different_entropy_produces_different_decoys() {
        let outputs = set_of(500);
        let (a, _) = select_ring(&outputs, 42, 16, &[1u8; 32]).unwrap();
        let (b, _) = select_ring(&outputs, 42, 16, &[2u8; 32]).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn rings_are_canonical_and_free_of_duplicates() {
        let outputs = set_of(300);
        for seed in 0u8..16 {
            let (ring, _) = select_ring(&outputs, 100, 16, &[seed; 32]).unwrap();
            assert!(crate::ring_is_canonical(&ring), "seed {seed} not canonical");
            let mut deduped = ring.clone();
            deduped.dedup();
            assert_eq!(deduped.len(), ring.len());
        }
    }

    #[test]
    fn selection_skews_recent_but_still_reaches_old_outputs() {
        // Both halves matter. Skewing recent is what makes decoys resemble
        // real spends; still reaching old outputs is what keeps a genuinely
        // old spend from being obvious by having no plausible company.
        let outputs = set_of(4_096);
        let newest = outputs.len() - 1;
        let mut recent = 0usize;
        let mut ancient = 0usize;

        for seed in 0u16..200 {
            let mut entropy = [0u8; 32];
            entropy[0..2].copy_from_slice(&seed.to_be_bytes());
            let (ring, _) = select_ring(&outputs, 2_000, 16, &entropy).unwrap();
            for member in &ring {
                let index = u64::from_be_bytes(member[0..8].try_into().unwrap()) as usize;
                let age = newest - index;
                if age < 64 {
                    recent += 1;
                } else if age > 1_000 {
                    ancient += 1;
                }
            }
        }
        assert!(recent > ancient, "recent {recent} vs ancient {ancient}");
        assert!(ancient > 0, "the tail must be reachable, got {ancient}");
    }

    #[test]
    fn a_ring_below_the_floor_is_refused() {
        let outputs = set_of(100);
        assert!(matches!(
            select_ring(&outputs, 0, 4, &[0u8; 32]),
            Err(PrivatePaymentError::RingTooSmall { .. })
        ));
    }

    #[test]
    fn an_output_set_smaller_than_the_ring_is_refused() {
        // Signing a ring larger than the set would mean repeating members,
        // which looks like anonymity and is not.
        let outputs = set_of(10);
        assert!(matches!(
            select_ring(&outputs, 0, 16, &[0u8; 32]),
            Err(PrivatePaymentError::OutputSetTooSmall { .. })
        ));
    }

    #[test]
    fn a_real_index_outside_the_set_is_refused() {
        let outputs = set_of(100);
        assert!(matches!(
            select_ring(&outputs, 500, 16, &[0u8; 32]),
            Err(PrivatePaymentError::RealOutputNotInSet)
        ));
    }

    #[test]
    fn a_set_exactly_the_ring_size_still_produces_a_full_ring() {
        // The degenerate case: every output is in the ring. Anonymity is
        // nil, but the payment must still be constructible -- refusing here
        // would strand the earliest users of a young network entirely.
        let outputs = set_of(16);
        let (ring, position) = select_ring(&outputs, 3, 16, &[9u8; 32]).unwrap();
        assert_eq!(ring.len(), 16);
        assert_eq!(ring[position], outputs.key_at(3).unwrap());
    }

    #[test]
    fn the_draw_is_uniform_below_its_bound() {
        // Rejection sampling, not modulo: biased indices are a distribution
        // an observer can fit against.
        let mut draw = Draw::new(&[5u8; 32]);
        let mut counts = [0usize; 7];
        for _ in 0..7_000 {
            counts[draw.below(7) as usize] += 1;
        }
        for count in counts {
            assert!((800..1_200).contains(&count), "skewed: {counts:?}");
        }
    }

    #[test]
    fn the_age_weight_table_is_frozen() {
        // Changing these splits the anonymity set between old and new
        // wallets, which is the harm the table exists to prevent.
        assert_eq!(AGE_WEIGHTS.len(), 16);
        assert_eq!(AGE_WEIGHTS[0], 4096);
        assert_eq!(*AGE_WEIGHTS.last().unwrap(), 2);
        assert!(
            AGE_WEIGHTS.windows(2).all(|pair| pair[0] > pair[1]),
            "weights must decrease monotonically with age"
        );
        assert_eq!(AGE_WEIGHTS.iter().map(|w| u64::from(*w)).sum::<u64>(), 8891);
    }
}
