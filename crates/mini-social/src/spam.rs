//! Content-layer spam resistance that does not depend on identity scarcity
//! (roadmap [#75](../../issues/75)).
//!
//! ## Why not identity scarcity
//!
//! The obvious anti-spam lever — "cost per identity" — conflicts with open
//! participation (constitution principle 7) and with this codebase's own
//! honest limit that an identity root is not a verified human
//! (`docs/INVARIANTS.md`'s frozen top-of-file limitation): anything that
//! makes spam expensive *per identity* is trivially defeated by minting more
//! identity roots, and anything that tries to close that gap by restricting
//! *who may hold* an identity root re-introduces the Sybil-solves-spam
//! assumption this project explicitly has not made (roadmap #18). What
//! [`PostRateLimiter`] enforces instead is a **flat, equal budget per
//! identity root already seen locally** — every author, honest or not, gets
//! the same window/count regardless of reputation or how many identities an
//! attacker controls. It does not claim to stop a well-resourced Sybil flood
//! (an attacker who mints N identity roots gets N times the aggregate
//! budget); it stops the cheaper, more common case — one identity hammering
//! a feed — the same "friction, not gatekeeping" posture `mini-sync`'s KEL
//! cache and [`crate::pairing::PairingNonceLedger`] already take on their
//! own resource-exhaustion surfaces.
//!
//! ## Bounded like every other cache in this tree
//!
//! [`PostRateLimiter`] tracks recent post timestamps per author, but an
//! attacker who cycles through many distinct (possibly freshly rotated)
//! authors must not be able to grow that tracking structure without limit —
//! the same principle [`crate::pex::AddressBook`] (mini-net) and
//! [`crate::pairing::PairingNonceLedger`] enforce on their own per-key
//! state. [`PostRateLimiter`] caps the number of authors it tracks
//! simultaneously and evicts the least-recently-active one to make room,
//! mirroring [`crate::gossip::GossipRouter`]'s (mini-net) eviction shape —
//! reused here, one layer up, because the resource-exhaustion shape is
//! identical: bound a growth path an untrusted peer influences.
//!
//! This is local, per-viewer bookkeeping — it decides what a single
//! device's overlay accepts (or how a single device ranks) posts it has
//! seen, matching this crate's "the feed is a locally computed view"
//! stance (see the crate-level docs); it is not a consensus rule and two
//! devices with different limiter state can legitimately reach different
//! admission decisions for the same object, the same way two devices can
//! legitimately show different feeds.

use std::collections::{HashMap, VecDeque};

use did_mini::Did;

use crate::{Result, SocialError};

/// Hard cap on distinct authors one [`PostRateLimiter`] tracks at once.
/// Chosen generously relative to a realistic single-device follow/discovery
/// graph; existence of the cap matters far more than its exact size — see
/// the module docs' bounded-cache rationale.
pub const MAX_TRACKED_AUTHORS: usize = 100_000;

/// One author's recent post timestamps, oldest first, bounded to
/// [`PostRateLimiter::max_per_window`] entries per author (a full window
/// evicts its own oldest timestamp before a new one can be recorded, so
/// this never grows past that bound either).
#[derive(Debug, Default)]
struct AuthorWindow {
    timestamps_ms: VecDeque<u64>,
}

/// A local, per-viewer, identity-root-keyed post rate limiter — content-layer
/// spam resistance that never depends on identity scarcity (see module
/// docs). Every author gets the same flat budget: at most `max_per_window`
/// posts inside any trailing `window_ms` span, tracked from timestamps this
/// limiter itself has admitted (never from an author's own self-reported
/// clock skew: admission always uses `now_ms` as observed locally, and a
/// post whose *own* claimed `timestamp_ms` is what a caller wants bounded
/// should pass that value consistently, but the limiter itself does not
/// trust an author's claimed value as a substitute for call order).
#[derive(Debug)]
pub struct PostRateLimiter {
    window_ms: u64,
    max_per_window: usize,
    max_tracked_authors: usize,
    windows: HashMap<Did, AuthorWindow>,
    /// Least-recently-active author first, so a full tracker evicts the
    /// coldest entry rather than an arbitrary or newest one — the same
    /// shape [`crate::gossip::GossipRouter`] (mini-net) uses to bound its
    /// seen-set.
    lru: VecDeque<Did>,
}

impl PostRateLimiter {
    /// A limiter allowing at most `max_per_window` posts per author inside
    /// any trailing `window_ms` span, tracking at most `max_tracked_authors`
    /// authors at once (least-recently-active evicted first). Panics if
    /// `max_per_window` or `max_tracked_authors` is zero — a limiter that
    /// allows zero posts, or tracks zero authors, is not a rate limit, it is
    /// a full block, and should be expressed as such by the caller instead
    /// of silently behaving like one.
    pub fn new(window_ms: u64, max_per_window: usize, max_tracked_authors: usize) -> Self {
        assert!(max_per_window > 0, "max_per_window must be at least 1");
        assert!(
            max_tracked_authors > 0,
            "max_tracked_authors must be at least 1"
        );
        PostRateLimiter {
            window_ms,
            max_per_window,
            max_tracked_authors: max_tracked_authors.min(MAX_TRACKED_AUTHORS),
            windows: HashMap::new(),
            lru: VecDeque::new(),
        }
    }

    /// The conventional limiter for a live feed: 30 posts per rolling
    /// minute per author, tracking up to [`MAX_TRACKED_AUTHORS`] authors.
    pub fn standard() -> Self {
        PostRateLimiter::new(60_000, 30, MAX_TRACKED_AUTHORS)
    }

    fn touch_lru(&mut self, author: &Did) {
        if let Some(pos) = self.lru.iter().position(|d| d == author) {
            self.lru.remove(pos);
        }
        self.lru.push_back(author.clone());
    }

    fn evict_coldest_if_new_author_needs_room(&mut self, author: &Did) {
        if self.windows.contains_key(author) {
            return;
        }
        while self.windows.len() >= self.max_tracked_authors {
            let Some(coldest) = self.lru.pop_front() else {
                break;
            };
            self.windows.remove(&coldest);
        }
    }

    /// Check and record one post attempt by `author` at `now_ms` (the
    /// viewer/local-node's own clock, not an author-claimed timestamp — see
    /// the type's own docs). Returns `Ok(())` and records the attempt if
    /// under budget; returns [`SocialError::PostRateLimited`] without
    /// recording if `author` has already posted `max_per_window` times
    /// inside the trailing `window_ms` span. A caller wires this in front
    /// of [`crate::publish_post`]/[`crate::publish_media_post`] (or as a
    /// local admission check before storing a post fetched from a peer);
    /// it is not itself a `Store` operation.
    pub fn admit(&mut self, author: &Did, now_ms: u64) -> Result<()> {
        self.evict_coldest_if_new_author_needs_room(author);

        let window = self.windows.entry(author.clone()).or_default();
        let floor_ms = now_ms.saturating_sub(self.window_ms);
        while matches!(window.timestamps_ms.front(), Some(&t) if t < floor_ms) {
            window.timestamps_ms.pop_front();
        }

        if window.timestamps_ms.len() >= self.max_per_window {
            // Do not touch LRU order on a rejection: a spamming author
            // hammering the limiter must not thereby keep itself perpetually
            // "warm" and evict genuinely idle, well-behaved authors sooner.
            return Err(SocialError::PostRateLimited);
        }

        window.timestamps_ms.push_back(now_ms);
        self.touch_lru(author);
        Ok(())
    }

    /// How many authors this limiter currently tracks — bounded by
    /// `max_tracked_authors` regardless of how many distinct authors
    /// [`PostRateLimiter::admit`] has ever been called with.
    pub fn tracked_authors(&self) -> usize {
        self.windows.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_crypto::{encoding, HashAlgorithm, Multihash};

    /// A cheap, structurally-valid but otherwise meaningless `did:mini` —
    /// no asymmetric key generation, just a hash — matching the fixture
    /// shape `mini-chain`'s own tests use for the same reason: many
    /// distinct identifiers fast, none of which need to actually verify.
    fn did(tag: u32) -> Did {
        let mh = Multihash::of(HashAlgorithm::Blake3, &tag.to_be_bytes());
        let scid = encoding::encode(encoding::BASE58BTC, &mh.to_bytes()).unwrap();
        Did::from_scid(&scid).unwrap()
    }

    #[test]
    fn admits_up_to_the_per_window_budget_then_rejects() {
        let mut limiter = PostRateLimiter::new(60_000, 3, 10);
        let author = did(1);
        assert!(limiter.admit(&author, 0).is_ok());
        assert!(limiter.admit(&author, 10).is_ok());
        assert!(limiter.admit(&author, 20).is_ok());
        assert_eq!(
            limiter.admit(&author, 30),
            Err(SocialError::PostRateLimited)
        );
    }

    #[test]
    fn old_posts_age_out_of_the_window_and_free_up_budget() {
        let mut limiter = PostRateLimiter::new(1_000, 2, 10);
        let author = did(1);
        assert!(limiter.admit(&author, 0).is_ok());
        assert!(limiter.admit(&author, 500).is_ok());
        assert_eq!(
            limiter.admit(&author, 900),
            Err(SocialError::PostRateLimited)
        );
        // Past the window from the first post: budget frees up.
        assert!(limiter.admit(&author, 1_100).is_ok());
    }

    #[test]
    fn every_author_gets_the_same_flat_budget_regardless_of_identity_count() {
        // No identity-scarcity assumption: minting more identity roots gets
        // an attacker more aggregate budget, but never a *larger per-author*
        // budget, and never bypasses any single author's own cap.
        let mut limiter = PostRateLimiter::new(60_000, 1, 1_000);
        for n in 0..500u32 {
            let author = did(n);
            assert!(limiter.admit(&author, 0).is_ok());
            assert_eq!(
                limiter.admit(&author, 1),
                Err(SocialError::PostRateLimited),
                "author {n} exceeded its own flat per-author budget"
            );
        }
    }

    #[test]
    fn tracked_authors_never_exceeds_the_configured_cap_under_a_sybil_flood() {
        let mut limiter = PostRateLimiter::new(60_000, 5, 50);
        for n in 0..2_000u32 {
            let author = did(n);
            let _ = limiter.admit(&author, n as u64);
            assert!(limiter.tracked_authors() <= 50);
        }
        assert_eq!(limiter.tracked_authors(), 50);
    }

    #[test]
    fn a_rejected_attempt_does_not_refresh_the_authors_lru_position() {
        // An author already at its cap keeps hammering `admit` — this must
        // not keep it "warm" at the expense of evicting idle authors sooner
        // than an honest author who posted once and went quiet.
        let mut limiter = PostRateLimiter::new(60_000, 1, 2);
        let spammer = did(1);
        let quiet = did(2);
        assert!(limiter.admit(&spammer, 0).is_ok());
        assert!(limiter.admit(&quiet, 1).is_ok());
        // Spammer hammers far past its budget.
        for t in 2..1_000u64 {
            let _ = limiter.admit(&spammer, t);
        }
        // Tracker is full (2/2). A third, brand-new author needs room:
        // the coldest entry (by LRU, ignoring rejected-attempt noise) is
        // evicted. Since `quiet` was touched once and never rejected, and
        // `spammer`'s rejections never refresh its LRU position either,
        // whichever was least-recently *admitted* goes first — `spammer`
        // (admitted once at t=0) is colder than `quiet` (admitted at t=1).
        let newcomer = did(3);
        assert!(limiter.admit(&newcomer, 1_000).is_ok());
        assert_eq!(limiter.tracked_authors(), 2);
        // `quiet` (admitted once, at t=1) is warmer than `spammer` (whose
        // only *successful* admit was at t=0; its rejections never
        // refreshed its LRU position), so eviction should have dropped
        // `spammer`, not `quiet`: `quiet` is still tracked and still within
        // its own one-post budget (rejected), while `spammer` was evicted
        // and so gets a fresh window (accepted).
        assert!(
            limiter.admit(&quiet, 2_000).is_err(),
            "quiet author must still be tracked with its budget already spent"
        );
        assert!(
            limiter.admit(&spammer, 2_001).is_ok(),
            "spammer must have been the one evicted to make room for newcomer"
        );
    }
}
