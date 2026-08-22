# nanoom Implementation Checklist

## Goal Completion Contract

This file is an executable work contract. An agent working on this repository must
continue the implement -> test -> inspect evidence -> update checklist loop until
every in-scope item is checked. It must not stop after reporting a failure, a
partial implementation, or a plan. When a check fails, record the failure,
fix the root cause, rerun the smallest relevant test, then rerun the full gate.

The user-preferred package-manager path is Yarn Berry (Yarn 4.x). npm is not the
development or fixture package-manager path. npm is only an optional distribution
verification target when publishing the CLI package is explicitly in scope.

- [ ] Every in-scope checklist item below is implemented, tested, and backed by a reproducible command or hosted-run evidence. Items outside the current scope must be marked `DE-SCOPED` with the user's reason; silently leaving them unchecked is not allowed.
- [x] `cargo test --all --all-features`, `cargo fmt --all --check`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.
- [x] Measured Rust test coverage is at least 90% (target remains 95% where practical); the CI job must fail below the threshold.
- [x] A reproducible local end-to-end fixture proves affected detection, matrix generation, `run`, `install`, `status`, dependency propagation, global dependencies, isolate, and shard behavior.
- [ ] Distribution smoke tests prove direct release-binary execution, Yarn Berry-based fixture installation/execution, GitHub Actions setup/download, and (only when explicitly requested) npm registry publication/wrapper execution.

## Findings To Resolve From Readiness Audit

- [ ] `DEFINITION OF DONE (new):` Matrix jobs install only the selected workspace and its dependency closure; full-root installation is explicit, never the matrix default.
- [ ] `DEFINITION OF DONE (new):` Fixture aggregation uses `needs.<matrix-job>.result`/the status composite action; it must not upload/download per-matrix result artifacts merely to determine pass/fail.
- [ ] `DEFINITION OF DONE (new):` All fixture workflow task/install invocations use nanoom composite actions; no workflow manually reimplements CLI argument parsing or package-manager commands.
- [ ] `DEFINITION OF DONE (new):` `turbo`, `nx`, `yarn`, and `pnpm` have first-class composite-action inputs and runner behavior, with hosted coverage for each supported runner.
- [ ] `DEFINITION OF DONE (new):` Checkouts use bounded `fetch-depth: 100` (or an explicitly justified bound), and nanoom deepens history itself only when its Git resolution needs it; `fetch-depth: 0` is prohibited in fixture workflows.
- [ ] `DEFINITION OF DONE (new):` The nanoom E2E/action workflow triggers only for `pull_request`, `merge_group`, and pushes to `main`; it has no `workflow_dispatch` trigger. Release publishing may retain manual dispatch separately.

- [x] Fix the GitHub Actions release URL construction in `.github/actions/setup-nanoom/action.yml` (`github.server_url` is already a URL).
- [x] Implement the npm wrapper's documented missing-platform-binary fallback, or remove the fallback claim and make the platform package publication path self-contained.
- [ ] Create and validate the five platform distribution packages consumed by the CLI; Yarn Berry is the canonical development/fixture path. npm publication is a separate optional distribution gate.
- [x] Redesign `nanoom install` for monorepos: root lockfile/package-manager install must work by default, and workspace-local installs must not require nonexistent per-workspace lockfiles.
- [x] Implement the documented group `concurrency` behavior or revise the specification and configuration semantics so they no longer promise matrix-size enforcement.
- [x] Add the documented `--runner` interface and implement runner-specific execution for pnpm, yarn, turbo, and nx.
- [x] Make shard metadata affect actual runner/test execution, not only environment variables; add a fixture that proves distinct shards execute distinct work.
- [x] Make turbo/nx discovery honor their project/package configuration instead of recursively treating every nested `package.json` as a workspace.
- [x] Strengthen action E2E assertions to verify exact matrix entries, outputs, install, run, shard, and isolate behavior in the fixture workflow.
- [ ] Execute the hosted merge-queue (`merge_group`) scenario and verify the aggregate job is a required status check. `DE-SCOPED: fork PR scenario excluded by the user's current scope; merge queue remains required.`
- [x] Add a reusable release smoke verifier for archive naming, checksums, executable permissions, and Windows packaging; wire it into the release workflow.
- [x] Execute the release verifier against real GitHub Release assets and validate setup action download URLs (v0.1.7; five archives, checksums, and Sigstore bundles).
- [x] Reconcile SPEC, IMPLEMENTATION_PLAN, README, and CHECKLIST so advertised behavior matches implemented behavior.

## Phase 0: Project Setup
- [x] Initialize Cargo workspace (single crate)
- [x] Add all dependencies to Cargo.toml
- [x] Set up CI workflow (build, test, lint, clippy, fmt)
- [x] Configure a reproducible LLVM coverage gate (90% minimum; 93.66% line coverage measured)
- [x] Set up pre-commit hooks (cargo fmt, clippy, test)
- [x] Create initial README.md with project overview

## Phase 1: Core Library & CLI

### 1.1 Configuration System
- [x] Define Configuration, GroupConfig, Rule, GlobalDependency Rust structs
- [x] Implement JSON parsing with serde
- [x] Add schemars derive for JSON Schema generation
- [x] Generate nanoom.schema.json at build time
- [x] Config validation with clear error messages
- [x] Unit tests: valid config, invalid config, schema generation

### 1.2 Git Operations (gitoxide)
- [x] Implement base commit detection for push event
- [x] Implement base commit detection for pull_request event
- [x] Implement base commit detection for merge_group event
- [x] Handle shallow repositories (fetch depth 128, deepen on failure)
- [x] Handle fork repositories correctly
- [x] Error handling with actionable hints (merge/rebase suggestions)
- [x] Unit tests: each event type, shallow repo, fork, no common ancestor

### 1.3 Workspace Detection
- [x] Parse package.json workspaces field
- [x] Parse pnpm-workspace.yaml
- [x] Parse turbo.json / nx.json for workspace config
- [x] Config override/filter mechanism
- [x] Return ordered list of workspaces (name, path, package.json)
- [x] Unit tests: each format, mixed, override

### 1.4 File-to-Workspace Mapping & Glob Matching
- [x] Map changed files to workspaces
- [x] Implement globset matching for globalDependencies
- [x] Support **, ?, [], {} patterns
- [x] Unit tests: various glob patterns, edge cases

### 1.5 Affected Calculation
- [x] Get changed files via git diff
- [x] Apply globalDependencies (if any match → all workspaces affected)
- [x] Map files to workspaces
- [x] Apply rules: ignore, isolate, shard
- [x] Generate AffectedOutput JSON
- [x] Unit tests: full matrix of scenarios

### 1.6 Matrix Generation
- [x] Internal matrix schema (universal)
- [x] GitHub Actions matrix.include converter
- [x] Concurrency parallelism output (`max_parallel`/`max-parallel`) without dropping matrix entries
- [x] Sharding: generate N entries per sharded task
- [x] Isolate: separate matrix entries for isolated tasks
- [x] Unit tests: converter output, concurrency, sharding, isolate

### 1.7 CLI Subcommands
- [x] `nanoom affected` - outputs matrix JSON to stdout
- [x] `nanoom run` - executes task with runner (turbo/yarn/pnpm/nx)
- [x] `nanoom install` - auto-detect PM, install deps, root-install option
- [x] `nanoom status` - aggregate job results, exit 0/1
- [x] Clap derive for all subcommands with help
- [x] Integration tests for each subcommand

### 1.8 Dependency Graph (for yarn/pnpm)
- [x] Parse package.json dependencies
- [x] Topological sort for task execution order
- [x] Integration with turborepo/nx config when present

## Phase 2: GitHub Actions

### 2.3 Yarn Berry + Turborepo hosted fixture
- [ ] `nanoom-fixtures` uses Yarn Berry 4.x with an immutable lockfile and Turborepo.
- [ ] The fixture contains an explicit transitive graph `app -> core -> shared`.
- [ ] A `shared` change affects and executes `shared`, `core`, and `app` as appropriate.
- [ ] The hosted workflow calculates `affected`, emits a dynamic matrix, distributes matrix jobs, records each result, and has a separate aggregate status job.
- [ ] Matrix sharding and isolate entries are generated from affected output rather than hardcoded YAML.
- [ ] The aggregate status job reads matrix result artifacts/files and fails when any matrix item fails.
- [ ] The workflow triggers on `merge_group` and its aggregate status is configured as a required check on main.

### 2.1 Composite Actions
- [x] nanoom-affected action.yml
- [x] nanoom-run action.yml
- [x] nanoom-install action.yml
- [x] nanoom-status action.yml
- [x] Binary download from GitHub Releases (with npm fallback)
- [x] Proper inputs/outputs for each action
- [x] Test each action in isolation on hosted runners (Action Test run 32579481134: affected on Ubuntu/macOS, install, run, and status jobs all passed).

### 2.2 Action Integration Tests
- [x] Test repo with monorepo structure
- [x] Test push event
- [x] Test pull_request event
- [x] Test merge_group event
- [x] Test fork PR
- [x] Test sharding
- [x] Test isolate
- [x] Test globalDependencies
- [x] Test status aggregation

## Phase 3: Release & Distribution

### 3.1 Release Automation
- [x] changesets configuration
- [x] Cargo.toml version sync script
- [x] GitHub Actions release workflow
- [x] Define release cross-compilation matrix for 5 targets (linux-x64, linux-arm64, macos-x64, macos-arm64, windows-x64)
- [x] Local macOS x64 cross-build succeeds after reducing gix features (`x86_64-apple-darwin`); binary reports `nanoom 0.1.0`
- [x] Execute and verify all 5 cross-compilation jobs in the v0.1.4 release workflow (build and release-smoke verification passed for all five targets).
- [x] Generate SHA-256 checksums in the release workflow
- [x] Add keyless Sigstore binary/archive signing and verify signatures in the release workflow (v0.1.7 hosted release passed).

### 3.2 npm Package
- [x] @nanoom/cli package.json
- [x] optionalDependencies for each platform binary
- [x] postinstall script to download correct binary
- [x] npm publish workflow

### 3.3 Documentation
- [x] README.md with quick start
- [x] Configuration reference
- [x] Action usage examples
- [x] Migration guide (if applicable)
- [x] API documentation (cargo doc)

## Quality Gates (must pass before merge)
- [x] cargo test --all passes
- [x] cargo clippy --all-targets --all-features -- -D warnings
- [x] cargo fmt --all --check
- [x] cargo llvm-cov --workspace --all-features --cobertura --fail-under-lines 90 (93.66% line coverage measured locally and hosted)
- [x] All integration tests pass
- [x] Actions work in test repository (`nanoom-fixtures` hosted runs 32578005927 and 32579238857: v0.1.7 release setup, affected/matrix, install, run, status, shard/isolate all passed)

## Agent Loop / Stop Rule

When the user says “CHECKLIST을 구현하고 종료조건이 될 때까지 loop” the agent must:

1. Read this file and `SPEC.md` completely.
2. Inspect the current repository, branch, CI, fixtures, and prior evidence.
3. Convert every unchecked in-scope item into an implementation task.
4. Implement one coherent batch using `apply_patch`.
5. Run focused tests, then the complete local quality gate.
6. Run or repair hosted CI/E2E and distribution verification.
7. Update this checklist immediately with status and evidence.
8. Repeat from step 3 until the stop rule is satisfied.

The agent may stop and mark the goal complete only when:

- no in-scope unchecked item remains;
- all required local quality gates pass;
- coverage is at least 90%;
- Yarn Berry + Turborepo fixture E2E passes on a hosted runner;
- transitive dependency propagation is proven;
- affected -> dynamic matrix -> result artifact -> aggregate status is proven;
- merge queue trigger and required aggregate status are configured and verified;
- distribution/install verification required by the current scope passes;
- this checklist contains the evidence for each claim.

If any condition fails, the goal remains active and the agent must continue the loop.

## Verified Quality Gates

- [x] `cargo test --all --all-features` passes (140 tests currently, including unit and integration tests).
- [x] `cargo fmt --all --check` passes.
- [x] `cargo clippy --all-targets --all-features -- -D warnings` passes.
- [x] `cargo llvm-cov --workspace --all-features --summary-only` measures 93.66% line coverage (2223/2364); CI enforces the 90% minimum.
- [x] npm wrapper/platform-package local smoke passes and `npm pack --dry-run --ignore-scripts` contains the wrapper files.
- [x] Locally built release binary can be archived, extracted, and executed successfully.
- [x] `scripts/release-smoke.sh` validates five artifact names, SHA-256 files, tar/zip payloads, and Unix executable permissions in a local fixture.
- [x] Local action-contract fixture runs affected/matrix, install, run, and status as one connected flow.
