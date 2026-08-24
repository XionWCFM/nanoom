#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

for action in affected install run status; do
  test -f ".github/actions/$action/action.yml"
done
test -f .github/actions/_setup/setup.sh
for action in affected install run _setup; do
  grep -q 'TOKEN:.*github.token' ".github/actions/$action/action.yml"
done
grep -q 'Authorization: Bearer' .github/actions/_setup/setup.sh
! grep -R -n 'XionWCFM/nanoom/.github/actions/_setup@main' .github/actions
! grep -R -nE 'PUSH_REF_NAME|PULL_REQUEST_(BASE|HEAD)_REF|MERGE_GROUP_(BASE|HEAD)_REF' .github/actions .github/workflows/ci.yml
grep -q 'github.event.before' .github/actions/affected/action.yml
grep -q 'github.base_ref' .github/actions/affected/action.yml
grep -q 'github.event.merge_group.base_sha' .github/actions/affected/action.yml
grep -q '^  groups:' .github/actions/affected/action.yml
grep -q 'matrix JSON:' .github/actions/install/action.yml
grep -q 'matrix JSON:' .github/actions/run/action.yml
grep -q 'GITHUB_STEP_SUMMARY' .github/actions/status/action.yml
! grep -q '^  version:' .github/actions/status/action.yml
! grep -R -nE 'PUSH_REF_NAME|PULL_REQUEST_(BASE|HEAD)_REF|MERGE_GROUP_(BASE|HEAD)_REF|root-install|setup-nanoom|nanoom-(affected|install|run|status)|"concurrency"' README.md docs/content

echo 'action contract passed'
