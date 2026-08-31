#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/work"

cat > "$tmp/bin/nanoom" <<'SH'
#!/usr/bin/env bash
/bin/sleep 0.12
printf '%s\n' '{"status":"success","executions":[{"workspace":"pkg-a","runner":"nx","durationMs":42}],"completed":["pkg-a"],"failed":[],"pending":[]}'
SH
cat > "$tmp/bin/sleep" <<'SH'
#!/usr/bin/env bash
/bin/sleep 0.02
SH
cat > "$tmp/bin/curl" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$FAKE_CURL_LOG"
url=''; for arg in "$@"; do [[ "$arg" == https://* ]] && url=$arg; done
if [[ "${FAIL_HEARTBEAT:-}" == 1 && "$*" == *heartbeat* ]]; then exit 22; fi
if [[ "$url" == */claims ]]; then
  count=$(cat "$FAKE_CLAIM_COUNT" 2>/dev/null || echo 0)
  if [[ "$count" == 0 ]]; then
    echo 1 > "$FAKE_CLAIM_COUNT"
    printf '%s\n' '{"itemId":"item-1","item":{"group":"ci","name":"pkg-a","task":"test"}}'
  else
    printf '%s\n' '{"item":null}'
  fi
fi
SH
chmod +x "$tmp/bin/nanoom" "$tmp/bin/sleep" "$tmp/bin/curl"

export PATH="$tmp/bin:$PATH"
export FAKE_CURL_LOG="$tmp/curl.log" FAKE_CLAIM_COUNT="$tmp/claims"
export GITHUB_ACTION_PATH="$root/.github/actions/run" GITHUB_OUTPUT="$tmp/output" GITHUB_STEP_SUMMARY="$tmp/summary"
export GITHUB_RUN_ID=1 GITHUB_RUN_ATTEMPT=1 RUNNER_TEMP="$tmp"
export MATRIX='{"agentId":"agent-1","runId":"run-1","mode":"continuous"}' GROUP=ci PM=pnpm TOOL=nx CWD="$tmp/work"
export SCHEDULER=http TIMING_ENVIRONMENT=linux-x64 COORDINATOR_URL=https://coordinator.test COORDINATOR_TOKEN=secret-value

output=$(bash "$GITHUB_ACTION_PATH/run.sh" 2>&1)
grep -q '/v1/runs/run-1/claims' "$FAKE_CURL_LOG"
grep -q '/v1/runs/run-1/claims/item-1' "$FAKE_CURL_LOG"
grep -q 'Idempotency-Key: run-1:agent-1:claim:1' "$FAKE_CURL_LOG"
grep -q 'status.*heartbeat' "$FAKE_CURL_LOG"
grep -q 'status.*success' "$FAKE_CURL_LOG"
grep -q '^result=' "$GITHUB_OUTPUT"
! grep -q 'secret-value' <<<"$output"

rm -f "$FAKE_CLAIM_COUNT" "$GITHUB_OUTPUT"; export FAIL_HEARTBEAT=1
if bash "$GITHUB_ACTION_PATH/run.sh" >/dev/null 2>&1; then echo 'heartbeat failure unexpectedly succeeded' >&2; exit 1; fi
result=$(sed -n 's/^result=//p' "$GITHUB_OUTPUT")
jq -e '.status == "failure" and (.reason | contains("heartbeat failed"))' <<<"$result" >/dev/null

echo 'coordinator client contract passed'
