//! Control algorithms for the benchmark.
//!
//! 1. `dfs_reachability` — what traditional bundlers (webpack/rollup style)
//!    actually do per entry point: traverse the module graph and accumulate
//!    the byte weight of everything reachable. It cannot answer "cheapest
//!    accumulated path to module j", only "is j pulled in and how big is the
//!    whole chunk".
//! 2. `dijkstra_all_pairs` — the classical exact single-source shortest-path
//!    solver run from every node. This is the ground-truth oracle used to
//!    validate that the tropical closure is 100% accurate.

use crate::tropical::{TropicalSparseMatrix, INF};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Iterative DFS from one entry: returns (modules reached, total bytes pulled).
/// Explicit stack — no recursion, safe for arbitrarily deep graphs.
pub fn dfs_reachability(graph: &TropicalSparseMatrix, entry: usize) -> (usize, u64) {
    let mut visited = vec![false; graph.size];
    let mut stack = vec![entry];
    visited[entry] = true;
    let mut reached = 0usize;
    let mut total_bytes = 0u64;

    while let Some(node) = stack.pop() {
        reached += 1;
        for &(next, weight) in &graph.rows[node] {
            let next = next as usize;
            if next != node && !visited[next] {
                visited[next] = true;
                total_bytes += weight as u64;
                stack.push(next);
            }
        }
    }
    (reached, total_bytes)
}

/// Exact shortest accumulated-weight paths from `source` to every module.
pub fn dijkstra(graph: &TropicalSparseMatrix, source: usize) -> Vec<u32> {
    let n = graph.size;
    let mut dist = vec![INF; n];
    dist[source] = 0;
    let mut heap = BinaryHeap::new();
    heap.push(Reverse((0u32, source as u32)));

    while let Some(Reverse((d, u))) = heap.pop() {
        let u = u as usize;
        if d > dist[u] {
            continue; // stale heap entry
        }
        for &(v, w) in &graph.rows[u] {
            let v = v as usize;
            let cand = d + w;
            if cand < dist[v] {
                dist[v] = cand;
                heap.push(Reverse((cand, v as u32)));
            }
        }
    }
    dist
}

/// Row-parallel all-pairs Dijkstra. Returns flat row-major n*n distances.
pub fn dijkstra_all_pairs(graph: &TropicalSparseMatrix, threads: usize) -> Vec<u32> {
    let n = graph.size;
    let mut out = vec![INF; n * n];
    let chunk_rows = n.div_ceil(threads.max(1));

    std::thread::scope(|s| {
        for (t, out_chunk) in out.chunks_mut(chunk_rows * n).enumerate() {
            let start = t * chunk_rows;
            s.spawn(move || {
                for (local, out_row) in out_chunk.chunks_mut(n).enumerate() {
                    out_row.copy_from_slice(&dijkstra(graph, start + local));
                }
            });
        }
    });
    out
}
