//! Tropical (min-plus) sparse matrix engine.
//!
//! Semiring: (min, +) over u32 byte weights.
//!   - Tropical addition  x (+) y = min(x, y)
//!   - Tropical product   x (x) y = x + y
//!   - Additive identity  = INF (absent edge, never stored)
//!   - Multiplicative id. = 0    (self-dependency, kept on the diagonal)

/// "Infinity" sentinel. u32::MAX / 4 so that `a + b` can never overflow even
/// when one operand is INF (dense path adds without branching on b).
pub const INF: u32 = u32::MAX / 4;

/// Density (in percent of n^2) at which the closure switches from the sparse
/// COO/CSR kernel to a flat dense kernel. Tropical closures of connected
/// graphs saturate quickly, so this switch is almost always taken.
const DENSE_SWITCH_PCT: usize = 15;

#[derive(Clone, PartialEq, Debug)]
pub struct TropicalSparseMatrix {
    pub size: usize,
    /// CSR-style storage: rows[i] holds (col, weight) sorted by col.
    /// Only finite (non-INF) entries are stored.
    pub rows: Vec<Vec<(u32, u32)>>,
}

pub struct ClosureTelemetry {
    pub squarings: usize,
    pub dense_switch_at: Option<usize>,
}

impl TropicalSparseMatrix {
    pub fn new(size: usize) -> Self {
        Self { size, rows: vec![Vec::new(); size] }
    }

    /// Insert edge r -> c with weight v, keeping the minimum on duplicates.
    pub fn set(&mut self, r: usize, c: usize, v: u32) {
        if v >= INF {
            return;
        }
        let row = &mut self.rows[r];
        match row.binary_search_by_key(&(c as u32), |e| e.0) {
            Ok(pos) => {
                if v < row[pos].1 {
                    row[pos].1 = v;
                }
            }
            Err(pos) => row.insert(pos, (c as u32, v)),
        }
    }

    pub fn get(&self, r: usize, c: usize) -> u32 {
        match self.rows[r].binary_search_by_key(&(c as u32), |e| e.0) {
            Ok(pos) => self.rows[r][pos].1,
            Err(_) => INF,
        }
    }

    /// Number of stored (finite) entries.
    pub fn nnz(&self) -> usize {
        self.rows.iter().map(|r| r.len()).sum()
    }

    /// Guarantee A[i][i] = 0 so that A (x) A always subsumes A itself
    /// (paths of length <= 2 include paths of length <= 1).
    pub fn ensure_diagonal(&mut self) {
        for i in 0..self.size {
            self.set(i, i, 0);
        }
    }

    pub fn to_dense(&self) -> Vec<u32> {
        let n = self.size;
        let mut d = vec![INF; n * n];
        for (i, row) in self.rows.iter().enumerate() {
            for &(j, v) in row {
                d[i * n + j as usize] = v;
            }
        }
        d
    }

    pub fn from_dense(d: &[u32], n: usize) -> Self {
        let mut m = Self::new(n);
        for i in 0..n {
            let row: Vec<(u32, u32)> = d[i * n..(i + 1) * n]
                .iter()
                .enumerate()
                .filter(|(_, &v)| v < INF)
                .map(|(j, &v)| (j as u32, v))
                .collect();
            m.rows[i] = row;
        }
        m
    }

    /// Sparse tropical matrix product self (x) other.
    /// Row-parallel: each worker owns a disjoint slice of output rows and a
    /// dense scratch row, so no locks are needed.
    pub fn multiply(&self, other: &Self, threads: usize) -> Self {
        assert_eq!(self.size, other.size, "matrix dimension mismatch");
        let n = self.size;
        let mut out: Vec<Vec<(u32, u32)>> = vec![Vec::new(); n];
        let chunk = n.div_ceil(threads.max(1));

        std::thread::scope(|s| {
            for (t, out_chunk) in out.chunks_mut(chunk).enumerate() {
                let start = t * chunk;
                let a = &self.rows;
                let b = &other.rows;
                s.spawn(move || {
                    let mut scratch = vec![INF; n];
                    let mut touched: Vec<u32> = Vec::with_capacity(256);
                    for (local, out_row) in out_chunk.iter_mut().enumerate() {
                        let i = start + local;
                        for &(k, v) in &a[i] {
                            for &(j, w) in &b[k as usize] {
                                let cell = &mut scratch[j as usize];
                                let cand = v + w; // tropical multiplication
                                if *cell == INF {
                                    touched.push(j);
                                }
                                if cand < *cell {
                                    // tropical addition (min)
                                    *cell = cand;
                                }
                            }
                        }
                        touched.sort_unstable();
                        let mut row = Vec::with_capacity(touched.len());
                        for &j in &touched {
                            row.push((j, scratch[j as usize]));
                            scratch[j as usize] = INF;
                        }
                        touched.clear();
                        *out_row = row;
                    }
                });
            }
        });

        Self { size: n, rows: out }
    }

    /// Transitive closure via repeated squaring: A, A^2, A^4, ... in
    /// ceil(log2(n)) steps max, with early exit on fixpoint. Switches to a
    /// flat dense kernel once the matrix saturates past DENSE_SWITCH_PCT.
    pub fn closure(&self, threads: usize) -> (Self, ClosureTelemetry) {
        let n = self.size;
        let max_steps = usize::BITS as usize - (n.max(2) - 1).leading_zeros() as usize;

        let mut cur = self.clone();
        cur.ensure_diagonal();
        let mut dense: Option<Vec<u32>> = None;
        let mut telemetry = ClosureTelemetry { squarings: 0, dense_switch_at: None };

        for step in 1..=max_steps {
            if dense.is_none() && cur.nnz() * 100 >= n * n * DENSE_SWITCH_PCT {
                dense = Some(cur.to_dense());
                telemetry.dense_switch_at = Some(step);
            }

            if let Some(d) = &dense {
                let next = multiply_dense(d, n, threads);
                if next == *d {
                    break; // fixpoint: all shortest paths found
                }
                telemetry.squarings = step;
                dense = Some(next);
            } else {
                let next = cur.multiply(&cur, threads);
                if next.rows == cur.rows {
                    break;
                }
                telemetry.squarings = step;
                cur = next;
            }
        }

        let result = match dense {
            Some(d) => Self::from_dense(&d, n),
            None => cur,
        };
        (result, telemetry)
    }
}

/// Rows per cache block in the dense kernel. 32 output rows of a 2500-wide
/// u32 matrix is ~320 KB — resident in L2 while each B-row (10 KB, L1) is
/// reused 32 times, cutting DRAM traffic ~BLOCK x vs the naive i-k-j order.
const DENSE_BLOCK_ROWS: usize = 32;

/// Dense min-plus squaring kernel: out = a (x) a over flat row-major storage.
/// Loop order: (row block) -> k -> i -> j. Branchless inner min/add loop so
/// LLVM emits packed SIMD (vpaddd + vpminud).
fn multiply_dense(a: &[u32], n: usize, threads: usize) -> Vec<u32> {
    let mut out = vec![INF; n * n];
    let chunk_rows = n.div_ceil(threads.max(1));

    std::thread::scope(|s| {
        for (t, out_chunk) in out.chunks_mut(chunk_rows * n).enumerate() {
            let start_row = t * chunk_rows;
            s.spawn(move || {
                for (blk, out_block) in out_chunk.chunks_mut(DENSE_BLOCK_ROWS * n).enumerate() {
                    let block_start = start_row + blk * DENSE_BLOCK_ROWS;
                    for k in 0..n {
                        let b_row = &a[k * n..(k + 1) * n];
                        for (local, out_row) in out_block.chunks_mut(n).enumerate() {
                            let av = a[(block_start + local) * n + k];
                            if av >= INF {
                                continue;
                            }
                            // No INF check on b: av + INF stays >= INF and loses the min.
                            for (o, &b) in out_row.iter_mut().zip(b_row) {
                                *o = (*o).min(av + b);
                            }
                        }
                    }
                }
            });
        }
    });

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Brute-force Floyd-Warshall as an independent oracle.
    fn floyd_warshall(m: &TropicalSparseMatrix) -> Vec<u32> {
        let n = m.size;
        let mut d = m.to_dense();
        for i in 0..n {
            d[i * n + i] = 0;
        }
        for k in 0..n {
            for i in 0..n {
                for j in 0..n {
                    let cand = d[i * n + k].saturating_add(d[k * n + j]);
                    if cand < d[i * n + j] {
                        d[i * n + j] = cand;
                    }
                }
            }
        }
        d
    }

    #[test]
    fn semiring_identities() {
        // e = 0 is neutral for (x), INF is neutral (absorbed) for min.
        let mut a = TropicalSparseMatrix::new(3);
        a.set(0, 1, 7);
        a.ensure_diagonal();
        let prod = a.multiply(&a, 1);
        assert_eq!(prod.get(0, 1), 7); // 0 (x) 7 = 7 via diagonal identity
        assert_eq!(prod.get(2, 0), INF); // absent edge stays absent
    }

    #[test]
    fn closure_matches_floyd_warshall() {
        let g = crate::graphgen::generate_scale_project(120, 500, 7);
        let oracle = floyd_warshall(&g);
        let (closed, _) = g.closure(2);
        for i in 0..120 {
            for j in 0..120 {
                let o = oracle[i * 120 + j].min(INF);
                assert_eq!(closed.get(i, j), o, "cell [{i}][{j}]");
            }
        }
    }

    #[test]
    fn multi_hop_min_beats_direct_edge() {
        // Direct edge 0->2 costs 100; path 0->1->2 costs 5+5=10. min wins.
        let mut a = TropicalSparseMatrix::new(3);
        a.set(0, 2, 100);
        a.set(0, 1, 5);
        a.set(1, 2, 5);
        let (c, _) = a.closure(1);
        assert_eq!(c.get(0, 2), 10);
    }
}
