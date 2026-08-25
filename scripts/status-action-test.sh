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

echo 'status action tests passed'
