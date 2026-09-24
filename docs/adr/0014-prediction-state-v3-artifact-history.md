# ADR-0014: PredictionState v3 artifact history

## Status

Accepted for the unreleased `codex/prediction-state-v3` candidate. Hosted GitHub, GHES, and released-consumer evidence remain pending.

## Context

The previous timing history grows with retained samples and requires the planner to read training data. Prediction lookup also needs strict identity boundaries: a pull request must not mix another pull request's history, while a missing PR row may use its base branch. Artifact lookup and update work must not add unbounded latency to the CI critical path.

## Decision

- Store successful run measurements in a temporary v3 `MeasurementArtifact`. Compile them into deterministic `ObservationBatch` aggregates, then atomically apply each batch to bounded `ModelState` using its batch receipt to prevent duplicate weighting.
- Keep the compact `PredictionArtifact` separate from `ModelState`. It contains the per-scope `PredictionTable` and the name/digest of the model artifact. Upload the prediction artifact last as the publish marker.
- The planner reads only the prediction artifact. The history updater follows its model reference and reads model state; neither path downloads raw measurements during planning.
- Scope identity includes repository, workflow path, group, runner, timing environment, and either branch or full pull-request identity. Prediction keys distinguish exact workspace work from workspace-free task fallback. For PRs, lookup order is PR scope then base-branch scope.
- Expired or unusable history falls back to cold scheduling. If no affected assignment choice can change, the action makes no history metadata request.
- Planning history I/O, bounded archive extraction, JSON inspection, and prediction loading share a three-second deadline. Per-file and total received-byte limits are checked before and after transfer; exceeding either limit keeps the previously computed cold plan.
- GitHub artifact Actions remain the default transport. GHES keeps explicit v3 wrappers and shares the same artifact wire format and shell lookup logic. This decision does not add a server dependency or change `scheduler:http`.

## Acceptance

- Same input observations produce the same batch and projection bytes regardless of input ordering.
- Duplicate batch submission does not increase aggregates; expired inputs, invalid rows, and PR/base scope mismatches are rejected or fall back cold.
- The CLI consumes a v3 prediction row in `affected`, reports its source, and preserves cold estimates for unmatched work.
- Action contracts prove push/PR selection, no metadata lookup for no-change work, prediction-only planning downloads, updater model reads, combined byte bounds, and deadline cancellation.
- OpenAPI schema/examples and RFC8785 digest vectors pass their standard validator. Hosted GitHub/GHES and released consumer evidence remain separate completion gates.

## Consequences

Planning transfers a compact, bounded projection instead of model state and measurements. Model corruption can reset learned state without injecting stale estimates as observations. The system may run cold when lookup is slow or invalid. The three-second cap and compact payload are safety limits, not evidence of a workflow speed improvement; total CI time and prediction error still require measurement.
