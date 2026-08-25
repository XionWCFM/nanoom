# ADR-0006: Repository-wide line coverage floor

- Status: Accepted
- Date: 2026-08-25

## Decision

Nanoom enforces a minimum of 96% Rust line coverage across the workspace.
The same `cargo llvm-cov --workspace --all-features --fail-under-lines 96`
check runs in CI and in `scripts/verify-completion.sh`.

Coverage is a regression guardrail, not a substitute for behavior tests. The
repository must also keep focused specification tests, error and edge-case
tests, CLI integration tests, and the existing fixture-backed CI checks.

## Consequences

New or changed behavior must add tests that exercise the public path and its
relevant failure modes. A change that drops total line coverage below 96%,
breaks the fixture path, or fails another required quality check cannot merge.
