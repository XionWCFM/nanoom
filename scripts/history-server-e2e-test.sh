#!/usr/bin/env bash
set -Eeuo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
worker_pid=''
cleanup() {
  local status=$?
  if [[ -n "$worker_pid" ]]; then kill "$worker_pid" 2>/dev/null || true; wait "$worker_pid" 2>/dev/null || true; fi
  rm -f "$tmp/worker/.dev.vars"
  if (( status == 0 )); then rm -rf "$tmp"; else echo "E2E diagnostics retained at $tmp" >&2; fi
}
trap cleanup EXIT

for tool in cargo curl git jq node npx shasum uuidgen; do command -v "$tool" >/dev/null || { echo "missing required tool: $tool" >&2; exit 1; }; done
wrangler() { npm_config_cache="$tmp/npm-cache" npx --yes wrangler@4.138.0 "$@"; }
cargo build --locked --bin nanoom >/dev/null
mkdir -p "$tmp/bin"
export PATH="$root/target/debug:$tmp/bin:$PATH" NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost
cat > "$tmp/bin/yarn" <<'SH'
#!/usr/bin/env bash
exec node "${NANOOM_YARN_PATH:?NANOOM_YARN_PATH is required}" "$@"
SH
chmod +x "$tmp/bin/yarn"

mkdir -p "$tmp/runner" "$tmp/worker" "$tmp/workspace" "$tmp/measurements"
(cd "$tmp" && bash "$root/.github/test-fixtures/setup-fixture.sh" --shards --distribution --change >/dev/null)
fixture="$tmp/.fixture"
if ! (cd "$fixture" && node .yarn/releases/yarn-4.11.0.cjs install) >"$tmp/fixture-install.log" 2>&1; then
  cat "$tmp/fixture-install.log" >&2
  exit 1
fi
git -C "$fixture" add yarn.lock
if ! git -C "$fixture" diff --cached --quiet; then git -C "$fixture" commit -q -m 'refresh fixture lockfile' --no-gpg-sign; fi
head=$(git -C "$fixture" rev-parse HEAD)
base=$(git -C "$fixture" rev-parse main)
run_id=970001
run_attempt=1
repository=${HISTORY_E2E_REPOSITORY:-XionWCFM/nanoom}
repository_id=${HISTORY_E2E_REPOSITORY_ID:-12345}
branch=${HISTORY_E2E_BRANCH:-feature}
workflow_path=${HISTORY_E2E_WORKFLOW_PATH:-.github/workflows/history-server-e2e.yml}
timing_environment=${HISTORY_E2E_TIMING_ENVIRONMENT:-'runner-labels:["ubuntu-latest"]'}
server="http://127.0.0.1:$(node -e 'const s=require("net").createServer();s.listen(0,"127.0.0.1",()=>{console.log(s.address().port);s.close()})')"
workflow_ref="$repository/$workflow_path@refs/heads/$branch"
token="local-e2e-$(uuidgen | tr '[:upper:]' '[:lower:]')"
token_digest=$(printf '%s' "$token" | shasum -a 256 | awk '{print $1}')

export API=https://api.github.com REPOSITORY="$repository" TOKEN=local-e2e-unused
export GITHUB_REPOSITORY_ID="$repository_id" GITHUB_SERVER_URL=https://github.com GITHUB_REF="refs/heads/$branch"
export GITHUB_EVENT_NAME=push WORKFLOW_REF="$workflow_ref" GITHUB_SHA="$head"
export BASE="$base" HEAD="$head" EVENT=push EVENT_BASE="$base" EVENT_HEAD="$head" REF_NAME="$branch" HISTORY_REF="$branch"
export SCHEDULER=artifact HISTORY_BACKEND=server HISTORY_SERVER_URL=http://127.0.0.1:1 HISTORY_SERVER_TOKEN="$token"
export TIMING_RUNNER=yarn TIMING_ENVIRONMENT="$timing_environment" PACKAGE_MANAGER=yarn
export CONFIG="$fixture/nanoom.config.json" RUN_ATTEMPT="$run_attempt" RUNNER_TEMP="$tmp/runner"
export GITHUB_RUN_ATTEMPT="$run_attempt" GITHUB_STEP_SUMMARY="$tmp/summary"

run_affected() {
  local id=$1 url=$2 out="$tmp/affected-$1.out" log="$tmp/affected-$1.log"
  export RUN_ID="$id" GITHUB_RUN_ID="$id" GITHUB_JOB=affected CWD="$fixture"
  export GITHUB_ACTION_PATH="$root/.github/actions/affected" GITHUB_OUTPUT="$out" HISTORY_SERVER_URL="$url"
  : > "$out"
  if ! bash "$GITHUB_ACTION_PATH/run.sh" >"$log" 2>&1; then cat "$log" >&2; return 1; fi
  AFFECTED_RESULT=$(sed -n 's/^result=//p' "$out")
  AFFECTED_PLAN=$(sed -n 's/^plan=//p' "$out")
  AFFECTED_GROUPS=$(sed -n 's/^groups=//p' "$out")
  AFFECTED_PLAN_DIR=$(sed -n 's/^plan_artifact_path=//p' "$out")
  jq -e '.historyScopes | length > 0' <<<"$AFFECTED_RESULT" >/dev/null || { cat "$log" >&2; echo 'affected produced no History Server scopes' >&2; return 1; }
}

# Get the exact authorized scope set from a cold real Action plan before starting D1.
run_affected "$run_id" http://127.0.0.1:1
jq -e '.historyNeeded == true and .scheduling.historyStatus != "loaded"' <<<"$AFFECTED_RESULT" >/dev/null || {
  cat "$tmp/affected-$run_id.log" >&2
  echo 'fixture did not produce the expected cold history plan' >&2
  exit 1
}
scopes=$(jq -c '.historyScopes | map({repositoryKey,scopeId}) | unique' <<<"$AFFECTED_RESULT")
repository_key=$(jq -er '.[0].repositoryKey' <<<"$scopes")
permissions=$(jq -c --arg key "$repository_key" '[.[] | select(.repositoryKey == $key) | {repositoryKey,scopeId}]' <<<"$scopes")
auth_json=$(jq -cn --arg key "$repository_key" --arg digest "$token_digest" --argjson permissions "$permissions" \
  '{repositories:[{repositoryKey:$key,apiOrigin:"https://github.com",repositoryId:"12345"}],principals:[{id:"local-e2e",tokenSha256:$digest,read:$permissions,write:$permissions}]}')

ln -s "$root/crates/history-worker/build" "$tmp/worker/build"
ln -s "$root/crates/history-worker/migrations" "$tmp/worker/migrations"
cat > "$tmp/worker/wrangler.jsonc" <<'JSON'
{
  "name": "nanoom-history-e2e",
  "main": "build/worker/shim.mjs",
  "compatibility_date": "2026-09-24",
  "d1_databases": [{
    "binding": "PREDICTION_STATE",
    "database_name": "nanoom-history-e2e",
    "database_id": "11111111-1111-4111-8111-111111111111",
    "migrations_dir": "migrations"
  }]
}
JSON
printf "NANOOM_AUTH_JSON='%s'\n" "$auth_json" > "$tmp/worker/.dev.vars"
if ! printf 'y\n' | wrangler d1 migrations apply nanoom-history-e2e --local --persist-to "$tmp/d1" --config "$tmp/worker/wrangler.jsonc" >"$tmp/wrangler-migrate.log" 2>&1; then
  cat "$tmp/wrangler-migrate.log" >&2
  exit 1
fi
port=${server##*:}
wrangler dev --config "$tmp/worker/wrangler.jsonc" --ip 127.0.0.1 --port "$port" --persist-to "$tmp/d1" --log-level debug >"$tmp/wrangler-dev.log" 2>&1 &
worker_pid=$!
ready=false
for _ in $(seq 1 40); do
  if [[ "$(curl --silent --max-time 1 -o "$tmp/ready.json" -w '%{http_code}' "$server/ready" || true)" == 200 ]] && jq -e '.status == "ready"' "$tmp/ready.json" >/dev/null; then ready=true; break; fi
  kill -0 "$worker_pid" 2>/dev/null || break
  sleep 0.5
done
if [[ "$ready" != true ]]; then cat "$tmp/wrangler-dev.log" >&2; echo 'local History Worker did not become ready' >&2; exit 1; fi

export HISTORY_SERVER_URL="$server"
run_affected "$run_id" "$server"
jq -e '.historyNeeded == true and .scheduling.historyStatus != "loaded"' <<<"$AFFECTED_RESULT" >/dev/null || {
  cat "$tmp/affected-$run_id.log" >&2
  echo 'empty local D1 did not produce a cold fallback' >&2
  exit 1
}

run_assignments() {
  local entries=$1 limit=$2 id=$3 workspace="$tmp/workspace-$3" index=0 entry selection out cwd sample result
  export RUN_ID="$id" GITHUB_RUN_ID="$id" GITHUB_WORKSPACE="$workspace"
  mkdir -p "$workspace"
  while IFS= read -r entry; do
    (( index < limit )) || break
    selection="$tmp/select-$id-$index"
    mkdir -p "$selection"
    cp "$AFFECTED_PLAN_DIR/plan-v1.json" "$selection/plan-v1.json"
    cp "$AFFECTED_PLAN_DIR/plan-reference.json" "$selection/plan-reference.json"
    export GITHUB_ACTION_PATH="$root/.github/actions/prepare" PLAN_DIR="$selection" PLAN="$AFFECTED_PLAN"
    export GROUP=ci ASSIGNMENT_ID="$(jq -er .assignmentId <<<"$entry")" MATRIX_INDEX="$index" GITHUB_JOB=run
    export GITHUB_OUTPUT="$tmp/prepare-$id-$index.out"
    : > "$GITHUB_OUTPUT"
    bash "$GITHUB_ACTION_PATH/select.sh" >"$tmp/prepare-$id-$index.log" 2>&1 || { cat "$tmp/prepare-$id-$index.log" >&2; return 1; }
    cwd=$(sed -n 's/^cwd=//p' "$GITHUB_OUTPUT")
    mkdir -p "$(dirname "$cwd")"
    git clone -q --no-checkout "$fixture" "$cwd"
    git -C "$cwd" checkout -q "$head"
    if ! (cd "$cwd" && node .yarn/releases/yarn-4.11.0.cjs install --immutable) >"$tmp/yarn-install-$id-$index.log" 2>&1; then
      cat "$tmp/yarn-install-$id-$index.log" >&2
      return 1
    fi
    export GITHUB_ACTION_PATH="$root/.github/actions/run" CWD="$cwd"
    export ASSIGNMENT_FILE="$selection/selected/assignment.json" NANOOM_YARN_PATH="$cwd/.yarn/releases/yarn-4.11.0.cjs"
    export TOOL=auto PM=yarn GITHUB_OUTPUT="$tmp/run-$id-$index.out" GITHUB_STEP_SUMMARY="$tmp/run-$id-$index.summary"
    : > "$GITHUB_OUTPUT"
    if ! bash "$GITHUB_ACTION_PATH/run.sh" >"$tmp/run-$id-$index.log" 2>&1; then cat "$tmp/run-$id-$index.log" >&2; return 1; fi
    result=$(sed -n 's/^result=//p' "$GITHUB_OUTPUT")
    jq -e '.status == "success" and .executedItemCount > 0' <<<"$result" >/dev/null || { cat "$tmp/run-$id-$index.log" >&2; return 1; }
    sample=$(sed -n 's/^sample-path=//p' "$GITHUB_OUTPUT")
    jq -e '.version == 3 and (.observations | length > 0)' "$sample" >/dev/null || { echo 'run Action emitted no v3 measurements' >&2; return 1; }
    if [[ "$id" == "$run_id" ]]; then cp "$sample" "$tmp/measurements/$(basename "$sample")"; fi
    index=$((index + 1))
  done <<<"$entries"
  (( index > 0 )) || { echo "no runnable assignments for run $id" >&2; return 1; }
  RUN_ASSIGNMENT_COUNT=$index
}

cold_entries=$(jq -c '.ci.include[]?' <<<"$AFFECTED_GROUPS")
cold_assignment_count=$(jq -s length <<<"$cold_entries")
run_assignments "$cold_entries" "$cold_assignment_count" "$run_id"
cold_run_assignment_count=$RUN_ASSIGNMENT_COUNT
test "$(find "$tmp/measurements" -type f -name '*.json' | wc -l | tr -d ' ')" -eq "$RUN_ASSIGNMENT_COUNT"

run_history() {
  local id=$1 out="$tmp/history-$1.out" log="$tmp/history-$1.log"
  export RUN_ID="$id" GITHUB_RUN_ID="$id" GITHUB_JOB=history
  export GITHUB_ACTION_PATH="$root/.github/actions/history" GITHUB_OUTPUT="$out"
  export MEASUREMENT_DIR="$tmp/measurements" MEASUREMENT_DOWNLOAD_OUTCOME=success
  export RUN_IDS='[]' MODEL_ARTIFACT=nanoom-model-v3 HISTORY_ARTIFACT=nanoom-prediction-v3
  : > "$out"
  if ! bash "$GITHUB_ACTION_PATH/run.sh" >"$log" 2>&1; then cat "$log" >&2; return 1; fi
  HISTORY_RESULT=$(sed -n 's/^result=//p' "$out")
}

assert_snapshots() {
  local destination=$1 scope_id status
  mkdir -p "$destination"
  while IFS= read -r scope_id; do
    status=$(curl --silent --show-error --max-time 3 -H "Authorization: Bearer $token" -H 'Accept: application/json' \
      -o "$destination/$scope_id.json" -w '%{http_code}' "$server/v1/repositories/$repository_key/scopes/$scope_id/snapshot")
    [[ "$status" == 200 ]] || { echo "snapshot for $scope_id returned HTTP $status" >&2; return 1; }
    jq -e '.version == 3 and (.rows | length > 0) and any(.rows[]; .[2] > 0)' "$destination/$scope_id.json" >/dev/null || {
      echo "snapshot for $scope_id has no prediction samples" >&2
      return 1
    }
  done < <(jq -r '.[].scopeId' <<<"$scopes")
}

run_history "$run_id"
jq -e '.status == "success" and .serverMerge.status == "merged" and .serverMerge.appliedBatchCount > 0' <<<"$HISTORY_RESULT" >/dev/null || {
  cat "$tmp/history-$run_id.log" >&2
  echo 'History Action did not apply any batches' >&2
  exit 1
}
assert_snapshots "$tmp/snapshots-before"
run_history "$run_id"
jq -e '.status == "success" and .serverMerge.appliedBatchCount == 0 and .serverMerge.duplicateBatchCount > 0' <<<"$HISTORY_RESULT" >/dev/null || {
  cat "$tmp/history-$run_id.log" >&2
  echo 'reposting the same batch was not reported as a duplicate' >&2
  exit 1
}
assert_snapshots "$tmp/snapshots-after"
for before in "$tmp"/snapshots-before/*.json; do cmp -s "$before" "$tmp/snapshots-after/$(basename "$before")" || { echo 'duplicate retry changed the stored PredictionTable' >&2; exit 1; }; done

warm_run_id=970002
run_affected "$warm_run_id" "$server"
jq -e '.scheduling.historyStatus == "loaded"' <<<"$AFFECTED_RESULT" >/dev/null &&
  jq -e '([.ci.include[]? | (.predictionSources.sampleCount // 0)] | add // 0) > 0' <<<"$AFFECTED_GROUPS" >/dev/null || {
  cat "$tmp/affected-$warm_run_id.log" >&2
  echo 'warm affected did not consume samples from the History Server' >&2
  exit 1
}
warm_entries=$(jq -c '[.ci.include[]? | select((.predictionSources.sampleCount // 0) > 0)] | .[:1][]' <<<"$AFFECTED_GROUPS")
run_assignments "$warm_entries" 1 "$warm_run_id"

printf 'History Server E2E passed: cold fallback, %s actual run assignments, applied D1 merge, duplicate no-op, %s warm samples, and warm run.\n' \
  "$cold_run_assignment_count" "$(jq '[.rows[] | .[2]] | add // 0' "$tmp"/snapshots-before/*.json | jq -s 'add')"
printf 'History Server scope IDs: %s\n' "$(jq -r '[.[].scopeId] | join(",")' <<<"$scopes")"
