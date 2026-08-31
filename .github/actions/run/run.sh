#!/usr/bin/env bash
set -Eeuo pipefail
ACTION_NAME=run ACTION_CWD=$CWD ACTION_PHASE=input-validation ACTION_COMMAND=not-started
source "$GITHUB_ACTION_PATH/../_setup/log.sh"; trap 'nanoom_fail "$?"' ERR
started=$(date +%s)
entry=$(jq -ce '(.include[0] // .)' <<<"$MATRIX"); mode=$(jq -r '.mode // "static"' <<<"$entry")
[[ "$SCHEDULER" =~ ^(off|artifact|http)$ ]] || { echo "invalid scheduler: $SCHEDULER" >&2; false; }

run_item() {
  local item=$1 group task name cli_result
  group=$(jq -r --arg fallback "$GROUP" '.group // $fallback' <<<"$item"); task=$(jq -er .task <<<"$item"); name=$(jq -er .name <<<"$item")
  local args=(-C "$CWD" run "$group" "$task" --all --filter "$name" --json)
  [[ -n $(jq -r '.shard // empty' <<<"$item") ]] && args+=(--shard "$(jq -r .shard <<<"$item")" --total-shards "$(jq -r .totalShards <<<"$item")")
  if [[ "$TOOL" == turbo && ! -x "$CWD/node_modules/.bin/turbo" ]]; then args+=(--runner "$PM"); elif [[ "$TOOL" != auto ]]; then args+=(--runner "$TOOL"); fi
  printf -v ACTION_COMMAND '%q ' nanoom "${args[@]}"; ACTION_COMMAND=${ACTION_COMMAND% }
  printf '  ▶ %s / %s / %s\n    cwd: %s\n    command: %s\n' "$group" "$name" "$task" "$CWD" "$ACTION_COMMAND" >&2
  cli_result=$(nanoom "${args[@]}")
  jq -cn --argjson item "$item" --arg command "$ACTION_COMMAND" --argjson cli "$cli_result" '{item:$item,command:$command,cli:$cli}'
}

results='[]'
if [[ "$mode" == continuous ]]; then
  [[ "$SCHEDULER" == http ]] || { echo 'continuous matrix requires scheduler=http' >&2; false; }
  [[ "$COORDINATOR_URL" == https://* && -n "$COORDINATOR_TOKEN" ]] || { echo 'scheduler=http requires an HTTPS coordinatorUrl and NANOOM_COORDINATOR_TOKEN' >&2; false; }
  run_id=$(jq -er .runId <<<"$entry"); agent_id=$(jq -er .agentId <<<"$entry")
  claim_index=0
  while :; do
    claim_index=$((claim_index + 1))
    claim=$(curl --fail-with-body --silent --show-error -X POST -H "Authorization: Bearer $COORDINATOR_TOKEN" -H 'Content-Type: application/json' -H "Idempotency-Key: $run_id:$agent_id:claim:$claim_index" "$COORDINATOR_URL/v1/runs/$run_id/claims" --data "$(jq -cn --arg agentId "$agent_id" '{agentId:$agentId}')")
    item=$(jq -c '.item // empty' <<<"$claim"); [[ -n "$item" ]] || break; item_id=$(jq -er .itemId <<<"$claim")
    (while sleep 30; do curl --fail --silent -X PATCH -H "Authorization: Bearer $COORDINATOR_TOKEN" -H 'Content-Type: application/json' -H "Idempotency-Key: $run_id:$item_id:heartbeat" "$COORDINATOR_URL/v1/runs/$run_id/claims/$item_id" --data '{"status":"heartbeat"}' >/dev/null || exit; done) & heartbeat_pid=$!
    if item_result=$(run_item "$item"); then
      kill "$heartbeat_pid" 2>/dev/null || true; wait "$heartbeat_pid" 2>/dev/null || true
      duration=$(jq -r '.cli.executions[0].durationMs' <<<"$item_result")
      curl --fail-with-body --silent --show-error -X PATCH -H "Authorization: Bearer $COORDINATOR_TOKEN" -H 'Content-Type: application/json' -H "Idempotency-Key: $run_id:$item_id:success" "$COORDINATOR_URL/v1/runs/$run_id/claims/$item_id" --data "$(jq -cn --argjson durationMs "$duration" '{status:"success",durationMs:$durationMs}')" >/dev/null
      results=$(jq -c --argjson result "$item_result" '. + [$result]' <<<"$results")
    else
      kill "$heartbeat_pid" 2>/dev/null || true; wait "$heartbeat_pid" 2>/dev/null || true
      curl --fail-with-body --silent --show-error -X PATCH -H "Authorization: Bearer $COORDINATOR_TOKEN" -H 'Content-Type: application/json' -H "Idempotency-Key: $run_id:$item_id:failure" "$COORDINATOR_URL/v1/runs/$run_id/claims/$item_id" --data '{"status":"failure"}' >/dev/null; exit 1
    fi
  done
else
  items=$(jq -c 'if .items then .items elif .name then [.] else error("matrix entry must contain items or name") end' <<<"$entry")
  while IFS= read -r item; do item_result=$(run_item "$item"); results=$(jq -c --argjson result "$item_result" '. + [$result]' <<<"$results"); done < <(jq -c '.[]' <<<"$items")
fi

elapsed=$(( $(date +%s) - started )); matrix_json=$(jq -c '{assignmentId,agentId,runId,mode,predictedDurationMs,items} | with_entries(select(.value != null))' <<<"$entry")
result=$(jq -cn --argjson matrix "$matrix_json" --argjson results "$results" --argjson elapsed "$elapsed" '{status:"success",reason:"executed assignment items in order",matrix:$matrix,results:$results,elapsedSeconds:$elapsed}'); echo "result=$result" >> "$GITHUB_OUTPUT"
if [[ "$SCHEDULER" == artifact ]]; then
  sample_dir="$RUNNER_TEMP/nanoom-timing"; mkdir -p "$sample_dir"; sample_name=$(jq -r '.assignmentId // "legacy"' <<<"$entry"); sample_path="$sample_dir/$sample_name.json"
  jq -n --arg group "$GROUP" --arg environment "$TIMING_ENVIRONMENT" --argjson results "$results" '{samples:[$results[] | .item as $item | .cli.executions[] | {group:$group,workspace:.workspace,task:$item.task,shard:$item.shard,runner:.runner,environment:$environment,durationMs:.durationMs}]}' > "$sample_path"
  echo "sample-path=$sample_path" >> "$GITHUB_OUTPUT"; echo "sample-name=nanoom-timing-sample-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT-$sample_name" >> "$GITHUB_OUTPUT"
fi
printf '  Result\n    ✓ items=%s; elapsed=%ss\n  Final JSON\n    %s\n' "$(jq length <<<"$results")" "$elapsed" "$result"
{ echo '### nanoom run'; echo; echo "**Result:** $(jq length <<<"$results") assignment items succeeded in ${elapsed}s."; } >> "$GITHUB_STEP_SUMMARY"
