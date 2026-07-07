//! Optional `tropical.config.json` loader (minimal JSON parser, zero deps).

use std::fs;
use std::path::{Path, PathBuf};

pub struct Config {
    pub entry: Option<String>,
    pub outdir: Option<String>,
    pub root: Option<String>,
    pub budget: Option<u32>,
}

impl Default for Config {
    fn default() -> Self {
        Self { entry: None, outdir: None, root: None, budget: None }
    }
}

pub fn load(path: &Path) -> std::io::Result<Config> {
    let text = fs::read_to_string(path)?;
    Ok(parse(&text))
}

fn parse(text: &str) -> Config {
    let mut cfg = Config::default();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("//") || line == "{" || line == "}" {
            continue;
        }
        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim().trim_matches('"');
            let val = val.trim().trim_end_matches(',').trim_matches('"');
            match key {
                "entry" => cfg.entry = Some(val.to_string()),
                "outdir" => cfg.outdir = Some(val.to_string()),
                "root" => cfg.root = Some(val.to_string()),
                "budget" | "budgetBytes" => {
                    cfg.budget = val.parse().ok();
                }
                _ => {}
            }
        }
    }
    cfg
}

pub fn discover_config(root: &Path) -> Option<PathBuf> {
    let p = root.join("tropical.config.json");
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_config() {
        let cfg = parse(
            r#"{
            "entry": "src/index.js",
            "outdir": "dist",
            "budget": 512000
        }"#,
        );
        assert_eq!(cfg.entry.as_deref(), Some("src/index.js"));
        assert_eq!(cfg.outdir.as_deref(), Some("dist"));
        assert_eq!(cfg.budget, Some(512000));
    }
}
