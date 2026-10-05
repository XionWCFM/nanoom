# ADR-0014: PredictionState v3 artifact history

## Status

Accepted for the unreleased `codex/prediction-state-v3` candidate. Hosted GitHub, GHES, and released-consumer evidence remain pending.

## Context

The previous timing history grows with retained samples and requires the planner to read training data. Prediction lookup also needs strict identity boundaries: a pull request must not mix another pull request's history, while a missing PR row may use its base branch. Artifact lookup and update work must not add unbounded latency to the CI critical path.

## Decision

- Store successful run measurements in a temporary v3 `MeasurementArtifact`. Compile them into deterministic `ObservationBatch` aggregates, then atomically apply each batch to bounded `ModelState` using its batch receipt to prevent duplicate weighting.
- Keep the compact `PredictionArtifact` separate from `ModelState`. It contains the per-scope `PredictionTable` and the name/digest of the model artifact. Upload the prediction artifact last as the publish marker.
- The planner reads only the prediction artifact. The history updater follows its model reference and reads model state; neither path downloads raw measurements during planning.
- New ModelState entries retain up to three recent batch/day mean summaries, ordered by observation timestamp/run/attempt. Estimates use their median within seven days of the latest summary; sparse histories use the latest available summary. Daily buckets remain for counts, expiry, and legacy weighted-mean entries. Replay cannot add a second summary and merge ordering cannot change retained summaries. The public prediction row and observation batch remain unchanged; older strict ModelState readers require the new release.
- Scope identity includes repository, workflow path, group, runner, timing environment, and either branch or full pull-request identity. Prediction keys distinguish exact workspace work from workspace-free task fallback. For PRs, lookup order is PR scope then base-branch scope.
- Expired or unusable history falls back to cold scheduling. If no affected assignment choice can change, the action makes no history metadata request.
- Planning history I/O, bounded archive extraction, JSON inspection, and prediction loading share a three-second deadline. Per-file and total received-byte limits are checked before and after transfer; exceeding either limit keeps the previously computed cold plan.
- GitHub artifact Actions remain the default transport. GHES keeps explicit v3 wrappers and shares the same artifact wire format and shell lookup logic. This decision does not add a server dependency or change `scheduler:http`.
- The v3 measurement artifact may add `preparationObservations`; old task-only v3 artifacts remain valid. Preparation keys distinguish package manager/version, install mode, lockfile digest, and exact checkout/workspace-set digests, plus a fallback without the latter two digests.
- Checkout/workspace-set digests use deduplicated, sorted JSON arrays encoded as UTF-8 without a trailing newline. Measurement producers and prediction consumers must hash identical bytes; Action regression checks use an independent JSON encoder.
- Preparation duration spans the first prepare Action step to the first planned task subprocess start. `affected` reads the package-manager version declared in the root manifest and performs no package-manager invocation. A missing/mismatched manager declaration or lockfile makes preparation prediction unknown.
- Warm task and preparation estimates compare LPT layouts for powers of two, configured tier concurrency values, and the tier cap. The objective is preparation-plus-task makespan, total runner time, checkout path count, assignment count, then stable order. Cold task history or any unknown candidate preparation estimate retains the tier cap.
- Successful measurements may carry a canonical runner profile and fingerprint. Compile and update retain independent per-profile summaries inside the same authorized logical scope; nested environment collections are rejected. Shared core semantics apply to both artifact history and the D1 Worker.
- New keys predict from the median of up to three recent batch means within seven days of the latest summary; legacy entries retain the previous daily weighted mean until an actual batch is learned. Unknown runner pools combine profile estimates using recent observation proportions and expose per-task environment ranges. Older profiles remain available for exact fingerprint lookup until normal expiry.
- Assignment and compact output include optional preparation estimate/source/sample count and `automatic` or `cold-cap` diagnostics. These fields do not change the Plan v1 version.

## Acceptance

- Same input observations produce the same batch and projection bytes regardless of input ordering.
- Duplicate batch submission does not increase aggregates; expired inputs, invalid rows, and PR/base scope mismatches are rejected or fall back cold.
- The CLI consumes a v3 prediction row in `affected`, reports its source, and preserves cold estimates for unmatched work.
- Preparation-only measurement compatibility, exact/fallback JCS key vectors, aggregate compilation, reversed-clock omission, cold-cap, and deterministic warm concurrency are covered locally.
- Action contracts prove push/PR selection, no metadata lookup for no-change work, prediction-only planning downloads, updater model reads, combined byte bounds, and deadline cancellation.
- OpenAPI schema/examples and RFC8785 digest vectors pass their standard validator. Hosted GitHub/GHES and released consumer evidence remain separate completion gates.

## Consequences

Planning transfers a compact, bounded projection instead of model state and measurements. Model corruption can reset learned state without injecting stale estimates as observations. The system may run cold when lookup is slow or invalid. The three-second cap and compact payload are safety limits, not evidence of a workflow speed improvement; total CI time, real-trace prediction error versus the prior median, and actual artifact sizes still require hosted measurement.
