# Publishing to GitHub & npm

## One-time setup

### 1. Create the GitHub repository

```bash
# If you have GitHub CLI:
gh repo create tropical-bundler --public --source=. --remote=origin

# Or create https://github.com/YOUR_USER/tropical-bundler manually, then:
git remote add origin https://github.com/YOUR_USER/tropical-bundler.git
```

Update `package.json` → `repository.url` if your GitHub username differs from `nareks`.

### 2. npm account + token

1. Create an account at https://www.npmjs.com
2. Create an automation token: https://www.npmjs.com/settings/~/tokens
3. Add it to GitHub repo **Settings → Secrets → Actions** as `NPM_TOKEN`

### 3. Log in locally (optional, for manual publish)

```bash
npm login
```

## Release flow (automated)

Every git tag `v*` triggers `.github/workflows/release.yml`:

1. Builds native binaries for Linux, macOS, Windows (x64 + arm64)
2. Creates a GitHub Release with `.tar.gz` assets
3. Publishes platform npm packages (`tropical-bundler-win32-x64`, etc.)
4. Publishes the main `tropical-bundler` package

```bash
git add .
git commit -m "Prepare v0.1.0"
git tag v0.1.0
git push origin main --tags
# or: git push origin master --tags
```

After CI completes, users can run:

```bash
npx tropical-bundler@0.1.0 build --entry src/index.js
```

## Manual publish (first time / debugging)

```bash
cargo build --release
node scripts/prepare-platform-pkgs.js 0.1.0 target/release-staged
# stage binaries as: target/release-staged/x86_64-pc-windows-gnu/tropical-bundler.exe

# Publish platform package(s) for your OS:
cd npm-dist/tropical-bundler-win32-x64 && npm publish --access public

# Publish main wrapper:
cd ../../ && npm publish --access public
```

## Package layout

| npm package | Contents |
|---|---|
| `tropical-bundler` | Node bin wrapper (`npx tropical-bundler`) |
| `tropical-bundler-win32-x64` | Windows x64 `.exe` |
| `tropical-bundler-linux-x64-gnu` | Linux x64 binary |
| `tropical-bundler-darwin-arm64` | macOS Apple Silicon |
| `tropical-bundler-darwin-x64` | macOS Intel |
| `tropical-bundler-linux-arm64-gnu` | Linux ARM64 |

The main package lists all platform packages as `optionalDependencies`; npm installs only the one matching your OS/CPU.
