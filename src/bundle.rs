//! Bundle emitter: writes main + async chunks + manifest to disk.

use crate::project::ProjectGraph;
use crate::splitter::{group_async_chunks, EntryClassification};
use crate::transform::transform_module;
use crate::tropical::TropicalSparseMatrix;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct BuildOptions {
    pub outdir: PathBuf,
    pub budget: u32,
}

pub struct BuildStats {
    pub modules_total: usize,
    pub modules_sync: usize,
    pub modules_async: usize,
    pub chunks: usize,
    pub main_bytes: u64,
    pub chunk_bytes: u64,
    pub externals: usize,
}

pub struct BuildResult {
    pub main_file: PathBuf,
    pub chunk_files: Vec<PathBuf>,
    pub manifest_file: PathBuf,
    pub stats: BuildStats,
}

pub fn emit(
    project: &ProjectGraph,
    closure: &TropicalSparseMatrix,
    class: &EntryClassification,
    opts: &BuildOptions,
) -> std::io::Result<BuildResult> {
    fs::create_dir_all(&opts.outdir)?;

    let imports: Vec<Vec<usize>> = project
        .modules
        .iter()
        .map(|m| {
            m.imports
                .iter()
                .filter_map(|i| i.target)
                .filter(|&t| t != m.id)
                .collect()
        })
        .collect();

    let force_async: Vec<usize> = project
        .modules
        .iter()
        .flat_map(|m| {
            m.imports
                .iter()
                .filter(|i| i.dynamic)
                .filter_map(|i| i.target)
        })
        .collect();

  let mut async_set = class.async_cut.clone();
  for id in &force_async {
      async_set.insert(*id);
  }
  // Re-propagate importers after adding dynamic targets.
  let mut sync = class.sync.clone();
  let mut changed = true;
  while changed {
      changed = false;
      for m in &project.modules {
          if !sync.contains(&m.id) {
              continue;
          }
          for imp in &m.imports {
              if let Some(t) = imp.target {
                  if async_set.contains(&t) || imp.dynamic {
                      sync.remove(&m.id);
                      async_set.insert(m.id);
                      changed = true;
                  }
              }
          }
      }
  }

    let async_chunks = group_async_chunks(&async_set, &imports);
    let mut module_to_chunk: HashMap<usize, usize> = HashMap::new();
    for (ci, chunk) in async_chunks.iter().enumerate() {
        for &m in chunk {
            module_to_chunk.insert(m, ci);
        }
    }

    let mut chunk_files = Vec::new();
    let mut chunk_bytes_total = 0u64;

    for (ci, chunk_modules) in async_chunks.iter().enumerate() {
        let path = opts.outdir.join(format!("chunk-{ci}.js"));
        let ordered = topo_sort(chunk_modules, &imports);
        let bytes = write_chunk_file(
            &path,
            &ordered,
            project,
            &sync,
            &module_to_chunk,
            ci,
        )?;
        chunk_bytes_total += bytes;
        chunk_files.push(path);
    }

    let main_path = opts.outdir.join("main.js");
    let sync_ordered = topo_sort(&sync.iter().copied().collect::<Vec<_>>(), &imports);
    let main_bytes = write_main_file(
        &main_path,
        &sync_ordered,
        project,
        &module_to_chunk,
        project.entry_id,
        &project.externals,
    )?;

    let manifest_path = opts.outdir.join("manifest.json");
    write_manifest(
        &manifest_path,
        project,
        &sync,
        &async_set,
        &module_to_chunk,
        &chunk_files,
        opts.budget,
        closure,
    )?;

    let meta_path = opts.outdir.join("tropical-meta.json");
    write_meta(&meta_path, project, class, &opts)?;

    Ok(BuildResult {
        main_file: main_path,
        chunk_files,
        manifest_file: manifest_path,
        stats: BuildStats {
            modules_total: project.modules.len(),
            modules_sync: sync.len(),
            modules_async: async_set.len(),
            chunks: async_chunks.len(),
            main_bytes,
            chunk_bytes: chunk_bytes_total,
            externals: project.externals.len(),
        },
    })
}

fn write_main_file(
    path: &Path,
    sync_modules: &[usize],
    project: &ProjectGraph,
    module_to_chunk: &HashMap<usize, usize>,
    entry_id: usize,
    externals: &HashSet<String>,
) -> std::io::Result<u64> {
    let mut f = fs::File::create(path)?;
    let id_map: HashMap<usize, String> = project
        .modules
        .iter()
        .map(|m| (m.id, m.id.to_string()))
        .collect();
    let id_ref: HashMap<usize, &str> = id_map.iter().map(|(&k, v)| (k, v.as_str())).collect();

    writeln!(f, "{}", RUNTIME)?;
    writeln!(f, "// chunk map for async modules")?;
    write!(f, "var __chunk_map__ = {{")?;
    for (&mid, &ci) in module_to_chunk {
        write!(f, " {mid}: \"./chunk-{ci}.js\",")?;
    }
    writeln!(f, " }};")?;

    if !externals.is_empty() {
        writeln!(f, "// external packages (not bundled — provide via import map / CDN):")?;
        for ext in externals {
            writeln!(f, "//   - {ext}")?;
        }
    }

    writeln!(f, "var __modules__ = {{}};")?;
    writeln!(f, "var __cache__ = {{}};")?;

    for &id in sync_modules {
        let m = &project.modules[id];
        let body = transform_module(
            &fs::read_to_string(&m.path)?,
            m,
            &id_ref,
            &project.externals,
        );
        writeln!(f)?;
        writeln!(f, "// --- {} ---", rel_display(&project.root, &m.path))?;
        writeln!(f, "__modules__[{id}] = async function(module, exports, __req__, __import__, __external__) {{")?;
        write!(f, "{body}")?;
        writeln!(f, "}};")?;
    }

    writeln!(f)?;
    writeln!(f, "(async () => {{ await __exec__({entry_id}); }})();")?;
    f.flush()?;
    Ok(fs::metadata(path)?.len())
}

fn write_chunk_file(
    path: &Path,
    modules: &[usize],
    project: &ProjectGraph,
    _sync_set: &HashSet<usize>,
    _module_to_chunk: &HashMap<usize, usize>,
    _chunk_id: usize,
) -> std::io::Result<u64> {
    let mut f = fs::File::create(path)?;
    let id_map: HashMap<usize, String> = project
        .modules
        .iter()
        .map(|m| (m.id, m.id.to_string()))
        .collect();
    let id_ref: HashMap<usize, &str> = id_map.iter().map(|(&k, v)| (k, v.as_str())).collect();

    writeln!(f, "{}", RUNTIME)?;
    writeln!(f, "var __chunk_map__ = {{}};")?;
    writeln!(f, "var __modules__ = {{}};")?;
    writeln!(f, "var __cache__ = {{}};")?;

    for &id in modules {
        let m = &project.modules[id];
        let src = fs::read_to_string(&m.path)?;
        let body = transform_module(&src, m, &id_ref, &project.externals);
        writeln!(f)?;
        writeln!(f, "// --- {} ---", rel_display(&project.root, &m.path))?;
        writeln!(f, "__modules__[{id}] = async function(module, exports, __req__, __import__, __external__) {{")?;
        write!(f, "{body}")?;
        writeln!(f, "}};")?;
    }

    // Execute all modules in chunk (entry = first in topo order).
    if let Some(&first) = modules.first() {
        writeln!(f, "(async () => {{ await __exec__({first}); }})();")?;
    }
    f.flush()?;
    Ok(fs::metadata(path)?.len())
}

fn write_manifest(
    path: &Path,
    project: &ProjectGraph,
    sync: &HashSet<usize>,
    async_set: &HashSet<usize>,
    module_to_chunk: &HashMap<usize, usize>,
    chunk_files: &[PathBuf],
    budget: u32,
    closure: &TropicalSparseMatrix,
) -> std::io::Result<()> {
    let mut f = fs::File::create(path)?;
    writeln!(f, "{{")?;
    writeln!(f, "  \"entry\": {},", project.entry_id)?;
    writeln!(f, "  \"entryPath\": {:?},", rel_display(&project.root, &project.entry))?;
    writeln!(f, "  \"budgetBytes\": {budget},")?;
    writeln!(f, "  \"main\": \"./main.js\",")?;
    writeln!(f, "  \"chunks\": [")?;
    for (i, cf) in chunk_files.iter().enumerate() {
        let comma = if i + 1 < chunk_files.len() { "," } else { "" };
        writeln!(f, "    {:?}{comma}", cf.file_name().unwrap().to_string_lossy())?;
    }
    writeln!(f, "  ],")?;
    writeln!(f, "  \"modules\": [")?;
    for (i, m) in project.modules.iter().enumerate() {
        let mode = if sync.contains(&m.id) {
            "sync"
        } else if async_set.contains(&m.id) {
            "async"
        } else {
            "unreachable"
        };
        let chunk = module_to_chunk.get(&m.id).map(|c| format!("chunk-{c}"));
        let cost = closure.get(project.entry_id, m.id);
        let comma = if i + 1 < project.modules.len() { "," } else { "" };
        writeln!(
            f,
            "    {{ \"id\": {}, \"path\": {:?}, \"bytes\": {}, \"mode\": \"{mode}\", \"chunk\": {:?}, \"minCostFromEntry\": {cost} }}{comma}",
            m.id,
            rel_display(&project.root, &m.path),
            m.size,
            chunk,
        )?;
    }
    writeln!(f, "  ]")?;
    writeln!(f, "}}")?;
    Ok(())
}

fn write_meta(
    path: &Path,
    project: &ProjectGraph,
    class: &EntryClassification,
    opts: &BuildOptions,
) -> std::io::Result<()> {
    let mut f = fs::File::create(path)?;
    writeln!(f, "{{")?;
    writeln!(f, "  \"bundler\": \"tropical-bundler\",")?;
    writeln!(f, "  \"version\": \"0.1.0\",")?;
    writeln!(f, "  \"root\": {:?},", project.root.display())?;
    writeln!(f, "  \"outdir\": {:?},", opts.outdir.display())?;
    writeln!(f, "  \"budgetBytes\": {},", opts.budget)?;
    writeln!(f, "  \"syncModules\": {},", class.sync.len())?;
    writeln!(f, "  \"asyncModules\": {},", class.async_cut.len())?;
    writeln!(f, "  \"syncBytesEstimate\": {}", class.sync_bytes)?;
    writeln!(f, "}}")?;
    Ok(())
}

fn topo_sort(modules: &[usize], imports: &[Vec<usize>]) -> Vec<usize> {
    let set: HashSet<usize> = modules.iter().copied().collect();
    let mut in_degree: HashMap<usize, usize> = modules.iter().map(|&m| (m, 0)).collect();
    for &m in modules {
        for &dep in &imports[m] {
            if set.contains(&dep) && dep != m {
                *in_degree.entry(m).or_default() += 1;
            }
        }
    }
    let mut queue: Vec<usize> = in_degree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(&m, _)| m)
        .collect();
    queue.sort_unstable_by(|a, b| b.cmp(a)); // pop from end => ascending
    let mut out = Vec::with_capacity(modules.len());
    while let Some(m) = queue.pop() {
        out.push(m);
        for &other in modules {
            if other == m {
                continue;
            }
            if imports[other].contains(&m) {
                let d = in_degree.get_mut(&other).unwrap();
                *d -= 1;
                if *d == 0 {
                    queue.push(other);
                }
            }
        }
    }
    for &m in modules {
        if !out.contains(&m) {
            out.push(m);
        }
    }
    out
}

fn rel_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

const RUNTIME: &str = r#"// tropical-bundler runtime
async function __req__(id) {
  if (__cache__[id]) return __cache__[id].exports;
  await __exec__(id);
  return __cache__[id].exports;
}
async function __import__(id) {
  if (__chunk_map__[id]) {
    await import(__chunk_map__[id]);
  }
  return __req__(id);
}
function __external__(name) {
  throw new Error(
    "[tropical-bundler] External module '" + name + "' is not bundled. " +
    "Install it separately and provide an import map, or mark it as reachable only via relative imports."
  );
}
async function __exec__(id) {
  if (__cache__[id]) return;
  var module = { exports: {} };
  __cache__[id] = module;
  await __modules__[id](module, module.exports, __req__, __import__, __external__);
}
"#;
