#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

git diff --check
cargo fmt --all --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all --all-features
cargo llvm-cov --locked --workspace --all-features --fail-under-lines 90 --summary-only
bash scripts/action-contract.sh
bash scripts/setup-smoke.sh
bash scripts/platform-package-smoke.sh
node packages/cli/smoke-test.js
node packages/cli/download-smoke.js
corepack yarn --cwd docs install --immutable
corepack yarn --cwd docs build

test -z "$(git ls-files | grep -E '(^|/)(node_modules|\.next|install-state\.gz)(/|$)' || true)"

if [[ ${1:-} == --local ]]; then
  echo 'local completion gate passed'
  exit 0
fi

run_id=${NANOOM_FIXTURE_RUN_ID:?Set NANOOM_FIXTURE_RUN_ID to a hosted nanoom-fixtures run}
jobs=$(gh api --paginate "repos/XionWCFM/nanoom-fixtures/actions/runs/$run_id/jobs?per_page=100" --jq '.jobs[] | {name,conclusion}')
jq -se '
  length >= 3 and
  any(.[]; .name == "affected" and .conclusion == "success") and
  ([.[] | select(.name | startswith("run"))] | length >= 4) and
  any(.[]; .name == "status" and .conclusion == "success") and
  all(.[] | select(.name | startswith("run")); .conclusion == "success")
' <<<"$jobs" >/dev/null
echo "hosted fixture completion gate passed: run $run_id"
