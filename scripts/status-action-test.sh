#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
action="$root/.github/actions/status/run.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

run_case() {
  local name=$1 needs=$2 expected=$3
  local output="$tmp/$name.output" summary="$tmp/$name.summary" log="$tmp/$name.log"
  if NEEDS="$needs" GITHUB_OUTPUT="$output" GITHUB_STEP_SUMMARY="$summary" GITHUB_ACTION_PATH="$root/.github/actions/status" bash "$action" >"$log" 2>&1; then
    actual=$(sed -n 's/^result=//p' "$output" | jq -r .status)
    [[ "$actual" == "$expected" ]] || { echo "$name: expected $expected, got $actual" >&2; return 1; }
  else
    [[ "$expected" == failure ]] || { echo "$name: expected success" >&2; return 1; }
    actual=$(sed -n 's/^result=//p' "$output" | jq -r .status)
    [[ "$actual" == failure ]] || { echo "$name: failure output missing" >&2; return 1; }
  fi
}

run_case all-success '{"build":{"result":"success"},"test":{"result":"success"}}' success
run_case success-and-skipped '{"build":{"result":"success"},"test":{"result":"skipped"}}' success
run_case failure '{"build":{"result":"success"},"test":{"result":"failure"}}' failure
run_case cancelled '{"test":{"result":"cancelled"}}' failure
run_case unknown '{"test":{"result":"queued"}}' failure
run_case empty '{}' failure
run_case malformed '{not-json}' failure

large_results=$(printf 'job%03d=success\n' $(seq 1 2000))
if RESULTS="$large_results" NEEDS= GITHUB_OUTPUT="$tmp/large.output" GITHUB_STEP_SUMMARY="$tmp/large.summary" GITHUB_ACTION_PATH="$root/.github/actions/status" bash "$action" >/dev/null 2>&1; then
  jq -e '.status == "success" and (.jobs | length) == 2000' < <(sed -n 's/^result=//p' "$tmp/large.output") >/dev/null
else
  echo 'large results case failed' >&2; exit 1
fi

if RESULTS=$'valid=success\nmalformed\n' NEEDS= GITHUB_OUTPUT="$tmp/malformed-results.output" GITHUB_STEP_SUMMARY="$tmp/malformed-results.summary" GITHUB_ACTION_PATH="$root/.github/actions/status" bash "$action" >/dev/null 2>&1; then
  echo 'malformed results case unexpectedly passed' >&2; exit 1
fi

if RESULTS='missing-result=' NEEDS= GITHUB_OUTPUT="$tmp/results-malformed.output" GITHUB_STEP_SUMMARY="$tmp/results-malformed.summary" GITHUB_ACTION_PATH="$root/.github/actions/status" bash "$action" >/dev/null 2>&1; then
  echo 'malformed results unexpectedly succeeded' >&2; exit 1
fi

echo 'status action tests passed'
