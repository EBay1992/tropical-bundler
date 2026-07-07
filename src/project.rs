//! Scan a real JS/TS project: discover modules, parse static imports, build the
//! tropical adjacency matrix. Externals (bare specifiers / node_modules) are
//! recorded but not included in the graph.

use crate::resolve::resolve_import;
use crate::tropical::TropicalSparseMatrix;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const SOURCE_EXTS: &[&str] = &[".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs"];

/// Directories skipped during discovery.
const SKIP_DIRS: &[&str] = &[
    "node_modules", "dist", "build", ".git", ".next", ".nuxt", "coverage", "target",
];

#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub id: usize,
    pub path: PathBuf,
    pub size: u32,
    pub imports: Vec<ImportRef>,
}

#[derive(Debug, Clone)]
pub struct ImportRef {
    pub specifier: String,
    /// Resolved local module id, if the import points to a project file.
    pub target: Option<usize>,
    pub dynamic: bool,
}

#[derive(Debug)]
pub struct ProjectGraph {
    pub root: PathBuf,
    pub entry: PathBuf,
    pub entry_id: usize,
    pub modules: Vec<ModuleInfo>,
    pub path_to_id: HashMap<PathBuf, usize>,
    pub matrix: TropicalSparseMatrix,
    pub externals: HashSet<String>,
    pub total_bytes: u64,
}

/// Discover and parse all modules reachable from `entry` under `root`.
pub fn build_graph(root: &Path, entry: &Path) -> std::io::Result<ProjectGraph> {
    let root = fs::canonicalize(root)?;
    let entry = if entry.is_absolute() {
        entry.to_path_buf()
    } else {
        root.join(entry)
    };
    let entry = resolve_existing(&entry).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("entry not found: {}", entry.display()),
        )
    })?;
    let entry = fs::canonicalize(&entry)?;

    let mut modules: Vec<ModuleInfo> = Vec::new();
    let mut path_to_id: HashMap<PathBuf, usize> = HashMap::new();
    let mut externals: HashSet<String> = HashSet::new();
    let mut queue: Vec<PathBuf> = vec![entry.clone()];
    let mut seen: HashSet<PathBuf> = HashSet::new();

    while let Some(path) = queue.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let id = modules.len();
        path_to_id.insert(path.clone(), id);

        let src = fs::read_to_string(&path)?;
        let size = fs::metadata(&path)?.len().min(u32::MAX as u64) as u32;
        let imports = parse_imports(&src);

        let mut resolved = Vec::new();
        for imp in imports {
            if imp.specifier.starts_with('.') || imp.specifier.starts_with('/') {
                if let Some(target_path) = resolve_import(&root, &path, &imp.specifier) {
                    let canon = fs::canonicalize(&target_path).unwrap_or(target_path);
                    if !seen.contains(&canon) {
                        queue.push(canon.clone());
                    }
                    // target id filled in second pass once all modules exist
                    resolved.push(ImportRef { specifier: imp.specifier, target: None, dynamic: imp.dynamic });
                    // stash path on the side via re-resolution below
                    let _ = canon;
                } else {
                    eprintln!("warn: unresolved import '{}' in {}", imp.specifier, path.display());
                    resolved.push(ImportRef { specifier: imp.specifier, target: None, dynamic: imp.dynamic });
                }
            } else {
                externals.insert(imp.specifier.clone());
                resolved.push(ImportRef { specifier: imp.specifier, target: None, dynamic: imp.dynamic });
            }
        }
        modules.push(ModuleInfo { id, path, size, imports: resolved });
    }

    // Second pass: wire import targets to module ids.
    for i in 0..modules.len() {
        let path = modules[i].path.clone();
        let raw: Vec<_> = modules[i].imports.iter().map(|i| (i.specifier.clone(), i.dynamic)).collect();
        modules[i].imports.clear();
        for (spec, dynamic) in raw {
            let target = if spec.starts_with('.') || spec.starts_with('/') {
                resolve_import(&root, &path, &spec)
                    .and_then(|p| fs::canonicalize(p).ok())
                    .and_then(|p| path_to_id.get(&p).copied())
            } else {
                None
            };
            modules[i].imports.push(ImportRef { specifier: spec, target, dynamic });
        }
    }

    let n = modules.len();
    let mut matrix = TropicalSparseMatrix::new(n);
    matrix.ensure_diagonal();
    let mut total_bytes = 0u64;
    for m in &modules {
        total_bytes += m.size as u64;
        for imp in &m.imports {
            if let Some(t) = imp.target {
                if t != m.id {
                    matrix.set(m.id, t, modules[t].size);
                }
            }
        }
    }

    let entry_id = *path_to_id.get(&entry).expect("entry must be in graph");

    Ok(ProjectGraph {
        root,
        entry,
        entry_id,
        modules,
        path_to_id,
        matrix,
        externals,
        total_bytes,
    })
}

struct RawImport {
    specifier: String,
    dynamic: bool,
}

/// Extract static and dynamic import specifiers from source text.
fn parse_imports(src: &str) -> Vec<RawImport> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for line in src.lines() {
        let line = strip_comments(line);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // import ... from 'x'  |  import 'x'  |  export ... from 'x'
        if trimmed.starts_with("import ") || trimmed.starts_with("export ") {
            if let Some(spec) = extract_quoted_specifier(trimmed) {
                push_import(&mut out, &mut seen, spec, false);
            }
        }
        // require('x')
        if let Some(pos) = trimmed.find("require(") {
            let rest = &trimmed[pos..];
            if let Some(spec) = extract_quoted_from_paren(rest) {
                push_import(&mut out, &mut seen, spec, false);
            }
        }
        // import('x') — dynamic, always an async boundary candidate
        if let Some(pos) = trimmed.find("import(") {
            let rest = &trimmed[pos..];
            if let Some(spec) = extract_quoted_from_paren(rest) {
                push_import(&mut out, &mut seen, spec, true);
            }
        }
    }
    out
}

fn push_import(out: &mut Vec<RawImport>, seen: &mut HashSet<String>, spec: String, dynamic: bool) {
    let key = format!("{spec}:{dynamic}");
    if seen.insert(key) {
        out.push(RawImport { specifier: spec, dynamic });
    }
}

fn extract_quoted_specifier(line: &str) -> Option<String> {
    // Find last quoted string on the line (handles `from '...'` and side-effect imports).
    let mut last = None;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let q = bytes[i];
        if q == b'\'' || q == b'"' {
            if let Some(spec) = read_quoted(line, i) {
                last = Some(spec);
            }
        }
        i += 1;
    }
    last
}

fn extract_quoted_from_paren(s: &str) -> Option<String> {
    let open = s.find('(')? + 1;
    let rest = s[open..].trim_start();
    read_quoted(rest, 0)
}

fn read_quoted(s: &str, start: usize) -> Option<String> {
    let bytes = s.as_bytes();
    if start >= bytes.len() {
        return None;
    }
    let q = bytes[start];
    if q != b'\'' && q != b'"' {
        return None;
    }
    let mut end = start + 1;
    while end < bytes.len() && bytes[end] != q {
        if bytes[end] == b'\\' {
            end += 1;
        }
        end += 1;
    }
    if end >= bytes.len() {
        return None;
    }
    Some(s[start + 1..end].to_string())
}

fn strip_comments(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' {
            if chars.peek() == Some(&'/') {
                break;
            }
            if chars.peek() == Some(&'*') {
                chars.next();
                while let Some(ch) = chars.next() {
                    if ch == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        break;
                    }
                }
                continue;
            }
        }
        out.push(c);
    }
    out
}

fn resolve_existing(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    for ext in SOURCE_EXTS {
        let with = PathBuf::from(format!("{}{}", path.display(), ext));
        if with.is_file() {
            return Some(with);
        }
    }
    for ext in SOURCE_EXTS {
        let index = path.join(format!("index{ext}"));
        if index.is_file() {
            return Some(index);
        }
    }
    None
}

/// List all source files under `dir` (for `analyze --all` mode).
pub fn discover_all_sources(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    walk_sources(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn walk_sources(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if SKIP_DIRS.contains(&name) || name.starts_with('.') {
                continue;
            }
            walk_sources(&path, out)?;
        } else if is_source_file(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn is_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            SOURCE_EXTS
                .iter()
                .any(|&s| s.trim_start_matches('.') == ext)
        })
        .unwrap_or(false)
}
