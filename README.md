# Tropical Bundler

A bundler that solves **global code-splitting boundaries** with tropical (min-plus) linear algebra instead of recursive graph traversal.

Point it at a real JS/TS project, set an entry and byte budget, and it emits:

- `dist/main.js` — synchronous chunk (entry + modules within budget)
- `dist/chunk-N.js` — async chunks (modules over budget)
- `dist/manifest.json` — per-module sync/async assignment with min costs
- `dist/tropical-meta.json` — build metadata

## Install

### npm / npx (recommended)

```bash
# Run without installing
npx tropical-bundler build --entry src/index.js

# Or install as a dev dependency
npm install -D tropical-bundler
npx tropical-bundler analyze --entry src/index.js
```

### From source (Rust)

```bash
git clone https://github.com/ehsanbayranvand/tropical-bundler.git
cd tropical-bundler
cargo build --release
# binary: target/release/tropical-bundler(.exe)
```

### Local development (npm link + cargo)

```bash
cargo build --release
npm link
tropical-bundler help
```

## Quick start (your project)

**1. Add a config** — `tropical.config.json` in your project root:

```json
{
  "entry": "src/index.js",
  "outdir": "dist",
  "root": ".",
  "budget": 512000
}
```

`budget` is the max accumulated byte cost for the synchronous chunk (512 KB default). Modules above that threshold are split into async chunks.

**2. Build**

```bash
tropical-bundler build
# or with explicit flags (CLI overrides config):
tropical-bundler build --entry src/main.js --outdir dist --budget 50000
```

**3. Run output**

```bash
node dist/main.js
```

**4. Analyze without building** (inspect split plan first)

```bash
tropical-bundler analyze --entry src/index.js
tropical-bundler analyze --entry src/index.js --json
```

## CLI reference

```text
tropical-bundler build   --entry <file> [--root .] [--outdir dist] [--budget 512000]
tropical-bundler analyze --entry <file> [--json]
tropical-bundler bench   [nodes] [edges] [budget] [seed]   # internal benchmark
tropical-bundler help
```

| Flag | Description |
|---|---|
| `-e, --entry` | Entry module (required unless set in config) |
| `-r, --root` | Project root |
| `-o, --outdir` | Output directory |
| `-b, --budget` | Sync chunk byte budget |
| `-c, --config` | Config file path |

## What it supports today

- `.js`, `.jsx`, `.mjs`, `.cjs`, `.ts`, `.tsx`, `.mts`, `.cts` source files
- Static `import` / `export` / `require()` / dynamic `import()`
- Relative imports (`./foo`, `../bar`) with extension + `index` resolution
- Bare specifiers (`react`, `lodash`) treated as **externals** (not bundled)
- Reachability scan from entry (only imported modules are included)
- Tropical closure for globally optimal min-cost path to every module
- Budget-based async chunk splitting with importer propagation

## Limitations (read before real-world testing)

- **No TypeScript transpilation** — `.ts` files are bundled as-is. Pre-compile TS, or use `.js` entry points.
- **No `node_modules` bundling** — npm packages are externals. Provide them via CDN/import maps at runtime.
- **Line-based transform** — complex ESM patterns (re-exports, barrel files, `import()` inline expressions) may need simplification.
- **No source maps yet** — output is a single concatenated runtime per chunk.
- **No dev server / watch mode** — build-only for now.

These are PoC boundaries; the tropical solver and split planner are production-grade, the emit pipeline is intentionally minimal so you can validate the split math on real graphs.

## Example

```bash
cd examples/hello
../../target/release/tropical-bundler build   # uses tropical.config.json
node dist/main.js
# Hello, world!
# entry ready function
```

## How splitting works

1. Scan project → build sparse tropical matrix (`A[i][j]` = byte size of `j` when `i` imports it)
2. Compute closure via repeated min-plus matrix squaring (`D[i][j]` = cheapest path cost from `i` to `j`)
3. For entry `e`: modules with `D[e][j] ≤ budget` → sync chunk; rest → async chunks
4. Static importers of async modules are promoted to async automatically

See `manifest.json` after build for the full per-module assignment.

## Internal benchmark

```bash
tropical-bundler bench 2500 12000 50000 42
cargo test --release
```
