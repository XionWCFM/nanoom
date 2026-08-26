#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

for action in affected install run status; do
  test -f ".github/actions/$action/action.yml"
  test -f ".github/actions/$action/run.sh"
  grep -q 'using: composite' ".github/actions/$action/action.yml"
  test "$(grep -c '^      run:' ".github/actions/$action/action.yml")" -eq 1
  ! grep -qE 'using: node|@actions/' ".github/actions/$action/action.yml" ".github/actions/$action/run.sh"
  grep -q '^  result:' ".github/actions/$action/action.yml"
  for section in 'Inputs' 'Resolved values' 'Why' 'Command' 'Progress' 'Result' 'Action outputs' 'Final JSON'; do
    grep -q "$section" ".github/actions/$action/run.sh"
  done
  ! grep -q 'jq \.' ".github/actions/$action/run.sh"
done
! grep -R -n --include='*.sh' '::group::\|::endgroup::' .github/actions
test -f .github/actions/_setup/setup.sh
for action in affected install run _setup; do
  grep -q 'TOKEN:.*github.token' ".github/actions/$action/action.yml"
done
for action in affected install run _setup; do
  grep -q 'ACTION_REF:.*github.action_ref' ".github/actions/$action/action.yml"
done
grep -q 'action_ref.*v\[0-9\]' .github/actions/_setup/setup.sh
grep -q 'Authorization: Bearer' .github/actions/_setup/setup.sh
grep -q 'ACTION_REF' .github/actions/_setup/setup.sh
grep -q 'requested=\${REQUESTED:-action}' .github/actions/_setup/setup.sh
grep -R -q 'version: {default: action}' .github/actions/{affected,install,run}/action.yml
! grep -R -n 'XionWCFM/nanoom/.github/actions/_setup@main' .github/actions
! grep -R -nE 'PUSH_REF_NAME|PULL_REQUEST_(BASE|HEAD)_REF|MERGE_GROUP_(BASE|HEAD)_REF' .github/actions .github/workflows/ci.yml
grep -q 'github.event.before' .github/actions/affected/action.yml
grep -q 'github.base_ref' .github/actions/affected/action.yml
grep -q 'github.event.merge_group.base_sha' .github/actions/affected/action.yml
grep -q '^  groups:' .github/actions/affected/action.yml
grep -q 'baseCommit' .github/actions/affected/run.sh
grep -q 'selected workspaces' .github/actions/affected/run.sh
grep -q 'matrix:' .github/actions/install/action.yml
grep -q 'matrix:' .github/actions/run/action.yml
grep -q 'GITHUB_STEP_SUMMARY' .github/actions/status/run.sh
! grep -q '^  version:' .github/actions/status/action.yml
! grep -qE 'affectedJob|matrixJob|GROUP|AFFECTED|MATRIX|FORMAT' .github/actions/status/action.yml .github/actions/status/run.sh
grep -q 'needs must contain at least one job result' .github/actions/status/run.sh
grep -q 'all needed jobs succeeded or were skipped' .github/actions/status/run.sh
! grep -R -nE 'PUSH_REF_NAME|PULL_REQUEST_(BASE|HEAD)_REF|MERGE_GROUP_(BASE|HEAD)_REF|root-install|setup-nanoom|nanoom-(affected|install|run|status)|"concurrency"' README.md

failure_dir=$(mktemp -d)
trap 'rm -rf "$failure_dir"' EXIT
if GITHUB_OUTPUT="$failure_dir/output" ACTION_NAME=run ACTION_PHASE=matrix-task ACTION_COMMAND='nanoom run ci test' ACTION_CWD=. bash -c 'source .github/actions/_setup/log.sh; nanoom_fail 7' >"$failure_dir/log" 2>&1; then
  echo 'failure logger unexpectedly succeeded' >&2
  exit 1
fi
grep -q '✗ failed during matrix-task (exit 7)' "$failure_dir/log"
grep -q 'Final JSON' "$failure_dir/log"
sed -n 's/^result=//p' "$failure_dir/output" | jq -e '.status == "failure" and .phase == "matrix-task" and .exitCode == 7' >/dev/null
bash scripts/status-action-test.sh

echo 'action contract passed'
