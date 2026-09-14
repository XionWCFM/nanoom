#!/usr/bin/env bash
set -Eeuo pipefail
ACTION_NAME=affected ACTION_CWD=$CWD ACTION_PHASE=input-resolution ACTION_COMMAND=not-started
source "$GITHUB_ACTION_PATH/../_setup/log.sh"; trap 'nanoom_fail "$?"' ERR
source "$GITHUB_ACTION_PATH/../_setup/artifacts.sh"
started=$(date +%s)
[[ "$SCHEDULER" =~ ^(off|artifact|http)$ ]] || { echo "scheduler must be off, artifact, or http" >&2; false; }
revision_source=explicit; successful_run_id=''
[[ -n "$HEAD" ]] || HEAD=$EVENT_HEAD
if [[ -z "$BASE" ]]; then
  case "$EVENT" in
    push)
      ACTION_PHASE=revision-resolution
      workflow_file=${WORKFLOW_REF#*/}; workflow_file=${workflow_file#*/}; workflow_file=${workflow_file%@*}
      [[ -n "$workflow_file" && -n "$REF_NAME" ]] || {
        echo 'push revision resolution requires github.workflow_ref and github.ref_name; set the base input to bootstrap this run' >&2
        false
      }
      encoded_workflow=$(jq -rn --arg value "$workflow_file" '$value | @uri')
      encoded_branch=$(jq -rn --arg value "$REF_NAME" '$value | @uri')
      runs_url="$API/repos/$REPOSITORY/actions/workflows/$encoded_workflow/runs?branch=$encoded_branch&event=push&status=success&per_page=20"
      set +e
      runs_json=$(curl --fail --silent --show-error -H "Authorization: Bearer $TOKEN" -H 'Accept: application/vnd.github+json' "$runs_url" 2>&1)
      curl_status=$?
      set -e
      (( curl_status == 0 )) || {
        echo "could not query successful push runs for workflow '$workflow_file' on branch '$REF_NAME' (GitHub API exit $curl_status); grant actions: read and contents: read, or set with.base to bootstrap" >&2
        false
      }
      successful_run_id=$(jq -er --arg current "$RUN_ID" '[.workflow_runs[]? | select(.conclusion == "success") | select((.id | tostring) != $current)] | sort_by(.created_at, .id) | .[-1].id // empty' <<<"$runs_json" 2>/dev/null) || {
        echo "GitHub API returned no valid successful push run for workflow '$workflow_file' on branch '$REF_NAME'; set with.base to bootstrap this run" >&2
        false
      }
      BASE=$(jq -er --arg current "$RUN_ID" '[.workflow_runs[]? | select(.conclusion == "success") | select((.id | tostring) != $current)] | sort_by(.created_at, .id) | .[-1].head_sha // empty' <<<"$runs_json" 2>/dev/null) || {
        echo "GitHub API returned no successful push SHA for workflow '$workflow_file' on branch '$REF_NAME'; set with.base to bootstrap this run" >&2
        false
      }
      [[ "$BASE" =~ ^[0-9a-fA-F]{40}$ ]] || { echo "successful workflow run $successful_run_id returned an invalid head SHA '$BASE'; set with.base to bootstrap this run" >&2; false; }
      revision_source=lastSuccessfulPush
      ;;
    pull_request) BASE=$EVENT_BASE; revision_source=pullRequestBase ;;
    merge_group) BASE=$EVENT_BASE; revision_source=mergeGroupBase ;;
    *) echo 'affected could not resolve a base revision from this event; set the base input explicitly' >&2; false ;;
  esac
fi
[[ -n "$BASE" ]] || { echo 'affected could not resolve a base revision from inputs or the GitHub event; set with.base to bootstrap this run' >&2; false; }
ACTION_PHASE=revision-validation
resolved_head=$(git -C "$CWD" rev-parse --verify "$HEAD^{commit}")

history_status=disabled; history_download_ms=0; history_source_run_id=''; history_path="$RUNNER_TEMP/nanoom-history.json"
if [[ "$SCHEDULER" == artifact ]]; then
  history_started=$(date +%s); history_status=bootstrap-fallback
  ACTION_PHASE=history-resolution
  history_source_run_id=$(nanoom_previous_successful_run "$WORKFLOW_REF" "$HISTORY_REF" "$RUN_ID")
  if [[ -n "$history_source_run_id" ]]; then
    artifacts=$(nanoom_run_artifacts "$history_source_run_id")
    history_dir="$RUNNER_TEMP/nanoom-previous-history"
    if nanoom_download_artifacts "$artifacts" exact "$HISTORY_ARTIFACT" "$history_dir" &&
      [[ -f "$history_dir/history.json" ]] &&
      jq -e '.samples | type == "array"' "$history_dir/history.json" >/dev/null; then
      cp "$history_dir/history.json" "$history_path"
      history_status=loaded
    elif nanoom_artifact_exists "$artifacts" prefix "nanoom-timing-sample-v2-$history_source_run_id-"; then
      echo "successful run $history_source_run_id uploaded timing samples but no merged '$HISTORY_ARTIFACT' history; add the standard nanoom history job" >&2
      false
    else
      echo "no timing history exists in successful run $history_source_run_id; using deterministic cold-start scheduling" >&2
    fi
  else
    echo 'no previous successful workflow run exists; using deterministic cold-start scheduling' >&2
  fi
  history_download_ms=$(( ($(date +%s) - history_started) * 1000 ))
fi

args=(-C "$CWD" -c "$CONFIG" affected --json --base "$BASE" --head "$resolved_head" --timing-runner "$TIMING_RUNNER" --timing-environment "$TIMING_ENVIRONMENT")
[[ -f "$history_path" ]] && args+=(--history "$history_path")
printf -v ACTION_COMMAND '%q ' nanoom "${args[@]}"; ACTION_COMMAND=${ACTION_COMMAND% }
printf '◆ nanoom affected\n  Inputs\n    cwd: %s\n    config: %s\n    scheduler: %s\n    timing environment: %s\n  Command\n    %s\n' "$CWD" "$CONFIG" "$SCHEDULER" "$TIMING_ENVIRONMENT" "$ACTION_COMMAND"
ACTION_PHASE=affected-calculation; report=$(nanoom "${args[@]}")
ACTION_PHASE=revision-validation
resolved_base=$(git -C "$CWD" rev-parse --verify "$BASE^{commit}")
git -C "$CWD" merge-base --is-ancestor "$resolved_base" "$resolved_head" || {
  echo "resolved base $resolved_base is not an ancestor of head $resolved_head; set with.base to a reachable commit" >&2
  false
}
report=$(jq -c --arg source "$revision_source" --arg base "$resolved_base" --arg head "$resolved_head" --arg successful "$successful_run_id" '. + {revisionResolution:{baseSource:$source,baseCommit:$base,headCommit:$head,successfulRunId:(if $successful == "" then null else ($successful | tonumber) end)}}' <<<"$report")
report=$(jq -c --arg historyStatus "$history_status" --arg sourceRun "$history_source_run_id" --argjson downloadMs "$history_download_ms" '.scheduling.historyStatus=(if .scheduling.historyStatus == "fallback" then "corrupt" else $historyStatus end) | .scheduling.historySourceRunId=(if $sourceRun == "" then null else ($sourceRun | tonumber) end) | .scheduling.historyDownloadMs=$downloadMs | .scheduling.reason=(if .scheduling.historyStatus == "loaded" then "recent successful samples loaded from the same workflow and branch" elif .scheduling.historyStatus == "disabled" then "historical scheduling explicitly disabled; deterministic equal weights" else "no usable previous history; deterministic cold-start scheduling" end)' <<<"$report")
history_status=$(jq -r .scheduling.historyStatus <<<"$report")
matrix=$(jq -c .matrix <<<"$report")

if [[ "$SCHEDULER" == http ]]; then
  [[ "$COORDINATOR_URL" == https://* && -n "$COORDINATOR_TOKEN" ]] || { echo 'scheduler=http requires an HTTPS coordinatorUrl and NANOOM_COORDINATOR_TOKEN' >&2; false; }
  coordinator=${COORDINATOR_URL%/}
  while IFS= read -r group; do
    distribution=$(jq -c --arg group "$group" '.affected.group[$group].distribution // empty' <<<"$report"); [[ -n "$distribution" ]] || continue
    items=$(jq -c --arg group "$group" '.affected.group[$group].workspaces' <<<"$report"); item_count=$(jq length <<<"$items"); (( item_count > 0 )) || continue
    checkout=$(jq -c --arg group "$group" '[.[$group].include[].checkout.sparseCheckout | split("\n")[]] | unique | {coneMode:true,sparseCheckout:join("\n")}' <<<"$matrix")
    concurrency=$(jq -r .concurrency <<<"$distribution"); (( concurrency > item_count )) && concurrency=$item_count
    resolved_environment=$(jq -r --arg fallback "$TIMING_ENVIRONMENT" '.timingEnvironment // $fallback' <<<"$distribution")
    body=$(jq -cn --arg repository "$REPOSITORY" --arg run "$RUN_ID.$RUN_ATTEMPT" --arg group "$group" --arg environment "$resolved_environment" --argjson workItems "$items" --argjson tier "$distribution" --argjson concurrency "$concurrency" '{repository:$repository,run:$run,group:$group,workItems:$workItems,tier:$tier,concurrency:$concurrency,environment:$environment}')
    group_key=$(jq -rn --arg value "$group" '$value | @uri')
    response=$(curl --fail-with-body --silent --show-error -X POST -H "Authorization: Bearer $COORDINATOR_TOKEN" -H 'Content-Type: application/json' -H "Idempotency-Key: $REPOSITORY:$RUN_ID:$RUN_ATTEMPT:$group_key" "$coordinator/v1/runs" --data "$body")
    runner_config=$(jq -c --arg group "$group" '.[$group].include[0] | {runnerLabels,timingEnvironment} | with_entries(select(.value != null))' <<<"$matrix")
    run_id=$(jq -er .runId <<<"$response"); agents=$(jq -cn --arg runId "$run_id" --argjson count "$concurrency" --argjson checkout "$checkout" --argjson runnerConfig "$runner_config" '[range(1; $count + 1) | ({agentId:("agent-" + tostring),runId:$runId,mode:"continuous",checkout:$checkout} + $runnerConfig)]')
    matrix=$(jq -c --arg group "$group" --argjson agents "$agents" '.[$group].include=$agents' <<<"$matrix")
  done < <(jq -r '.affected.group | keys[]' <<<"$report")
  report=$(jq -c --argjson matrix "$matrix" '.matrix=$matrix' <<<"$report")
fi

compact_matrix=$(jq -c 'with_entries(.value.include |= map(if .items then {assignmentId,predictedDurationMs,checkoutPathCount,predictionSources,reason,checkout,runnerLabels,timingEnvironment,items:[.items[] | {group,name,task,shard,totalShards} | with_entries(select(.value != null))]} elif .mode == "continuous" then {agentId,runId,mode,checkout,runnerLabels,timingEnvironment} else {name,task,shard,totalShards,checkoutPathCount,runnerLabels,timingEnvironment} | with_entries(select(.value != null)) end))' <<<"$matrix")
groups=$(jq -c 'with_entries(.value = {hasChange:((.value.include|length)>0),matrix:.value})' <<<"$compact_matrix"); has=$(jq -r 'any(to_entries[]; .value.include | length > 0)' <<<"$compact_matrix")
result=$(jq -c --argjson groups "$groups" '. + {groups:($groups | with_entries(.value |= {hasChange,assignmentCount:(.matrix.include|length)}))}' <<<"$report")
output_bytes=$(printf 'has_change=%s\ngroups=%s\nresult=%s\n' "$has" "$groups" "$result" | iconv -f UTF-8 -t UTF-16LE | wc -c | tr -d ' ')
(( output_bytes <= 1048576 )) || { echo "Action outputs exceed GitHub's 1 MiB UTF-16 limit: $output_bytes bytes" >&2; false; }
echo "has_change=$has" >> "$GITHUB_OUTPUT"; echo "groups=$groups" >> "$GITHUB_OUTPUT"; echo "result=$result" >> "$GITHUB_OUTPUT"
assignments=$(jq '[to_entries[].value.include[]] | length' <<<"$matrix"); items=$(jq '[.affected.group[].workspaces[]] | length' <<<"$report"); elapsed=$(( $(date +%s) - started ))
printf '  Resolved revisions\n    source: %s\n    base: %s\n    head: %s\n    successful run: %s\n  Result\n    ✓ affected work items=%s; assignments=%s; history=%s; elapsed=%ss\n  Final JSON\n    %s\n' "$revision_source" "$resolved_base" "$resolved_head" "${successful_run_id:-none}" "$items" "$assignments" "$history_status" "$elapsed" "$result"
{ echo '### nanoom affected'; echo; echo "**Revision:** \`$revision_source\` $resolved_base → $resolved_head (successful run: ${successful_run_id:-none})"; echo; echo "**Result:** $items work items in $assignments assignments; history \`$history_status\`."; echo; echo '| Group | Total | Affected | Percent | Tier | Concurrency |'; echo '|---|---:|---:|---:|---|---:|'; jq -r '.affected.group | to_entries[] | "| \(.key) | \(.value.totalWorkspaces) | \(.value.affectedWorkspaces) | \(.value.affectedPercent) | \(.value.distribution.name // "legacy") | \(.value.distribution.concurrency // (.value.workspaces|length)) |"' <<<"$report"; } >> "$GITHUB_STEP_SUMMARY"
