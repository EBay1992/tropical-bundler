// Times esbuild (and optionally rollup) on the synthetic 2500-module project.
// Reports the wall time for a full bundle of entry.js so it can be compared
// with the tropical engine's scan -> solve -> split pipeline.
import { performance } from "node:perf_hooks";
import * as esbuild from "esbuild";

const runs = Number(process.argv[2] ?? 3);
const withRollup = process.argv.includes("--rollup");

async function timeEsbuild() {
  const t0 = performance.now();
  await esbuild.build({
    entryPoints: ["src/entry.js"],
    bundle: true,
    write: false,
    format: "esm",
    logLevel: "silent",
  });
  return performance.now() - t0;
}

const esbuildTimes = [];
for (let i = 0; i < runs; i++) {
  esbuildTimes.push(await timeEsbuild());
}
console.log(
  `esbuild full bundle (${runs} runs): ${esbuildTimes.map((t) => t.toFixed(0)).join(" / ")} ms, best ${Math.min(...esbuildTimes).toFixed(0)} ms`
);

if (withRollup) {
  try {
    const { rollup } = await import("rollup");
    const t0 = performance.now();
    const bundle = await rollup({ input: "src/entry.js", logLevel: "silent" });
    await bundle.generate({ format: "esm" });
    await bundle.close();
    console.log(`rollup full bundle (1 run): ${(performance.now() - t0).toFixed(0)} ms`);
  } catch (err) {
    // Rollup's recursive module analysis overflows the JS call stack on the
    // 2500-deep static import chain — the exact failure mode the tropical
    // matrix formulation sidesteps (no traversal recursion at all).
    console.log(`rollup full bundle: FAILED (${err.constructor.name}: ${err.message})`);
  }
}
