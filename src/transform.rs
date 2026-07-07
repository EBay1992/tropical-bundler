//! Minimal import/export rewriter for bundled CJS-style module wrappers.

use crate::project::ModuleInfo;
use std::collections::HashMap;

/// Rewrite ESM source into an async `function(module, exports, __req__, ...) { ... }` body.
pub fn transform_module(
    src: &str,
    module: &ModuleInfo,
    id_to_var: &HashMap<usize, &str>,
    externals: &std::collections::HashSet<String>,
) -> String {
    let mut body = String::new();

    for line in src.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("import type ") || trimmed.starts_with("export type ") {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("import ") {
            if let Some(transformed) = rewrite_import(rest, module, id_to_var, externals) {
                body.push_str(&transformed);
                body.push('\n');
                continue;
            }
        }

        if let Some(rest) = trimmed.strip_prefix("export ") {
            if rest.starts_with("default ") {
                let expr = rest.strip_prefix("default ").unwrap().trim_end_matches(';');
                body.push_str(&format!("exports.default = {expr};\n"));
                continue;
            }
            if rest.starts_with("from ") {
                if let Some(transformed) = rewrite_reexport(rest, module, id_to_var, externals) {
                    body.push_str(&transformed);
                    body.push('\n');
                    continue;
                }
            }
            if let Some(rewritten) = rewrite_export_declaration(rest) {
                body.push_str(&rewritten);
                body.push('\n');
                continue;
            }
        }

        body.push_str(line);
        body.push('\n');
    }

    body
}

fn rewrite_export_declaration(rest: &str) -> Option<String> {
    if let Some(r) = rest.strip_prefix("function ") {
        let name = fn_name(r)?;
        return Some(format!("exports.{name} = function {r}"));
    }
    if let Some(r) = rest.strip_prefix("async function ") {
        let name = fn_name(r)?;
        return Some(format!("exports.{name} = async function {r}"));
    }
    if let Some(r) = rest.strip_prefix("const ") {
        let name = binding_name(r)?;
        let val = r.splitn(2, '=').nth(1).unwrap_or("undefined").trim().trim_end_matches(';');
        return Some(format!("exports.{name} = {val};"));
    }
    if let Some(r) = rest.strip_prefix("let ") {
        let name = binding_name(r)?;
        let val = r.splitn(2, '=').nth(1).unwrap_or("undefined").trim().trim_end_matches(';');
        return Some(format!("exports.{name} = {val};"));
    }
    if let Some(r) = rest.strip_prefix("class ") {
        let name = r.split_whitespace().next()?;
        return Some(format!("exports.{name} = class {r}"));
    }
    None
}

fn fn_name(s: &str) -> Option<&str> {
  Some(s.split('(').next()?.trim())
}

fn binding_name(s: &str) -> Option<&str> {
    let name = s.split('=').next()?.trim();
    if name.is_empty() { None } else { Some(name) }
}

fn rewrite_import(
    rest: &str,
    module: &ModuleInfo,
    id_to_var: &HashMap<usize, &str>,
    externals: &std::collections::HashSet<String>,
) -> Option<String> {
    let spec = extract_from_clause(rest).or_else(|| extract_side_effect_spec(rest))?;
    let (req, is_async) = req_expr_for_spec(&spec, module, id_to_var, externals)?;

    if rest.contains("from ") {
        let before = rest.split("from ").next().unwrap().trim();
        let expr = format!("await {req}");
        if before.starts_with('{') {
            let names = before.trim_matches(|c| c == '{' || c == '}' || c == ' ');
            return Some(format!("const {{ {names} }} = {expr};"));
        }
        if before.starts_with('*') {
            let alias = before.split("as").nth(1).unwrap_or("star").trim();
            return Some(format!("const {alias} = {expr};"));
        }
        let name = before.trim();
        return Some(format!(
            "const {name} = {expr}.default !== undefined ? {expr}.default : {expr};"
        ));
    }
    let expr = format!("await {req}");
    Some(format!("{expr};"))
}

fn rewrite_reexport(
    rest: &str,
    module: &ModuleInfo,
    id_to_var: &HashMap<usize, &str>,
    externals: &std::collections::HashSet<String>,
) -> Option<String> {
    let spec = extract_from_clause(rest)?;
    let (req, is_async) = req_expr_for_spec(&spec, module, id_to_var, externals)?;
    let expr = format!("await {req}");
    if rest.starts_with("* ") {
        return Some(format!("Object.assign(exports, {expr});"));
    }
    if rest.starts_with("{") {
        return Some(format!("Object.assign(exports, {expr});"));
    }
    None
}

fn req_expr_for_spec(
    spec: &str,
    module: &ModuleInfo,
    id_to_var: &HashMap<usize, &str>,
    externals: &std::collections::HashSet<String>,
) -> Option<(String, bool)> {
    if spec.starts_with('.') || spec.starts_with('/') {
        let imp = module.imports.iter().find(|i| i.specifier == spec)?;
        let id = imp.target?;
        let var = *id_to_var.get(&id)?;
        if imp.dynamic {
            return Some((format!("__import__({var})"), true));
        }
        return Some((format!("__req__({var})"), false));
    }
    if externals.contains(spec) {
        return Some((format!("__external__({spec:?})"), false));
    }
    Some((format!("__external__({spec:?})"), false))
}

fn extract_from_clause(rest: &str) -> Option<String> {
    let pos = rest.find("from ")?;
    let tail = rest[pos + 5..].trim().trim_end_matches(';');
    read_quote(tail)
}

fn extract_side_effect_spec(rest: &str) -> Option<String> {
    let t = rest.trim().trim_end_matches(';');
    read_quote(t)
}

fn read_quote(s: &str) -> Option<String> {
    let q = s.chars().next()?;
    if q != '\'' && q != '"' {
        return None;
    }
    let end = s[1..].find(q)?;
    Some(s[1..1 + end].to_string())
}
