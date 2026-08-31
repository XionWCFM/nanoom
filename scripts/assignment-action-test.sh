#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd); tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/work"
cat > "$tmp/bin/nanoom" <<'SH'
#!/usr/bin/env bash
printf '%s\n' '{"status":"failure","completed":[],"failed":["pkg-a"],"pending":[],"executions":[],"error":"expected"}'
exit 1
SH
chmod +x "$tmp/bin/nanoom"
export PATH="$tmp/bin:$PATH" GITHUB_ACTION_PATH="$root/.github/actions/run" GITHUB_OUTPUT="$tmp/output" GITHUB_STEP_SUMMARY="$tmp/summary" RUNNER_TEMP="$tmp"
export MATRIX='{"assignmentId":"ci-1","items":[{"group":"ci","name":"pkg-a","task":"test"},{"group":"ci","name":"pkg-b","task":"test"}]}' GROUP=ci PM=pnpm TOOL=pnpm CWD="$tmp/work"
export SCHEDULER=off TIMING_ENVIRONMENT=linux-x64 COORDINATOR_URL='' COORDINATOR_TOKEN='' GITHUB_RUN_ID=1 GITHUB_RUN_ATTEMPT=1
if bash "$GITHUB_ACTION_PATH/run.sh" >/dev/null 2>&1; then echo 'failure assignment unexpectedly succeeded' >&2; exit 1; fi
result=$(sed -n 's/^result=//p' "$GITHUB_OUTPUT")
jq -e '.status == "failure" and [.failed[].name] == ["pkg-a"] and [.pending[].name] == ["pkg-b"] and (.completed | length) == 0' <<<"$result" >/dev/null
echo 'assignment failure contract passed'
