# nanoom

`nanoom` detects affected JavaScript/TypeScript workspaces, expands changes through the local dependency graph, and emits GitHub Actions matrices with isolate and shard metadata.

## Install

```bash
npm install --save-dev @nanoom/cli
```

The npm wrapper uses the exact-version platform package when available. Its fallback downloads the matching GitHub release archive and verifies the published SHA-256 checksum before extraction.

## Configure

```json
{
  "$schema": "./nanoom.schema.json",
  "group": {
    "ci": {
      "tasks": ["lint", "test", "build"],
      "rules": [
        { "name": "@repo/e2e", "shard": [{ "task": "test", "shard": 2 }] },
        { "name": "@repo/app", "isolate": ["test"] }
      ]
    }
  },
  "globalDependencies": ["yarn.lock", "tsconfig.json"],
  "workspace": {
    "include": ["packages/*", "apps/*"],
    "exclude": []
  }
}
```

Generate the schema with `nanoom schema --output nanoom.schema.json`. Unknown fields, duplicate tasks/rules, invalid globs, unknown rule tasks, zero shards, and isolate/shard conflicts are rejected before work starts.

## CLI

```text
nanoom affected --base <revision> [--head <revision>] [--matrix]
nanoom run <group> <task> [--filter <workspace>] [--all]
           [--shard N --total-shards N] [--isolate] [--continue-on-error]
nanoom install [--package-manager auto|pnpm|yarn|npm]
               [--filter <workspace>] [--workspace-install]
nanoom status <job,...> [--results job=status,...] [--format text|json|markdown]
nanoom cache-key --runner <name> --task <task> [--filter <workspace>]
nanoom schema [--output <file>]
nanoom version [--json]
```

`affected` deliberately requires an explicit base revision. The composite Action resolves `github.event.before`, `github.base_ref`, or `github.event.merge_group.base_sha` at the GitHub boundary and passes explicit `--base`/`--head` arguments to the platform-agnostic CLI.

`install` always installs the monorepo root. `--filter` performs a focused dependency-closure install for Yarn Berry or pnpm. `--workspace-install` additionally runs legacy per-workspace installs.

`status` treats `failure` and `cancelled` as failure. Missing or unknown results are errors; `skipped` is accepted only when explicitly reported.

## GitHub Actions

Four public composite Actions live under `.github/actions`: `affected`, `install`, `run`, and `status`. Pin consumers to a release tag:

```yaml
- id: affected
  uses: XionWCFM/nanoom/.github/actions/affected@v0.2.3

- uses: XionWCFM/nanoom/.github/actions/install@v0.2.3
  with:
    matrix: ${{ toJSON(matrix) }}

- uses: XionWCFM/nanoom/.github/actions/run@v0.2.3
  with:
    matrix: ${{ toJSON(matrix) }}
```

The `status` Action evaluates the workflow's `needs` JSON directly and does not download the CLI:

```yaml
- uses: XionWCFM/nanoom/.github/actions/status@v0.2.3
  with:
    needs: ${{ toJSON(needs) }}
    matrixJob: run
    group: ci
```

## Quality and completion

```bash
bash scripts/verify-completion.sh --local
NANOOM_FIXTURE_RUN_ID=<hosted-run-id> bash scripts/verify-completion.sh
```

The exact repeatable exit conditions, evidence lifetime, retry policy, and regression ownership are defined in [ADR-0002](docs/adr/0002-repeatable-completion-gates.md). Coverage is a guardrail; a non-skipped released-binary consumer fixture is the final behavior gate.

## License

MIT
