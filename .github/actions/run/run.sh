#!/usr/bin/env bash
set -Eeuo pipefail
ACTION_NAME=run ACTION_CWD=$CWD ACTION_PHASE=input-validation ACTION_COMMAND=not-started
source "$GITHUB_ACTION_PATH/../_setup/log.sh"; trap 'nanoom_fail "$?"' ERR
source "$GITHUB_ACTION_PATH/../_setup/artifacts.sh"
started=$(date +%s)
static_plan=false
if [[ -n ${ASSIGNMENT_FILE:-} ]]; then
  if [[ -z "$CWD" || "$CWD" == . ]]; then
    CWD="$GITHUB_WORKSPACE/.nanoom/$RUN_ID/$RUN_ATTEMPT/$GITHUB_JOB/${MATRIX_INDEX:-0}"
    ACTION_CWD=$CWD
  fi
  source "$GITHUB_ACTION_PATH/../_setup/assignment.sh"
  nanoom_validate_assignment_file "$ASSIGNMENT_FILE" "$CWD"
  mode=static; static_plan=true
  GROUP=$(jq -er .group "$ASSIGNMENT_FILE")
  ASSIGNMENT_ID=$(jq -er .assignmentId "$ASSIGNMENT_FILE")
  planned_count=$(jq -er '.items | select(type == "array" and length > 0) | length' "$ASSIGNMENT_FILE")
  planned_tool=$(jq -er .taskRunner "$ASSIGNMENT_FILE")
  if [[ "$TOOL" == auto ]]; then
    TOOL=$planned_tool
  elif [[ "$TOOL" != "$planned_tool" ]]; then
    echo "static assignment task runner '$planned_tool' cannot be changed to '$TOOL'" >&2
    false
  fi
  matrix_timing_environment=$(jq -r '.timingEnvironment // empty' "$ASSIGNMENT_FILE")
else
  [[ -n ${MATRIX:-} ]] || {
    echo 'run requires assignmentFile; only scheduler=http continuous agents may use matrix' >&2
    false
  }
  entry=$(jq -ce '(.include[0] // .)' <<<"${MATRIX:-}")
  mode=$(jq -r '.mode // "static"' <<<"$entry")
  [[ "$mode" == continuous ]] || {
    echo 'static assignment run requires a validated assignment-file from nanoom prepare' >&2
    false
  }
  matrix_timing_environment=$(jq -r '.timingEnvironment // empty' <<<"$entry")
fi
[[ -z "$matrix_timing_environment" ]] || TIMING_ENVIRONMENT=$matrix_timing_environment
[[ "$SCHEDULER" =~ ^(off|artifact|http)$ ]] || { echo "invalid scheduler: $SCHEDULER" >&2; false; }
artifact_version=$(nanoom_artifact_version "$SCHEDULER" "${ARTIFACT_VERSION:-}")
if [[ "$static_plan" == true ]]; then
  if [[ "$TOOL" =~ ^(turbo|nx)$ && ! -x "$CWD/node_modules/.bin/$TOOL" ]]; then
    echo "planned task runner '$TOOL' is missing from the installed root tooling; switching runners is forbidden" >&2
    false
  fi
  detail_dir="$RUNNER_TEMP/nanoom-run-$RUN_ID-$RUN_ATTEMPT-${GITHUB_JOB:-run}"
  mkdir -p "$detail_dir"
  DETAIL_FILE="$detail_dir/assignment.jsonl"
  : > "$DETAIL_FILE"
fi

run_item() {
  local item=$1 group task name cli_result
  group=$(jq -r --arg fallback "$GROUP" '.group // $fallback' <<<"$item"); task=$(jq -er .task <<<"$item"); name=$(jq -er .name <<<"$item")
  local args=(-C "$CWD" run "$group" "$task" --all --filter "$name" --json)
  [[ -n $(jq -r '.shard // empty' <<<"$item") ]] && args+=(--shard "$(jq -r .shard <<<"$item")" --total-shards "$(jq -r .totalShards <<<"$item")")
  if [[ "$static_plan" == true ]]; then
    args+=(--runner "$TOOL")
  elif [[ "$TOOL" == turbo && ! -x "$CWD/node_modules/.bin/turbo" ]]; then
    args+=(--runner "$PM")
  elif [[ "$TOOL" != auto ]]; then
    args+=(--runner "$TOOL")
  fi
  printf -v ACTION_COMMAND '%q ' nanoom "${args[@]}"; ACTION_COMMAND=${ACTION_COMMAND% }
  printf '  ▶ %s / %s / %s\n    cwd: %s\n    command: %s\n' "$group" "$name" "$task" "$CWD" "$ACTION_COMMAND" >&2
  trap - ERR
  if cli_result=$(nanoom "${args[@]}"); then
    if jq -e --arg name "$name" '(.executions | type) == "array" and ([.executions[].workspace] | index($name) != null)' <<<"$cli_result" >/dev/null; then
      if [[ "$static_plan" == true ]]; then
        detail=$(jq -cn --argjson item "$item" --arg command "$ACTION_COMMAND" --argjson cli "$cli_result" '{status:"success",item:$item,command:$command,cli:$cli}')
        printf '%s\n' "$detail" >> "$DETAIL_FILE"
        execution=$(jq -ce --arg name "$name" '[.executions[] | select(.workspace == $name) | {workspace,runner,durationMs}] | first' <<<"$cli_result")
        jq -cn --argjson item "$item" --argjson execution "$execution" '{status:"success",item:$item,execution:$execution}'
      else
        jq -cn --argjson item "$item" --arg command "$ACTION_COMMAND" --argjson cli "$cli_result" '{status:"success",item:$item,command:$command,cli:$cli}'
      fi
    else
      if [[ "$static_plan" == true ]]; then
        detail=$(jq -cn --argjson item "$item" --arg command "$ACTION_COMMAND" --argjson cli "$cli_result" '{status:"failure",item:$item,command:$command,cli:$cli,reason:"planned item produced no matching execution"}')
        printf '%s\n' "$detail" >> "$DETAIL_FILE"
        jq -cn --argjson item "$item" '{status:"failure",item:$item,reason:"planned item produced no matching execution"}'
      else
        jq -cn --argjson item "$item" --arg command "$ACTION_COMMAND" --argjson cli "$cli_result" '{status:"failure",item:$item,command:$command,cli:$cli,reason:"planned item produced no matching execution"}'
      fi
    fi
  else
    if [[ "$static_plan" == true ]]; then
      detail=$(jq -cn --argjson item "$item" --arg command "$ACTION_COMMAND" --argjson cli "${cli_result:-null}" '{status:"failure",item:$item,command:$command,cli:$cli}')
      printf '%s\n' "$detail" >> "$DETAIL_FILE"
      jq -cn --argjson item "$item" '{status:"failure",item:$item}'
    else
      jq -cn --argjson item "$item" --arg command "$ACTION_COMMAND" --argjson cli "${cli_result:-null}" '{status:"failure",item:$item,command:$command,cli:$cli}'
    fi
  fi
}

if [[ "$static_plan" == true ]]; then
  completed_count=0; item_index=0
  while IFS= read -r item; do
    item_result=$(run_item "$item")
    if [[ $(jq -r .status <<<"$item_result") == success ]]; then
      completed_count=$((completed_count + 1))
    else
      pending_count=$((planned_count - item_index - 1))
      elapsed=$(( $(date +%s) - started ))
      failed_item=$(jq -c '.item' <<<"$item_result")
      failed_identity=$(jq -c '.item | {name,task,shard,totalShards}' <<<"$item_result")
      jq -cn --arg assignmentId "$ASSIGNMENT_ID" --argjson completed "$completed_count" --argjson failed "$failed_item" --argjson pendingCount "$pending_count" '{status:"assignment-stopped",assignmentId:$assignmentId,completedCount:$completed,failed:$failed,pendingCount:$pendingCount}' >> "$DETAIL_FILE"
      jq -c --argjson firstPending "$((item_index + 1))" '.items | to_entries[] | select(.key >= $firstPending) | {status:"pending",item:(.value | {name,task,shard,totalShards})}' "$ASSIGNMENT_FILE" >> "$DETAIL_FILE"
      result=$(jq -cn --arg assignmentId "$ASSIGNMENT_ID" --argjson completed "$completed_count" --argjson failed "$failed_identity" --argjson pendingCount "$pending_count" --arg reason "$(jq -r '.reason // "task failed"' <<<"$item_result")" --arg detailFile "$DETAIL_FILE" --argjson elapsed "$elapsed" '{status:"failure",assignmentId:$assignmentId,completedCount:$completed,failed:$failed,pendingCount:$pendingCount,reason:$reason,detailFile:$detailFile,elapsedSeconds:$elapsed}')
      echo "result=$result" >> "$GITHUB_OUTPUT"
      echo "detail-file=$DETAIL_FILE" >> "$GITHUB_OUTPUT"
      printf '  Final JSON\n    %s\n' "$result"
      trap - ERR
      exit 1
    fi
    item_index=$((item_index + 1))
  done < <(jq -c '.items[]' "$ASSIGNMENT_FILE")

  elapsed=$(( $(date +%s) - started ))
  result=$(jq -cn --arg group "$GROUP" --arg assignmentId "$ASSIGNMENT_ID" --arg detailFile "$DETAIL_FILE" --argjson planned "$planned_count" --argjson executed "$completed_count" --argjson elapsed "$elapsed" --arg artifactVersion "$artifact_version" --arg scheduler "$SCHEDULER" '{status:"success",group:$group,assignmentId:$assignmentId,plannedItemCount:$planned,executedItemCount:$executed,detailFile:$detailFile,elapsedSeconds:$elapsed} + (if $scheduler == "artifact" then {artifactVersion:$artifactVersion} else {} end)')
  echo "result=$result" >> "$GITHUB_OUTPUT"
  echo "detail-file=$DETAIL_FILE" >> "$GITHUB_OUTPUT"
  if [[ "$SCHEDULER" == artifact ]]; then
    sample_dir="$RUNNER_TEMP/nanoom-timing"; mkdir -p "$sample_dir"
    sample_name=$(printf '%s-%s' "${GITHUB_JOB:-run}" "$ASSIGNMENT_ID" | tr -c 'A-Za-z0-9._-' '-' | cut -c1-80)
    sample_path="$sample_dir/$sample_name.json"
    jq -s --arg group "$GROUP" --arg environment "$TIMING_ENVIRONMENT" --arg assignmentId "$ASSIGNMENT_ID" --argjson predicted "$(jq -c '.predictedDurationMs // 0' "$ASSIGNMENT_FILE")" --argjson checkoutPathCount "$(jq '.checkoutPaths | length' "$ASSIGNMENT_FILE")" '{batch:{assignmentId:$assignmentId,predictedDurationMs:$predicted,checkoutPathCount:$checkoutPathCount},samples:[.[] | select(.status == "success") | .item as $item | .cli.executions[] | {group:$group,workspace:.workspace,task:$item.task,shard:$item.shard,totalShards:$item.totalShards,runner:.runner,environment:$environment,durationMs:.durationMs} | with_entries(select(.value != null))]}' "$DETAIL_FILE" > "$sample_path"
    echo "sample-path=$sample_path" >> "$GITHUB_OUTPUT"
    echo "sample-name=nanoom-timing-sample-v2-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT-$sample_name" >> "$GITHUB_OUTPUT"
    echo "upload-started=$(date +%s)" >> "$GITHUB_OUTPUT"
  fi
  printf '  Result\n    ✓ items=%s; elapsed=%ss\n  Final JSON\n    %s\n' "$completed_count" "$elapsed" "$result"
  { echo '### nanoom run'; echo; echo "**Result:** $completed_count assignment items succeeded in ${elapsed}s."; echo; echo "Detailed result: \`$DETAIL_FILE\`."; } >> "$GITHUB_STEP_SUMMARY"
  exit 0
fi

results='[]'
if [[ "$mode" == continuous ]]; then
  [[ "$SCHEDULER" == http ]] || { echo 'continuous matrix requires scheduler=http' >&2; false; }
  [[ "$COORDINATOR_URL" == https://* && -n "$COORDINATOR_TOKEN" ]] || { echo 'scheduler=http requires an HTTPS coordinatorUrl and NANOOM_COORDINATOR_TOKEN' >&2; false; }
  run_id=$(jq -er .runId <<<"$entry"); agent_id=$(jq -er .agentId <<<"$entry")
  coordinator=${COORDINATOR_URL%/}; run_key=$(jq -rn --arg value "$run_id" '$value | @uri'); agent_key=$(jq -rn --arg value "$agent_id" '$value | @uri')
  claim_index=0
  while :; do
    claim_index=$((claim_index + 1))
    claim=$(curl --fail-with-body --silent --show-error -X POST -H "Authorization: Bearer $COORDINATOR_TOKEN" -H 'Content-Type: application/json' -H "Idempotency-Key: $run_key:$agent_key:claim:$claim_index" "$coordinator/v1/runs/$run_key/claims" --data "$(jq -cn --arg agentId "$agent_id" '{agentId:$agentId}')")
    item=$(jq -c '.item // empty' <<<"$claim"); [[ -n "$item" ]] || break; item_id=$(jq -er .itemId <<<"$claim")
    item_key=$(jq -rn --arg value "$item_id" '$value | @uri'); heartbeat_failed="$RUNNER_TEMP/nanoom-heartbeat-$run_key-$agent_key-$item_key.failed"; rm -f "$heartbeat_failed"
    (while sleep 30; do curl --fail --silent -X PATCH -H "Authorization: Bearer $COORDINATOR_TOKEN" -H 'Content-Type: application/json' -H "Idempotency-Key: $run_key:$item_key:heartbeat" "$coordinator/v1/runs/$run_key/claims/$item_key" --data '{"status":"heartbeat"}' >/dev/null || { : > "$heartbeat_failed"; exit 1; }; done) & heartbeat_pid=$!
    item_result=$(run_item "$item")
    if [[ $(jq -r .status <<<"$item_result") == success ]]; then
      kill "$heartbeat_pid" 2>/dev/null || true; wait "$heartbeat_pid" 2>/dev/null || true
      if [[ -f "$heartbeat_failed" ]]; then
        result=$(jq -cn --argjson completed "$results" --argjson failed "$item" '{status:"failure",completed:[$completed[].item],failed:[$failed],pending:[],reason:"coordinator heartbeat failed; static fallback is forbidden after run start"}')
        echo "result=$result" >> "$GITHUB_OUTPUT"; printf '  Final JSON\n    %s\n' "$result"; trap - ERR; exit 1
      fi
      duration=$(jq -r '.cli.executions[0].durationMs' <<<"$item_result")
      curl --fail-with-body --silent --show-error -X PATCH -H "Authorization: Bearer $COORDINATOR_TOKEN" -H 'Content-Type: application/json' -H "Idempotency-Key: $run_key:$item_key:success" "$coordinator/v1/runs/$run_key/claims/$item_key" --data "$(jq -cn --argjson durationMs "$duration" '{status:"success",durationMs:$durationMs}')" >/dev/null
      results=$(jq -c --argjson result "$item_result" '. + [$result]' <<<"$results")
    else
      kill "$heartbeat_pid" 2>/dev/null || true; wait "$heartbeat_pid" 2>/dev/null || true
      curl --fail-with-body --silent --show-error -X PATCH -H "Authorization: Bearer $COORDINATOR_TOKEN" -H 'Content-Type: application/json' -H "Idempotency-Key: $run_key:$item_key:failure" "$coordinator/v1/runs/$run_key/claims/$item_key" --data '{"status":"failure"}' >/dev/null
      result=$(jq -cn --argjson completed "$results" --argjson failed "$item_result" '{status:"failure",completed:[$completed[].item],failed:[$failed.item],pending:[],reason:"task failed; future claims remain coordinator-owned"}')
      echo "result=$result" >> "$GITHUB_OUTPUT"; printf '  Final JSON\n    %s\n' "$result"; trap - ERR; exit 1
    fi
  done
else
  items=$(jq -c 'if .items then .items elif .name then [.] else error("matrix entry must contain items or name") end' <<<"$entry")
  item_index=0
  while IFS= read -r item; do
    item_result=$(run_item "$item")
    if [[ $(jq -r .status <<<"$item_result") == success ]]; then
      results=$(jq -c --argjson result "$item_result" '. + [$result]' <<<"$results")
    else
      pending=$(jq -c --argjson start "$((item_index + 1))" '.[$start:]' <<<"$items")
      result=$(jq -cn --argjson completed "$results" --argjson failed "$item_result" --argjson pending "$pending" '{status:"failure",completed:[$completed[].item],failed:[$failed.item],pending:$pending,reason:"first task failure stopped the assignment"}')
      echo "result=$result" >> "$GITHUB_OUTPUT"; printf '  Final JSON\n    %s\n' "$result"; trap - ERR; exit 1
    fi
    item_index=$((item_index + 1))
  done < <(jq -c '.[]' <<<"$items")
fi

elapsed=$(( $(date +%s) - started )); matrix_json=$(jq -c '{assignmentId,agentId,runId,mode,predictedDurationMs,runnerLabels,timingEnvironment,items} | with_entries(select(.value != null))' <<<"$entry")
result=$(jq -cn --argjson matrix "$matrix_json" --argjson results "$results" --argjson elapsed "$elapsed" --arg artifactVersion "$artifact_version" --arg scheduler "$SCHEDULER" '{status:"success",reason:"executed assignment items in order",matrix:$matrix,results:$results,elapsedSeconds:$elapsed} + (if $scheduler == "artifact" then {artifactVersion:$artifactVersion} else {} end)'); echo "result=$result" >> "$GITHUB_OUTPUT"
if [[ "$SCHEDULER" == artifact ]]; then
  sample_dir="$RUNNER_TEMP/nanoom-timing"; mkdir -p "$sample_dir"; assignment_id=$(jq -r '.assignmentId // "legacy"' <<<"$entry"); sample_name=$(printf '%s-%s' "${GITHUB_JOB:-local}" "$assignment_id" | tr -c 'A-Za-z0-9._-' '-' | cut -c1-80); sample_path="$sample_dir/$sample_name.json"
  jq -n --arg group "$GROUP" --arg environment "$TIMING_ENVIRONMENT" --argjson entry "$entry" --argjson results "$results" '{batch:{assignmentId:($entry.assignmentId // "legacy"),predictedDurationMs:($entry.predictedDurationMs // 0),checkoutPathCount:($entry.checkoutPathCount // 0)},samples:[$results[] | .item as $item | .cli.executions[] | {group:$group,workspace:.workspace,task:$item.task,shard:$item.shard,totalShards:$item.totalShards,runner:.runner,environment:$environment,durationMs:.durationMs} | with_entries(select(.value != null))]}' > "$sample_path"
  echo "sample-path=$sample_path" >> "$GITHUB_OUTPUT"; echo "sample-name=nanoom-timing-sample-v2-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT-$sample_name" >> "$GITHUB_OUTPUT"; echo "upload-started=$(date +%s)" >> "$GITHUB_OUTPUT"
fi
printf '  Result\n    ✓ items=%s; elapsed=%ss\n  Final JSON\n    %s\n' "$(jq length <<<"$results")" "$elapsed" "$result"
{ echo '### nanoom run'; echo; echo "**Result:** $(jq length <<<"$results") assignment items succeeded in ${elapsed}s."; } >> "$GITHUB_STEP_SUMMARY"
