# nanoom Specification

## Overview

Large monorepos take too long to run all tasks on a single runner. We detect changed workspaces and dynamically compose CI using GitHub Actions matrix, enabling few runners for small changes and many runners for large changes.

**Mental Model:**
1. Detect changed workspaces
2. Generate matrix data from results
3. Dynamically compose CI via matrix
4. Aggregate results: fail if any fails, pass if all succeed

**Distribution:** Rust CLI binary, dual distribution via GitHub Releases + npm package bundling. Primary access via GitHub Actions.

## Technical Stack & Principles

- **Engine:** Rust, binary committed to repo
- **Testing:** 96%+ line coverage, E2E tests with in-memory Git (gitoxide/libgit2)
- **Performance:** Top priority
- **Configuration:** Declarative, single `nanoom.config.json` (no JS/TS config — no Node runtime needed)
- **Schema:** JSON Schema provided for validation
- **License:** MIT, open source
- **CI:** Build, test, lint; defend against external contributor attacks via label-based OSS CI best practices
- **Error Messages:** Actionable, user-friendly
- **CLI:** Unix standard compliance (exit codes, stdout/stderr, signals)
- **Pre-commit:** Test, build, lint mandatory — no bypass
- **Git:** Merge queue aware, concurrent contributors, large commit history
- **Package Managers:** Yarn Berry (Yarn 4.x) is the canonical development and fixture package manager; pnpm is first-class; npm is supported only as an optional distribution/consumer path.
- **Workspaces:** yarn workspace, pnpm workspace, nx, turborepo first-class

---

## Configuration

Single file: `nanoom.config.json` at repository root. JSON Schema at `nanoom.schema.json`.

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
  "globalDependencies": ["yarn.lock", "pnpm-lock.yaml", "package-lock.json"]
}
```

### TypeScript Interface

```ts
interface Configuration {
  group: {
    [groupName: string]: {
      tasks: string[];           // package.json script names
      concurrency: number;       // maximum parallel matrix jobs; entries are not dropped
      rules?: Rule[];
    };
  };
  globalDependencies: string[];  // glob patterns; change triggers full rebuild
}

interface Rule {
  name: string;                  // package.json name field
  ignore?: boolean;              // skip even if changed
  isolate?: string[];            // tasks that get dedicated runner
  shard?: ShardRule[];           // split task across N runners
}

interface ShardRule {
  task: string;
  shard: number;                 // number of shards
}
```

### Key Concepts

- **Group:** Separate CI environments (e.g., `ci` on ubuntu-latest, `e2e` on custom Docker container)
- **Concurrency:** Maximum parallel matrix jobs per group — independent of GitHub Actions `concurrency`
- **Isolate:** Heavy tasks get dedicated runner (no sharing)
- **Shard:** Split arbitrary CLI command into N parallel executions (generic sharding)
- **Glob Support:** Full globset/globwalk — `**`, `?`, `[]`, `{}` all supported

---

## Binary Distribution

- **GitHub Releases:** Platform-specific binaries (linux-x64, linux-arm64, macos-x64, macos-arm64, windows-x64)
- **Primary fixture/distribution validation:** Yarn Berry 4.x installs and executes the CLI/fixtures; npm registry publication and wrapper installation are optional release gates, not the development workflow.
- **npm Package (optional release target):** `@nanoom/cli` bundles all platforms, postinstall downloads correct binary
- **GitHub Actions:** `actions/setup-nanoom` downloads from Releases; `nanoom-run`, `nanoom-affected`, `nanoom-install`, `nanoom-status` actions wrap CLI

---

## CLI Interface

Single binary `nanoom` with subcommands:

```
nanoom affected    # Detect changed workspaces, output matrix JSON
nanoom run         # Execute task (turbo/yarn/pnpm/nx) — unified runner
nanoom install     # Install dependencies (auto-detect pm)
nanoom status      # Aggregate job results (CI + local)
nanoom version     # Print version
nanoom help        # Usage
```

### `nanoom run`

**Required input:** `runner: "turbo" | "yarn" | "pnpm" | "nx"`

```bash
nanoom run --runner turbo --task test --filter @myorg/pkg
nanoom run --runner pnpm --task build --filter "..."
```

---

## Workspace Detection

**Auto-detection (priority order):**
1. `pnpm-workspace.yaml` → pnpm workspaces
2. `turbo.json` → turborepo pipelines
3. `nx.json` / `project.json` → nx projects
4. `package.json` `workspaces` field → yarn/npm workspaces

**Override/Filter via config:**
```json
{
  "workspace": {
    "include": ["packages/*", "apps/*"],
    "exclude": ["packages/deprecated-*"]
  }
}
```

---

## Dependency Graph & Task Execution

| Tool | Graph Source |
|------|--------------|
| turborepo | `turbo.json` pipeline |
| nx | `nx.json` / `project.json` |
| yarn/pnpm | Topological sort from `package.json` `dependencies`/`devDependencies` |

**Execution:** `nanoom run` invokes the appropriate runner with computed filter/shard/isolate.

---

## Affected Output (Matrix Generation)

**Generated entirely in Rust CLI** — outputs JSON for GitHub Actions `matrix.include`.

```ts
interface AffectedOutput {
  group: {
    [groupName: string]: {
      label: string;
      workspaces: {
        name: string;        // package.json name
        path: string;        // relative path from root
        task: string;        // script name
        shard?: number;      // shard index (1-based) if sharded
        isolate?: boolean;   // dedicated runner
      }[];
    }[];
  };
  hasChange: boolean;
}
```

**GitHub Actions Usage:**

```yaml
jobs:
  affected:
    runs-on: ubuntu-latest
    outputs:
      ci: ${{ steps.affected.outputs.group-ci }}
      e2e: ${{ steps.affected.outputs.group-e2e }}
      has-change: ${{ steps.affected.outputs.has-change }}
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 1 }
      - id: affected
        uses: nanoom/action-affected@v1

  ci:
    needs: affected
    if: needs.affected.outputs.has-change == 'true'
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix: ${{ fromJson(needs.affected.outputs.ci) }}
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 1 }
      - uses: actions/setup-node@v4
      - uses: nanoom/action-install@v1
        with:
          package-manager: ${{ matrix.packageManager }}
          root-install: true
      - uses: nanoom/action-run@v1
        with:
          runner: ${{ matrix.runner }}
          task: ${{ matrix.task }}
          filter: ${{ matrix.name }}
          shard: ${{ matrix.shard }}
          isolate: ${{ matrix.isolate }}

  status:
    needs: [affected, ci, e2e]
    if: always()
    runs-on: ubuntu-latest
    steps:
      - uses: nanoom/action-status@v1
        with:
          jobs: affected,ci,e2e
```

**Programmatic Access:** Users can consume `nanoom affected --json` output and transform before feeding to matrix.

---

## Base Commit Resolution (Git Logic)

Rust implementation of the shell logic, tested against real Git repos (gitoxide). Handles:

- **Events:** push, pull_request, merge_group
- **Comparison modes:** `merge-base` (default) vs `tip`
- **Shallow clones:** Auto-deepen on merge-base failure
- **Fork repos:** `origin` vs `upstream` resolution
- **Default branch detection:** `main` > `master` via `git ls-remote`
- **Force push / rebase:** Deepening fetches both base and head refs

**Inputs (env):**
- `MERGE_GROUP_BASE_REF`, `PULL_REQUEST_BASE_REF`, `PUSH_REF_NAME`
- `COMPARISON` = `merge-base` | `tip`
- `GITHUB_OUTPUT` for Actions output

**Output:** Base commit SHA, written to `GITHUB_OUTPUT` and stdout.

---

## Changed Package Calculation

1. Resolve base commit (above)
2. `git diff --name-only <base>...HEAD` → changed files
3. Map files → workspaces (via workspace detection)
4. Apply `globalDependencies` globs: match → all workspaces changed
5. Apply dependency graph: transitive dependents of changed workspaces
6. Apply config rules: `ignore`, `isolate`, `shard`
7. Group by `group.tasks` membership → matrix per group

---

## Concurrency (Matrix Parallelism)

`concurrency` in config is the maximum number of matrix jobs that should run in
parallel for the group. Affected entries are never dropped to enforce this
value. `nanoom affected --matrix <group>` and the `nanoom-affected` action expose
the value as `max_parallel`/`max-parallel`, so a workflow can apply it as
`strategy.max-parallel` while preserving every affected workspace and shard.

**Distinct from** GitHub Actions `concurrency:` (cancels in-progress runs).

---

## Error Reporting

All of the following provided:

- **Real-time streaming:** Stdout/stderr passed through unbuffered
- **Summary:** End-of-run Markdown summary (GitHub Actions job summary API)
- **Standard Reports:** JUnit XML, JSON test results
- **Artifacts:** Upload all reports + logs as GitHub Actions artifacts
- **Unix Compliance:** Exit code 0=success, non-zero=failure; SIGTERM handling

---

## Caching

**Key generation helper only** — no built-in cache backend.

```bash
nanoom cache-key --runner turbo --task test --filter @myorg/pkg
# Outputs: turbo-test-<hash>-<os>-<arch>
```

Users integrate with `actions/cache` or their own cache layer.

---

## Version Management

**changesets** only. Single source of truth for version bumps and changelog.

- `changeset add` → `.changeset/*.md`
- `changeset version` → bumps `package.json` versions, updates `CHANGELOG.md`
- `changeset publish` → publishes to npm
- Rust `Cargo.toml` version synced via `cargo-set-version` in release workflow

---

## Install Action (`nanoom-action-install`)

```yaml
- uses: nanoom/action-install@v1
  with:
    package-manager: auto | pnpm | yarn | npm
    root-install: true | false
    # nx/turbo are NOT package managers — they use the detected pm for install
```

**Auto-detection priority:** `pnpm-lock.yaml` → `yarn.lock` → `package-lock.json` → `npm`

---

## Status Job / Action

**CLI:** `nanoom status ci,e2e --format json|markdown`

**Action:** `nanoom/action-status@v1`

Both aggregate results from previous jobs (via `needs.<job>.result` or CLI args) and:
- Exit 0 if all passed
- Exit 1 if any failed
- Post summary to PR / job summary

---

## Sharding (Generic)

Any CLI command can be sharded:

```json
{
  "shard": [{ "task": "test:e2e", "shard": 4 }]
}
```

CLI splits test files (or arbitrary args) into N shards, each runner gets `--shard=X --total-shards=N`.

**Runner implementation:** `nanoom run --shard 1 --total-shards 4 --runner pnpm --task test:e2e`

---

## Output Format

- **GitHub Actions First-Class:** `::set-output`, `::notice`, `::error`, job summary API
- **Universal JSON:** All CLI commands support `--json` for CI-agnostic consumption
- **Structured Logs:** JSON lines on stderr for log aggregation

---

## E2E Testing Strategy

- **In-Memory Git:** gitoxide/libgit2 — no filesystem, no network
- **Test Scenarios:** fork PRs, merge queues, force pushes, shallow clones, deep histories, concurrent edits
- **Fixtures:** Programmatic repo construction (commits, branches, tags, worktrees)
- **Coverage:** All Git event types, all comparison modes, all workspace layouts

---

## File Structure

```
nanoom/
├── cli/                    # Rust CLI (cargo workspace)
│   ├── src/
│   │   ├── main.rs
│   │   ├── commands/
│   │   │   ├── affected.rs
│   │   │   ├── run.rs
│   │   │   ├── install.rs
│   │   │   ├── status.rs
│   │   │   └── cache_key.rs
│   │   ├── git/
│   │   │   ├── base_commit.rs
│   │   │   └── diff.rs
│   │   ├── workspace/
│   │   │   ├── detect.rs
│   │   │   ├── graph.rs
│   │   │   └── config.rs
│   │   ├── matrix/
│   │   │   └── generator.rs
│   │   └── output/
│   │       └── formatter.rs
│   └── tests/
│       └── e2e/           # in-memory git tests
├── action-affected/        # GitHub Action wrapper
├── action-run/
├── action-install/
├── action-status/
├── action-setup-nanoom/
├── nanoom.config.json
├── nanoom.schema.json
├── Cargo.toml
├── package.json            # @nanoom/cli npm package
└── CHANGELOG.md
```

---

## Release Workflow

1. `changeset version` → version bumps + changelog
2. `cargo build --release` → binaries
3. `cargo publish` (crates.io) — optional
4. `npm publish` (@nanoom/cli)
5. GitHub Release with all platform binaries
6. `actions/setup-nanoom` updated to new version
