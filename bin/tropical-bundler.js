#!/usr/bin/env node
"use strict";

const { spawnSync } = require("child_process");
const fs = require("fs");
const path = require("path");
const { getPlatform } = require("../scripts/platform");

function findBinary() {
  if (process.env.TROPICAL_BUNDLER_BINARY) {
    return process.env.TROPICAL_BUNDLER_BINARY;
  }

  const { pkg, bin } = getPlatform();
  const ext = process.platform === "win32" ? ".exe" : "";

  // 1. Optional platform-specific npm package (published alongside main package).
  try {
    const pkgRoot = path.dirname(require.resolve(`${pkg}/package.json`));
    const candidate = path.join(pkgRoot, bin);
    if (fs.existsSync(candidate)) return candidate;
  } catch {
    // not installed
  }

  // 2. Cached binary from postinstall GitHub download.
  const cached = path.join(__dirname, "..", "bin-native", `tropical-bundler${ext}`);
  if (fs.existsSync(cached)) return cached;

  // 3. Local cargo release build (development).
  const local = path.join(__dirname, "..", "target", "release", `tropical-bundler${ext}`);
  if (fs.existsSync(local)) return local;

    console.error(
    "[tropical_bundler] Native binary not found.\n" +
      "  Try: npm install tropical_bundler\n" +
      "  Or:  cargo build --release (from source checkout)"
  );
  process.exit(1);
}

const bin = findBinary();
const result = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}
process.exit(result.status ?? 1);
