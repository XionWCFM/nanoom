#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

positive_plan=$(jq -nc '{version:1,hasChange:true,assignmentCount:2,itemCount:3,groups:{ci:{assignments:[{items:[{},{}]},{items:[{}]}]}}}')
no_change_plan=$(jq -nc '{version:1,hasChange:false,assignmentCount:0,itemCount:0,groups:{ci:{assignments:[]}}}')
positive_jobs='{"name":"affected","conclusion":"success"}
{"name":"run (scale-1)","conclusion":"success"}
{"name":"run (scale-2)","conclusion":"success"}
{"name":"status","conclusion":"success"}'
check_case() {
  local name=$1 plan=$2 jobs=$3 expected=$4 actual
  printf '%s\n' "$plan" > "$tmp/plan.json"
  if jq -se --slurpfile plan "$tmp/plan.json" -f "$root/scripts/fixture-completion.jq" <<<"$jobs" >/dev/null 2>&1; then
    actual=success
  else
    actual=failure
  fi
  [[ "$actual" == "$expected" ]] || { echo "$name: expected $expected, got $actual" >&2; return 1; }
}

check_case plan-counted-positive "$positive_plan" "$positive_jobs" success
check_case missing-positive-assignment "$positive_plan" "$(jq -c 'select(.name != "run (scale-2)")' <<<"$positive_jobs")" failure
check_case skipped-positive-assignment "$positive_plan" "$(jq -c 'if .name == "run (scale-2)" then .conclusion = "skipped" else . end' <<<"$positive_jobs")" failure
check_case zero-positive-runs "$positive_plan" "$(jq -c 'select((.name | startswith("run")) | not)' <<<"$positive_jobs")" failure
check_case no-change-skipped-run "$no_change_plan" '{"name":"affected","conclusion":"success"}
{"name":"run","conclusion":"skipped"}
{"name":"status","conclusion":"success"}' success
check_case no-change-no-run "$no_change_plan" '{"name":"affected","conclusion":"success"}
{"name":"status","conclusion":"success"}' success
check_case unexpected-no-change-run "$no_change_plan" '{"name":"affected","conclusion":"success"}
{"name":"run","conclusion":"success"}
{"name":"status","conclusion":"success"}' failure
check_case inconsistent-plan "$(jq -c '.itemCount = 4' <<<"$positive_plan")" "$positive_jobs" failure

echo 'fixture completion contract passed'
