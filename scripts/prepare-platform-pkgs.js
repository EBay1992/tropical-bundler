#!/usr/bin/env node
"use strict";
const fs = require("fs");
const path = require("path");
const { PLATFORMS } = require("./platform");

const version = process.argv[2];
const artifactsDir = process.argv[3];
if (!version || !artifactsDir) {
  console.error("Usage: node scripts/prepare-platform-pkgs.js <version> <artifacts-dir>");
  process.exit(1);
}

const outRoot = path.join(__dirname, "..", "npm-dist");
fs.rmSync(outRoot, { recursive: true, force: true });
fs.mkdirSync(outRoot, { recursive: true });

for (const info of Object.values(PLATFORMS)) {
  const src = path.join(artifactsDir, info.triple, info.bin);
  if (!fs.existsSync(src)) {
    console.warn(`skip ${info.pkg}: missing ${src}`);
    continue;
  }
  const pkgDir = path.join(outRoot, info.pkg);
  fs.mkdirSync(pkgDir, { recursive: true });
  fs.copyFileSync(src, path.join(pkgDir, info.bin));
  if (process.platform !== "win32") {
    fs.chmodSync(path.join(pkgDir, info.bin), 0o755);
  }
  const pkgJson = {
    name: info.pkg,
    version,
    description: `Native binary of tropical-bundler for ${info.triple}`,
    license: "MIT",
    repository: {
      type: "git",
      url: "git+https://github.com/EBay1992/tropical-bundler.git",
    },
    os: info.os,
    cpu: info.cpu,
    files: [info.bin],
  };
  fs.writeFileSync(path.join(pkgDir, "package.json"), JSON.stringify(pkgJson, null, 2) + "\n");
  console.log(`prepared ${info.pkg}`);
}

console.log(`Platform packages written to ${outRoot}`);
