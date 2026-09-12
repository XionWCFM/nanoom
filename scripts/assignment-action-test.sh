#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd); tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/work"
cat > "$tmp/bin/nanoom" <<'SH'
#!/usr/bin/env bash
if [[ "${FAKE_SUCCESS:-}" == 1 ]]; then
  printf '%s\n' '{"status":"success","item":{"name":"pkg-a","task":"test"},"executions":[{"workspace":"pkg-a","runner":"turbo","durationMs":1}]}'
  exit 0
fi
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

export FAKE_SUCCESS=1 SCHEDULER=artifact MATRIX='{"assignmentId":"ci-1","items":[{"group":"ci","name":"pkg-a","task":"test"}]}'
: > "$GITHUB_OUTPUT"
if ARTIFACT_VERSION=v5 bash "$GITHUB_ACTION_PATH/run.sh" >"$tmp/version.log" 2>&1; then echo 'invalid artifact version unexpectedly succeeded' >&2; exit 1; fi
grep -q 'artifactVersion must be v4 or v3' "$tmp/version.log"
export ARTIFACT_VERSION=v4
export GITHUB_JOB=yarn-run
bash "$GITHUB_ACTION_PATH/run.sh" >/dev/null
result=$(sed -n 's/^result=//p' "$GITHUB_OUTPUT")
jq -e '.artifactVersion == "v4"' <<<"$result" >/dev/null
yarn_sample=$(sed -n 's/^sample-path=//p' "$GITHUB_OUTPUT")
yarn_name=$(sed -n 's/^sample-name=//p' "$GITHUB_OUTPUT")
: > "$GITHUB_OUTPUT"
export GITHUB_JOB=pnpm-run ARTIFACT_VERSION=v3
bash "$GITHUB_ACTION_PATH/run.sh" >/dev/null
result=$(sed -n 's/^result=//p' "$GITHUB_OUTPUT")
jq -e '.artifactVersion == "v3"' <<<"$result" >/dev/null
pnpm_sample=$(sed -n 's/^sample-path=//p' "$GITHUB_OUTPUT")
pnpm_name=$(sed -n 's/^sample-name=//p' "$GITHUB_OUTPUT")
test "$yarn_sample" != "$pnpm_sample" && test "$yarn_name" != "$pnpm_name"
test -s "$yarn_sample" && test -s "$pnpm_sample"
jq -e '.samples | length == 1' "$yarn_sample" "$pnpm_sample" >/dev/null
echo 'assignment failure contract passed'
