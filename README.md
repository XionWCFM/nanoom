# nanoom

**Monorepo task runner with affected project detection.**

`nanoom` figures out which projects in your JS/TS monorepo are actually affected
by a change, then generates GitHub Actions matrices and runs tasks for exactly
those projects — nothing more.

```
detect → matrix → compose → aggregate
```

## Features

- **Affected detection** — maps changed files to workspaces via `git diff`,
  expands to dependents, and short-circuits to "everything" when global files
  (lockfiles etc.) change.
- **GitHub Actions native** — emits `matrix.include` JSON ready for
  `strategy.matrix`, plus shard/isolate metadata.
- **Workspace discovery** — understands npm/yarn/pnpm `package.json`
  workspaces, `pnpm-workspace.yaml`, `turbo.json`, and `nx.json`.
- **Rules** — `ignore` projects, force `isolate`d runners, or split long tasks
  into `shard`s per group.
- **Status aggregation** — collects job results written to `$GITHUB_OUTPUT`
  and fails the final step when anything failed.

## Installation

### npm

```bash
npm install -D @nanoom/cli
```

The right platform binary is installed automatically via
`optionalDependencies`; the wrapper falls back to downloading from GitHub
Releases when needed.

### Direct download

Grab `nanoom-<os>-<arch>.tar.gz` from the
[releases page](https://github.com/XionWCFM/nanoom/releases) — every asset
has a `.sha256` checksum.

## Quick start

1. Add `nanoom.config.json` to the repo root:

```jsonc
{
  "$schema": "./nanoom.schema.json",
  "group": {
    "ci": {
      "tasks": ["lint", "test", "build"],
      "concurrency": 4,
      "rules": [
        { "name": "@heavy/e2e-suite", "isolate": ["test"] },
        { "name": "@slow/*", "ignore": false }
      ]
    },
    "e2e": {
      "tasks": ["test:e2e"],
      "concurrency": 2,
      "rules": [
        { "name": "@app/web", "shard": [{ "task": "test:e2e", "shard": 4 }] }
      ]
    }
  },
  "globalDependencies": ["pnpm-lock.yaml", "tsconfig.base.json"],
  "workspace": {
    "include": ["packages/*", "apps/*"],
    "exclude": ["packages/legacy-*"]
  }
}
```

2. Generate the JSON schema for editor support:

```bash
nanoom schema --output nanoom.schema.json
```

3. Use it in a workflow:

```yaml
jobs:
  affected:
    runs-on: ubuntu-latest
    outputs:
      groups: ${{ steps.detect.outputs.groups }}
      has-change: ${{ steps.detect.outputs.has_change }}
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - id: detect
        uses: ./.github/actions/affected
```

## Configuration reference

| Field | Type | Description |
|-------|------|-------------|
| `$schema` | string | Optional path to the generated JSON schema |
| `group.<name>.tasks` | string[] | Tasks that can run in this group (**required**) |
| `group.<name>.concurrency` | number | Maximum parallel matrix jobs for the group (> 0, **required**); emitted as `max_parallel` |
| `group.<name>.rules[].name` | string | Project name the rule applies to |
| `group.<name>.rules[].ignore` | boolean | Exclude the project from the group entirely |
| `group.<name>.rules[].isolate` | string[] | Tasks that must run on a dedicated runner |
| `group.<name>.rules[].shard[]` | `{ task, shard }` | Split a task into N shards |
| `globalDependencies` | glob[] | Files that affect every project when changed (`**`, `?`, `[]`, `{}` supported) |
| `workspace.include` / `workspace.exclude` | glob[] | Override workspace discovery |

## CLI

```
nanoom affected [--json|--matrix|--format json|text]
nanoom run <group> <task> [--filter <ws>] [--shard N] [--total-shards N]
            [--isolate] [--all] [--continue-on-error]
nanoom install [--package-manager auto|pnpm|yarn|npm] [--root-install]
nanoom status "<job1,job2>" [--format text|json|markdown]
nanoom cache-key --runner <runner> --task <task> [--filter <ws>]
nanoom schema [--output <file>]
nanoom version
```

Event context is read from the standard environment variables:
`PUSH_REF_NAME`, `PULL_REQUEST_BASE_REF`/`PULL_REQUEST_HEAD_REF`,
`MERGE_GROUP_BASE_REF`/`MERGE_GROUP_HEAD_REF`, plus `COMPARISON=tip|merge-base`.
Shallow clones are deepened automatically; fork PRs use the head repository.

## Development

```bash
cargo test --all --all-features      # tests
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all --check
cargo tarpaulin --out Xml --output-dir coverage   # coverage (95% target)
```

Pre-commit hooks:

```bash
pip install pre-commit && pre-commit install
```

Version bumps go through [changesets](https://github.com/changesets/changesets);
`scripts/sync-version.sh` keeps `Cargo.toml` in sync with
`packages/cli/package.json`.

## License

MIT
