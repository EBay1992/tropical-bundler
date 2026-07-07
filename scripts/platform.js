"use strict";

/** Map Node (process.platform, process.arch) → Rust target triple + npm platform package suffix. */
const PLATFORMS = {
  "win32-x64": {
    triple: "x86_64-pc-windows-msvc",
    pkg: "tropical_bundler-win32-x64",
    bin: "tropical-bundler.exe",
    os: ["win32"],
    cpu: ["x64"],
  },
  "linux-x64": {
    triple: "x86_64-unknown-linux-gnu",
    pkg: "tropical_bundler-linux-x64-gnu",
    bin: "tropical-bundler",
    os: ["linux"],
    cpu: ["x64"],
  },
  "darwin-x64": {
    triple: "x86_64-apple-darwin",
    pkg: "tropical_bundler-darwin-x64",
    bin: "tropical-bundler",
    os: ["darwin"],
    cpu: ["x64"],
  },
  "darwin-arm64": {
    triple: "aarch64-apple-darwin",
    pkg: "tropical_bundler-darwin-arm64",
    bin: "tropical-bundler",
    os: ["darwin"],
    cpu: ["arm64"],
  },
  "linux-arm64": {
    triple: "aarch64-unknown-linux-gnu",
    pkg: "tropical_bundler-linux-arm64-gnu",
    bin: "tropical-bundler",
    os: ["linux"],
    cpu: ["arm64"],
  },
};

function getPlatform() {
  const key = `${process.platform}-${process.arch}`;
  const info = PLATFORMS[key];
  if (!info) {
    throw new Error(
      `Unsupported platform: ${key}. Supported: ${Object.keys(PLATFORMS).join(", ")}`
    );
  }
  return { key, ...info };
}

module.exports = { PLATFORMS, getPlatform };
