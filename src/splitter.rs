//! Code-splitting boundary resolver.
//!
//! Input: the closed tropical matrix D where D[i][j] is the minimum
//! accumulated byte cost of reaching module j from entry point i.
//!
//! Cutoff rule: if the accumulated cost D[i][j] exceeds the budget, module j
//! is decoupled into an async chunk. Dynamic imports and importers of async
//! modules are promoted into the async set automatically.

use crate::tropical::{TropicalSparseMatrix, INF};
use std::collections::HashSet;

pub struct SplitPlan {
    pub entry: usize,
    pub sync_modules: usize,
    pub sync_bytes: u64,
    pub async_modules: usize,
    pub unreachable: usize,
}

/// Full module classification for one entry — used by the bundler emitter.
pub struct EntryClassification {
    pub entry: usize,
    pub sync: HashSet<usize>,
    pub async_cut: HashSet<usize>,
    pub sync_bytes: u64,
}

pub fn resolve_boundaries(
    closure: &TropicalSparseMatrix,
    entries: &[usize],
    budget_bytes: u32,
) -> Vec<SplitPlan> {
    entries
        .iter()
        .map(|&entry| {
            let direct: Vec<Vec<usize>> = (0..closure.size).map(|_| Vec::new()).collect();
            let c = classify_entry(closure, entry, budget_bytes, &[], &direct);
            SplitPlan {
                entry,
                sync_modules: c.sync.len(),
                sync_bytes: c.sync_bytes,
                async_modules: c.async_cut.len(),
                unreachable: closure.size - c.sync.len() - c.async_cut.len(),
            }
        })
        .collect()
}

/// Classify every module reachable from `entry` into sync vs async chunks.
/// `direct_imports[i]` lists modules that `i` statically imports (not transitive).
pub fn classify_entry(
    closure: &TropicalSparseMatrix,
    entry: usize,
    budget_bytes: u32,
    force_async: &[usize],
    direct_imports: &[Vec<usize>],
) -> EntryClassification {
    let n = closure.size;
    let mut sync = HashSet::new();
    let mut async_cut = HashSet::new();
    let mut sync_bytes = 0u64;

    for &(col, cost) in &closure.rows[entry] {
        if cost >= INF {
            continue;
        }
        let j = col as usize;
        if force_async.contains(&j) || cost > budget_bytes {
            async_cut.insert(j);
        } else {
            sync.insert(j);
            sync_bytes += cost as u64;
        }
    }

    // Static importers of async modules must themselves be async (direct edges only).
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..n {
            if !sync.contains(&i) {
                continue;
            }
            for &j in &direct_imports[i] {
                if i != j && async_cut.contains(&j) {
                    sync.remove(&i);
                    async_cut.insert(i);
                    changed = true;
                }
            }
        }
    }

    EntryClassification { entry, sync, async_cut, sync_bytes }
}

/// Group async modules into chunk roots: each root owns its async-only deps.
pub fn group_async_chunks(
    async_modules: &HashSet<usize>,
    imports: &[Vec<usize>],
) -> Vec<Vec<usize>> {
    let mut roots: Vec<usize> = async_modules
        .iter()
        .copied()
        .filter(|&m| {
            imports[m]
                .iter()
                .all(|&dep| !async_modules.contains(&dep) || dep == m)
        })
        .collect();
    if roots.is_empty() && !async_modules.is_empty() {
        roots.push(*async_modules.iter().min().unwrap());
    }
    roots.sort_unstable();

    let mut chunks = Vec::new();
    let mut assigned = HashSet::new();
    for root in roots {
        let mut chunk = Vec::new();
        let mut stack = vec![root];
        while let Some(m) = stack.pop() {
            if !async_modules.contains(&m) || !assigned.insert(m) {
                continue;
            }
            chunk.push(m);
            for &dep in &imports[m] {
                if async_modules.contains(&dep) && !assigned.contains(&dep) {
                    stack.push(dep);
                }
            }
        }
        if !chunk.is_empty() {
            chunk.sort_unstable();
            chunks.push(chunk);
        }
    }

    // Catch any unassigned async modules (cycles).
    for &m in async_modules {
        if !assigned.contains(&m) {
            chunks.push(vec![m]);
        }
    }
    chunks
}
