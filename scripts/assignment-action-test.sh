#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
workspace="$tmp/work"
cwd="$workspace/.nanoom/1/1/run/0"
mkdir -p "$tmp/bin" "$tmp/selected" "$cwd"
git -C "$cwd" init -q
git -C "$cwd" config user.email test@example.com
git -C "$cwd" config user.name test
printf '{"name":"root"}\n' > "$cwd/package.json"
git -C "$cwd" add package.json
git -C "$cwd" commit -qm checkout
head=$(git -C "$cwd" rev-parse HEAD)
reference=$(jq -cn --arg head "$head" '{version:1,artifactName:"nanoom-plan-v1-1-1-test",sha256:"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",provenance:{repository:"owner/repo",workflow:"owner/repo/.github/workflows/ci.yml@refs/heads/main",runId:"1",producerAttempt:1,planningJob:"test",base:$head,head:$head},current:{repository:"owner/repo",workflow:"owner/repo/.github/workflows/ci.yml@refs/heads/main",runId:"1",attempt:1,base:$head,head:$head}}')
assignment_file="$tmp/selected/assignment.json"
plan_source="$tmp/plan-v1.json"
reference_source="$tmp/plan-reference.json"
printf '{"version":1}\n' > "$plan_source"
printf '%s\n' "$reference" > "$reference_source"

write_assignment() {
  jq -n --arg head "$head" --argjson reference "$reference" --argjson items "$1" \
    '{version:1,planSha256:$reference.sha256,provenance:$reference.provenance,current:$reference.current,taskRunner:"pnpm",group:"ci",assignmentId:"ci-1",items:$items,checkoutPaths:["packages/pkg-a","packages/pkg-b"],predictionReason:"test"}' > "$tmp/selected-source.json"
  cp "$tmp/selected-source.json" "$assignment_file"
  cp "$plan_source" "$tmp/selected/plan-v1.json"
  cp "$reference_source" "$tmp/selected/plan-reference.json"
}
write_assignment '[{"group":"ci","name":"pkg-a","path":"packages/pkg-a","task":"test"},{"group":"ci","name":"pkg-b","path":"packages/pkg-b","task":"test"}]'

cat > "$tmp/bin/nanoom" <<'SH'
#!/usr/bin/env bash
if [[ " $* " == *" plan select "* ]]; then
  while (($#)); do
    if [[ "$1" == --output-dir ]]; then out=$2; shift 2; else shift; fi
  done
  mkdir -p "$out"
  cp "$FAKE_SELECTED_ASSIGNMENT" "$out/assignment.json"
  printf '%s\n' '{"status":"success"}'
  exit 0
fi
if [[ "${3:-}" == run ]]; then printf '%s\n' "$*" >> "$FAKE_RUN_CALLS"; fi
if [[ "${3:-}" == install ]]; then
  printf '%s\n' "$*" >> "$FAKE_INSTALL_CALLS"
  while (($#)); do
    if [[ "$1" == --filter-file ]]; then cp "$2" "$FAKE_FILTER_FILE"; shift 2; else shift; fi
  done
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
export PATH="$tmp/bin:$PATH" GITHUB_ACTION_PATH="$root/.github/actions/run" GITHUB_STEP_SUMMARY="$tmp/summary" RUNNER_TEMP="$tmp/runner"
mkdir -p "$RUNNER_TEMP"
export GITHUB_WORKSPACE="$workspace" GITHUB_SHA="$head" REPOSITORY=owner/repo
export WORKFLOW_REF=owner/repo/.github/workflows/ci.yml@refs/heads/main RUN_ID=1 RUN_ATTEMPT=1 GITHUB_RUN_ID=1 GITHUB_RUN_ATTEMPT=1 GITHUB_JOB=run MATRIX_INDEX=0
export PLAN="$reference" ASSIGNMENT_FILE="$assignment_file" FAKE_SELECTED_ASSIGNMENT="$tmp/selected-source.json"
export GITHUB_OUTPUT="$tmp/output" FAKE_RUN_CALLS="$tmp/run-calls" FAKE_INSTALL_CALLS="$tmp/install-calls" FAKE_FILTER_FILE="$tmp/filter-file.json"
export MATRIX='' GROUP= PM=pnpm TOOL=auto CWD="$cwd" SCHEDULER=off TIMING_ENVIRONMENT=linux-x64 COORDINATOR_URL='' COORDINATOR_TOKEN=''

: > "$FAKE_RUN_CALLS"
if bash "$GITHUB_ACTION_PATH/run.sh" >/dev/null 2>&1; then echo 'failed assignment unexpectedly succeeded' >&2; exit 1; fi
result=$(sed -n 's/^result=//p' "$GITHUB_OUTPUT")
jq -e '.status == "failure" and .failed.name == "pkg-a" and .pendingCount == 1 and .completedCount == 0' <<<"$result" >/dev/null
detail_file=$(jq -er .detailFile <<<"$result")
jq -s -e 'any(.[]; .status == "assignment-stopped" and .pendingCount == 1) and any(.[]; .status == "pending" and .item.name == "pkg-b")' "$detail_file" >/dev/null
test "$(wc -l < "$FAKE_RUN_CALLS" | tr -d ' ')" -eq 1

write_assignment '[{"group":"ci","name":"pkg-a","path":"packages/pkg-a","task":"test"}]'
: > "$GITHUB_OUTPUT"; : > "$FAKE_RUN_CALLS"
if TOOL=turbo bash "$GITHUB_ACTION_PATH/run.sh" >"$tmp/runner-switch.log" 2>&1; then echo 'static assignment runner switch unexpectedly succeeded' >&2; exit 1; fi
grep -q 'cannot be changed' "$tmp/runner-switch.log"
test ! -s "$FAKE_RUN_CALLS"
write_assignment '[{"group":"ci","name":"pkg-a","path":"packages/pkg-a","task":"test"},{"group":"ci","name":"pkg-b","path":"packages/pkg-b","task":"test"}]'

export FAKE_EMPTY=1
: > "$GITHUB_OUTPUT"; : > "$FAKE_RUN_CALLS"
if bash "$GITHUB_ACTION_PATH/run.sh" >/dev/null 2>&1; then echo 'empty planned execution unexpectedly succeeded' >&2; exit 1; fi
result=$(sed -n 's/^result=//p' "$GITHUB_OUTPUT")
jq -e '.status == "failure" and .failed.name == "pkg-a" and .pendingCount == 1' <<<"$result" >/dev/null
test "$(wc -l < "$FAKE_RUN_CALLS" | tr -d ' ')" -eq 1
unset FAKE_EMPTY

export FAKE_SUCCESS=1 SCHEDULER=artifact TIMING_ENVIRONMENT='runner-labels:["linux","self-hosted"]'
write_assignment '[{"group":"ci","name":"pkg-a","path":"packages/pkg-a","task":"test","shard":1,"totalShards":4}]'
: > "$GITHUB_OUTPUT"
if ARTIFACT_VERSION=v5 bash "$GITHUB_ACTION_PATH/run.sh" >"$tmp/version.log" 2>&1; then echo 'invalid artifact version unexpectedly succeeded' >&2; exit 1; fi
grep -q 'artifactVersion must be v4 or v3' "$tmp/version.log"
: > "$GITHUB_OUTPUT"
export ARTIFACT_VERSION=v4 GITHUB_JOB=yarn-run
bash "$GITHUB_ACTION_PATH/run.sh" >/dev/null
result=$(sed -n 's/^result=//p' "$GITHUB_OUTPUT")
jq -e '.status == "success" and .artifactVersion == "v4" and .plannedItemCount == 1 and .executedItemCount == 1' <<<"$result" >/dev/null
! jq -e 'has("items") or has("matrix")' <<<"$result" >/dev/null
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
jq -e --arg env 'runner-labels:["linux","self-hosted"]' '.samples[0].environment == $env' "$yarn_sample" "$pnpm_sample" >/dev/null
jq -e '.samples[0].shard == 1 and .samples[0].totalShards == 4' "$yarn_sample" "$pnpm_sample" >/dev/null

GITHUB_ACTION_PATH="$root/.github/actions/install"
export GITHUB_ACTION_PATH PM=pnpm GITHUB_JOB=install SCHEDULER=off
write_assignment '[]'
: > "$GITHUB_OUTPUT"; : > "$FAKE_INSTALL_CALLS"
if bash "$GITHUB_ACTION_PATH/run.sh" >/dev/null 2>&1; then echo 'empty static install unexpectedly succeeded' >&2; exit 1; fi
test ! -s "$FAKE_INSTALL_CALLS"

write_assignment '[{"group":"ci","name":"pkg-a","path":"packages/pkg-a","task":"test"},{"group":"ci","name":"pkg-b","path":"packages/pkg-b","task":"test"}]'
: > "$GITHUB_OUTPUT"; : > "$FAKE_INSTALL_CALLS"
bash "$GITHUB_ACTION_PATH/run.sh" >/dev/null
grep -q -- '--filter-file' "$FAKE_INSTALL_CALLS"
jq -e '. == ["pkg-a","pkg-b"]' "$FAKE_FILTER_FILE" >/dev/null
install_result=$(sed -n 's/^result=//p' "$GITHUB_OUTPUT")
jq -e '.assignment.itemCount == 2 and (.assignment | has("items") | not)' <<<"$install_result" >/dev/null

export ASSIGNMENT_FILE='' PLAN=''
export MATRIX='{"mode":"continuous","items":[]}'
: > "$GITHUB_OUTPUT"
bash "$GITHUB_ACTION_PATH/run.sh" >/dev/null
grep -q ' install --package-manager pnpm --json' "$FAKE_INSTALL_CALLS"
! grep -q -- '--filter' "$FAKE_INSTALL_CALLS"
echo 'assignment failure contract passed'
