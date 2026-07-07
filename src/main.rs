//! Tropical Geometry-Based Dependency Bundler
//!
//! ```text
//! tropical-bundler build  --entry src/index.js [--outdir dist] [--root .] [--budget 512000]
//! tropical-bundler analyze --entry src/index.js [--json]
//! tropical-bundler bench   [nodes] [edges] [budget] [seed]   (internal benchmark)
//! ```

mod baseline;
mod bundle;
mod config;
mod graphgen;
mod jsgen;
mod project;
mod resolve;
mod splitter;
mod transform;
mod tropical;

use std::path::PathBuf;
use std::time::Instant;
use tropical::INF;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }
    match args[1].as_str() {
        "build" => cmd_build(&args[2..]),
        "analyze" | "analyse" => cmd_analyze(&args[2..]),
        "bench" => cmd_bench(&args[2..]),
        "gen-js" => cmd_gen_js(&args[2..]),
        "help" | "-h" | "--help" => print_usage(),
        other => {
            eprintln!("unknown command: {other}");
            print_usage();
            std::process::exit(1);
        }
    }
}

fn cmd_build(args: &[String]) {
    let opts = parse_project_args(args);
    let threads = std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1);
    let t0 = Instant::now();

    banner("TROPICAL BUNDLER :: BUILD");
    println!("| root   : {}", opts.root.display());
    println!("| entry  : {}", opts.entry.display());
    println!("| outdir : {}", opts.outdir.display());
    println!("| budget : {} KB", opts.budget / 1000);

    let t = Instant::now();
    let graph = project::build_graph(&opts.root, &opts.entry).unwrap_or_else(|e| {
        eprintln!("error: failed to scan project: {e}");
        std::process::exit(1);
    });
    println!(
        "| scan   : {} modules, {} externals, {:.1} MB ({:.0} ms)",
        graph.modules.len(),
        graph.externals.len(),
        graph.total_bytes as f64 / 1e6,
        ms(t)
    );

    let direct_imports: Vec<Vec<usize>> = graph
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

    let force_async: Vec<usize> = graph
        .modules
        .iter()
        .flat_map(|m| {
            m.imports
                .iter()
                .filter(|i| i.dynamic)
                .filter_map(|i| i.target)
        })
        .collect();

    let t = Instant::now();
    let (closure, tele) = graph.matrix.closure(threads);
    let solve_ms = ms(t);
    println!(
        "| solve  : {} squarings, {} reachable pairs ({:.0} ms)",
        tele.squarings,
        closure.nnz(),
        solve_ms
    );

    let class = splitter::classify_entry(
        &closure,
        graph.entry_id,
        opts.budget,
        &force_async,
        &direct_imports,
    );

    println!(
        "| split  : {} sync / {} async (est. sync cost {} B)",
        class.sync.len(),
        class.async_cut.len(),
        class.sync_bytes
    );

    let t = Instant::now();
    let result = bundle::emit(
        &graph,
        &closure,
        &class,
        &bundle::BuildOptions {
            outdir: opts.outdir.clone(),
            budget: opts.budget,
        },
    )
    .unwrap_or_else(|e| {
        eprintln!("error: emit failed: {e}");
        std::process::exit(1);
    });
    let emit_ms = ms(t);

    println!("| emit   : main {:.1} KB + {} chunks {:.1} KB ({:.0} ms)",
        result.stats.main_bytes as f64 / 1024.0,
        result.stats.chunks,
        result.stats.chunk_bytes as f64 / 1024.0,
        emit_ms,
    );
    println!("| output : {}", result.main_file.display());
    for c in &result.chunk_files {
        println!("|          {}", c.display());
    }
    println!("| manifest: {}", result.manifest_file.display());
    println!("| TOTAL  : {:.0} ms", ms(t0));
    println!();
    println!("Run with: node {}/main.js", opts.outdir.display());
}

fn cmd_analyze(args: &[String]) {
    let opts = parse_project_args(args);
    let json = flag(args, "--json");
    let threads = std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1);

    let graph = project::build_graph(&opts.root, &opts.entry).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    });

    let direct_imports: Vec<Vec<usize>> = graph
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

    let (closure, tele) = graph.matrix.closure(threads);
    let class = splitter::classify_entry(
        &closure,
        graph.entry_id,
        opts.budget,
        &[],
        &direct_imports,
    );

    if json {
        println!("{{");
        println!("  \"modules\": {},", graph.modules.len());
        println!("  \"externals\": {},", graph.externals.len());
        println!("  \"totalBytes\": {},", graph.total_bytes);
        println!("  \"squarings\": {},", tele.squarings);
        println!("  \"syncModules\": {},", class.sync.len());
        println!("  \"asyncModules\": {},", class.async_cut.len());
        println!("  \"syncBytesEstimate\": {},", class.sync_bytes);
        println!("  \"budgetBytes\": {}", opts.budget);
        println!("}}");
        return;
    }

    banner("TROPICAL BUNDLER :: ANALYZE");
    println!("| entry     : {}", graph.entry.display());
    println!("| modules   : {}", graph.modules.len());
    println!("| externals : {:?}", graph.externals);
    println!("| budget    : {} KB", opts.budget / 1000);
    println!("| sync      : {} modules (~{} KB est.)", class.sync.len(), class.sync_bytes / 1024);
    println!("| async     : {} modules", class.async_cut.len());
    println!("| squarings : {}", tele.squarings);
    println!();
    println!("Top async-cut modules (by min cost from entry):");
    let mut async_list: Vec<(usize, u32, &PathBuf)> = graph
        .modules
        .iter()
        .filter(|m| class.async_cut.contains(&m.id))
        .map(|m| (m.id, closure.get(graph.entry_id, m.id), &m.path))
        .collect();
    async_list.sort_by_key(|(_, c, _)| *c);
    for (id, cost, path) in async_list.iter().take(15) {
        let rel = path.strip_prefix(&graph.root).unwrap_or(path);
        println!("  [{id:>4}] {cost:>10} B  {}", rel.display());
    }
    if async_list.len() > 15 {
        println!("  ... and {} more", async_list.len() - 15);
    }
}

fn cmd_bench(args: &[String]) {
    let nodes: usize = args.get(0).and_then(|a| a.parse().ok()).unwrap_or(2500);
    let edges: usize = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(12_000);
    let budget: u32 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(50_000);
    let seed: u64 = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(42);
    scale_benchmark(nodes, edges, budget, seed);
}

fn cmd_gen_js(args: &[String]) {
    let dir = PathBuf::from(args.get(0).map(String::as_str).unwrap_or("bench-project/src"));
    let nodes: usize = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(2500);
    let edges: usize = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(12_000);
    let seed: u64 = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(42);
    let t = Instant::now();
    let stats = jsgen::write_project(&dir, nodes, edges, seed).expect("write project");
    println!(
        "wrote {} files / {} imports / {:.1} MB to {} in {:.0} ms",
        stats.files,
        stats.imports,
        stats.bytes as f64 / 1e6,
        dir.display(),
        ms(t)
    );
}

struct ProjectArgs {
    root: PathBuf,
    entry: PathBuf,
    outdir: PathBuf,
    budget: u32,
}

fn parse_project_args(args: &[String]) -> ProjectArgs {
    let mut root = PathBuf::from(".");
    let mut entry = None;
    let mut outdir = PathBuf::from("dist");
    let mut budget = 512_000u32;
    let mut config_path = None;
    let mut cli_root = false;
    let mut cli_entry = false;
    let mut cli_outdir = false;
    let mut cli_budget = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" | "-r" => {
                i += 1;
                root = PathBuf::from(&args[i]);
                cli_root = true;
            }
            "--entry" | "-e" => {
                i += 1;
                entry = Some(PathBuf::from(&args[i]));
                cli_entry = true;
            }
            "--outdir" | "-o" => {
                i += 1;
                outdir = PathBuf::from(&args[i]);
                cli_outdir = true;
            }
            "--budget" | "-b" => {
                i += 1;
                budget = args[i].parse().unwrap_or(512_000);
                cli_budget = true;
            }
            "--config" | "-c" => {
                i += 1;
                config_path = Some(PathBuf::from(&args[i]));
            }
            _ => {
                if entry.is_none() && !args[i].starts_with('-') {
                    entry = Some(PathBuf::from(&args[i]));
                    cli_entry = true;
                }
            }
        }
        i += 1;
    }

    if let Some(cfg_path) = config_path.or_else(|| config::discover_config(&root)) {
        if let Ok(cfg) = config::load(&cfg_path) {
            if entry.is_none() && !cli_entry {
                entry = cfg.entry.map(PathBuf::from);
            }
            if !cli_outdir {
                if let Some(o) = cfg.outdir {
                    outdir = PathBuf::from(o);
                }
            }
            if !cli_budget {
                if let Some(b) = cfg.budget {
                    budget = b;
                }
            }
            if !cli_root {
                if let Some(r) = cfg.root {
                    root = PathBuf::from(r);
                }
            }
        }
    }

    let entry = entry.unwrap_or_else(|| {
        eprintln!("error: --entry is required (or set \"entry\" in tropical.config.json)");
        std::process::exit(1);
    });

    ProjectArgs { root, entry, outdir, budget }
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn scale_benchmark(nodes: usize, edges: usize, budget: u32, seed: u64) {
    let threads = std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1);

    banner("TROPICAL BUNDLER :: SCALE BENCHMARK");
    println!("| graph        : {nodes} modules, {edges} static imports");
    println!("| weights      : 500 B .. 45 KB per module (seed {seed})");
    println!("| async budget : {} KB", budget / 1000);
    println!("| threads      : {threads}");

    let t = Instant::now();
    let graph = graphgen::generate_scale_project(nodes, edges, seed);
    let gen_ms = ms(t);
    println!("| generated    : {} finite entries in {gen_ms:.2} ms", graph.nnz());

    banner("PHASE 1 :: TROPICAL CLOSURE");
    let t = Instant::now();
    let (closure, tele) = graph.closure(threads);
    let tropical_ms = ms(t);
    println!("| squarings     : {}", tele.squarings);
    println!("| time          : {tropical_ms:.2} ms");

    banner("PHASE 2 :: ALL-PAIRS DIJKSTRA (oracle)");
    let t = Instant::now();
    let oracle = baseline::dijkstra_all_pairs(&graph, threads);
    let dijkstra_ms = ms(t);
    println!("| time          : {dijkstra_ms:.2} ms");

    let mut mismatches = 0usize;
    let mut checked = 0usize;
    for i in 0..nodes {
        for j in 0..nodes {
            let d_oracle = oracle[i * nodes + j];
            let d_trop = closure.get(i, j);
            if d_oracle < INF || d_trop < INF {
                checked += 1;
                if d_oracle != d_trop {
                    mismatches += 1;
                }
            }
        }
    }
    println!("| accuracy      : {:.4}%", 100.0 * (checked - mismatches) as f64 / checked.max(1) as f64);

    banner("SUMMARY");
    println!("| tropical : {tropical_ms:.2} ms | dijkstra : {dijkstra_ms:.2} ms");
}

fn print_usage() {
    println!(
        r#"tropical-bundler — tropical-geometry code splitter & bundler

USAGE:
  tropical-bundler build   --entry <file> [options]
  tropical-bundler analyze --entry <file> [options]
  tropical-bundler bench   [nodes] [edges] [budget] [seed]

BUILD / ANALYZE OPTIONS:
  -e, --entry <file>     Entry module (required unless in config)
  -r, --root <dir>       Project root          [default: .]
  -o, --outdir <dir>     Output directory      [default: dist]
  -b, --budget <bytes>   Sync chunk byte budget [default: 512000]
  -c, --config <file>    Config file           [default: tropical.config.json]
      --json             (analyze) machine-readable output

CONFIG (tropical.config.json):
  {{ "entry": "src/index.js", "outdir": "dist", "root": ".", "budget": 512000 }}

EXAMPLES:
  tropical-bundler build --entry src/main.js --outdir dist --budget 50000
  tropical-bundler analyze --entry apps/web/src/index.ts --json
  tropical-bundler build   # reads tropical.config.json from cwd

OUTPUT:
  dist/main.js           synchronous chunk (entry + budget-respecting modules)
  dist/chunk-N.js          async chunks (modules over budget)
  dist/manifest.json       per-module sync/async assignment + costs
  dist/tropical-meta.json  build metadata
"#
    );
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn banner(title: &str) {
    println!("\n+{}+", "-".repeat(68));
    println!("| {title:<66} |");
    println!("+{}+", "-".repeat(68));
}
