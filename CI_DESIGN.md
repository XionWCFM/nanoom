# nanoom CI/CD Design

## 1. Repository Structure for CI
```
.github/
├── workflows/
│   ├── ci.yml              # Main CI pipeline
│   ├── release.yml         # Release automation
│   ├── action-test.yml     # Test composite actions
│   ├── dependabot.yml      # Dependency updates
│   └── security.yml        # Security scanning
├── dependabot.yml          # Dependabot config
└── CODEOWNERS              # Required reviews
```

## 2. Main CI Pipeline (ci.yml)

### Triggers
- push to main
- pull_request to main
- pull_request from forks (limited permissions)

### Jobs Matrix
```yaml
strategy:
  matrix:
    os: [ubuntu-latest, macos-latest, windows-latest]
    rust: [stable]  # beta optional
```

### Steps (in order, fail-fast)
1. **Checkout** - actions/checkout@v4 with fetch-depth: 0 (need full history for merge-base tests)
2. **Setup Rust** - dtolnay/rust-toolchain@stable with rust-toolchain.toml
3. **Cache** - Swatinem/rust-cache@v2 (cargo registry, target, gitoxide cache)
4. **Format Check** - `cargo fmt --all --check`
5. **Clippy** - `cargo clippy --all-targets --all-features -- -D warnings`
6. **Test** - `cargo test --all --workspace`
7. **Coverage** - `cargo llvm-cov --workspace --all-features --fail-under-lines 96`
8. **Build Release** - `cargo build --release --all-targets`
9. **Audit** - `cargo audit --deny warnings` (security vulnerabilities)
10. **Deny** - `cargo deny check` (license, bans, sources) - optional but recommended

### Fork PR Security
```yaml
permissions:
  contents: read
  # No write permissions for fork PRs
```
- Use `github.event.pull_request.head.repo.fork` to detect forks
- Run reduced pipeline for forks (no deploy, no secrets)

## 3. Release Pipeline (release.yml)

### Trigger
- push tag matching `v*`

### Steps
1. **Checkout** with fetch-depth: 0
2. **Setup Rust** + targets:
   - x86_64-unknown-linux-gnu
   - aarch64-unknown-linux-gnu
   - x86_64-apple-darwin
   - aarch64-apple-darwin
   - x86_64-pc-windows-msvc
3. **Cache** cargo
4. **Build** all targets: `cargo build --release --target=$TARGET`
5. **Test** each target (cross or native)
6. **Package** binaries with checksums (sha256)
7. **Create GitHub Release** - softprops/action-gh-release@v1
   - Upload all 5 binaries + checksums.txt
   - Generate changelog from changesets
8. **Publish npm** - `@nanoom/cli`
   - Build npm package with optionalDependencies
   - `npm publish --access public`

### Version Management
- changesets in `.changeset/*.md`
- `changeset version` bumps package.json + updates CHANGELOG.md
- Cargo.toml version synced via script: `cargo set-version $VERSION`
- Tag pushed triggers release.yml

## 4. Action Testing Pipeline (action-test.yml)

### Trigger
- push to main
- pull_request
- workflow_dispatch (manual)

### Test Matrix
```yaml
strategy:
  matrix:
    scenario:
      - basic-monorepo
      - pnpm-workspace
      - yarn-workspace
      - turbo-repo
      - nx-repo
      - fork-pr
      - merge-queue
      - sharding
      - isolate
      - global-deps
```

### Steps
1. **Checkout** nanoom repo (for actions)
2. **Setup test scenario** - checkout test fixture repo OR create in-memory
3. **Run composite actions** using local path: `uses: ./.github/actions/nanoom-affected`
4. **Verify outputs** - matrix JSON structure, hasChange, workspace mapping
5. **Verify status aggregation** - pass/fail detection

### Test Fixtures
- Store in `.github/test-fixtures/` or separate test repos
- Each fixture: minimal monorepo demonstrating the scenario
- Use `actions/checkout` with `repository:` for cross-repo testing

## 5. Composite Action Design

### Structure per Action
```
.github/actions/nanoom-affected/
├── action.yml
├── scripts/
│   └── download-binary.sh
└── README.md
```

### action.yml Template
```yaml
name: "nanoom Affected"
description: "Detect affected workspaces and generate matrix"
inputs:
  config-file:
    description: "Path to nanoom.config.json"
    default: "nanoom.config.json"
    required: false
  comparison:
    description: "Comparison mode: merge-base or tip"
    default: "merge-base"
    required: false
  github-token:
    description: "GitHub token for API calls"
    required: true
    default: "${{ github.token }}"
outputs:
  matrix:
    description: "Matrix include JSON"
    value: ${{ steps.run.outputs.matrix }}
  has-changes:
    description: "Whether any changes detected"
    value: ${{ steps.run.outputs.has-changes }}
runs:
  using: "composite"
  steps:
    - name: Download nanoom binary
      id: download
      uses: ./.github/actions/setup-nanoom
    - name: Run affected
      id: run
      shell: bash
      run: |
        ${{ steps.download.outputs.binary }} affected \
          --config ${{ inputs.config-file }} \
          --comparison ${{ inputs.comparison }} \
          --output matrix.json
        echo "matrix=$(cat matrix.json)" >> $GITHUB_OUTPUT
        echo "has-changes=$(jq -r '.hasChange' matrix.json)" >> $GITHUB_OUTPUT
```

### Shared Setup Action
```
.github/actions/setup-nanoom/
├── action.yml
└── scripts/
    └── download.sh
```
- Downloads correct binary for platform from GitHub Releases
- Falls back to npm (@nanoom/cli) if release not found
- Caches binary in runner tool cache

## 6. Security Practices

### Supply Chain
- `cargo deny` for license compliance, banned crates
- `cargo audit` in CI for vulnerabilities
- `cargo vet` for supply chain audit (optional)
- SBOM generation: `cargo sbom` → upload as artifact

### Fork Protection
- Fork PRs get read-only permissions
- No secrets passed to fork PR workflows
- Label-based approval: `safe-to-test` label required for fork PRs to run full CI
- `pull_request_target` only for trusted actions (label check)

### Secrets Management
- `GITHUB_TOKEN` only for release workflow
- No secrets in action-test.yml
- npm token only in release.yml (environment protection)

## 7. Quality Gates (Required for Merge)

All must pass:
- [ ] Format check
- [ ] Clippy (no warnings)
- [ ] All tests pass
- [x] Coverage ≥ 96%
- [ ] Build succeeds on all 3 OS
- [ ] No security vulnerabilities (cargo audit)
- [ ] No license violations (cargo deny)
- [ ] Action integration tests pass

## 8. Dependabot Configuration

```yaml
# .github/dependabot.yml
version: 2
updates:
  - package-ecosystem: "cargo"
    directory: "/"
    schedule:
      interval: "weekly"
    groups:
      rust-deps:
        patterns: ["*"]
    labels: ["dependencies", "rust"]
  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "weekly"
    labels: ["dependencies", "github-actions"]
  - package-ecosystem: "npm"
    directory: "/packages/cli"  # if separate
    schedule:
      interval: "weekly"
    labels: ["dependencies", "npm"]
```

## 9. Pre-commit Hooks (Local)

```bash
# .git/hooks/pre-commit (or use lefthook)
#!/bin/bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```

Or use `lefthook.yml`:
```yaml
pre-commit:
  commands:
    fmt:
      run: cargo fmt --all --check
    clippy:
      run: cargo clippy --all-targets --all-features -- -D warnings
    test:
      run: cargo test --all
```

## 10. Monitoring & Observability

- CI metrics: build time, test time, coverage trend
- Release frequency tracking
- Action usage analytics (download counts)
- Error rate tracking for actions
