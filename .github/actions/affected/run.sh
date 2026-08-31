#!/usr/bin/env bash
set -Eeuo pipefail
ACTION_NAME=affected ACTION_CWD=$CWD ACTION_PHASE=input-resolution ACTION_COMMAND=not-started
source "$GITHUB_ACTION_PATH/../_setup/log.sh"; trap 'nanoom_fail "$?"' ERR
started=$(date +%s)
[[ "$SCHEDULER" =~ ^(off|artifact|http)$ ]] || { echo "scheduler must be off, artifact, or http" >&2; false; }
[[ -n "$BASE" ]] || BASE=$EVENT_BASE; [[ -n "$HEAD" ]] || HEAD=$EVENT_HEAD
[[ -n "$BASE" ]] || { echo 'affected could not resolve a base revision from inputs or the GitHub event' >&2; false; }; [[ -n "$HEAD" ]] || HEAD=HEAD
if ! git -C "$CWD" rev-parse --verify "$BASE^{commit}" >/dev/null 2>&1; then
  ACTION_PHASE=git-fetch
  if [[ "$EVENT" == pull_request && "$BASE" != refs/* ]]; then git -C "$CWD" fetch --no-tags --depth=100 origin "$BASE:refs/remotes/origin/$BASE"; BASE="refs/remotes/origin/$BASE"; else git -C "$CWD" fetch --no-tags --depth=100 origin "$BASE"; BASE=FETCH_HEAD; fi
fi

history_status=disabled; history_download_ms=0; history_path="$RUNNER_TEMP/nanoom-history.json"
if [[ "$SCHEDULER" == artifact ]]; then
  history_started=$(date +%s); history_status=fallback
  archive="$RUNNER_TEMP/nanoom-history.zip"
  set +e
  artifacts=$(curl --fail --silent --show-error -H "Authorization: Bearer $TOKEN" -H 'Accept: application/vnd.github+json' "$API/repos/$REPOSITORY/actions/artifacts?name=$HISTORY_ARTIFACT&per_page=1")
  archive_url=$(jq -r '.artifacts | map(select(.expired | not)) | first | .archive_download_url // empty' <<<"$artifacts" 2>/dev/null)
  [[ -n "$archive_url" ]] && curl --fail --silent --show-error -L -H "Authorization: Bearer $TOKEN" -H 'Accept: application/vnd.github+json' "$archive_url" -o "$archive" && unzip -p "$archive" history.json > "$history_path" && jq -e '.samples | type == "array"' "$history_path" >/dev/null
  history_ok=$?
  set -e
  if (( history_ok == 0 )); then history_status=loaded; else rm -f "$history_path"; echo 'timing history unavailable; using deterministic equal-weight scheduling' >&2; fi
  history_download_ms=$(( ($(date +%s) - history_started) * 1000 ))
fi

args=(-C "$CWD" -c "$CONFIG" affected --json --base "$BASE" --head "$HEAD" --timing-runner "$TIMING_RUNNER" --timing-environment "$TIMING_ENVIRONMENT")
[[ -f "$history_path" ]] && args+=(--history "$history_path")
printf -v ACTION_COMMAND '%q ' nanoom "${args[@]}"; ACTION_COMMAND=${ACTION_COMMAND% }
printf '◆ nanoom affected\n  Inputs\n    cwd: %s\n    config: %s\n    scheduler: %s\n    timing environment: %s\n  Command\n    %s\n' "$CWD" "$CONFIG" "$SCHEDULER" "$TIMING_ENVIRONMENT" "$ACTION_COMMAND"
ACTION_PHASE=affected-calculation; report=$(nanoom "${args[@]}")
report=$(jq -c --arg historyStatus "$history_status" --argjson downloadMs "$history_download_ms" '.scheduling.historyStatus=$historyStatus | .scheduling.historyDownloadMs=$downloadMs | .scheduling.reason=(if $historyStatus == "loaded" then "recent successful samples loaded" elif $historyStatus == "disabled" then "telemetry disabled; deterministic equal weights" else "history unavailable; deterministic equal weights" end)' <<<"$report")
matrix=$(jq -c .matrix <<<"$report")

if [[ "$SCHEDULER" == http ]]; then
  [[ "$COORDINATOR_URL" == https://* && -n "$COORDINATOR_TOKEN" ]] || { echo 'scheduler=http requires an HTTPS coordinatorUrl and NANOOM_COORDINATOR_TOKEN' >&2; false; }
  while IFS= read -r group; do
    distribution=$(jq -c --arg group "$group" '.affected.group[$group].distribution // empty' <<<"$report"); [[ -n "$distribution" ]] || continue
    items=$(jq -c --arg group "$group" '.affected.group[$group].workspaces' <<<"$report"); item_count=$(jq length <<<"$items"); (( item_count > 0 )) || continue
    concurrency=$(jq -r .concurrency <<<"$distribution"); (( concurrency > item_count )) && concurrency=$item_count
    body=$(jq -cn --arg repository "$REPOSITORY" --arg run "$RUN_ID.$RUN_ATTEMPT" --arg group "$group" --arg environment "$TIMING_ENVIRONMENT" --argjson workItems "$items" --argjson tier "$distribution" --argjson concurrency "$concurrency" '{repository:$repository,run:$run,group:$group,workItems:$workItems,tier:$tier,concurrency:$concurrency,environment:$environment}')
    response=$(curl --fail-with-body --silent --show-error -X POST -H "Authorization: Bearer $COORDINATOR_TOKEN" -H 'Content-Type: application/json' -H "Idempotency-Key: $REPOSITORY:$RUN_ID:$RUN_ATTEMPT:$group" "$COORDINATOR_URL/v1/runs" --data "$body")
    run_id=$(jq -er .runId <<<"$response"); agents=$(jq -cn --arg runId "$run_id" --argjson count "$concurrency" '[range(1; $count + 1) | {agentId:("agent-" + tostring),runId:$runId,mode:"continuous"}]')
    matrix=$(jq -c --arg group "$group" --argjson agents "$agents" '.[$group].include=$agents' <<<"$matrix")
  done < <(jq -r '.affected.group | keys[]' <<<"$report")
  report=$(jq -c --argjson matrix "$matrix" '.matrix=$matrix' <<<"$report")
fi

compact_matrix=$(jq -c 'with_entries(.value.include |= map(if .items then {assignmentId,predictedDurationMs,reason,items:[.items[] | {group,name,task,shard,totalShards} | with_entries(select(.value != null))]} else {name,task,shard,totalShards} | with_entries(select(.value != null)) end))' <<<"$matrix")
groups=$(jq -c 'with_entries(.value = {hasChange:((.value.include|length)>0),matrix:.value})' <<<"$compact_matrix"); has=$(jq -r 'any(to_entries[]; .value.include | length > 0)' <<<"$compact_matrix")
result=$(jq -c --argjson groups "$groups" '. + {groups:($groups | with_entries(.value |= {hasChange,assignmentCount:(.matrix.include|length)}))}' <<<"$report")
output_bytes=$(printf 'has_change=%s\ngroups=%s\nresult=%s\n' "$has" "$groups" "$result" | iconv -f UTF-8 -t UTF-16LE | wc -c | tr -d ' ')
(( output_bytes <= 1048576 )) || { echo "Action outputs exceed GitHub's 1 MiB UTF-16 limit: $output_bytes bytes" >&2; false; }
echo "has_change=$has" >> "$GITHUB_OUTPUT"; echo "groups=$groups" >> "$GITHUB_OUTPUT"; echo "result=$result" >> "$GITHUB_OUTPUT"
assignments=$(jq '[to_entries[].value.include[]] | length' <<<"$matrix"); items=$(jq '[.affected.group[].workspaces[]] | length' <<<"$report"); elapsed=$(( $(date +%s) - started ))
printf '  Result\n    ✓ affected work items=%s; assignments=%s; history=%s; elapsed=%ss\n  Final JSON\n    %s\n' "$items" "$assignments" "$history_status" "$elapsed" "$result"
{ echo '### nanoom affected'; echo; echo "**Result:** $items work items in $assignments assignments; history \`$history_status\`."; echo; echo '| Group | Total | Affected | Percent | Tier | Concurrency |'; echo '|---|---:|---:|---:|---|---:|'; jq -r '.affected.group | to_entries[] | "| \(.key) | \(.value.totalWorkspaces) | \(.value.affectedWorkspaces) | \(.value.affectedPercent) | \(.value.distribution.name // "legacy") | \(.value.distribution.concurrency // (.value.workspaces|length)) |"' <<<"$report"; } >> "$GITHUB_STEP_SUMMARY"
