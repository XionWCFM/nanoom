#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

for action in affected install run status history; do
  test -f ".github/actions/$action/action.yml"
  test -f ".github/actions/$action/run.sh"
  grep -q '^  result:' ".github/actions/$action/action.yml"
  grep -q 'Final JSON' ".github/actions/$action/action.yml" ".github/actions/$action/run.sh"
  ! grep -q '::group::\|::endgroup::' ".github/actions/$action/action.yml" ".github/actions/$action/run.sh"
done
for action in affected-ghes prepare-ghes run-ghes history-ghes; do
  test -f ".github/actions/$action/action.yml"
  ruby -e 'require "yaml"; YAML.load_file(ARGV.fetch(0))' ".github/actions/$action/action.yml"
  ruby -e 'require "yaml"; inputs = YAML.load_file(ARGV.fetch(0)).fetch("inputs"); abort "input description missing" unless inputs.values.all? { |input| input["description"].is_a?(String) && !input["description"].empty? }' ".github/actions/$action/action.yml"
  grep -q 'TOKEN:.*github.token' ".github/actions/$action/action.yml"
done
for action in prepare prepare-ghes; do
  ruby -e 'require "yaml"; steps = YAML.load_file(ARGV.fetch(0)).dig("runs", "steps"); abort "preparation clock must run first" unless steps.first["id"] == "started"' ".github/actions/$action/action.yml"
  grep -q 'prepared-at-ms:' ".github/actions/$action/action.yml"
done
test -f .github/actions/_setup/setup.sh
for action in affected install run history _setup; do
  grep -q 'TOKEN:.*github.token' ".github/actions/$action/action.yml"
done
grep -q 'ACTION_REF' .github/actions/_setup/setup.sh
grep -q 'requested=\${REQUESTED:-action}' .github/actions/_setup/setup.sh
grep -R -q 'version: {description: ".*default: action}' .github/actions/{affected,install,run,history}/action.yml
for action in affected install run status history; do
  ruby -e 'require "yaml"; YAML.load_file(ARGV.fetch(0))' ".github/actions/$action/action.yml"
done
! grep -R -n 'latest' .github/actions/_setup/setup.sh
for action in affected install run status history; do
  ruby -e 'require "yaml"; inputs = YAML.load_file(ARGV.fetch(0)).fetch("inputs"); abort "input description missing" unless inputs.values.all? { |input| input["description"].is_a?(String) && !input["description"].empty? }' ".github/actions/$action/action.yml"
done
! grep -R -n 'XionWCFM/nanoom/.github/actions/_setup@main' .github/actions
! grep -R -nE 'PUSH_REF_NAME|PULL_REQUEST_(BASE|HEAD)_REF|MERGE_GROUP_(BASE|HEAD)_REF' .github/actions .github/workflows/ci.yml
grep -q 'github.event.before' .github/actions/affected/action.yml
grep -Fq 'EVENT_BASE: ${{ github.event.pull_request.base.sha || github.event.merge_group.base_sha || github.event.before || github.base_ref }}' .github/actions/affected/action.yml
grep -q 'github.base_ref' .github/actions/affected/action.yml
grep -q 'github.event.merge_group.base_sha' .github/actions/affected/action.yml
grep -q 'github.workflow_ref' .github/actions/affected/action.yml
grep -q 'github.ref_name' .github/actions/affected/action.yml
grep -q '^  groups:' .github/actions/affected/action.yml
grep -q 'output_bytes=.*has_change=.*groups=.*result' .github/actions/affected/run.sh
grep -q 'UTF-16LE' .github/actions/affected/run.sh
grep -q 'distribution' .github/actions/affected/run.sh
grep -q 'runnerLabels,timingEnvironment' .github/actions/affected/run.sh
grep -q 'historyStatus' .github/actions/affected/run.sh
grep -q 'revisionResolution' .github/actions/affected/run.sh
grep -q 'matrix:' .github/actions/install/action.yml
grep -q 'matrix:' .github/actions/run/action.yml
grep -q '^  group: {description: "Affected group' .github/actions/run/action.yml
grep -q '^  cleanupCheckout:' .github/actions/run/action.yml
grep -q 'always() && inputs.cleanupCheckout' .github/actions/run/action.yml
grep -q 'items' .github/actions/run/run.sh
grep -q 'durationMs' .github/actions/run/run.sh
grep -q 'startedAtMs' .github/actions/run/run.sh
grep -q 'preparationObservations' .github/actions/run/run.sh
grep -q 'matrix_timing_environment' .github/actions/run/run.sh
grep -q 'retention-days: 30' .github/actions/{affected,run,history}/action.yml
test "$(grep -R -l 'actions/upload-artifact@v3.2.2' .github/actions/{affected-ghes,run-ghes,history-ghes} | wc -l | tr -d ' ')" -eq 3
for file in .github/actions/{affected,history,run}/action.yml .github/workflows/{ci,history-server-e2e,release}.yml; do
  grep -q 'actions/upload-artifact@v4.6.2' "$file"
done
test "$(grep -R -l 'actions/download-artifact@v3.1.0' .github/actions/prepare-ghes | wc -l | tr -d ' ')" -eq 1
test "$(grep -R -l 'actions/download-artifact@v4.3.0' .github/actions/prepare | wc -l | tr -d ' ')" -eq 1
! grep -R -nE 'actions/(upload|download)-artifact@(v4$|v3$)' .github
grep -q 'default: artifact' .github/actions/{affected,run,history}/action.yml
! grep -R -q 'upload-artifact@v3' .github/actions/{run,history}
! grep -R -q 'upload-artifact@v4' .github/actions/{affected-ghes,run-ghes,history-ghes}
! grep -R -q 'download-artifact@v4' .github/actions/prepare-ghes
grep -q 'ARTIFACT_VERSION: v4' .github/actions/{run,history}/action.yml
grep -q 'ARTIFACT_VERSION: v3' .github/actions/{run-ghes,history-ghes}/action.yml
grep -q 'runner.environment.*self-hosted' .github/actions/{affected,run}/action.yml
grep -q 'GITHUB_STEP_SUMMARY' .github/actions/status/run.sh
grep -q '^  requiredJobs:' .github/actions/status/action.yml
grep -q 'required jobs must succeed' .github/actions/status/run.sh
! grep -q '^  version:' .github/actions/status/action.yml
! grep -qE 'affectedJob|matrixJob|GROUP|AFFECTED|MATRIX|FORMAT' .github/actions/status/action.yml .github/actions/status/run.sh
grep -q 'needs must contain at least one job result' .github/actions/status/run.sh
grep -q 'all needed jobs succeeded or were skipped' .github/actions/status/run.sh
grep -q 'Optional newline-delimited job=result pairs' .github/actions/status/action.yml
bash scripts/status-action-test.sh
bash scripts/coordinator-contract-test.sh
bash scripts/assignment-action-test.sh
grep -Fq 'version:3,scope:' .github/actions/run/run.sh
grep -Fq 'totalShards:($item.totalShards // null)' .github/actions/run/run.sh
grep -Fq 'executionId:$executionId' .github/actions/run/run.sh
! grep -q 'nanoom-timing-sample-v2' .github/actions/run/run.sh
grep -q 'planned item produced no matching execution' .github/actions/run/run.sh
grep -q 'static assignment install requires at least one workspace' .github/actions/install/run.sh
grep -q 'static assignment run requires a validated assignment-file' .github/actions/run/run.sh
grep -q 'assignment-file:' .github/actions/prepare/action.yml
grep -q 'inputs.plan' .github/actions/prepare/action.yml
grep -q '../affected/run.sh' .github/actions/affected-ghes/action.yml
grep -q '../prepare/select.sh' .github/actions/prepare-ghes/action.yml
grep -q 'original Plan reference output' .github/actions/_setup/assignment.sh
grep -q 'actions/checkout@v4' .github/actions/prepare/action.yml
grep -q 'sparse-checkout set --cone --stdin' .github/actions/prepare/checkout.sh
bash scripts/history-artifact-test.sh
bash scripts/plan-action-test.sh
bash scripts/revision-action-test.sh
bash scripts/cleanup-checkout-test.sh
bash scripts/fixture-completion-test.sh
grep -Fq "runs-on: \${{ matrix.runnerLabels || 'ubuntu-latest' }}" .github/workflows/ci.yml README.md examples/advanced/README.md
schema=$(mktemp); trap 'rm -f "$schema"' EXIT
target/debug/nanoom schema --output "$schema" >/dev/null
cmp nanoom.schema.json "$schema"
! grep -R -nE 'PUSH_REF_NAME|PULL_REQUEST_(BASE|HEAD)_REF|MERGE_GROUP_(BASE|HEAD)_REF|root-install|setup-nanoom|nanoom-(affected|install|run|status)' README.md docs/adr

echo 'action contract passed'
