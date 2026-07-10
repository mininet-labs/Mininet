//! Error type for `mini-erasure`.

use core::fmt;

/// Errors this crate can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErasureError {
    /// `encode()` was given zero bytes -- nothing to shard.
    EmptyData,
    /// `ErasureParams` had `data_shards == 0`, `parity_shards == 0`, or a
    /// total shard count above 255 (the largest index `GF(2^8)`'s
    /// Vandermonde construction can assign a distinct nonzero coefficient
    /// to).
    InvalidParams {
        data_shards: usize,
        parity_shards: usize,
    },
    /// `reconstruct()` was given a shard slice whose length did not match
    /// `data_shards + parity_shards`.
    WrongShardCount { expected: usize, got: usize },
    /// Fewer than `data_shards` shards were available -- erasure coding
    /// cannot recover data from less than the original share count, by
    /// construction (this is the same limit ordinary replication has,
    /// just at a smaller per-shard cost).
    TooManyMissingShards { available: usize, needed: usize },
    /// An internal consistency failure: the selected `k` shard rows of the
    /// generator matrix failed to invert. Never expected in practice --
    /// [`crate::matrix::generator_matrix`]'s Vandermonde construction
    /// guarantees every `k`-row subset is invertible.
    SingularSubmatrix,
    /// A shard's actual bytes did not hash to its recorded digest --
    /// silent corruption, not mere absence. Caught by
    /// [`crate::health::verify_shard`] before a corrupted shard is ever
    /// trusted as "available".
    IntegrityMismatch { shard_index: usize },
}

impl fmt::Display for ErasureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErasureError::EmptyData => write!(f, "cannot encode zero bytes"),
            ErasureError::InvalidParams {
                data_shards,
                parity_shards,
            } => write!(
                f,
                "invalid erasure params: data_shards={data_shards}, parity_shards={parity_shards} (need both >= 1 and their sum <= 255)"
            ),
            ErasureError::WrongShardCount { expected, got } => {
                write!(f, "expected {expected} shard slots, got {got}")
            }
            ErasureError::TooManyMissingShards { available, needed } => write!(
                f,
                "only {available} shards available, need at least {needed} to reconstruct"
            ),
            ErasureError::SingularSubmatrix => {
                write!(f, "internal error: selected shard rows produced a singular matrix")
            }
            ErasureError::IntegrityMismatch { shard_index } => {
                write!(f, "shard {shard_index} failed its integrity check")
            }
        }
    }
}

impl std::error::Error for ErasureError {}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, ErasureError>;
