#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

git diff --check
cargo fmt --all --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all --all-features
cargo llvm-cov --locked --workspace --all-features --fail-under-lines 96 --summary-only
bash scripts/action-contract.sh
bash scripts/setup-smoke.sh
bash scripts/platform-package-smoke.sh
node packages/cli/smoke-test.js
test -z "$(git ls-files | grep -E '(^|/)(node_modules|\.next|install-state\.gz)(/|$)' || true)"

if [[ ${1:-} == --local ]]; then
  echo 'local completion gate passed'
  exit 0
fi

run_id=${NANOOM_FIXTURE_RUN_ID:?Set NANOOM_FIXTURE_RUN_ID to a hosted nanoom-fixtures run}
[[ "$run_id" =~ ^[0-9]+$ ]] || { echo 'NANOOM_FIXTURE_RUN_ID must be numeric' >&2; exit 2; }
repo=XionWCFM/nanoom-fixtures
run_attempt=$(gh api "repos/$repo/actions/runs/$run_id" --jq '.run_attempt')
[[ "$run_attempt" =~ ^[0-9]+$ ]] || { echo "run $run_id returned an invalid attempt" >&2; exit 2; }
plan_prefix="nanoom-plan-v1-$run_id-$run_attempt-"
artifacts=$(gh api "repos/$repo/actions/runs/$run_id/artifacts?per_page=100")
artifact_name=$(jq -er --arg prefix "$plan_prefix" '[.artifacts[]? | select(.expired == false and (.name | startswith($prefix))) | .name] | if length == 1 then .[0] else error("expected exactly one Plan artifact for the latest attempt") end' <<<"$artifacts")
plan_dir=$(mktemp -d)
trap 'rm -rf "$plan_dir"' EXIT
gh run download "$run_id" --repo "$repo" --name "$artifact_name" --dir "$plan_dir"
plan_file="$plan_dir/plan-v1.json"
test -s "$plan_file"
jobs=$(gh api --paginate "repos/XionWCFM/nanoom-fixtures/actions/runs/$run_id/jobs?per_page=100" --jq '.jobs[] | {name,conclusion}')
jq -se --slurpfile plan "$plan_file" -f scripts/fixture-completion.jq <<<"$jobs" >/dev/null
echo "hosted fixture completion gate passed: run $run_id"
