#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd); tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/work"
cat > "$tmp/bin/nanoom" <<'SH'
#!/usr/bin/env bash
if [[ "${3:-}" == run ]]; then printf '%s\n' "$*" >> "$FAKE_RUN_CALLS"; fi
if [[ "${3:-}" == install ]]; then
  printf '%s\n' "$*" >> "$FAKE_INSTALL_CALLS"
  printf '%s\n' '{"status":"success","scope":{"root":true}}'
  exit 0
fi
if [[ "${FAKE_EMPTY:-}" == 1 ]]; then
  printf '%s\n' '{"status":"success","projects":[],"executions":[]}'
  exit 0
fi
if [[ "${FAKE_SUCCESS:-}" == 1 ]]; then
  printf '%s\n' '{"status":"success","item":{"name":"pkg-a","task":"test"},"executions":[{"workspace":"pkg-a","runner":"turbo","durationMs":1}]}'
  exit 0
fi
printf '%s\n' '{"status":"failure","completed":[],"failed":["pkg-a"],"pending":[],"executions":[],"error":"expected"}'
exit 1
SH
chmod +x "$tmp/bin/nanoom"
export PATH="$tmp/bin:$PATH" GITHUB_ACTION_PATH="$root/.github/actions/run" GITHUB_OUTPUT="$tmp/output" GITHUB_STEP_SUMMARY="$tmp/summary" RUNNER_TEMP="$tmp"
export FAKE_RUN_CALLS="$tmp/run-calls" FAKE_INSTALL_CALLS="$tmp/install-calls"
export MATRIX='{"assignmentId":"ci-1","items":[{"group":"ci","name":"pkg-a","task":"test"},{"group":"ci","name":"pkg-b","task":"test"}]}' GROUP=ci PM=pnpm TOOL=pnpm CWD="$tmp/work"
export SCHEDULER=off TIMING_ENVIRONMENT=linux-x64 COORDINATOR_URL='' COORDINATOR_TOKEN='' GITHUB_RUN_ID=1 GITHUB_RUN_ATTEMPT=1
: > "$FAKE_RUN_CALLS"
if bash "$GITHUB_ACTION_PATH/run.sh" >/dev/null 2>&1; then echo 'failure assignment unexpectedly succeeded' >&2; exit 1; fi
result=$(sed -n 's/^result=//p' "$GITHUB_OUTPUT")
jq -e '.status == "failure" and [.failed[].name] == ["pkg-a"] and [.pending[].name] == ["pkg-b"] and (.completed | length) == 0' <<<"$result" >/dev/null
test "$(wc -l < "$FAKE_RUN_CALLS" | tr -d ' ')" -eq 1

export FAKE_EMPTY=1
: > "$GITHUB_OUTPUT"; : > "$FAKE_RUN_CALLS"
if bash "$GITHUB_ACTION_PATH/run.sh" >/dev/null 2>&1; then echo 'empty planned execution unexpectedly succeeded' >&2; exit 1; fi
result=$(sed -n 's/^result=//p' "$GITHUB_OUTPUT")
jq -e '.status == "failure" and [.failed[].name] == ["pkg-a"] and [.pending[].name] == ["pkg-b"]' <<<"$result" >/dev/null
test "$(wc -l < "$FAKE_RUN_CALLS" | tr -d ' ')" -eq 1
unset FAKE_EMPTY

export FAKE_SUCCESS=1 SCHEDULER=artifact MATRIX='{"assignmentId":"ci-1","timingEnvironment":"runner-labels:[\"linux\",\"self-hosted\"]","items":[{"group":"ci","name":"pkg-a","task":"test","shard":1,"totalShards":4}]}'
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
jq -e '.samples[0].environment == "runner-labels:[\"linux\",\"self-hosted\"]"' "$yarn_sample" "$pnpm_sample" >/dev/null
jq -e '.samples[0].shard == 1 and .samples[0].totalShards == 4' "$yarn_sample" "$pnpm_sample" >/dev/null

GITHUB_ACTION_PATH="$root/.github/actions/install"
export GITHUB_ACTION_PATH PM=pnpm
export MATRIX='{"assignmentId":"ci-empty","items":[]}'
: > "$GITHUB_OUTPUT"; : > "$FAKE_INSTALL_CALLS"
if bash "$GITHUB_ACTION_PATH/run.sh" >/dev/null 2>&1; then echo 'empty static install unexpectedly succeeded' >&2; exit 1; fi
test ! -s "$FAKE_INSTALL_CALLS"

export MATRIX='{"mode":"continuous","items":[]}'
: > "$GITHUB_OUTPUT"
bash "$GITHUB_ACTION_PATH/run.sh" >/dev/null
grep -q ' install --package-manager pnpm --json' "$FAKE_INSTALL_CALLS"
! grep -q -- '--filter' "$FAKE_INSTALL_CALLS"
echo 'assignment failure contract passed'
