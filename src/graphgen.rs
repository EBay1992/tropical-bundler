//! Synthetic enterprise-monorepo graph generator. Zero dependencies:
//! deterministic xorshift64* RNG so every benchmark run is reproducible.

use crate::tropical::TropicalSparseMatrix;

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    pub fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }
}

/// Build a synthetic app graph with `node_count` modules and exactly
/// `edge_count` static imports. Weights are 500 B .. 45 KB, mimicking
/// minified module sizes. A backbone i -> i+1 guarantees the graph is
/// connected (deep multi-hop chains), the rest are random cross-imports.
pub fn generate_scale_project(node_count: usize, edge_count: usize, seed: u64) -> TropicalSparseMatrix {
    let mut rng = Rng::new(seed);
    let mut matrix = TropicalSparseMatrix::new(node_count);

    matrix.ensure_diagonal();

    let mut placed = 0usize;

    // Backbone chain: forces long dependency paths (worst case for traversal).
    for i in 0..node_count - 1 {
        if placed >= edge_count {
            break;
        }
        let w = 500 + rng.below(44_500) as u32;
        matrix.set(i, i + 1, w);
        placed += 1;
    }

    // Random cross-module imports.
    while placed < edge_count {
        let from = rng.below(node_count as u64) as usize;
        let to = rng.below(node_count as u64) as usize;
        if from == to {
            continue;
        }
        let before = matrix.rows[from].len();
        let w = 500 + rng.below(44_500) as u32;
        matrix.set(from, to, w);
        if matrix.rows[from].len() > before {
            placed += 1;
        }
    }

    matrix
}
