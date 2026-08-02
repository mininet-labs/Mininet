//! F7: bounded local history over signed crawl-observation objects.
//!
//! F1 (`publish_crawl_observation`) already stores independent
//! [`mini_web_types::CrawlObservation`] objects over time. This module builds
//! a rebuildable local view over those objects. It deliberately does not turn
//! crawler-claimed timestamps into canonical time, one provider's observation
//! into truth, or an absent digest into a content change.
//!
//! Authentication remains layered exactly as in F1: callers verify the
//! wrapping [`mini_objects::Object`] before calling `read_crawl_observation`,
//! then pass the resulting observation and object id here. This index checks
//! internal consistency, deterministic ordering, canonical F1 field bounds,
//! and bounded memory proxies; it does not re-verify signatures or derive the
//! still-caller-supplied `CrawlObservationId`.

use std::collections::HashMap;

use mini_crypto::Multihash;
use mini_objects::ObjectId;
use mini_web_types::{CanonicalUrl, CrawlObservation};

use crate::error::{FederationError, Result};
use crate::observation::observation_wire_len;

/// Default ceiling for the number of final URLs held by one local index.
/// Production defaults still require weakest-device measurement; callers may
/// choose smaller limits immediately.
pub const DEFAULT_MAX_SNAPSHOT_URLS: usize = 4_096;
/// Default history depth for one final URL.
pub const DEFAULT_MAX_SNAPSHOTS_PER_URL: usize = 256;
/// Default total observation count across the local index.
pub const DEFAULT_MAX_TOTAL_SNAPSHOTS: usize = 32_768;
/// Default canonical F1 payload-byte ceiling for one stored observation.
pub const DEFAULT_MAX_SNAPSHOT_WIRE_BYTES: usize = 64 * 1024;
/// Default total canonical F1 payload-byte ceiling across the index.
pub const DEFAULT_MAX_TOTAL_SNAPSHOT_WIRE_BYTES: usize = 16 * 1024 * 1024;

/// Explicit local-memory bounds for [`SnapshotIndex`]. A zero field is valid
/// and disables insertion for that dimension.
///
/// The byte fields count canonical F1 payload bytes, not allocator-specific
/// Rust heap overhead. This makes the accounting deterministic and reviewable,
/// while remaining an honest proxy rather than a claim of exact resident RAM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotLimits {
    pub max_urls: usize,
    pub max_snapshots_per_url: usize,
    pub max_total_snapshots: usize,
    pub max_snapshot_wire_bytes: usize,
    pub max_total_snapshot_wire_bytes: usize,
}

impl Default for SnapshotLimits {
    fn default() -> Self {
        SnapshotLimits {
            max_urls: DEFAULT_MAX_SNAPSHOT_URLS,
            max_snapshots_per_url: DEFAULT_MAX_SNAPSHOTS_PER_URL,
            max_total_snapshots: DEFAULT_MAX_TOTAL_SNAPSHOTS,
            max_snapshot_wire_bytes: DEFAULT_MAX_SNAPSHOT_WIRE_BYTES,
            max_total_snapshot_wire_bytes: DEFAULT_MAX_TOTAL_SNAPSHOT_WIRE_BYTES,
        }
    }
}

/// What this observation can honestly say about the last earlier known digest.
///
/// This is an observation relation, not proof of when the origin changed. The
/// timestamp is supplied by the crawler and several providers may disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum VersionRelation {
    /// First digest-bearing observation in the locally held history.
    Baseline,
    /// Same digest as the last earlier digest-bearing observation.
    Unchanged,
    /// Different digest from the last earlier digest-bearing observation.
    Changed,
    /// This observation carries no digest, so no version statement is possible.
    Unknown,
    /// Digest-bearing observations carrying the same timestamp disagree.
    /// No arbitrary object-id ordering is promoted into a temporal change.
    SameTimestampDisagreement,
}

/// One F1 observation in a final URL's local history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// Content id of the signed F1 object that carried `observation`.
    pub object_id: ObjectId,
    /// Full decoded observation, preserving crawler pseudonym, requested/final
    /// URL, status, digest, redirect chain, and claimed observation time.
    pub observation: CrawlObservation,
    /// Canonical F1 payload size used for deterministic local budget accounting.
    pub wire_bytes: usize,
    /// Relation to the last earlier known digest, recomputed deterministically
    /// whenever insertion changes ordering.
    pub version_relation: VersionRelation,
}

/// Result of inserting one signed-observation object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotInsert {
    Inserted,
    AlreadyPresent,
}

/// A bounded local index from an observation's **final URL** to its history.
///
/// The index is not signed, persisted, or exchanged. It is reconstructed from
/// whatever authenticated F1 objects the caller already holds. `requested_url`
/// aliases and redirect discovery remain in each stored observation but are not
/// silently indexed as if they were the fetched resource itself.
#[derive(Debug, Clone)]
pub struct SnapshotIndex {
    limits: SnapshotLimits,
    by_final_url: HashMap<String, Vec<Snapshot>>,
    object_bindings: HashMap<ObjectId, String>,
    total_snapshots: usize,
    total_wire_bytes: usize,
}

impl Default for SnapshotIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotIndex {
    /// Create an index with explicit, unbenchmarked safety ceilings. These are
    /// finite defaults, not a production weakest-device claim.
    pub fn new() -> Self {
        Self::with_limits(SnapshotLimits::default())
    }

    /// Create an index with caller-selected explicit bounds.
    pub fn with_limits(limits: SnapshotLimits) -> Self {
        SnapshotIndex {
            limits,
            by_final_url: HashMap::new(),
            object_bindings: HashMap::new(),
            total_snapshots: 0,
            total_wire_bytes: 0,
        }
    }

    pub fn limits(&self) -> SnapshotLimits {
        self.limits
    }

    pub fn len(&self) -> usize {
        self.total_snapshots
    }

    pub fn is_empty(&self) -> bool {
        self.total_snapshots == 0
    }

    pub fn url_count(&self) -> usize {
        self.by_final_url.len()
    }

    /// Canonical F1 payload bytes currently counted against this local index's
    /// deterministic byte budget.
    pub fn total_wire_bytes(&self) -> usize {
        self.total_wire_bytes
    }

    /// Insert one already-decoded F1 observation, deriving every indexed field
    /// from that typed observation rather than accepting parallel caller-supplied
    /// URL/time/digest values that could disagree with it.
    ///
    /// The same object id with the exact same observation is idempotent. Reusing
    /// one content id for different observation bytes or a different final URL
    /// fails closed as [`FederationError::ConflictingObjectBinding`]. A new
    /// observation must also satisfy the same canonical field bounds as F1's
    /// publisher/reader and every configured count/byte ceiling before mutation.
    pub fn insert_observation(
        &mut self,
        object_id: ObjectId,
        observation: CrawlObservation,
    ) -> Result<SnapshotInsert> {
        let key = observation.final_url.canonical_string();

        if let Some(existing_key) = self.object_bindings.get(&object_id) {
            let existing = self
                .by_final_url
                .get(existing_key)
                .and_then(|entries| entries.iter().find(|entry| entry.object_id == object_id))
                .ok_or(FederationError::ConflictingObjectBinding)?;
            if existing_key != &key || existing.observation != observation {
                return Err(FederationError::ConflictingObjectBinding);
            }
            return Ok(SnapshotInsert::AlreadyPresent);
        }

        let wire_bytes = observation_wire_len(&observation)?;
        let next_total_wire_bytes = self
            .total_wire_bytes
            .checked_add(wire_bytes)
            .ok_or(FederationError::LimitExceeded)?;
        let is_new_url = !self.by_final_url.contains_key(&key);
        let snapshots_for_url = self.by_final_url.get(&key).map_or(0, Vec::len);
        if (is_new_url && self.by_final_url.len() >= self.limits.max_urls)
            || self.total_snapshots >= self.limits.max_total_snapshots
            || snapshots_for_url >= self.limits.max_snapshots_per_url
            || wire_bytes > self.limits.max_snapshot_wire_bytes
            || next_total_wire_bytes > self.limits.max_total_snapshot_wire_bytes
        {
            return Err(FederationError::LimitExceeded);
        }

        let binding_id = object_id.clone();
        let entries = self.by_final_url.entry(key.clone()).or_default();
        entries.push(Snapshot {
            object_id,
            observation,
            wire_bytes,
            version_relation: VersionRelation::Unknown,
        });
        entries.sort_by(|a, b| {
            a.observation
                .observed_at_ms
                .cmp(&b.observation.observed_at_ms)
                .then_with(|| a.object_id.as_str().cmp(b.object_id.as_str()))
        });
        recompute_version_relations(entries);

        self.object_bindings.insert(binding_id, key);
        self.total_snapshots += 1;
        self.total_wire_bytes = next_total_wire_bytes;
        Ok(SnapshotInsert::Inserted)
    }

    /// Full history for a final URL, oldest claimed timestamp first. Equal
    /// timestamps are deterministically ordered by object id but are not
    /// treated as a real temporal order.
    pub fn history(&self, final_url: &CanonicalUrl) -> &[Snapshot] {
        self.by_final_url
            .get(&final_url.canonical_string())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Every observation at the greatest timestamp held for `final_url`.
    /// Returning the whole timestamp group avoids arbitrarily selecting one
    /// provider when equally-timestamped observations disagree.
    pub fn latest(&self, final_url: &CanonicalUrl) -> &[Snapshot] {
        self.at_or_before(final_url, u64::MAX)
    }

    /// Every observation at the greatest crawler-claimed timestamp at or
    /// before `ms`. Empty when none qualify. This answers “what observations
    /// do I hold for the latest point not after T,” not “what was objectively
    /// true at T.”
    pub fn at_or_before(&self, final_url: &CanonicalUrl, ms: u64) -> &[Snapshot] {
        let history = self.history(final_url);
        let end = history.partition_point(|s| s.observation.observed_at_ms <= ms);
        if end == 0 {
            return &[];
        }
        let timestamp = history[end - 1].observation.observed_at_ms;
        let start = history[..end].partition_point(|s| s.observation.observed_at_ms < timestamp);
        &history[start..end]
    }

    /// Observations in `[after_ms, before_ms)`, using the same lower-inclusive,
    /// upper-exclusive convention as `mini_query::ParsedQuery`.
    pub fn between(
        &self,
        final_url: &CanonicalUrl,
        after_ms: Option<u64>,
        before_ms: Option<u64>,
    ) -> Vec<&Snapshot> {
        self.history(final_url)
            .iter()
            .filter(|s| after_ms.is_none_or(|a| s.observation.observed_at_ms >= a))
            .filter(|s| before_ms.is_none_or(|b| s.observation.observed_at_ms < b))
            .collect()
    }

    /// One deterministic representative observation for each locally
    /// supportable version boundary. Unknown-digest and same-timestamp-
    /// disagreement observations are excluded rather than promoted into false
    /// changes. Multiple corroborating observations with the same timestamp and
    /// digest collapse to the smallest object id after deterministic sorting.
    pub fn distinct_versions(&self, final_url: &CanonicalUrl) -> Vec<&Snapshot> {
        let mut versions: Vec<&Snapshot> = Vec::new();
        for snapshot in self.history(final_url) {
            if !matches!(
                snapshot.version_relation,
                VersionRelation::Baseline | VersionRelation::Changed
            ) {
                continue;
            }
            let duplicate_group = versions.iter().any(|existing| {
                existing.observation.observed_at_ms == snapshot.observation.observed_at_ms
                    && existing.observation.content_digest == snapshot.observation.content_digest
            });
            if !duplicate_group {
                versions.push(snapshot);
            }
        }
        versions
    }

    /// All observations in a same-timestamp group whose known digests disagree.
    pub fn disagreements(&self, final_url: &CanonicalUrl) -> Vec<&Snapshot> {
        self.history(final_url)
            .iter()
            .filter(|s| s.version_relation == VersionRelation::SameTimestampDisagreement)
            .collect()
    }
}

fn recompute_version_relations(entries: &mut [Snapshot]) {
    for entry in entries.iter_mut() {
        entry.version_relation = VersionRelation::Unknown;
    }

    let mut previous_known: Option<Multihash> = None;
    let mut start = 0;
    while start < entries.len() {
        let timestamp = entries[start].observation.observed_at_ms;
        let mut end = start + 1;
        while end < entries.len() && entries[end].observation.observed_at_ms == timestamp {
            end += 1;
        }

        let mut agreed_digest: Option<Multihash> = None;
        let mut disagreement = false;
        for entry in &entries[start..end] {
            if let Some(digest) = &entry.observation.content_digest {
                match &agreed_digest {
                    None => agreed_digest = Some(digest.clone()),
                    Some(existing) if existing != digest => disagreement = true,
                    Some(_) => {}
                }
            }
        }

        if disagreement {
            for entry in &mut entries[start..end] {
                entry.version_relation = if entry.observation.content_digest.is_some() {
                    VersionRelation::SameTimestampDisagreement
                } else {
                    VersionRelation::Unknown
                };
            }
            // Do not choose one disagreeing digest as the next comparison base.
        } else if let Some(digest) = agreed_digest {
            let relation = match &previous_known {
                None => VersionRelation::Baseline,
                Some(previous) if previous == &digest => VersionRelation::Unchanged,
                Some(_) => VersionRelation::Changed,
            };
            for entry in &mut entries[start..end] {
                entry.version_relation = if entry.observation.content_digest.is_some() {
                    relation
                } else {
                    VersionRelation::Unknown
                };
            }
            previous_known = Some(digest);
        }

        start = end;
    }
}
