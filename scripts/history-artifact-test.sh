#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/prediction" "$tmp/model" "$tmp/runner"
cat > "$tmp/prediction/prediction-v3.json" <<'JSON'
{"version":3,"predictions":[{"table":{"version":3,"scope":{"repositoryKey":"github-12345","workflowPath":".github/workflows/ci.yml","ref":{"kind":"push","ref":"refs/heads/main"},"group":"ci","taskRunner":"yarn","timingEnvironment":"linux-x64-node24"},"modelUpdatedAtMs":1790208000000,"rows":[]},"modelArtifact":{"name":"nanoom-model-v3-88-1","sha256":"ada20f873e74812b9e056d9134c73ae06102c808739d5fd1c73f0b40e800302b"}}]}
JSON
printf '%s\n' '{"version":3,"states":[]}' > "$tmp/model/model-v3.json"
(cd "$tmp/prediction" && zip -q "$tmp/prediction.zip" prediction-v3.json)
(cd "$tmp/model" && zip -q "$tmp/model.zip" model-v3.json)
prediction_size=$(wc -c < "$tmp/prediction.zip" | tr -d ' ')
model_size=$(wc -c < "$tmp/model.zip" | tr -d ' ')
jq -cn --argjson prediction "$prediction_size" --argjson model "$model_size" \
  '{artifacts:[{id:101,name:"nanoom-prediction-v3",expired:false,size_in_bytes:$prediction},{id:102,name:"nanoom-model-v3-88-1",expired:false,size_in_bytes:$model},{id:103,name:"nanoom-measurement-v3-88-1-ci-1",expired:false,size_in_bytes:100}]}' > "$tmp/artifacts.json"
cat > "$tmp/bin/curl" <<'SH'
#!/usr/bin/env bash
output=''
url=''
while (($#)); do
  case "$1" in
    -o) output=$2; shift 2 ;;
    http*) url=$1; shift ;;
    *) shift ;;
  esac
done
printf '%s\n' "$url" >> "$FAKE_REQUESTS"
case "$url" in
  *'/workflows/'*'event=pull_request'*)
    printf '%s\n' '{"workflow_runs":[{"id":79,"status":"completed","conclusion":"success","created_at":"2026-09-23T01:00:00Z","head_branch":"other","head_repository":{"id":222},"pull_requests":[{"number":42}]},{"id":77,"status":"completed","conclusion":"success","created_at":"2026-09-23T02:00:00Z","head_branch":"feature","head_repository":{"id":222},"pull_requests":[{"number":42}]},{"id":76,"status":"completed","conclusion":"success","created_at":"2026-09-23T03:00:00Z","head_branch":"feature","head_repository":{"id":222},"pull_requests":[{"number":43}]}]}'
    ;;
  *'/workflows/'*'event=push'*)
    printf '%s\n' '{"workflow_runs":[{"id":80,"status":"completed","conclusion":"success","created_at":"2026-08-01T00:00:00Z"},{"id":87,"status":"completed","conclusion":"failure","created_at":"2026-09-23T01:00:00Z"},{"id":88,"status":"completed","conclusion":"success","created_at":"2026-09-23T02:00:00Z"},{"id":99,"status":"completed","conclusion":"success","created_at":"2026-09-24T00:00:00Z"}]}'
    ;;
  *'/actions/runs/88/artifacts'*) cat "$FAKE_ARTIFACTS" ;;
  *'/actions/artifacts/101/zip'*) cp "$FAKE_PREDICTION_ZIP" "$output" ;;
  *'/actions/artifacts/102/zip'*) cp "$FAKE_MODEL_ZIP" "$output" ;;
  *) echo "unexpected curl URL: $url" >&2; exit 22 ;;
esac
SH
chmod +x "$tmp/bin/curl"
export PATH="$tmp/bin:$root/target/debug:$PATH" API=https://api.example REPOSITORY=owner/repo TOKEN=test-token RUNNER_TEMP="$tmp/runner"
export FAKE_REQUESTS="$tmp/requests" FAKE_ARTIFACTS="$tmp/artifacts.json"
export FAKE_PREDICTION_ZIP="$tmp/prediction.zip" FAKE_MODEL_ZIP="$tmp/model.zip"
source "$root/.github/actions/_setup/artifacts.sh"

identity=$(nanoom_prediction_identity push owner/repo/.github/workflows/ci.yml@refs/heads/main 12345 https://github.com refs/heads/main)
jq -e '.repositoryKey == "github-12345" and .workflowPath == ".github/workflows/ci.yml" and .ref.ref == "refs/heads/main"' <<<"$identity" >/dev/null
nanoom_history_budget_start 10 8388608
push_run=$(nanoom_previous_successful_run_for_event owner/repo/.github/workflows/ci.yml@refs/heads/main main 99 push)
test "$push_run" = 88
pr_run=$(nanoom_previous_successful_run_for_event owner/repo/.github/workflows/ci.yml@refs/heads/feature feature 99 pull_request 42 222)
test "$pr_run" = 77
test -z "$(nanoom_previous_successful_run_for_event owner/repo/.github/workflows/ci.yml@refs/heads/feature feature 99 pull_request 44 222)"

# Planning reads the compact prediction marker only.
: > "$FAKE_REQUESTS"
artifacts=$(nanoom_run_artifacts 88)
nanoom_history_budget_start 3 8388608
nanoom_download_prediction_artifact "$artifacts" nanoom-prediction-v3 "$tmp/planner"
test -s "$tmp/planner/prediction-v3.json"
test "$(jq -r '.predictions | length' "$tmp/planner/prediction-v3.json")" -eq 1
test "$(grep -c '/actions/artifacts/.*/zip' "$FAKE_REQUESTS")" -eq 1
grep -q '/actions/artifacts/101/zip' "$FAKE_REQUESTS"
! grep -q '/actions/artifacts/102/zip\|/actions/artifacts/103/zip' "$FAKE_REQUESTS"
test "$NANOOM_HISTORY_BYTES" -le 8388608

# Only the updater follows the model pointer.
nanoom_history_budget_start 60 25165824
nanoom_download_artifact_bounded "$artifacts" nanoom-model-v3-88-1 "$tmp/updater/model" 16777216 16777216
test -s "$tmp/updater/model/model-v3.json"

# Oversized metadata is rejected before downloading its archive.
big_artifacts='{"artifacts":[{"id":104,"name":"nanoom-prediction-v3","expired":false,"size_in_bytes":4194305}]}'
: > "$FAKE_REQUESTS"
nanoom_history_budget_start 3 8388608
if nanoom_download_prediction_artifact "$big_artifacts" nanoom-prediction-v3 "$tmp/oversized"; then
  echo 'oversized PredictionArtifact metadata unexpectedly passed' >&2
  exit 1
fi
! grep -q '/actions/artifacts/104/zip' "$FAKE_REQUESTS"

# Metadata and archives share one byte cap, and parsing/decompression commands share the deadline.
nanoom_history_budget_start 2 10
nanoom_history_charge_bytes 7
if nanoom_history_charge_bytes 4; then
  echo 'combined history byte cap unexpectedly accepted an oversized response' >&2
  exit 1
fi
nanoom_history_budget_start 1 8388608
started_ms=$(nanoom_now_ms)
if nanoom_history_timeout sleep 2; then
  echo 'history command unexpectedly outlived its shared deadline' >&2
  exit 1
fi
elapsed_ms=$(($(nanoom_now_ms) - started_ms))
(( elapsed_ms < 1500 )) || { echo "history command cancellation was too slow: ${elapsed_ms}ms" >&2; exit 1; }

# A no-change plan avoids even the metadata call.
: > "$FAKE_REQUESTS"
head=$(git -C "$root" rev-parse HEAD)
export ACTION_NAME=affected ACTION_CWD="$root" GITHUB_ACTION_PATH="$root/.github/actions/affected"
export CWD="$root" CONFIG=nanoom.config.json BASE="$head" HEAD="$head" EVENT=push EVENT_BASE="$head" EVENT_HEAD="$head"
export REF_NAME=main HISTORY_REF=main WORKFLOW_REF=owner/repo/.github/workflows/ci.yml@refs/heads/main
export SCHEDULER=artifact TIMING_RUNNER=auto TIMING_ENVIRONMENT=linux-x64-node24 COORDINATOR_URL='' COORDINATOR_TOKEN=''
export GITHUB_REPOSITORY_ID=12345 GITHUB_SERVER_URL=https://github.com GITHUB_REF=refs/heads/main GITHUB_EVENT_NAME=push
export RUN_ID=99 RUN_ATTEMPT=1 GITHUB_JOB=affected GITHUB_OUTPUT="$tmp/affected-output" GITHUB_STEP_SUMMARY="$tmp/summary"
export HISTORY_ARTIFACT=nanoom-prediction-v3
bash "$GITHUB_ACTION_PATH/run.sh" > "$tmp/affected.log"
affected_result=$(sed -n 's/^result=//p' "$GITHUB_OUTPUT")
jq -e '.scheduling.historyStatus == "history_not_needed" and .scheduling.historyFetchMs == 0' <<<"$affected_result" >/dev/null
test ! -s "$FAKE_REQUESTS"
echo 'v3 history artifact, scope lookup, bounded planner download, updater model read, and no-read contracts passed'
