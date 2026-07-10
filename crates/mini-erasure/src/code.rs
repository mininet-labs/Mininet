//! Systematic Reed-Solomon encode/reconstruct: split data into
//! `data_shards` pieces, compute `parity_shards` additional pieces from
//! [`crate::matrix::generator_matrix`], and recover the original data from
//! *any* `data_shards` of the resulting `data_shards + parity_shards`
//! total shards -- the maximum-distance-separable (MDS) property a
//! Vandermonde generator matrix guarantees.

use crate::error::{ErasureError, Result};
use crate::matrix::{generator_matrix, Matrix};

/// How many data shards to split into, and how many parity shards to
/// compute alongside them. `n = data_shards + parity_shards` total shards
/// are produced; any `data_shards` of them reconstruct the original data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErasureParams {
    pub data_shards: usize,
    pub parity_shards: usize,
}

impl ErasureParams {
    /// Validated construction. Errors if either count is zero or the total
    /// exceeds 255 (the largest distinct nonzero `GF(2^8)` coefficient the
    /// Vandermonde construction can assign).
    pub fn new(data_shards: usize, parity_shards: usize) -> Result<Self> {
        if data_shards == 0 || parity_shards == 0 || data_shards + parity_shards > 255 {
            return Err(ErasureError::InvalidParams {
                data_shards,
                parity_shards,
            });
        }
        Ok(ErasureParams {
            data_shards,
            parity_shards,
        })
    }

    /// Total shard count: `data_shards + parity_shards`.
    pub fn total_shards(&self) -> usize {
        self.data_shards + self.parity_shards
    }
}

/// One shard of an [`encode`]d file: its position among the total shards
/// (`0..data_shards` are the raw data shards, `data_shards..total_shards`
/// are parity), and its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shard {
    pub index: usize,
    pub bytes: Vec<u8>,
}

/// The result of encoding one file: every shard, plus the bookkeeping
/// [`reconstruct`] needs to strip the padding a non-`data_shards`-aligned
/// input required.
#[derive(Debug, Clone)]
pub struct EncodedData {
    pub params: ErasureParams,
    pub original_len: usize,
    pub shards: Vec<Shard>,
}

/// Split `data` into `params.data_shards` pieces (zero-padded to an equal
/// length if `data.len()` doesn't divide evenly) and compute
/// `params.parity_shards` additional shards from them.
pub fn encode(data: &[u8], params: ErasureParams) -> Result<EncodedData> {
    if data.is_empty() {
        return Err(ErasureError::EmptyData);
    }
    let shard_len = data.len().div_ceil(params.data_shards);

    let mut d = Matrix::zeros(params.data_shards, shard_len);
    for i in 0..params.data_shards {
        for col in 0..shard_len {
            let idx = i * shard_len + col;
            let byte = data.get(idx).copied().unwrap_or(0);
            d.set(i, col, byte);
        }
    }

    let g = generator_matrix(params.data_shards, params.parity_shards);
    let r = g.mul(&d);

    let shards = (0..r.rows())
        .map(|i| Shard {
            index: i,
            bytes: (0..r.cols()).map(|c| r.get(i, c)).collect(),
        })
        .collect();

    Ok(EncodedData {
        params,
        original_len: data.len(),
        shards,
    })
}

/// Recover the original data from `shards` (length must be exactly
/// `params.total_shards()`, `None` at any missing index) given the
/// original byte length recorded at encode time.
pub fn reconstruct(
    params: ErasureParams,
    shards: &[Option<Shard>],
    original_len: usize,
) -> Result<Vec<u8>> {
    let n = params.total_shards();
    if shards.len() != n {
        return Err(ErasureError::WrongShardCount {
            expected: n,
            got: shards.len(),
        });
    }

    let available: Vec<usize> = shards
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.as_ref().map(|_| i))
        .collect();
    if available.len() < params.data_shards {
        return Err(ErasureError::TooManyMissingShards {
            available: available.len(),
            needed: params.data_shards,
        });
    }
    let chosen: Vec<usize> = available.into_iter().take(params.data_shards).collect();
    let shard_len = shards[chosen[0]].as_ref().unwrap().bytes.len();

    let g = generator_matrix(params.data_shards, params.parity_shards);
    let s = g.select_rows(&chosen);
    let s_inv = s.invert().ok_or(ErasureError::SingularSubmatrix)?;

    let mut a = Matrix::zeros(params.data_shards, shard_len);
    for (row, &idx) in chosen.iter().enumerate() {
        let bytes = &shards[idx].as_ref().unwrap().bytes;
        for (col, &byte) in bytes.iter().enumerate().take(shard_len) {
            a.set(row, col, byte);
        }
    }

    let d = s_inv.mul(&a);
    let mut out = Vec::with_capacity(params.data_shards * shard_len);
    for i in 0..params.data_shards {
        for col in 0..shard_len {
            out.push(d.get(i, col));
        }
    }
    out.truncate(original_len);
    Ok(out)
}

/// Recompute shard `index`'s bytes from the original data matrix -- used
/// by [`crate::health::repair`] to regenerate exactly the shards that went
/// missing, without recomputing every shard.
pub(crate) fn regenerate_shard(
    params: ErasureParams,
    original_data: &[u8],
    shard_len: usize,
    index: usize,
) -> Vec<u8> {
    let g = generator_matrix(params.data_shards, params.parity_shards);
    let row = g.row(index);

    let mut d = Matrix::zeros(params.data_shards, shard_len);
    for i in 0..params.data_shards {
        for col in 0..shard_len {
            let idx = i * shard_len + col;
            d.set(i, col, original_data.get(idx).copied().unwrap_or(0));
        }
    }

    let r = row.mul(&d);
    (0..r.cols()).map(|c| r.get(0, c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_data_or_parity_shards_is_rejected() {
        assert!(matches!(
            ErasureParams::new(0, 2),
            Err(ErasureError::InvalidParams { .. })
        ));
        assert!(matches!(
            ErasureParams::new(4, 0),
            Err(ErasureError::InvalidParams { .. })
        ));
    }

    #[test]
    fn too_many_total_shards_is_rejected() {
        assert!(matches!(
            ErasureParams::new(200, 100),
            Err(ErasureError::InvalidParams { .. })
        ));
    }

    #[test]
    fn empty_data_is_rejected() {
        let params = ErasureParams::new(4, 2).unwrap();
        assert!(matches!(encode(&[], params), Err(ErasureError::EmptyData)));
    }

    #[test]
    fn encoding_then_reconstructing_all_shards_recovers_the_original() {
        let params = ErasureParams::new(4, 2).unwrap();
        let data =
            b"the quick brown fox jumps over the lazy dog, thirty-six bytes and change".to_vec();
        let encoded = encode(&data, params).unwrap();
        let shards: Vec<Option<Shard>> = encoded.shards.into_iter().map(Some).collect();
        let recovered = reconstruct(params, &shards, encoded.original_len).unwrap();
        assert_eq!(recovered, data);
    }

    #[test]
    fn reconstructing_from_any_k_of_n_shards_recovers_the_original() {
        let params = ErasureParams::new(4, 3).unwrap();
        let data =
            b"erasure coding recovers data from any k of n shards, not just the first k".to_vec();
        let encoded = encode(&data, params).unwrap();

        for missing in combinations(
            params.total_shards(),
            params.total_shards() - params.data_shards,
        ) {
            let shards: Vec<Option<Shard>> = encoded
                .shards
                .iter()
                .map(|s| {
                    if missing.contains(&s.index) {
                        None
                    } else {
                        Some(s.clone())
                    }
                })
                .collect();
            let recovered = reconstruct(params, &shards, encoded.original_len).unwrap();
            assert_eq!(recovered, data, "failed with missing shards {missing:?}");
        }
    }

    #[test]
    fn fewer_than_data_shards_available_fails_cleanly() {
        let params = ErasureParams::new(4, 2).unwrap();
        let data = b"not enough shards survive to reconstruct this".to_vec();
        let encoded = encode(&data, params).unwrap();
        let mut shards: Vec<Option<Shard>> = encoded.shards.into_iter().map(Some).collect();
        // Drop 3 of the 6, leaving only 3 -- one short of data_shards=4.
        shards[0] = None;
        shards[1] = None;
        shards[2] = None;
        assert!(matches!(
            reconstruct(params, &shards, data.len()),
            Err(ErasureError::TooManyMissingShards {
                available: 3,
                needed: 4
            })
        ));
    }

    #[test]
    fn wrong_shard_slice_length_is_rejected() {
        let params = ErasureParams::new(4, 2).unwrap();
        let shards = vec![None; 5];
        assert!(matches!(
            reconstruct(params, &shards, 10),
            Err(ErasureError::WrongShardCount {
                expected: 6,
                got: 5
            })
        ));
    }

    #[test]
    fn non_aligned_data_length_round_trips_exactly() {
        let params = ErasureParams::new(3, 2).unwrap();
        for len in 1..40usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 7 % 251) as u8).collect();
            let encoded = encode(&data, params).unwrap();
            let shards: Vec<Option<Shard>> = encoded.shards.into_iter().map(Some).collect();
            let recovered = reconstruct(params, &shards, encoded.original_len).unwrap();
            assert_eq!(recovered, data, "round trip failed for length {len}");
        }
    }

    fn combinations(n: usize, k: usize) -> Vec<Vec<usize>> {
        let mut out = Vec::new();
        let mut current = Vec::new();
        fn go(
            n: usize,
            k: usize,
            start: usize,
            current: &mut Vec<usize>,
            out: &mut Vec<Vec<usize>>,
        ) {
            if current.len() == k {
                out.push(current.clone());
                return;
            }
            for i in start..n {
                current.push(i);
                go(n, k, i + 1, current, out);
                current.pop();
            }
        }
        go(n, k, 0, &mut current, &mut out);
        out
    }
}
