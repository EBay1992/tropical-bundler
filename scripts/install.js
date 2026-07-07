"use strict";

const fs = require("fs");
const path = require("path");
const https = require("https");
const { getPlatform } = require("./platform");

function githubRepo() {
  try {
    const url = require("../package.json").repository.url;
    const m = url.match(/github\.com[:/](.+?)(?:\.git)?$/);
    if (m) return m[1];
  } catch {}
  return "ehsanbayranvand/tropical-bundler";
}

const VERSION = require("../package.json").version;
const REPO = githubRepo();

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);
    const request = (currentUrl) => {
      https
        .get(currentUrl, (res) => {
          if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
            request(res.headers.location);
            return;
          }
          if (res.statusCode !== 200) {
            reject(new Error(`Download failed: HTTP ${res.statusCode} for ${currentUrl}`));
            return;
          }
          res.pipe(file);
          file.on("finish", () => file.close(resolve));
        })
        .on("error", reject);
    };
    request(url);
  });
}

async function installFromGitHub() {
  const { triple, bin } = getPlatform();
  const root = path.join(__dirname, "..");
  const binDir = path.join(root, "bin-native");
  const dest = path.join(binDir, bin);
  const archive = `tropical-bundler-v${VERSION}-${triple}.tar.gz`;
  const url = `https://github.com/${REPO}/releases/download/v${VERSION}/${archive}`;

  fs.mkdirSync(binDir, { recursive: true });

  const tmpArchive = path.join(binDir, archive);
  console.log(`[tropical-bundler] Downloading ${url}`);
  await download(url, tmpArchive);

  const { execSync } = require("child_process");
  try {
    execSync(`tar -xzf "${tmpArchive}" -C "${binDir}"`, { stdio: "inherit" });
    fs.unlinkSync(tmpArchive);
    const flat = path.join(binDir, "tropical-bundler" + (bin.endsWith(".exe") ? ".exe" : ""));
    if (!fs.existsSync(dest) && fs.existsSync(flat)) {
      fs.renameSync(flat, dest);
    }
    if (process.platform !== "win32" && fs.existsSync(dest)) {
      fs.chmodSync(dest, 0o755);
    }
    console.log(`[tropical-bundler] Installed binary to ${dest}`);
  } catch (err) {
    console.warn(`[tropical-bundler] tar extract failed: ${err.message}`);
  }
}

async function main() {
  try {
    const { pkg } = getPlatform();
    require.resolve(`${pkg}/package.json`);
    console.log(`[tropical-bundler] Using platform package ${pkg}`);
    return;
  } catch {
    // optional dep not installed for this platform
  }

  const ext = process.platform === "win32" ? ".exe" : "";
  const local = path.join(__dirname, "..", "target", "release", `tropical-bundler${ext}`);
  if (fs.existsSync(local)) {
    console.log(`[tropical-bundler] Using local cargo build: ${local}`);
    return;
  }

  const cached = path.join(__dirname, "..", "bin-native", `tropical-bundler${ext}`);
  if (fs.existsSync(cached)) {
    return;
  }

  await installFromGitHub();
}

main().catch((err) => {
  console.warn(`[tropical-bundler] install: ${err.message}`);
});
