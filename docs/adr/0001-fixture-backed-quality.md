# ADR-0001: Fixture-backed interface and regression quality

- Status: Accepted
- Date: 2026-08-24

## Context

nanoom is consumed through public CLI and GitHub Action interfaces. The nanoom-fixtures repository is the representative consumer of that contract. A high unit-test coverage percentage can still pass while the public happy path fails when the action, released binary, Git refs, matrix, and aggregate status are not exercised together.

## Decision

Treat the real fixture workflow and the intended public interface as release-level acceptance criteria. Design and implementation decisions must preserve the consumer-facing contract unless an explicit breaking change is approved. Tests must verify behavior and regression risk, not only coverage: specification examples, failure/edge cases, and at least one fixture-backed end-to-end execution are required.

For nanoom GitHub Actions, the minimum observable contract is:

1. `affected` resolves the event comparison through standard GitHub context and produces the expected matrix.
2. Every generated matrix entry completes its install/run path.
3. `status` reports success when the affected calculation and all matrix entries succeed.

Coverage thresholds remain guardrails. They cannot replace a passing fixture or justify marking a broken public happy path complete.

## Alternatives

- **Coverage-only acceptance:** rejected because it can miss action/binary/release/fixture integration failures.
- **Mock-only action tests:** rejected because mocks do not validate the released binary, Git checkout state, package-manager install, or matrix aggregation.
- **Consumer-specific workarounds:** rejected when the defect is in nanoom's shared public contract.

## Acceptance criteria

- The relevant fixture workflow passes on a hosted runner using the released/public action and binary path.
- The fixture asserts the expected affected matrix, executes all matrix entries, and validates aggregate status.
- Focused tests cover the intended interface and the regression that originally failed.
- Full unit/integration tests, formatting/lint, and coverage guardrails pass.
- The PR links this ADR and reports the fixture run URL and outcome.

## Consequences

Changes touching a public interface may require a fixture update, a release artifact, and a hosted rerun before merge. This costs more than a coverage-only check, but prevents false-green releases and keeps the real consumer contract executable.
