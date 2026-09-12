#!/usr/bin/env bash
set -Eeuo pipefail
started=$(date +%s)
source "$GITHUB_ACTION_PATH/../_setup/artifacts.sh"
if [[ "$SCHEDULER" == off ]]; then
  result='{"status":"success","scheduler":"off","reason":"historical scheduling explicitly disabled"}'
elif [[ "$SCHEDULER" == http ]]; then
  [[ "$COORDINATOR_URL" == https://* && -n "$COORDINATOR_TOKEN" ]] || { echo 'scheduler=http requires an HTTPS coordinatorUrl and NANOOM_COORDINATOR_TOKEN' >&2; false; }
  coordinator=${COORDINATOR_URL%/}
  count=0
  while IFS= read -r run_id; do
    run_key=$(jq -rn --arg value "$run_id" '$value | @uri')
    curl --fail-with-body --silent --show-error -X POST -H "Authorization: Bearer $COORDINATOR_TOKEN" -H 'Content-Type: application/json' -H "Idempotency-Key: $run_key:complete" "$coordinator/v1/runs/$run_key/complete" --data '{"status":"success"}' >/dev/null
    count=$((count + 1))
  done < <(jq -er '.[]' <<<"$RUN_IDS")
  result=$(jq -cn --argjson count "$count" '{status:"success",scheduler:"http",completedRuns:$count}')
else
  [[ "$SCHEDULER" == artifact ]] || { echo 'scheduler must be artifact or http' >&2; false; }
  sample_dir="$RUNNER_TEMP/nanoom-timing-samples"; output="$RUNNER_TEMP/nanoom-timing-history/history.json"; mkdir -p "$(dirname "$output")"
  current_artifacts=$(nanoom_run_artifacts "$RUN_ID")
  nanoom_download_artifacts "$current_artifacts" prefix "nanoom-timing-sample-v2-$RUN_ID-$RUN_ATTEMPT-" "$sample_dir" || {
    echo 'no successful timing sample artifacts were uploaded for this run attempt' >&2
    false
  }
  current_inputs=(); current_files=(); while IFS= read -r path; do current_inputs+=(--input "$path"); current_files+=("$path"); done < <(find "$sample_dir" -type f -name '*.json' -print | sort)
  (( ${#current_inputs[@]} > 0 )) || { echo 'no successful timing samples were downloaded' >&2; false; }
  current_summary=$(jq -s '{assignmentCount:length,predictedMakespanMs:(map(.batch.predictedDurationMs // 0) | max),actualTaskMakespanMs:(map([.samples[].durationMs] | add // 0) | max),totalCheckoutPathCount:(map(.batch.checkoutPathCount // 0) | add)}' "${current_files[@]}")
  source_run_id=$(nanoom_previous_successful_run "$WORKFLOW_REF" "$HISTORY_REF" "$RUN_ID")
  previous_dir="$RUNNER_TEMP/nanoom-previous-history"; previous_json="$previous_dir/history.json"
  inputs=()
  if [[ -n "$source_run_id" ]]; then
    previous_artifacts=$(nanoom_run_artifacts "$source_run_id")
    if nanoom_download_artifacts "$previous_artifacts" exact "$HISTORY_ARTIFACT" "$previous_dir" && jq -e '.samples | type == "array"' "$previous_json" >/dev/null; then
      inputs+=(--input "$previous_json")
    fi
  fi
  inputs+=("${current_inputs[@]}")
  cli_result=$(nanoom history "${inputs[@]}" --output "$output")
  elapsed=$(( $(date +%s) - started )); result=$(jq -cn --argjson cli "$cli_result" --argjson current "$current_summary" --arg sourceRun "$source_run_id" --argjson elapsed "$elapsed" '{status:"success",scheduler:"artifact",historySourceRunId:(if $sourceRun == "" then null else ($sourceRun | tonumber) end),current:$current,cli:$cli,mergeMs:($elapsed*1000)}')
  echo "history-path=$output" >> "$GITHUB_OUTPUT"
  echo "upload-started=$(date +%s)" >> "$GITHUB_OUTPUT"
fi
echo "result=$result" >> "$GITHUB_OUTPUT"
printf 'Final JSON\n%s\n' "$result"
{ echo '### nanoom history'; echo; echo "\`$SCHEDULER\` timing lifecycle completed."; } >> "$GITHUB_STEP_SUMMARY"
