#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/repo"

git -C "$tmp/repo" init -q -b main
git -C "$tmp/repo" config user.email fixture@example.invalid
git -C "$tmp/repo" config user.name fixture
printf 'base\n' > "$tmp/repo/file"
git -C "$tmp/repo" add file
git -C "$tmp/repo" commit -q -m base
base=$(git -C "$tmp/repo" rev-parse HEAD)
printf 'head\n' >> "$tmp/repo/file"
git -C "$tmp/repo" commit -q -am head
head=$(git -C "$tmp/repo" rev-parse HEAD)

printf '%s\n' '#!/usr/bin/env bash' 'printf "%s\\n" "$*" >> "$FAKE_NANOOM_ARGS"' 'printf "%s\\n" '\''{"affected":{"has_change":false,"group":{"ci":{"label":"ci","workspaces":[],"totalWorkspaces":1,"affectedWorkspaces":0,"affectedPercent":0.0}},"diagnostics":{"changedFiles":[],"comparison":{"baseCommit":"BASE_PLACEHOLDER","headCommit":"HEAD_PLACEHOLDER","mode":"merge-base","requestedBase":"BASE_PLACEHOLDER","requestedHead":"HEAD_PLACEHOLDER"},"reasons":{}}},"matrix":{"ci":{"include":[]}},"scheduling":{"historyStatus":"disabled","timingRunner":"yarn"}}'\''' > "$tmp/bin/nanoom"
printf '%s\n' '#!/usr/bin/env bash' 'printf "%s\\n" "$*" >> "$FAKE_CURL_LOG"' 'if [[ "${FAKE_CURL_FAIL:-}" == 1 ]]; then exit 22; fi' 'printf "%s\\n" "${FAKE_CURL_RESPONSE:?FAKE_CURL_RESPONSE must be set}"' > "$tmp/bin/curl"
chmod +x "$tmp/bin/nanoom" "$tmp/bin/curl"

run_action() {
  local output=$1 summary=$2 event=${3:-push} event_base=${4:-}
  : > "$output"; : > "$summary"; : > "$FAKE_CURL_LOG"; : > "$FAKE_NANOOM_ARGS"
  env PATH="$tmp/bin:$PATH" GITHUB_ACTION_PATH="$root/.github/actions/affected" GITHUB_OUTPUT="$output" GITHUB_STEP_SUMMARY="$summary" RUNNER_TEMP="$tmp" \
    CWD="$tmp/repo" CONFIG=nanoom.config.json EVENT="$event" EVENT_BASE="$event_base" EVENT_HEAD="$head" REF_NAME=main \
    WORKFLOW_REF=owner/repo/.github/workflows/ci.yml@refs/heads/main API=https://api.example REPOSITORY=owner/repo \
    TOKEN=token RUN_ID=99 RUN_ATTEMPT=1 SCHEDULER=off TIMING_RUNNER=yarn TIMING_ENVIRONMENT=linux-x64 \
    BASE="${BASE_OVERRIDE:-}" HEAD="$head" HISTORY_ARTIFACT=unused COORDINATOR_URL= COORDINATOR_TOKEN= \
    FAKE_CURL_LOG="$FAKE_CURL_LOG" FAKE_NANOOM_ARGS="$FAKE_NANOOM_ARGS" FAKE_CURL_RESPONSE="$FAKE_CURL_RESPONSE" \
    bash "$root/.github/actions/affected/run.sh"
}

export FAKE_CURL_LOG="$tmp/curl.log" FAKE_NANOOM_ARGS="$tmp/nanoom.log"
export FAKE_CURL_RESPONSE="{\"workflow_runs\":[{\"id\":87,\"head_sha\":\"$head\",\"conclusion\":\"success\",\"event\":\"push\",\"created_at\":\"2026-09-01T00:00:00Z\"},{\"id\":88,\"head_sha\":\"$base\",\"conclusion\":\"success\",\"event\":\"push\",\"created_at\":\"2026-09-01T00:00:00Z\"},{\"id\":99,\"head_sha\":\"$head\",\"conclusion\":\"success\",\"event\":\"push\",\"created_at\":\"2026-09-01T01:00:00Z\"}]}"
run_action "$tmp/output" "$tmp/summary" >/dev/null
jq -e --arg base "$base" --arg head "$head" '.revisionResolution.baseSource == "lastSuccessfulPush" and .revisionResolution.baseCommit == $base and .revisionResolution.headCommit == $head and .revisionResolution.successfulRunId == 88' <(sed -n 's/^result=//p' "$tmp/output") >/dev/null
grep -q -- "--base $base --head $head" "$tmp/nanoom.log"
grep -q "lastSuccessfulPush" "$tmp/summary"
grep -q "actions/workflows/.github%2Fworkflows%2Fci.yml/runs" "$tmp/curl.log"

export BASE_OVERRIDE="$base"
run_action "$tmp/explicit-output" "$tmp/explicit-summary" >/dev/null
test ! -s "$tmp/curl.log"
jq -e '.revisionResolution.baseSource == "explicit" and .revisionResolution.successfulRunId == null' <(sed -n 's/^result=//p' "$tmp/explicit-output") >/dev/null
unset BASE_OVERRIDE

export FAKE_CURL_RESPONSE='{"workflow_runs":[]}'
run_action "$tmp/pr-output" "$tmp/pr-summary" pull_request main >/dev/null
test ! -s "$tmp/curl.log"
jq -e '.revisionResolution.baseSource == "pullRequestBase" and .revisionResolution.successfulRunId == null' <(sed -n 's/^result=//p' "$tmp/pr-output") >/dev/null

run_action "$tmp/merge-output" "$tmp/merge-summary" merge_group main >/dev/null
test ! -s "$tmp/curl.log"
jq -e '.revisionResolution.baseSource == "mergeGroupBase" and .revisionResolution.successfulRunId == null' <(sed -n 's/^result=//p' "$tmp/merge-output") >/dev/null

export FAKE_CURL_RESPONSE='{"workflow_runs":[]}'
if run_action "$tmp/missing-output" "$tmp/missing-summary" >/dev/null 2>&1; then
  echo 'missing successful push unexpectedly succeeded' >&2
  exit 1
fi
test ! -s "$tmp/nanoom.log"

export FAKE_CURL_RESPONSE='not-json'
if run_action "$tmp/malformed-output" "$tmp/malformed-summary" >/dev/null 2>&1; then
  echo 'malformed API response unexpectedly succeeded' >&2
  exit 1
fi
test ! -s "$tmp/nanoom.log"

export FAKE_CURL_FAIL=1
if run_action "$tmp/api-error-output" "$tmp/api-error-summary" >/dev/null 2>&1; then
  echo 'API error unexpectedly succeeded' >&2
  exit 1
fi
test ! -s "$tmp/nanoom.log"
unset FAKE_CURL_FAIL

unrelated=$(printf 'unrelated\n' | git -C "$tmp/repo" commit-tree "$(git -C "$tmp/repo" rev-parse HEAD^{tree})")
export FAKE_CURL_RESPONSE="{\"workflow_runs\":[{\"id\":87,\"head_sha\":\"$unrelated\",\"conclusion\":\"success\",\"event\":\"push\",\"created_at\":\"2026-09-01T00:00:00Z\"}]}"
if run_action "$tmp/nonancestor-output" "$tmp/nonancestor-summary" >/dev/null 2>&1; then
  echo 'non-ancestor successful push unexpectedly succeeded' >&2
  exit 1
fi
grep -q -- "--base $unrelated --head $head" "$tmp/nanoom.log"

echo 'revision action contract passed'
