# nanoom Implementation Plan

## Project Overview

| Property | Value |
|----------|-------|
| **Name** | nanoom |
| **Language** | Rust (single binary, monocrater) |
| **Distribution** | GitHub Releases + npm (@nanoom/cli with optionalDependencies for platform binaries) |
| **License** | MIT |
| **CI** | GitHub Actions with security best practices (labels for fork PRs) |

---

## Architecture Decisions

### 1. CLI Structure

Single binary `nanoom` with subcommands:

| Subcommand | Description |
|------------|-------------|
| `nanoom affected` | Detect changed workspaces, output matrix JSON |
| `nanoom run` | Execute tasks (turbo/yarn/pnpm/nx via runner input) |
| `nanoom install` | Install dependencies (auto-detect pm, root-install option) |
| `nanoom status` | Aggregate job results (pass/fail) |
| `nanoom cache-key` | Generate a deterministic task cache key |
| `nanoom version` | Print the binary version |

Also available as GitHub Actions: `nanoom-affected`, `nanoom-run`, `nanoom-install`, `nanoom-status`

---

### 2. Configuration

**File**: `nanoom.config.json` only (no JS/TS - no Node runtime needed)

**Schema**: JSON Schema generated via `schemars` from Rust types

**Structure**:
```json
{
  "$schema": "nanoom.schema.json",
  "group": {
    "ci": {
      "tasks": ["test", "build", "typecheck"],
      "concurrency": 4,
      "rules": [
        { "name": "@nanoom/test", "ignore": true },
        { "name": "@nanoom/run", "isolate": ["build"] }
      ]
    },
    "e2e": {
      "tasks": ["test:e2e"],
      "concurrency": 4,
      "rules": [
        { "name": "@nanoom/core", "shard": [{ "task": "test:e2e", "shard": 4 }] }
      ]
    }
  },
  "globalDependencies": ["yarn.lock", "**/pnpm-lock.yaml"]
}
```

**Field Definitions**:
- `group`: Named task groups (ci, e2e, etc.)
  - `tasks`: List of task names to run
  - `concurrency`: Maximum parallel matrix jobs; all eligible matrix entries are retained
  - `rules`: Per-workspace overrides
    - `ignore`: Skip this workspace entirely
    - `isolate`: Tasks that get dedicated runner (separate matrix entry)
    - `shard`: Split task into N shards via env vars
- `globalDependencies`: Glob patterns for files that affect all workspaces (full glob support via globset)

---

### 3. Workspace Detection

**Auto-detection** from:
- `package.json` workspaces
- `pnpm-workspace.yaml`
- `turbo.json` / `nx.json` (scoped by their project/workspace configuration)

**Override/filter** via config

---

### 4. Dependency Graph

| Condition | Strategy |
|-----------|----------|
| turborepo/nx config exists | Use their pipeline definitions |
| yarn/pnpm workspaces | Topological sort based on package.json dependencies |

---

### 5. Git Operations (Base Commit Detection)

| Aspect | Decision |
|--------|----------|
| **Backend** | `gitoxide` (pure Rust, no libgit2 dependency) |
| **Events supported** | push, pull_request, merge_group |
| **Logic** | Port shell script logic to Rust with proper error handling |
| **Shallow repo handling** | Fetch depth 128, deepen on merge-base failure |
| **Fork support** | Full support for fork PRs |

---

### 6. Affected Calculation

1. Get base commit
2. `git diff --name-only BASE_COMMIT HEAD`
3. Map changed files to workspaces
4. Apply `globalDependencies` glob matching (globset)
5. Apply rules: ignore, isolate, shard
6. Output: `AffectedOutput` JSON

---

### 7. Matrix Generation

- **Fully in Rust CLI** - outputs complete `matrix.include` JSON
- **Internal schema** + converter for GitHub Actions `matrix.include`
- Also usable by GitLab CI, CircleCI, etc.

---

### 8. Sharding

- **Generic sharding**: Split arbitrary CLI command into N shards
- Shard index passed via env var:
  - `NANOOM_SHARD_INDEX`
  - `NANOOM_SHARD_TOTAL`
- Test runners (vitest/jest/playwright) can read these for native sharding

---

### 9. Isolate Rule

- Isolated task gets dedicated runner (separate matrix entry)
- Means "this task is heavy, give it full runner resources"

---

### 10. Error Reporting

- Real-time streaming logs
- Summary at end
- JUnit/XML report generation
- Artifacts for failed runs
- Unix-compliant exit codes, stderr for errors

---

### 11. Caching

- **Key generation helper only** - no storage backend
- Output cache key based on:
  - Lockfile hash
  - Config hash
  - Workspace paths

---

### 12. Version Management

- **changesets** for package.json versioning
- Cargo.toml version synced separately (script or manual)

---

### 13. Install Action (`nanoom install` / `nanoom-install`)

- Auto-detect package manager priority:
  1. pnpm (`pnpm-lock.yaml`)
  2. yarn (`yarn.lock`)
  3. npm (`package-lock.json`)
- nx/turbo are NOT package managers - install is same regardless
- `root-install` option for root dependencies
- Implemented as **Composite Action** + **npm package** + **CLI subcommand**

---

### 14. Run Action (`nanoom run` / `nanoom-run`)

- **Required input**: `runner: "turbo" | "yarn" | "pnpm" | "nx"`
- Constructs filter/shard arguments for the runner
- Executes the command

---

### 15. Status Aggregation

| Interface | Description |
|-----------|-------------|
| **CLI** | `nanoom status affected,ci` - reads job outputs, exits 0/1 |
| **Action** | `nanoom-status` - composite action for workflow use |

---

### 16. Tech Stack

| Category | Library | Purpose |
|----------|---------|---------|
| CLI parsing | `clap` (derive API) | Command-line interface |
| Serialization | `serde` + `serde_json` | JSON handling |
| Glob matching | `globset` | Full glob + .gitignore patterns |
| Error handling | `anyhow` + `thiserror` | Error types & handling |
| Async runtime | `tokio` (full) | GitHub API calls, network fetch |
| Git operations | `gitoxide` | Sync API for local ops, async for fetch |
| Schema gen | `schemars` | JSON Schema generation from Rust types |
| Testing | In-memory Git via gitoxide | No fixtures, prefer real integration tests |

---

### 17. GitHub Actions Structure

```
actions/
├── nanoom-affected/    # composite or docker
├── nanoom-run/         # composite
├── nanoom-install/     # composite
├── nanoom-status/      # composite
```

Each action downloads binary from GitHub Releases OR uses npm package.

---

### 18. Release Process

1. changesets for version bump (package.json)
2. Cargo.toml version synced separately
3. GitHub Release with platform binaries:
   - linux-x64
   - linux-arm64
   - macos-x64
   - macos-arm64
   - windows-x64
4. npm publish @nanoom/cli with optionalDependencies for each platform

---

## Implementation Phases

### Phase 1: Core Rust CLI (Weeks 1-3)

| Week | Tasks |
|------|-------|
| **Week 1** | 1. Cargo project setup with dependencies<br>2. Config parsing + JSON Schema generation<br>3. Git operations (gitoxide) - base commit detection |
| **Week 2** | 4. Workspace detection (auto + config override)<br>5. File-to-workspace mapping + glob matching<br>6. Affected calculation + matrix generation |
| **Week 3** | 7. CLI subcommands: affected, run, install, status<br>8. Unit tests (95% coverage target) |

---

### Phase 2: GitHub Actions (Week 4)

| Task | Description |
|------|-------------|
| 1. Composite actions | Create composite action for each subcommand |
| 2. Binary download | Download from Releases / npm fallback |
| 3. Action I/O | Action inputs/outputs matching spec |
| 4. Integration testing | Test in dedicated test repository |

---

### Phase 3: Polish & Release (Week 5)

| Task | Description |
|------|-------------|
| 1. Error messages | Improve error message quality and clarity |
| 2. Documentation | Write user-facing docs, API docs, examples |
| 3. Release automation | Automate release pipeline with changesets |
| 4. Example repos | Create example repositories demonstrating usage |

---

## Detailed Task Breakdown

### Phase 1.1: Project Setup

- [ ] Initialize Cargo workspace
- [ ] Add dependencies: clap, serde, serde_json, globset, anyhow, thiserror, tokio, gitoxide, schemars
- [ ] Set up JSON Schema generation via schemars
- [ ] Configure CI (GitHub Actions) for Rust project
- [ ] Add pre-commit hooks (cargo fmt, cargo clippy)

### Phase 1.2: Configuration System

- [ ] Define Rust types for `nanoom.config.json`
- [ ] Implement config loading with validation
- [ ] Generate `nanoom.schema.json` via schemars
- [ ] Add `$schema` support in config
- [ ] Unit tests for config parsing

### Phase 1.3: Git Operations

- [ ] Implement base commit detection for:
  - [ ] push events
  - [ ] pull_request events
  - [ ] merge_group events
- [ ] Handle shallow repositories (fetch depth 128, deepen on failure)
- [ ] Full fork PR support
- [ ] Unit tests with in-memory git repositories

### Phase 1.4: Workspace Detection

- [ ] Parse `package.json` workspaces
- [ ] Parse `pnpm-workspace.yaml`
- [ ] Parse `turbo.json` / `nx.json`
- [ ] Config override/filter support
- [ ] Return ordered workspace list with paths

### Phase 1.5: Dependency Graph

- [ ] Detect turborepo/nx configs
- [ ] Extract pipeline definitions from turbo/nx
- [ ] Fallback: topological sort from package.json dependencies
- [ ] Handle cycles gracefully

### Phase 1.6: Affected Calculation

- [ ] `git diff --name-only BASE_COMMIT HEAD` via gitoxide
- [ ] Map changed files → workspaces
- [ ] Apply `globalDependencies` glob matching (globset)
- [ ] Apply rules: ignore, isolate, shard
- [ ] Generate `AffectedOutput` JSON

### Phase 1.7: Matrix Generation

- [ ] Internal matrix representation
- [ ] Converter to GitHub Actions `matrix.include` format
- [ ] Support for isolate (dedicated runner entries)
- [ ] Support for shard (N matrix entries per sharded task)

### Phase 1.8: CLI Subcommands

#### `nanoom affected`
- [ ] Input: config path, base commit (optional), head commit (optional)
- [ ] Output: AffectedOutput JSON to stdout
- [ ] Exit codes: 0=success, 1=error, 2=no affected workspaces

#### `nanoom run`
- [ ] Required input: `runner` (turbo|yarn|pnpm|nx)
- [ ] Input: task name, filter, shard config
- [ ] Construct runner-specific arguments
- [ ] Execute and stream output
- [ ] Exit with runner's exit code

#### `nanoom install`
- [ ] Auto-detect package manager (pnpm > yarn > npm)
- [ ] Support `root-install` flag
- [ ] Execute install command
- [ ] Exit with install command's exit code

#### `nanoom status`
- [ ] Input: list of job names/ids
- [ ] Read job outputs (from files or GH API)
- [ ] Aggregate pass/fail
- [ ] Exit 0 if all pass, 1 if any fail

### Phase 1.9: Testing

- [ ] Unit tests for all modules (95% coverage target)
- [ ] Integration tests with real git repos
- [ ] Property-based tests for matrix generation
- [ ] Test fixtures: sample monorepos with various configs

---

### Phase 2: GitHub Actions

#### Common Infrastructure
- [ ] Binary download action (reusable)
- [ ] Platform detection (os/arch)
- [ ] Version resolution (latest/specific)

#### `nanoom-affected` Action
- [ ] Inputs: config-file, base-ref, head-ref, token
- [ ] Outputs: matrix (JSON), affected-workspaces (JSON)
- [ ] Composite action calling CLI

#### `nanoom-run` Action
- [ ] Inputs: runner, task, filter, shard-index, shard-total, config-file
- [ ] Outputs: result (pass/fail), duration
- [ ] Composite action calling CLI

#### `nanoom-install` Action
- [ ] Inputs: root-install, config-file, working-directory
- [ ] Composite action calling CLI

#### `nanoom-status` Action
- [ ] Inputs: jobs (comma-separated), token
- [ ] Outputs: overall-status (pass/fail)
- [ ] Composite action calling CLI

---

### Phase 3: Polish & Release

#### Error Handling & UX
- [ ] Structured error messages with suggestions
- [ ] Colored output (when TTY)
- [ ] Progress indicators for long operations
- [ ] Verbose/quiet flags

#### Documentation
- [ ] README with quick start
- [ ] Configuration reference
- [ ] GitHub Actions usage guide
- [ ] Migration guide from turbo/nx
- [ ] API documentation (cargo doc)

#### Release Automation
- [ ] changesets configuration
- [ ] Version sync script (package.json ↔ Cargo.toml)
- [ ] GitHub Release workflow with matrix builds
- [ ] npm publish workflow for @nanoom/cli

#### Examples
- [ ] Basic monorepo example
- [ ] Turbo/nx migration example
- [ ] Sharding example
- [ ] Isolate rule example
