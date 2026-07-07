# Publishing to GitHub & npm

Matches the same publish style as `search_vid` on npm (`philothinker` account, unscoped package name).

## One-time setup

### 1. GitHub repository

Already published at: https://github.com/EBay1992/tropical-bundler

### 2. npm account + token

1. Log in as `philothinker` on https://www.npmjs.com
2. Create a granular token at https://www.npmjs.com/settings/~/tokens
3. Permissions: **Publish** on all packages (or specifically `tropical_bundler`)
4. Enable **Bypass 2FA** if required by your account policy
5. Add token to GitHub Actions as `NPM_TOKEN` (optional, for release workflow)

### 3. Log in locally

```bash
npm logout
npm config delete //registry.npmjs.org/:_authToken
npm login
npm whoami   # should print: philothinker
```

## Publish

```bash
cd G:\projects\personal\tropical-bundler
npm publish
```

No `--access public` needed (unscoped package, same as `search_vid`).

## After publish, users run

```bash
npx tropical_bundler build --entry src/index.js
```

## Release flow (automated via git tag)

```bash
git tag v0.1.0
git push origin main --tags
```

CI (`.github/workflows/release.yml`) will:
1. Build native binaries for Linux, macOS, Windows
2. Create a GitHub Release
3. Publish platform packages (`tropical_bundler-win32-x64`, etc.)
4. Publish main `tropical_bundler` package

## Package layout

| npm package | Contents |
|---|---|
| `tropical_bundler` | Node bin wrapper (`npx tropical_bundler`) |
| `tropical_bundler-win32-x64` | Windows x64 `.exe` |
| `tropical_bundler-linux-x64-gnu` | Linux x64 binary |
| `tropical_bundler-darwin-arm64` | macOS Apple Silicon |
| `tropical_bundler-darwin-x64` | macOS Intel |
| `tropical_bundler-linux-arm64-gnu` | Linux ARM64 |
