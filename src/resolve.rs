//! Path resolution for static import specifiers.

use std::path::{Path, PathBuf};

const SOURCE_EXTS: &[&str] = &[
    ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs",
];

/// Resolve `specifier` imported from `from_file` within `root`.
/// Returns `None` for bare specifiers (node packages) or unresolvable paths.
pub fn resolve_import(root: &Path, from_file: &Path, specifier: &str) -> Option<PathBuf> {
    if specifier.starts_with('.') || specifier.starts_with('/') {
        let base = from_file.parent().unwrap_or(root);
        let mut candidate = if specifier.starts_with('/') {
            root.join(specifier.trim_start_matches('/'))
        } else {
            base.join(specifier)
        };
        candidate = normalize_path(&candidate);
        if try_existing(&candidate) {
            return Some(candidate);
        }
        for ext in SOURCE_EXTS {
            let with_ext = candidate.with_extension(ext.trim_start_matches('.'));
            if with_ext.exists() {
                return Some(with_ext);
            }
            let as_ext = PathBuf::from(format!("{}{}", candidate.display(), ext));
            if as_ext.exists() {
                return Some(as_ext);
            }
        }
        for ext in SOURCE_EXTS {
            let index = candidate.join(format!("index{ext}"));
            if index.exists() {
                return Some(index);
            }
        }
        None
    } else {
        // Bare specifier -> external package; not part of the local graph.
        None
    }
}

fn try_existing(path: &Path) -> bool {
    path.is_file()
}

/// Collapse `.` and `..` without hitting the filesystem.
fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn resolves_relative_with_extension() {
        let dir = std::env::temp_dir().join("tropical_resolve_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::File::create(dir.join("src/a.js")).unwrap();
        let from = dir.join("src/index.js");
        let got = resolve_import(&dir, &from, "./a.js").unwrap();
        assert_eq!(got, dir.join("src/a.js"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolves_index_file() {
        let dir = std::env::temp_dir().join("tropical_resolve_index");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("lib")).unwrap();
        let mut f = fs::File::create(dir.join("lib/index.js")).unwrap();
        writeln!(f, "export const x = 1;").unwrap();
        let from = dir.join("src/index.js");
        fs::create_dir_all(dir.join("src")).unwrap();
        let got = resolve_import(&dir, &from, "../lib").unwrap();
        assert_eq!(got, dir.join("lib/index.js"));
        let _ = fs::remove_dir_all(&dir);
    }
}
