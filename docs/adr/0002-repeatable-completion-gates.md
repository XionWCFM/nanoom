# ADR-0002: Repeatable completion gates

- Status: Accepted
- Date: 2026-08-24

## Context

A green unit suite is not proof that nanoom's public CLI, composite Actions, release archives, npm wrapper, and consumer workflow agree. A completion rule must be objective, runnable again by another person, and unable to pass when the real fixture matrix was skipped.

## Decision

Work is complete only when every gate below passes for the same source commit and the evidence identifies that commit. A later gate cannot compensate for an earlier failure.

| Gate | Pass condition | Evidence |
| --- | --- | --- |
| G1 deterministic local | `bash scripts/verify-completion.sh --local` passes twice from a clean checkout without changing tracked files | two command logs and commit SHA |
| G2 producer CI | Test on Linux/macOS/Windows, lint/format, coverage, docs build, Action contract, build, and the in-repo fixture jobs all conclude `success` | nanoom workflow URL and commit SHA |
| G3 release contract | tag equals Cargo, lockfile, wrapper, and all platform package versions; five archives, checksums, and Sigstore bundles verify; published binaries report that version; npm wrapper and optional packages use the exact same version | release URL, release workflow URL, npm version |
| G4 real consumer | nanoom-fixtures uses the released Action/binary path; `affected` succeeds; at least four non-skipped `run` matrix jobs succeed; `status` succeeds | fixture run ID accepted by `NANOOM_FIXTURE_RUN_ID=<id> bash scripts/verify-completion.sh` |
| G5 merge protection | the stable producer and consumer aggregate checks are required on `main`; the implementation PR is merged only after G1-G4 | branch-rule output and merged PR URL |

The four fixture entries are specification-derived: shared, app, and two core shards. A fixture run with `run=skipped`, fewer than four run jobs, an unpublished branch binary, or a source-tree-only Action is not completion.

## Failure and retry policy

- A failed gate is fixed at its owning layer and the gate plus all downstream gates are rerun.
- A retry without a code/configuration change does not convert a flaky failure into a pass. Repeated infrastructure failures require a linked incident and a fresh full run; product failures require a regression test.
- Tests may be quarantined only with an owner, issue, expiry date, and a non-blocking replacement signal. Public-contract, checksum, release-version, and fixture tests cannot be quarantined.
- Coverage is a guardrail. It never replaces a specification example or G4.
- Evidence expires when the source commit, Action definition, release tag, lockfile, fixture contract, or branch rule changes.

## Regression ownership

| Defect class | Permanent prevention |
| --- | --- |
| CLI accepts invalid or ambiguous input | boundary validation plus a failing CLI/config test |
| npm/GitHub binary mismatch | exact version synchronization, committed Cargo lockfile, checksum-before-extract smoke test |
| Action resolves moving internal code | scripts loaded from the checked-out Action ref; contract check forbids `@main` self-reference |
| matrix silently does no work | hosted gate requires at least four successful `run` jobs and successful aggregate status |
| docs describe removed contracts | docs build is required; contract changes update reference pages in the same PR |

## Consequences

Releases take longer because producer, distribution, and consumer evidence is sequential. In return, “done” has one repeatable meaning and cannot be declared from coverage, skipped jobs, or unreleased source behavior.
