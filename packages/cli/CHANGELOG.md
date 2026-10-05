# @nanoom/cli

## 0.9.0

- Predict the next execution from bounded recent batch summaries, preserving daily aggregates and legacy model reads.
- Learn workspace and task costs independently for each measured runner environment; automatically fingerprint CPU, memory, cgroup-v2 limits, image, Node, and package-manager profiles.
- Report environment uncertainty for runner pools and use recent observation proportions so retired environments do not dominate new predictions.
- Preserve environment-specific history through artifact compilation and the shared Cloudflare D1 core, with replay, scope, expiry, and nesting validation.
- Align preparation checkout/workspace-set hashes with the planner by excluding trailing newlines.

## 0.8.0

- Emit project-relative affected workspace paths and activate the package manager after Plan validation in both direct and prepared install paths.
- Clean up the complete isolated checkout when running a nested project, while rejecting the primary repository as a deletion target.
- Preserve nested project working directories through Plan selection, repository-relative sparse checkout, focused install, and run; detect Git roots and normalize relative affected paths.
- Add npm focused install and local run revision inputs; include root tooling, internal dependency closures, and development dependencies in focused installs.
- Treat root execution inputs and declared internal tooling as global changes, and preserve custom configuration paths through Plan selection and file-level checkout.
- Record preparation timing in the default four-step template and preserve cold Plans when warm replanning is interrupted.
- Bound current-attempt measurement downloads on GitHub.com and GHES, preserve distinct artifacts for long job names, and respect npm shrinkwrap in preparation predictions.
- Honor explicit required jobs without requiring unrelated skipped groups, and reject invalid or ambiguous package-manager declarations.
- Keep metadata commands independent of configuration parsing and hash the selected configuration and package-manager inputs in cache keys.
- Repair the default examples and cross-platform validation paths.

## 0.7.7

- Resolve annotated base/head tags to commit identities for affected comparisons.
- Preserve Unicode and embedded separators in Git file-list APIs.

- Preserve completed, failed, pending, and execution details in JSON failures with `--continue-on-error`.
- Scope shard environment variables to the executed child process.
- Reject duplicate job results in the status CLI and Action.
- Detect shallow history correctly in linked Git worktrees.
- Exclude incompatible registry dependencies from sparse checkout's internal workspace closure.

## 0.7.5

- Resolve installed Turbo/Nx command shims from the repository on Windows.
- Share Windows Node-tool executable resolution across run, focused install, and full install.

## 0.7.4

- Validate Windows native checkout paths as absolute paths while preserving workspace isolation.

## 0.7.3

- Keep PR synchronize Plans on the GitHub merge SHA instead of the event branch head.
- Infer workspace paths from pnpm-workspace.yaml or package.json workspaces, preserving explicit overrides and exclusions.

## 0.7.2

- Verify checksums from file contents without filename escaping, including Windows runner paths.
- Remove obsolete successful-push lookup wording from affected summaries.

## 0.7.1

- Allow release artifact listing without a history lookup budget while retaining bounded history reads.

## 0.7.0

- Provide semantic matrix names and exact source SHA/sparse checkout patterns from the affected Plan.
- Select and validate focused-install assignments directly after official checkout; pass the validated assignment to run.
- Resolve push comparisons from event.before/event.after and fetch missing comparison commits for shallow checkouts.
- Require positive planned work to execute before status succeeds, and publish successful measurement history from status.
- Match installed binaries to the downloaded Action source version.

## 0.6.0

### Minor Changes

- Route assignment matrix jobs with group- or tier-specific `runnerLabels` from `nanoom.config.json` and keep historical timing separated by the resolved runner pool.

## 0.5.2

### Patch Changes

- Add a compact aggregate-status input for large matrix workflows whose full `needs` JSON exceeds runner process limits.

## 0.5.1

### Patch Changes

- Restore the executable bit that npm package archives remove from Unix platform binaries before running them.

## 0.5.0

### Minor Changes

- Enable artifact-backed historical scheduling by default and scope history to the last successful run of the same workflow and branch.
- Prefer lower total sparse-checkout path duplication when candidate assignments have the same predicted runtime makespan.
- Default artifact uploads to v4 on GitHub.com, provide explicit GHES v3 Node 24 Action entry points, and expose prediction-source and checkout-cost diagnostics.

## 0.4.2

### Patch Changes

- Ignore generated manifests excluded by `.gitignore` and keep workspace `*` globs to one path segment.

## 0.4.1

### Patch Changes

- Accept the fixture's three positive build, test, and typecheck run jobs in the hosted completion gate.

## 0.4.0

### Minor Changes

- Add manifest-only affected calculation, bounded commit-only history deepening, and cone-mode checkout plans for affected workspaces and their dependency closure.
- Resolve push comparisons from the last successful run of the same workflow and branch.
- Support opt-in cleanup of dynamically isolated sparse-checkout worktrees.
- Publish a `latest` Action tag that resolves and verifies the newest GitHub Release asset.

## 0.3.1

### Patch Changes

- Resolve Turbo and Nx executables from nested relative Action working directories.

## 0.3.0

### Minor Changes

- Add affected-percentage distribution tiers, deterministic timing-aware assignments, successful wall-time samples, artifact history, and the HTTPS coordinator client contract.
- Support multi-workspace focused installs and sequential assignment execution with canonical completed/failed/pending results.
- Keep pnpm/Yarn workspace globs from rediscovering installed packages under nested `node_modules` directories.
- Remove the unobservable `isolate` config, CLI, and matrix contract. Use shards or separate groups for explicit isolation.

## 0.2.8

### Patch Changes

- [#67](https://github.com/XionWCFM/nanoom/pull/67) [`a1430cb`](https://github.com/XionWCFM/nanoom/commit/a1430cb4343bff2a5ec1336380b612e700566e43) Thanks [@XionWCFM](https://github.com/XionWCFM)! - Simplify the GitHub status Action to aggregate all `needs` results directly. The Action now accepts only `needs`, treats `success` and `skipped` as passing, and rejects failed, cancelled, missing, or unknown results.

## 0.2.7

### Patch Changes

- [#65](https://github.com/XionWCFM/nanoom/pull/65) [`2f3e2d9`](https://github.com/XionWCFM/nanoom/commit/2f3e2d94c6e94f41247f8d0989dfcff51958049c) Thanks [@XionWCFM](https://github.com/XionWCFM)! - Stream install and task output in JSON mode, flatten child log groups, and make every Bash composite Action use one structured shell step with compact canonical JSON.

## 0.1.8

### Patch Changes

- [#26](https://github.com/XionWCFM/nanoom/pull/26) [`08efed6`](https://github.com/XionWCFM/nanoom/commit/08efed6c79c180a25fbc60db23c5b5ac295ce2c2) Thanks [@XionWCFM](https://github.com/XionWCFM)! - Make local `node_modules/.bin` tools available to Turbo and Nx runner processes.

## 0.1.7

### Patch Changes

- [#21](https://github.com/XionWCFM/nanoom/pull/21) [`ddf5a66`](https://github.com/XionWCFM/nanoom/commit/ddf5a66f31d4e98c7bfc79fa20a26fbf069356af) Thanks [@XionWCFM](https://github.com/XionWCFM)! - Run the cross-platform release signing command with a portable shell on Windows.

## 0.1.6

### Patch Changes

- [#19](https://github.com/XionWCFM/nanoom/pull/19) [`09604a9`](https://github.com/XionWCFM/nanoom/commit/09604a98beb84db5f9ad5b6162a88b4ba133f177) Thanks [@XionWCFM](https://github.com/XionWCFM)! - Sign and verify GitHub Release archives with keyless Sigstore bundles.

## 0.1.5

### Patch Changes

- Allow reusable setup actions to download releases from an explicitly configured repository.

## 0.1.4

### Patch Changes

- Verify cross-platform release archives without executing foreign-architecture binaries.

## 0.1.3

### Patch Changes

- Write portable relative paths into Unix release checksum files.

## 0.1.2

### Patch Changes

- Use the available macOS runner label for the x64 release build.

## 0.1.1

### Patch Changes

- [#1](https://github.com/XionWCFM/nanoom/pull/1) [`ef351af`](https://github.com/XionWCFM/nanoom/commit/ef351afcf391bbdcb15a59696b09eb5a84a0223c) Thanks [@XionWCFM](https://github.com/XionWCFM)! - Complete the nanoom engine, GitHub Actions integration, and release distribution path.
# 0.2.3

- Validate ambiguous configuration, shard arguments, status inputs, and continued task failures.
- Use npm-compatible semver ranges for workspace dependency edges.
- Verify fallback binary checksums before extraction and synchronize every npm package version.
- Remove moving `@main` internal Action references and add repeatable local/released-fixture completion gates.
