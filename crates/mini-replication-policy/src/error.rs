/// Replication-placement errors. All are caller-input problems (a
/// malformed candidate list, an unknown shard index) — this crate never
/// fails for an internal reason.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReplicationError {
    /// The same [`did_mini::Did`] appeared twice in a candidate list.
    /// Rejected rather than silently deduplicated: a caller who did not
    /// intend to offer the same holder twice deserves to know their input
    /// was wrong, not have it quietly repaired.
    DuplicateCandidate,
    /// Fewer distinct candidates were offered than shards need holders.
    InsufficientDistinctCandidates { needed: usize, available: usize },
    /// A candidate offered as a *fresh* replacement holder already holds a
    /// different shard in this same plan — the one-holder-per-shard
    /// diversity invariant this crate exists to enforce.
    CandidateAlreadyHolder,
    /// A shard index outside `0..params.total_shards()`.
    UnknownShardIndex { index: usize, total: usize },
    /// The same shard index appeared twice in a repair request.
    DuplicateShardIndex { index: usize },
}

pub type Result<T> = std::result::Result<T, ReplicationError>;
