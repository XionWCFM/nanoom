# ADR-0003: Explainable command and Action results

- Status: Accepted
- Date: 2026-08-25

## Context

A matrix can be correct but still be operationally opaque. A user debugging CI needs the exact revisions, changed paths, selection reason, executed command, final JSON, and exported Action value. Recomputing those facts independently in human logs and machine outputs allows them to drift.

## Decision

Every CLI and composite Action exposes one canonical result and derives its human view from that value.

- JSON CLI mode writes exactly one JSON document to stdout; diagnostics and child output use stderr.
- Affected reports contain requested refs, resolved 40-character base/head commits, comparison mode, changed paths, and one stable direct, transitive, or global reason per selected workspace.
- Install and run report their working directory, exact command, selection scope, reason, and final status.
- Every composite Action prints an always-visible final JSON and exports it as `result`; custom `::group::` folding is forbidden. Affected retains its existing `has_change` and `groups` outputs.
- Actions render the same result in notices and Step Summary tables without hiding the canonical JSON.
- Secrets and authorization values never enter commands, result JSON, or summaries.

The reusable agent rule is installed as the `explainable-cli-actions` skill. The repository contract remains authoritative for Nanoom behavior.

## Alternatives

- Verbose-only diagnostics were rejected because failed default CI runs would still lack evidence.
- Separate text and JSON calculations were rejected because their revisions and reasons could diverge.
- A new logging dependency was rejected; Rust serialization, shell, `jq`, Action outputs, and Step Summary already cover the contract.

## Acceptance and termination

The change is complete only when all conditions hold for one immutable producer commit and one released consumer tag:

1. CLI regression tests parse the canonical JSON, verify full commit hashes, and distinguish direct from transitive selection.
2. Action contract tests require `result`, always-visible final JSON, resolved commits, and reasons on every public Action, and reject custom log groups.
3. Local completion gates pass twice from a clean tree without tracked-file drift.
4. Hosted producer CI passes every required check for the producer commit.
5. The released tag points to that commit and its published binary reports the same version.
6. A hosted nanoom-fixtures run using that tag proves exactly four shared-change entries, all focused-install invariants, every real task, and aggregate status.
7. Both repositories merge only after the required hosted checks pass; evidence becomes stale after any producer, Action, fixture, lockfile, or branch-rule change.

Any missing or skipped positive fixture entry, absent reason/hash/output, install invariant failure, unpublished path, or changed evidence commit resets the affected gate and every downstream gate.

## Consequences

Default logs are longer, but failures are reproducible without rerunning in a special debug mode. Logs are intentionally flat so the user does not need to open a dropdown to understand a result. Public JSON gains additive diagnostic fields and Actions gain an additive `result` output; existing matrix consumers remain compatible.
