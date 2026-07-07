//! Writes a synthetic JS project to disk mirroring the in-memory scale graph:
//! real files, real `import` statements, and filler code sized to the edge
//! weights, so external bundlers (esbuild, rollup) can consume the same
//! project we solve.

use crate::graphgen::Rng;
use std::fs;
use std::io::Write;
use std::path::Path;

pub struct JsProjectStats {
    pub files: usize,
    pub imports: usize,
    pub bytes: u64,
}

pub fn write_project(
    dir: &Path,
    node_count: usize,
    edge_count: usize,
    seed: u64,
) -> std::io::Result<JsProjectStats> {
    fs::create_dir_all(dir)?;
    let mut rng = Rng::new(seed);

    // Decide each module's byte size first (500 B .. 45 KB), then wire the
    // same backbone + random-cross-import topology as the in-memory generator.
    let sizes: Vec<usize> = (0..node_count).map(|_| 500 + rng.below(44_500) as usize).collect();
    let mut imports: Vec<Vec<usize>> = vec![Vec::new(); node_count];
    let mut placed = 0usize;

    for i in 0..node_count - 1 {
        if placed >= edge_count {
            break;
        }
        imports[i].push(i + 1);
        placed += 1;
    }
    while placed < edge_count {
        let from = rng.below(node_count as u64) as usize;
        let to = rng.below(node_count as u64) as usize;
        if from == to || imports[from].contains(&to) {
            continue;
        }
        imports[from].push(to);
        placed += 1;
    }

    let mut total_bytes = 0u64;
    for i in 0..node_count {
        let path = dir.join(format!("mod{i}.js"));
        let mut f = fs::File::create(&path)?;
        let mut written = 0usize;
        for &t in &imports[i] {
            written += write_line(&mut f, &format!("import {{ v{t} }} from \"./mod{t}.js\";"))?;
        }
        written += write_line(&mut f, &format!("export const v{i} = {i};"))?;
        // Filler payload to hit the target byte size (simulates real code).
        let mut line_no = 0usize;
        while written < sizes[i] {
            let line = format!("export const pad_{i}_{line_no} = \"{}\";", "x".repeat(60));
            written += write_line(&mut f, &line)?;
            line_no += 1;
        }
        total_bytes += written as u64;
    }

    // Entry point importing mod0 (backbone head).
    let mut f = fs::File::create(dir.join("entry.js"))?;
    writeln!(f, "import {{ v0 }} from \"./mod0.js\";")?;
    writeln!(f, "console.log(v0);")?;

    Ok(JsProjectStats { files: node_count + 1, imports: placed, bytes: total_bytes })
}

fn write_line(f: &mut fs::File, line: &str) -> std::io::Result<usize> {
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(line.len() + 1)
}
