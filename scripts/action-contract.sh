#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

for action in affected install run status; do
  test -f ".github/actions/$action/action.yml"
  test -f ".github/actions/$action/run.sh"
  grep -q '^  result:' ".github/actions/$action/action.yml"
  grep -q 'Final JSON' ".github/actions/$action/action.yml" ".github/actions/$action/run.sh"
  ! grep -q '::group::\|::endgroup::' ".github/actions/$action/action.yml" ".github/actions/$action/run.sh"
done
test -f .github/actions/_setup/setup.sh
for action in affected install run _setup; do
  grep -q 'TOKEN:.*github.token' ".github/actions/$action/action.yml"
done
grep -q 'ACTION_REF' .github/actions/_setup/setup.sh
grep -q 'requested=\${REQUESTED:-action}' .github/actions/_setup/setup.sh
grep -R -q 'version: {description: ".*default: action}' .github/actions/{affected,install,run}/action.yml
for action in affected install run status; do
  ruby -e 'require "yaml"; YAML.load_file(ARGV.fetch(0))' ".github/actions/$action/action.yml"
done
! grep -R -n 'latest' .github/actions/_setup/setup.sh
for action in affected install run status; do
  ruby -e 'require "yaml"; inputs = YAML.load_file(ARGV.fetch(0)).fetch("inputs"); abort "input description missing" unless inputs.values.all? { |input| input["description"].is_a?(String) && !input["description"].empty? }' ".github/actions/$action/action.yml"
done
! grep -R -n 'XionWCFM/nanoom/.github/actions/_setup@main' .github/actions
! grep -R -nE 'PUSH_REF_NAME|PULL_REQUEST_(BASE|HEAD)_REF|MERGE_GROUP_(BASE|HEAD)_REF' .github/actions .github/workflows/ci.yml
grep -q 'github.event.before' .github/actions/affected/action.yml
grep -q 'github.base_ref' .github/actions/affected/action.yml
grep -q 'github.event.merge_group.base_sha' .github/actions/affected/action.yml
grep -q '^  groups:' .github/actions/affected/action.yml
grep -q 'name, task, shard, totalShards, isolate' .github/actions/affected/action.yml
grep -q 'output_bytes=.*has_change=.*groups=.*result' .github/actions/affected/action.yml
grep -q 'UTF-16LE' .github/actions/affected/action.yml
grep -q 'baseCommit' .github/actions/affected/action.yml
grep -q 'Why these workspaces' .github/actions/affected/action.yml
grep -q 'matrix:' .github/actions/install/action.yml
grep -q 'matrix:' .github/actions/run/action.yml
grep -q '^  group: {description: "Affected group' .github/actions/run/action.yml
grep -q '.group = \$group' .github/actions/run/action.yml
grep -q 'GITHUB_STEP_SUMMARY' .github/actions/status/run.sh
! grep -q '^  version:' .github/actions/status/action.yml
! grep -qE 'affectedJob|matrixJob|GROUP|AFFECTED|MATRIX|FORMAT' .github/actions/status/action.yml .github/actions/status/run.sh
grep -q 'needs must contain at least one job result' .github/actions/status/run.sh
grep -q 'all needed jobs succeeded or were skipped' .github/actions/status/run.sh
bash scripts/status-action-test.sh
! grep -R -nE 'PUSH_REF_NAME|PULL_REQUEST_(BASE|HEAD)_REF|MERGE_GROUP_(BASE|HEAD)_REF|root-install|setup-nanoom|nanoom-(affected|install|run|status)|"concurrency"' README.md docs/adr

echo 'action contract passed'
