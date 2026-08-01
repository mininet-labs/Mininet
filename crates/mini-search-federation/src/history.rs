//! F7: historical snapshots — "Store and search versioned page
//! observations" (`docs/research/
//! MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
//! §29).
//!
//! F1 (`publish_crawl_observation`) already lets a caller store as many
//! independent [`mini_web_types::CrawlObservation`]s of the same URL over
//! time as it likes — nothing about F1's wire format assumes one
//! observation per URL. What was missing is the *search* half: given a
//! URL, find its observation history, what it looked like at a given
//! time, or which observations actually represent a distinct version
//! (not just a re-fetch of unchanged content). [`SnapshotIndex`] is a
//! small, local, in-memory structure a caller builds by feeding it
//! observations as they arrive — mirroring [`crate::rerank`]'s or
//! `mini_query::DocumentContextTable`'s own "caller-built local table,
//! not itself signed or stored" pattern — and then queries.

use std::collections::HashMap;

use mini_crypto::Multihash;
use mini_objects::ObjectId;
use mini_web_types::CanonicalUrl;

/// One observation's place in a URL's history: when it was made, which
/// signed [`mini_objects::Object`] (from [`crate::publish_crawl_observation`])
/// holds the full observation, and whether it represents a genuine content
/// change from the immediately-preceding snapshot in this index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub object_id: ObjectId,
    pub observed_at_ms: u64,
    pub content_digest: Option<Multihash>,
    /// `true` for the first snapshot of a URL, or if `content_digest`
    /// differs from the previous snapshot's; `false` if it matches
    /// (a re-fetch that found the same content). Two consecutive `None`
    /// digests are treated as unchanged (no signal either way), not as a
    /// change.
    pub content_changed: bool,
}

/// A local index from canonical URL to its observation history, sorted by
/// `observed_at_ms` ascending. Not signed, not itself stored — a caller
/// builds one from whatever observations it already holds (typically
/// fetched via [`crate::read_crawl_observation`] from a [`mini_store::Store`])
/// and queries it in memory.
#[derive(Debug, Default, Clone)]
pub struct SnapshotIndex {
    by_url: HashMap<String, Vec<Snapshot>>,
}

impl SnapshotIndex {
    pub fn new() -> Self {
        SnapshotIndex {
            by_url: HashMap::new(),
        }
    }

    /// Record one observation of `url`. Idempotent: inserting the same
    /// `object_id` for the same URL twice is a no-op. Snapshots are kept
    /// sorted by `observed_at_ms`; `content_changed` is (re)computed for
    /// every snapshot whose predecessor could have changed, so insertion
    /// order does not matter — the same set of observations always
    /// produces the same history.
    pub fn insert_observation(
        &mut self,
        url: &CanonicalUrl,
        object_id: ObjectId,
        observed_at_ms: u64,
        content_digest: Option<Multihash>,
    ) {
        let entries = self.by_url.entry(url.canonical_string()).or_default();
        if entries.iter().any(|s| s.object_id == object_id) {
            return;
        }
        entries.push(Snapshot {
            object_id,
            observed_at_ms,
            content_digest,
            content_changed: false, // recomputed below
        });
        entries.sort_by(|a, b| {
            a.observed_at_ms
                .cmp(&b.observed_at_ms)
                .then_with(|| a.object_id.as_str().cmp(b.object_id.as_str()))
        });
        recompute_content_changed(entries);
    }

    /// Full history for `url`, oldest first. Empty if nothing has been
    /// recorded for it.
    pub fn history(&self, url: &CanonicalUrl) -> &[Snapshot] {
        self.by_url
            .get(&url.canonical_string())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// The most recent snapshot of `url`, if any.
    pub fn latest(&self, url: &CanonicalUrl) -> Option<&Snapshot> {
        self.history(url).last()
    }

    /// The most recent snapshot observed at or before `ms` — "what did
    /// this page look like at time T" — or `None` if every snapshot postdates
    /// `ms` or none exist.
    pub fn at_or_before(&self, url: &CanonicalUrl, ms: u64) -> Option<&Snapshot> {
        self.history(url)
            .iter()
            .rev()
            .find(|s| s.observed_at_ms <= ms)
    }

    /// Snapshots of `url` in `[after_ms, before_ms)` — the identical
    /// inclusive-lower/exclusive-upper convention `mini_query::ParsedQuery`'s
    /// own `after_ms`/`before_ms` fields already use (an `after:` bound is
    /// pre-adjusted to the next day's midnight by the caller, exactly as
    /// `mini_query::parse_query` does), so a caller can pass those fields
    /// straight through. Either bound may be omitted.
    pub fn between(
        &self,
        url: &CanonicalUrl,
        after_ms: Option<u64>,
        before_ms: Option<u64>,
    ) -> Vec<&Snapshot> {
        self.history(url)
            .iter()
            .filter(|s| after_ms.is_none_or(|a| s.observed_at_ms >= a))
            .filter(|s| before_ms.is_none_or(|b| s.observed_at_ms < b))
            .collect()
    }

    /// Only the snapshots that represent a distinct version (the first
    /// snapshot, plus every later one whose content actually changed from
    /// its predecessor) — "search versioned page observations" without
    /// having to filter out repeat fetches of unchanged content by hand.
    pub fn distinct_versions(&self, url: &CanonicalUrl) -> Vec<&Snapshot> {
        self.history(url)
            .iter()
            .filter(|s| s.content_changed)
            .collect()
    }
}

fn recompute_content_changed(entries: &mut [Snapshot]) {
    for i in 0..entries.len() {
        entries[i].content_changed = match i {
            0 => true,
            _ => entries[i].content_digest != entries[i - 1].content_digest,
        };
    }
}
