# ADR-0005: Repository-wide quality floor

- Status: Accepted
- Date: 2026-08-25

## Decision

The following are the minimum acceptable properties of any Nanoom change. A change may exceed this floor, but may not trade one item away to make another check green.

### Product and contract

- GitHub-specific event context is resolved at the Action boundary; the platform-agnostic CLI receives explicit `--base`/`--head` (or equivalent) arguments.
- The final executed command is tested, not only the diagnostic values printed before it is built.
- Public outputs are statically declared and JSON contracts remain backward-compatible or are versioned with specification tests and docs.
- Every Action and CLI result explains inputs, resolved values, reason, exact command, result, final JSON, and the commit/revision used when relevant.
- Public logs are readable without collapsible groups; `::group::` and `::endgroup::` are forbidden.

### Tests and consumers

- Every behavior change has a focused specification test, a regression test, and an applicable error/edge test.
- Coverage is a guardrail, never the acceptance criterion.
- Public Action/CLI changes are validated through the released binary and a real `nanoom-fixtures` consumer; source-tree or local-binary-only evidence is incomplete.
- Affected changes prove direct and transitive paths, no-change behavior, exact matrix entries, every run shard, and aggregate status.
- Install changes prove root development tools, the selected workspace, its transitive closure, and exclusion of unrelated workspaces in a clean install.
- Fixture tooling remains runnable and declares package-local tools. Never move a test tool from `devDependencies` to runtime dependencies merely to hide an installer defect.
- Nanoom integration fixtures use `nodeLinker: node-modules` when `node_modules/.bin` is part of the contract.

### Documentation and distribution

- Public behavior, configuration, output, workflow, or release changes update the relevant reference docs in the same PR.
- Version, Cargo lockfile, wrappers, platform packages, checksums, signatures, and release artifacts stay synchronized.
- Release evidence names the exact source commit and is followed by a released-consumer rerun; a release that skips the meaningful matrix is not sufficient evidence.

### Delivery and evidence

- Work is performed on a branch and merged through a PR. Direct `main` pushes and protection bypasses are prohibited.
- All applicable required checks must finish successfully before merge; pending or skipped checks are not green.
- Post-merge `main` CI is checked, and evidence records the source/merge SHA, commands, URLs/run IDs, and any intentionally skipped gate with owner and expiry.
- A failed product gate requires a code/specification fix or a documented incident; a blind rerun does not convert failure into quality evidence.

## Enforcement

`nanoom-change-review`, `.codex/agents/nanoom-reviewer.md`, `scripts/review-change.sh`, and ADR-0002 consume this floor. When rules conflict, the stricter rule applies and this ADR is the repository baseline.
