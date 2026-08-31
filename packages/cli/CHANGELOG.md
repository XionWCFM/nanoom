# @nanoom/cli

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
