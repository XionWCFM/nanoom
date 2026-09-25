#!/usr/bin/env bash
set -Eeuo pipefail
ACTION_NAME=affected ACTION_CWD=$CWD ACTION_PHASE=input-resolution ACTION_COMMAND=not-started
source "$GITHUB_ACTION_PATH/../_setup/log.sh"; trap 'nanoom_fail "$?"' ERR
source "$GITHUB_ACTION_PATH/../_setup/artifacts.sh"
started=$(date +%s)
planning_job=${GITHUB_JOB:-affected}
[[ "$SCHEDULER" =~ ^(off|artifact|http)$ ]] || { echo "scheduler must be off, artifact, or http" >&2; false; }
HISTORY_BACKEND=${HISTORY_BACKEND:-artifact}
HISTORY_SERVER_URL=${HISTORY_SERVER_URL:-}
HISTORY_SERVER_TOKEN=${HISTORY_SERVER_TOKEN:-}
[[ "$HISTORY_BACKEND" =~ ^(artifact|server)$ ]] || { echo "historyBackend must be artifact or server" >&2; false; }
if [[ "$HISTORY_BACKEND" == server && "$SCHEDULER" != artifact ]]; then
  echo 'historyBackend=server requires scheduler=artifact so successful task measurements are produced' >&2
  false
fi
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
resolved_base=$(git -C "$CWD" rev-parse --verify "$BASE^{commit}")
git -C "$CWD" merge-base --is-ancestor "$resolved_base" "$resolved_head" || {
  echo "resolved base $resolved_base is not an ancestor of head $resolved_head; set with.base to a reachable commit" >&2
  false
}

history_status=disabled; history_fetch_ms=0; history_source_run_id=''; history_path=''

if [[ "$SCHEDULER" == http ]]; then
  args=(-C "$CWD" -c "$CONFIG" affected --json --base "$BASE" --head "$resolved_head" --timing-runner "$TIMING_RUNNER" --timing-environment "$TIMING_ENVIRONMENT")
  [[ -f "$history_path" ]] && args+=(--history "$history_path")
  printf -v ACTION_COMMAND '%q ' nanoom "${args[@]}"; ACTION_COMMAND=${ACTION_COMMAND% }
  printf '◆ nanoom affected\n  Inputs\n    cwd: %s\n    config: %s\n    scheduler: %s\n    timing environment: %s\n  Command\n    %s\n' "$CWD" "$CONFIG" "$SCHEDULER" "$TIMING_ENVIRONMENT" "$ACTION_COMMAND"
  ACTION_PHASE=affected-calculation; report=$(nanoom "${args[@]}")
  ACTION_PHASE=revision-validation
  report=$(jq -c --arg source "$revision_source" --arg base "$resolved_base" --arg head "$resolved_head" --arg successful "$successful_run_id" '. + {revisionResolution:{baseSource:$source,baseCommit:$base,headCommit:$head,successfulRunId:(if $successful == "" then null else ($successful | tonumber) end)}}' <<<"$report")
  report=$(jq -c --arg historyStatus "$history_status" --arg sourceRun "$history_source_run_id" --argjson fetchMs "$history_fetch_ms" '.scheduling.historyStatus=(if .scheduling.historyStatus == "fallback" then "corrupt" else $historyStatus end) | .scheduling.historySourceRunId=(if $sourceRun == "" then null else ($sourceRun | tonumber) end) | .scheduling.historyFetchMs=$fetchMs | .scheduling.reason=(if .scheduling.historyStatus == "loaded" then "recent successful samples loaded from the same workflow and branch" elif .scheduling.historyStatus == "disabled" then "historical scheduling explicitly disabled; deterministic equal weights" else "no usable previous history; deterministic cold-start scheduling" end)' <<<"$report")
  history_status=$(jq -r .scheduling.historyStatus <<<"$report")
fi

# Static assignments cross the producer/consumer boundary as a bounded
# reference plus a 30-day Plan artifact. Keep scheduler=http's existing live
# coordinator rows on its explicit continuous-only path below.
if [[ "$SCHEDULER" != http ]]; then
  task_runner=$TIMING_RUNNER
  if [[ "$task_runner" == auto ]]; then
    if [[ -f "$CWD/turbo.json" ]]; then
      task_runner=turbo
    elif [[ -f "$CWD/nx.json" ]]; then
      task_runner=nx
    else
      task_runner=$(jq -r '.packageManager // empty | split("@")[0]' "$CWD/package.json" 2>/dev/null || true)
      if [[ ! "$task_runner" =~ ^(pnpm|yarn|npm)$ ]]; then
        if [[ -f "$CWD/pnpm-lock.yaml" ]]; then task_runner=pnpm
        elif [[ -f "$CWD/yarn.lock" ]]; then task_runner=yarn
        else task_runner=npm
        fi
      fi
    fi
  fi

  plan_dir="$RUNNER_TEMP/nanoom-plan-$RUN_ID-$RUN_ATTEMPT-$planning_job"
  mkdir -p "$plan_dir"
  plan_path="$plan_dir/plan-v1.json"
  context_path="$plan_dir/plan-context.json"
  prediction_context_path="$plan_dir/prediction-context.json"
  preparation_context_args=()
  if [[ "$SCHEDULER" == artifact || "$HISTORY_BACKEND" == server ]]; then
    preparation_declaration=$(jq -r '.packageManager // empty' "$CWD/package.json" 2>/dev/null || true)
    declared_pm=${preparation_declaration%%@*}
    declared_pm_version=''
    if [[ "$preparation_declaration" == *@* ]]; then
      declared_pm_version=${preparation_declaration#*@}
      declared_pm_version=${declared_pm_version%%+*}
    fi
    preparation_pm=${PACKAGE_MANAGER:-auto}
    if [[ "$preparation_pm" == auto ]]; then
      preparation_pm=$declared_pm
      if [[ ! "$preparation_pm" =~ ^(pnpm|yarn|npm)$ ]]; then
        if [[ -f "$CWD/pnpm-lock.yaml" ]]; then preparation_pm=pnpm
        elif [[ -f "$CWD/yarn.lock" ]]; then preparation_pm=yarn
        else preparation_pm=npm
        fi
      fi
    fi
    case "$preparation_pm" in
      pnpm) preparation_lockfile="$CWD/pnpm-lock.yaml" ;;
      yarn) preparation_lockfile="$CWD/yarn.lock" ;;
      npm) preparation_lockfile="$CWD/package-lock.json" ;;
      *) preparation_lockfile='' ;;
    esac
    if [[ -n "$preparation_lockfile" && -f "$preparation_lockfile" && "$preparation_pm" == "$declared_pm" ]]; then
      preparation_pm_version=$declared_pm_version
      if [[ "$preparation_pm_version" =~ ^[A-Za-z0-9._+-]+$ ]]; then
        if command -v sha256sum >/dev/null 2>&1; then
          preparation_lockfile_digest=$(sha256sum "$preparation_lockfile" | awk '{print $1}')
        elif command -v shasum >/dev/null 2>&1; then
          preparation_lockfile_digest=$(shasum -a 256 "$preparation_lockfile" | awk '{print $1}')
        else
          preparation_lockfile_digest=''
        fi
        if [[ "$preparation_lockfile_digest" =~ ^[0-9a-f]{64}$ ]]; then
          preparation_context_path="$plan_dir/preparation-context.json"
          jq -cn --arg packageManager "$preparation_pm" --arg packageManagerVersion "$preparation_pm_version" --arg lockfileDigest "$preparation_lockfile_digest" '{packageManager:$packageManager,packageManagerVersion:$packageManagerVersion,installMode:"focused",lockfileDigest:$lockfileDigest}' > "$preparation_context_path"
          preparation_context_args+=(--preparation-context "$preparation_context_path")
        fi
      fi
    fi
    if ((${#preparation_context_args[@]} == 0)); then
      echo 'preparation prediction context unavailable (package-manager version or root lockfile missing); candidate selection will retain the concurrency cap' >&2
    fi
  fi
  prediction_identity=''
  if [[ "$SCHEDULER" == artifact ]]; then
    prediction_identity=$(nanoom_prediction_identity "${GITHUB_EVENT_NAME:-$EVENT}" "$WORKFLOW_REF" "${GITHUB_REPOSITORY_ID:-}" "${GITHUB_SERVER_URL:-}" "${GITHUB_REF:-}" "${PR_NUMBER:-}" "${PR_HEAD_REPOSITORY_ID:-}" "${PR_HEAD_REF:-}" "${PR_BASE_REF:-}") || prediction_identity=''
    if [[ -n "$prediction_identity" ]]; then
      printf '%s\n' "$prediction_identity" > "$prediction_context_path"
    fi
    history_status=fallback
  fi
  jq -n \
    --arg repository "$REPOSITORY" \
    --arg workflow "$WORKFLOW_REF" \
    --arg run "$RUN_ID" \
    --argjson attempt "$RUN_ATTEMPT" \
    --arg job "$planning_job" \
    --arg base "$resolved_base" \
    --arg head "$resolved_head" \
    --arg taskRunner "$task_runner" \
    --arg reason "$(if [[ "$SCHEDULER" == artifact ]]; then echo 'PredictionArtifact v3 lookup is cold until affected confirms a scheduling choice exists'; else echo 'historical scheduling disabled; deterministic cold-start scheduling'; fi)" \
    '{repository:$repository,workflow:$workflow,runId:$run,producerAttempt:$attempt,planningJob:$job,base:$base,head:$head,taskRunner:$taskRunner,predictionReason:$reason}' > "$context_path"

  plan_args=(-C "$CWD" -c "$CONFIG" affected --base "$resolved_base" --head "$resolved_head" --timing-runner "$TIMING_RUNNER" --timing-environment "$TIMING_ENVIRONMENT" --plan-output "$plan_path" --plan-context "$context_path")
  if [[ -n "$prediction_identity" ]]; then
    plan_args+=(--prediction-context "$prediction_context_path")
  fi
  if ((${#preparation_context_args[@]})); then
    plan_args+=("${preparation_context_args[@]}")
  fi
  plan_args+=(--history-status "$history_status")
  printf -v ACTION_COMMAND '%q ' nanoom "${plan_args[@]}"; ACTION_COMMAND=${ACTION_COMMAND% }
  printf '◆ nanoom affected plan\n  Command\n    %s\n' "$ACTION_COMMAND"
  ACTION_PHASE=affected-calculation
  compact=$(nanoom "${plan_args[@]}")
  history_needed=$(jq -r '.result.historyNeeded // false' <<<"$compact")
  if [[ "$HISTORY_BACKEND" == server && "$history_needed" == true ]]; then
    history_started_ms=$(nanoom_now_ms)
    ACTION_PHASE=history-server-read
    history_source_run_id=''
    table_dir="$plan_dir/server-predictions"
    mkdir -p "$table_dir"
    table_args=()
    server_read_failed=false
    if ! nanoom_history_server_trusted_event; then
      echo 'History Server read skipped for an event that may run untrusted code; using the cold plan' >&2
    elif [[ -z "$HISTORY_SERVER_TOKEN" ]] || ! nanoom_history_server_url_valid "$HISTORY_SERVER_URL"; then
      echo 'History Server URL or credential is unavailable; using the cold plan' >&2
    elif [[ -z "$prediction_identity" ]]; then
      echo 'History Server read skipped because the prediction scope is unavailable' >&2
    else
      nanoom_history_budget_start 3 8388608
      history_server_base=${HISTORY_SERVER_URL%/}
      while IFS= read -r scope; do
        scope_id=$(jq -er '.scopeId | select(test("^[a-f0-9]{64}$"))' <<<"$scope") || { server_read_failed=true; break; }
        repository_key=$(jq -er '.repositoryKey | select(test("^[a-z0-9][a-z0-9._-]{0,63}$"))' <<<"$scope") || { server_read_failed=true; break; }
        snapshot_path="$table_dir/$scope_id.json"
        remaining=$(nanoom_history_remaining) || { server_read_failed=true; break; }
        status_code=$(curl --silent --show-error --max-time "$remaining" --max-filesize 8388608 \
          -H "Authorization: Bearer $HISTORY_SERVER_TOKEN" \
          -H 'Accept: application/json' \
          -o "$snapshot_path" -w '%{http_code}' \
          "$history_server_base/v1/repositories/$repository_key/scopes/$scope_id/snapshot") || {
            server_read_failed=true
            break
          }
        response_bytes=$(wc -c < "$snapshot_path" | tr -d ' ')
        nanoom_history_charge_bytes "$response_bytes" || { server_read_failed=true; break; }
        case "$status_code" in
          200) table_args+=(--prediction-table "$snapshot_path") ;;
          404) rm -f "$snapshot_path" ;;
          *) server_read_failed=true; break ;;
        esac
      done < <(jq -c '.result.historyScopes[]?' <<<"$compact")
      if [[ "$server_read_failed" == false ]] && ((${#table_args[@]})); then
        ACTION_PHASE=affected-server-calculation
        if warm_compact=$(nanoom_history_timeout nanoom "${plan_args[@]}" "${table_args[@]}"); then
          compact=$warm_compact
        else
          echo 'History Server table validation exceeded the shared history budget; using the cold plan' >&2
        fi
      elif [[ "$server_read_failed" == true ]]; then
        echo 'History Server read failed or exceeded its shared budget; using the cold plan' >&2
      fi
    fi
    history_fetch_ms=$(($(nanoom_now_ms) - history_started_ms))
  fi

  if [[ "$HISTORY_BACKEND" == artifact && "$SCHEDULER" == artifact && "$history_needed" == true ]]; then
    history_started_ms=$(nanoom_now_ms)
    ACTION_PHASE=history-resolution
    nanoom_history_budget_start 3 8388608
    nanoom_try_prediction_run() {
      local candidate_run=$1 candidate_dir=$2 candidate_artifacts
      [[ -n "$candidate_run" ]] || return 1
      candidate_artifacts=$(nanoom_run_artifacts "$candidate_run") || return 1
      nanoom_download_prediction_artifact "$candidate_artifacts" "$HISTORY_ARTIFACT" "$candidate_dir" || return 1
      history_path=$(find "$candidate_dir" -maxdepth 1 -type f -name '*.json' -print | sort | head -n 1)
      [[ -n "$history_path" ]]
    }
    nanoom_try_prediction_run_for_event() {
      local branch=$1 event=$2 pr_number=$3 head_repository_id=$4 candidate_run
      while IFS= read -r candidate_run; do
        [[ -n "$candidate_run" ]] || continue
        if nanoom_try_prediction_run "$candidate_run" "$RUNNER_TEMP/nanoom-pr-prediction-$candidate_run"; then
          history_source_run_id=$candidate_run
          return 0
        fi
      done < <(nanoom_previous_successful_runs_for_event "$WORKFLOW_REF" "$branch" "$RUN_ID" "$event" "$pr_number" "$head_repository_id")
      return 1
    }

    if [[ -n "$prediction_identity" ]]; then
      case "${GITHUB_EVENT_NAME:-$EVENT}" in
        pull_request|pull_request_target)
          if ! nanoom_try_prediction_run_for_event "$PR_HEAD_REF" pull_request "$PR_NUMBER" "$PR_HEAD_REPOSITORY_ID"; then
            nanoom_try_prediction_run_for_event "$PR_BASE_REF" push '' '' || true
          fi
          ;;
        *)
          nanoom_try_prediction_run_for_event "$HISTORY_REF" push '' '' || true
          ;;
      esac
    else
      echo 'history lookup skipped: event does not have a supported branch or pull-request scope' >&2
    fi

    if [[ -n "$history_path" ]]; then
      prediction_sha=''; prediction_metadata=''; model_name=''; model_sha=''
      if command -v sha256sum >/dev/null 2>&1; then
        prediction_sha=$(nanoom_history_timeout sha256sum "$history_path" | awk '{print $1}') || history_path=''
      else
        prediction_sha=$(nanoom_history_timeout shasum -a 256 "$history_path" | awk '{print $1}') || history_path=''
      fi
      prediction_metadata=$(nanoom_prediction_model_metadata "$history_path") || history_path=''
      model_name=$(jq -r '.name // empty' <<<"$prediction_metadata")
      model_sha=$(jq -r '.sha256 // empty' <<<"$prediction_metadata")
      if [[ -n "$history_path" && "$prediction_sha" =~ ^[0-9a-f]{64}$ ]]; then
        if nanoom_history_timeout jq --arg predictionName "$HISTORY_ARTIFACT" --arg predictionSha "$prediction_sha" --arg modelName "$model_name" --arg modelSha "$model_sha" '.predictionReason="a bounded PredictionArtifact v3 was selected and validated by Nanoom" | .predictionArtifact={name:$predictionName,sha256:$predictionSha} | .modelArtifact={name:$modelName,sha256:$modelSha}' "$context_path" > "$context_path.tmp"; then
          mv "$context_path.tmp" "$context_path"
          plan_args+=(--prediction "$history_path")
        else
          history_path=''
        fi
      else
        history_path=''
      fi
    fi
    if [[ -n "$history_path" ]]; then
      ACTION_PHASE=affected-calculation
      if warm_compact=$(nanoom_history_timeout nanoom "${plan_args[@]}"); then
        compact=$warm_compact
      else
        echo 'bounded prediction lookup or validation exceeded its shared history budget; using the already-computed cold plan' >&2
        history_source_run_id=''
      fi
    fi
    history_fetch_ms=$(($(nanoom_now_ms) - history_started_ms))
  fi
  ACTION_PHASE=output-serialization
  history_status=$(jq -er '.result.historyStatus' <<<"$compact")
  [[ "$history_status" == loaded ]] || history_source_run_id=''
  jq '.plan' <<<"$compact" > "$plan_dir/plan-reference.json"
  plan_ref=$(jq -c '.plan' <<<"$compact")
  groups=$(jq -c '.groups' <<<"$compact")
  result=$(jq -c --arg source "$revision_source" --arg base "$resolved_base" --arg head "$resolved_head" --arg successful "$successful_run_id" --arg historyStatus "$history_status" --arg sourceRun "$history_source_run_id" --arg historyBackend "$HISTORY_BACKEND" --argjson fetchMs "$history_fetch_ms" '.result + {historyStatus:$historyStatus,revisionResolution:{baseSource:$source,baseCommit:$base,headCommit:$head,successfulRunId:(if $successful == "" then null else ($successful | tonumber) end)},scheduling:{historyBackend:$historyBackend,historyStatus:$historyStatus,historySourceRunId:(if $sourceRun == "" then null else ($sourceRun | tonumber) end),historyFetchMs:$fetchMs,reason:(if $historyStatus == "loaded" and $historyBackend == "server" then "bounded PredictionTable v3 snapshots loaded from the History Server" elif $historyStatus == "loaded" then "bounded PredictionArtifact v3 loaded; ModelState and measurements were not downloaded" elif $historyStatus == "history_not_needed" then "no affected assignment choice could be changed by history; no history metadata request was made" elif $historyStatus == "disabled" then "historical scheduling explicitly disabled; deterministic cold scheduling" elif $historyStatus == "corrupt" then "prediction history was invalid; using deterministic cold scheduling" else "no usable history; using deterministic cold scheduling" end)}}' <<<"$compact")
  has=$(jq -r '.has_change' <<<"$compact")
  output_bytes=$(printf 'has_change=%s\nplan=%s\ngroups=%s\nresult=%s\n' "$has" "$plan_ref" "$groups" "$result" | iconv -f UTF-8 -t UTF-16LE | wc -c | tr -d ' ')
  (( output_bytes <= 1048576 )) || { echo "Action outputs exceed GitHub's 1 MiB UTF-16 limit: $output_bytes bytes" >&2; false; }
  echo "has_change=$has" >> "$GITHUB_OUTPUT"
  echo "plan=$plan_ref" >> "$GITHUB_OUTPUT"
  echo "plan_artifact_path=$plan_dir" >> "$GITHUB_OUTPUT"
  echo "groups=$groups" >> "$GITHUB_OUTPUT"
  echo "result=$result" >> "$GITHUB_OUTPUT"
  assignments=$(jq -r '.result.assignmentCount' <<<"$compact")
  items=$(jq -r '.result.itemCount' <<<"$compact")
  elapsed=$(( $(date +%s) - started ))
  printf '  Resolved revisions\n    source: %s\n    base: %s\n    head: %s\n    successful run: %s\n  Result\n    ✓ affected work items=%s; assignments=%s; history=%s; elapsed=%ss\n  Final JSON\n    %s\n' "$revision_source" "$resolved_base" "$resolved_head" "${successful_run_id:-none}" "$items" "$assignments" "$history_status" "$elapsed" "$result"
  { echo '### nanoom affected'; echo; echo "**Revision:** \`$revision_source\` $resolved_base → $resolved_head (successful run: ${successful_run_id:-none})"; echo; echo "**Result:** $items work items in $assignments assignments; history \`$history_status\`."; echo; echo '| Group | Assignments | Items |'; echo '|---|---:|---:|'; jq -r --slurpfile plan "$plan_path" '.groups | to_entries[] | .key as $group | [$group, .value.include | length, ($plan[0].groups[$group].assignments | map(.items | length) | add // 0)] | "| \(.[0]) | \(.[1]) | \(.[2]) |"' <<<"$compact"; } >> "$GITHUB_STEP_SUMMARY"
  exit 0
fi

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
