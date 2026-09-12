#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/history" "$tmp/sample-a" "$tmp/sample-b"
printf '%s\n' '{"samples":[]}' > "$tmp/history/history.json"
printf '%s\n' '{"samples":[{"durationMs":1}]}' > "$tmp/sample-a/a.json"
printf '%s\n' '{"samples":[{"durationMs":2}]}' > "$tmp/sample-b/b.json"
(cd "$tmp/history" && zip -q "$tmp/history.zip" history.json)
(cd "$tmp/sample-a" && zip -q "$tmp/sample-a.zip" a.json)
(cd "$tmp/sample-b" && zip -q "$tmp/sample-b.zip" b.json)

cat > "$tmp/bin/curl" <<'SH'
#!/usr/bin/env bash
output=''
url=''
while (($#)); do
  if [[ $1 == -o ]]; then output=$2; shift 2
  elif [[ $1 == http* ]]; then url=$1; shift
  else shift
  fi
done
case "$url" in
  */actions/workflows/*/runs*)
    printf '%s\n' '{"workflow_runs":[{"id":99,"status":"in_progress","conclusion":null,"created_at":"2026-09-12T02:00:00Z"},{"id":87,"status":"completed","conclusion":"failure","created_at":"2026-09-12T01:00:00Z"},{"id":88,"status":"completed","conclusion":"success","created_at":"2026-09-12T00:00:00Z"}]}'
    ;;
  */actions/runs/88/artifacts*)
    if [[ ${FAKE_MISSING_HISTORY:-0} == 1 ]]; then
      printf '%s\n' '{"artifacts":[{"id":2,"name":"nanoom-timing-sample-v2-88-1-old","expired":false}]}'
    else
      printf '%s\n' '{"artifacts":[{"id":1,"name":"nanoom-timing-history","expired":false},{"id":2,"name":"nanoom-timing-sample-v2-88-1-old","expired":true}]}'
    fi
    ;;
  */actions/runs/99/artifacts*)
    if [[ ${FAKE_PAGINATED:-0} == 1 && $url == *'page=1' ]]; then
      jq -cn '{artifacts:[range(100) | {id:.,name:("artifact-" + tostring),expired:false}]}'
    elif [[ ${FAKE_PAGINATED:-0} == 1 ]]; then
      printf '%s\n' '{"artifacts":[{"id":100,"name":"artifact-100","expired":false}]}'
    else
      printf '%s\n' '{"artifacts":[{"id":2,"name":"nanoom-timing-sample-v2-99-1-a","expired":false},{"id":3,"name":"nanoom-timing-sample-v2-99-1-b","expired":false},{"id":4,"name":"other","expired":false}]}'
    fi
    ;;
  */actions/artifacts/1/zip) cp "$FAKE_HISTORY_ZIP" "$output" ;;
  */actions/artifacts/2/zip) cp "$FAKE_SAMPLE_A_ZIP" "$output" ;;
  */actions/artifacts/3/zip) cp "$FAKE_SAMPLE_B_ZIP" "$output" ;;
  *) echo "unexpected curl URL: $url" >&2; exit 22 ;;
esac
SH
chmod +x "$tmp/bin/curl"

export PATH="$tmp/bin:$PATH" API=https://api.example REPOSITORY=owner/repo TOKEN=token RUNNER_TEMP="$tmp"
export FAKE_HISTORY_ZIP="$tmp/history.zip" FAKE_SAMPLE_A_ZIP="$tmp/sample-a.zip" FAKE_SAMPLE_B_ZIP="$tmp/sample-b.zip"
source "$root/.github/actions/_setup/artifacts.sh"

run_id=$(nanoom_previous_successful_run owner/repo/.github/workflows/ci.yml@refs/heads/main main 99)
test "$run_id" = 88
previous=$(nanoom_run_artifacts 88)
nanoom_artifact_exists "$previous" exact nanoom-timing-history
! nanoom_artifact_exists "$previous" prefix nanoom-timing-sample-v2-88-
nanoom_download_artifacts "$previous" exact nanoom-timing-history "$tmp/download-history"
jq -e '.samples == []' "$tmp/download-history/history.json" >/dev/null

current=$(nanoom_run_artifacts 99)
nanoom_download_artifacts "$current" prefix nanoom-timing-sample-v2-99-1- "$tmp/download-samples"
test -f "$tmp/download-samples/a.json"
test -f "$tmp/download-samples/b.json"
! nanoom_download_artifacts "$current" prefix nanoom-timing-sample-v2-99-1- "$tmp/download-samples"

export FAKE_PAGINATED=1
test "$(nanoom_run_artifacts 99 | jq '.artifacts | length')" = 101
unset FAKE_PAGINATED

export FAKE_MISSING_HISTORY=1
commit=$(git -C "$root" rev-parse HEAD)
if ACTION_CWD="$root" GITHUB_ACTION_PATH="$root/.github/actions/affected" GITHUB_OUTPUT="$tmp/output" GITHUB_STEP_SUMMARY="$tmp/summary" \
  CWD="$root" CONFIG=nanoom.config.json BASE="$commit" HEAD="$commit" EVENT=pull_request EVENT_BASE="$commit" EVENT_HEAD="$commit" \
  REF_NAME=main HISTORY_REF=main WORKFLOW_REF=owner/repo/.github/workflows/ci.yml@refs/heads/main SCHEDULER=artifact \
  TIMING_RUNNER=auto TIMING_ENVIRONMENT=linux-x64 COORDINATOR_URL='' COORDINATOR_TOKEN='' RUN_ID=99 RUN_ATTEMPT=1 \
  HISTORY_ARTIFACT=nanoom-timing-history bash "$root/.github/actions/affected/run.sh" >"$tmp/missing.log" 2>&1; then
  echo 'affected unexpectedly accepted samples without merged history' >&2
  exit 1
fi
grep -q 'add the standard nanoom history job' "$tmp/missing.log"

echo 'history artifact contract passed'
