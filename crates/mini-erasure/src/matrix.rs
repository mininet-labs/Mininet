//! Small dense matrices over `GF(2^8)`, and the systematic Reed-Solomon
//! generator matrix built from them: a Vandermonde-derived `(k + m) x k`
//! matrix whose top `k x k` block is the identity (so the first `k` output
//! rows are exactly the original data, unencoded -- "systematic" coding),
//! and whose bottom `m` rows are the parity coefficients. Any `k` rows of
//! this matrix form a `k x k` submatrix that is guaranteed invertible
//! (the Vandermonde/MDS property), which is exactly what lets
//! [`crate::code::reconstruct`] recover the original data from *any* `k`
//! of the `k + m` shards, not just the first `k`.

use crate::gf256;

/// A dense matrix over `GF(2^8)`, stored row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<u8>,
}

impl Matrix {
    /// A zero-filled `rows x cols` matrix.
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Matrix {
            rows,
            cols,
            data: vec![0u8; rows * cols],
        }
    }

    /// The `n x n` identity matrix.
    pub fn identity(n: usize) -> Self {
        let mut m = Matrix::zeros(n, n);
        for i in 0..n {
            m.set(i, i, 1);
        }
        m
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn get(&self, row: usize, col: usize) -> u8 {
        self.data[row * self.cols + col]
    }

    pub fn set(&mut self, row: usize, col: usize, value: u8) {
        self.data[row * self.cols + col] = value;
    }

    /// The submatrix formed by taking exactly `row_indices` (in order),
    /// keeping all columns.
    pub fn select_rows(&self, row_indices: &[usize]) -> Matrix {
        let mut out = Matrix::zeros(row_indices.len(), self.cols);
        for (new_row, &old_row) in row_indices.iter().enumerate() {
            for col in 0..self.cols {
                out.set(new_row, col, self.get(old_row, col));
            }
        }
        out
    }

    /// A single row as a standalone `1 x cols` matrix.
    pub fn row(&self, row: usize) -> Matrix {
        self.select_rows(&[row])
    }

    /// Matrix-matrix product over `GF(2^8)` (addition is XOR).
    pub fn mul(&self, other: &Matrix) -> Matrix {
        assert_eq!(self.cols, other.rows, "matrix dimension mismatch in mul");
        let mut out = Matrix::zeros(self.rows, other.cols);
        for i in 0..self.rows {
            for k in 0..self.cols {
                let a = self.get(i, k);
                if a == 0 {
                    continue;
                }
                for j in 0..other.cols {
                    let product = gf256::mul(a, other.get(k, j));
                    let existing = out.get(i, j);
                    out.set(i, j, existing ^ product);
                }
            }
        }
        out
    }

    /// The Gauss-Jordan inverse of this square matrix, or `None` if it is
    /// singular (never expected for the Vandermonde submatrices this crate
    /// builds, but checked rather than assumed).
    pub fn invert(&self) -> Option<Matrix> {
        assert_eq!(self.rows, self.cols, "only square matrices can be inverted");
        let n = self.rows;
        let mut work = self.clone();
        let mut inverse = Matrix::identity(n);

        for pivot_col in 0..n {
            let pivot_row = (pivot_col..n).find(|&r| work.get(r, pivot_col) != 0)?;
            if pivot_row != pivot_col {
                work.swap_rows(pivot_row, pivot_col);
                inverse.swap_rows(pivot_row, pivot_col);
            }

            let pivot_val = work.get(pivot_col, pivot_col);
            let pivot_inv = gf256::inv(pivot_val);
            work.scale_row(pivot_col, pivot_inv);
            inverse.scale_row(pivot_col, pivot_inv);

            for r in 0..n {
                if r == pivot_col {
                    continue;
                }
                let factor = work.get(r, pivot_col);
                if factor == 0 {
                    continue;
                }
                work.add_scaled_row(r, pivot_col, factor);
                inverse.add_scaled_row(r, pivot_col, factor);
            }
        }

        Some(inverse)
    }

    fn swap_rows(&mut self, a: usize, b: usize) {
        for col in 0..self.cols {
            self.data.swap(a * self.cols + col, b * self.cols + col);
        }
    }

    fn scale_row(&mut self, row: usize, factor: u8) {
        for col in 0..self.cols {
            let v = self.get(row, col);
            self.set(row, col, gf256::mul(v, factor));
        }
    }

    /// `row_dst += row_src * factor` (GF(2^8) addition is XOR).
    fn add_scaled_row(&mut self, dst: usize, src: usize, factor: u8) {
        for col in 0..self.cols {
            let addend = gf256::mul(self.get(src, col), factor);
            let existing = self.get(dst, col);
            self.set(dst, col, existing ^ addend);
        }
    }
}

/// The `(data_shards + parity_shards) x data_shards` systematic generator
/// matrix: the top `data_shards` rows are the identity, the bottom
/// `parity_shards` rows are Vandermonde coefficients `x_i^j` for distinct
/// nonzero `x_i` (one per parity row, `x_i = data_shards + row_offset + 1`
/// so no parity row ever collides with a value already used by the
/// identity block). Every `k x k` submatrix of a Vandermonde matrix is
/// invertible, which is the MDS (maximum-distance-separable) property
/// that lets [`crate::code::reconstruct`] use *any* `k` of the `n` rows.
pub fn generator_matrix(data_shards: usize, parity_shards: usize) -> Matrix {
    let n = data_shards + parity_shards;
    let mut g = Matrix::zeros(n, data_shards);
    for i in 0..data_shards {
        g.set(i, i, 1);
    }
    for p in 0..parity_shards {
        let x = (data_shards + p + 1) as u8;
        for j in 0..data_shards {
            g.set(data_shards + p, j, gf256::pow(x, j as u32));
        }
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_times_anything_is_itself() {
        let g = generator_matrix(4, 2);
        let sub = g.select_rows(&[0, 1, 2, 3]);
        let id = Matrix::identity(4);
        assert_eq!(id.mul(&sub), sub);
    }

    #[test]
    fn generator_top_block_is_identity() {
        let g = generator_matrix(5, 3);
        for i in 0..5 {
            for j in 0..5 {
                let expected = if i == j { 1 } else { 0 };
                assert_eq!(g.get(i, j), expected);
            }
        }
    }

    #[test]
    fn every_k_row_subset_of_the_generator_is_invertible() {
        let data_shards = 4;
        let parity_shards = 3;
        let g = generator_matrix(data_shards, parity_shards);
        let n = data_shards + parity_shards;

        // All C(n, k) subsets for small n -- exhaustive, not sampled.
        for subset in combinations(n, data_shards) {
            let sub = g.select_rows(&subset);
            assert!(
                sub.invert().is_some(),
                "subset {subset:?} produced a singular matrix"
            );
        }
    }

    #[test]
    fn inverse_of_identity_is_identity() {
        let id = Matrix::identity(5);
        assert_eq!(id.invert().unwrap(), id);
    }

    #[test]
    fn matrix_times_its_inverse_is_identity() {
        let data_shards = 4;
        let parity_shards = 2;
        let g = generator_matrix(data_shards, parity_shards);
        let sub = g.select_rows(&[0, 2, 4, 5]);
        let inverse = sub.invert().unwrap();
        assert_eq!(sub.mul(&inverse), Matrix::identity(data_shards));
        assert_eq!(inverse.mul(&sub), Matrix::identity(data_shards));
    }

    #[test]
    fn a_singular_matrix_has_no_inverse() {
        let mut m = Matrix::zeros(2, 2);
        m.set(0, 0, 1);
        m.set(0, 1, 2);
        m.set(1, 0, 1);
        m.set(1, 1, 2);
        assert!(m.invert().is_none());
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
