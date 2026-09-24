#!/usr/bin/env bash
set -Eeuo pipefail
started=$(date +%s)
source "$GITHUB_ACTION_PATH/../_setup/artifacts.sh"
artifact_version=$(nanoom_artifact_version "$SCHEDULER" "${ARTIFACT_VERSION:-}")
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
  measurement_dir=${MEASUREMENT_DIR:-"$RUNNER_TEMP/nanoom-current-measurements"}
  output_dir="$RUNNER_TEMP/nanoom-v3-history-$RUN_ID-$RUN_ATTEMPT"
  model_path="$output_dir/model-v3.json"
  prediction_path="$output_dir/prediction-v3.json"
  mkdir -p "$output_dir"
  current_inputs=(); current_files=(); current_file_count=0
  while IFS= read -r path; do
    current_inputs+=(--input "$path"); current_files+=("$path"); current_file_count=$((current_file_count + 1))
  done < <(find "$measurement_dir" -type f -name '*.json' -print 2>/dev/null | sort)
  current_summary=$(jq -cn --argjson count "$current_file_count" '{measurementFileCount:$count}')
  source_run_id=''; previous_model_name=''; previous_prediction_path=''; previous_model_path=''; previous_model_unavailable=false

  nanoom_history_budget_start 60 25165824
  nanoom_try_previous_model() {
    local candidate_run=$1 candidate_dir=$2 candidate_artifacts pointer
    [[ -n "$candidate_run" ]] || return 1
    candidate_artifacts=$(nanoom_run_artifacts "$candidate_run") || return 1
    nanoom_download_artifact_bounded "$candidate_artifacts" "$HISTORY_ARTIFACT" "$candidate_dir/prediction" 8388608 8388608 || return 1
    previous_prediction_path=$(find "$candidate_dir/prediction" -maxdepth 1 -type f -name '*.json' -print | sort | head -n 1)
    [[ -n "$previous_prediction_path" ]] || return 1
    pointer=$(jq -er '.predictions[0].modelArtifact | select(.name and .sha256)' "$previous_prediction_path" 2>/dev/null) || return 1
    previous_model_name=$(jq -er .name <<<"$pointer") || return 1
    nanoom_download_artifact_bounded "$candidate_artifacts" "$previous_model_name" "$candidate_dir/model" 16777216 16777216 || {
      previous_model_unavailable=true
      return 1
    }
    previous_model_path=$(find "$candidate_dir/model" -maxdepth 1 -type f -name '*.json' -print | sort | head -n 1)
    [[ -n "$previous_model_path" ]]
  }

  case "${GITHUB_EVENT_NAME:-}" in
    pull_request|pull_request_target)
      candidate_run=$(nanoom_previous_successful_run_for_event "$WORKFLOW_REF" "$PR_HEAD_REF" "$RUN_ID" pull_request "$PR_NUMBER" "$PR_HEAD_REPOSITORY_ID")
      if nanoom_try_previous_model "$candidate_run" "$RUNNER_TEMP/nanoom-prev-pr"; then
        source_run_id=$candidate_run
      else
        candidate_run=$(nanoom_previous_successful_run_for_event "$WORKFLOW_REF" "$PR_BASE_REF" "$RUN_ID" push)
        if nanoom_try_previous_model "$candidate_run" "$RUNNER_TEMP/nanoom-prev-base"; then source_run_id=$candidate_run; fi
      fi
      ;;
    *)
      candidate_run=$(nanoom_previous_successful_run_for_event "$WORKFLOW_REF" "$HISTORY_REF" "$RUN_ID" push)
      if nanoom_try_previous_model "$candidate_run" "$RUNNER_TEMP/nanoom-prev-push"; then source_run_id=$candidate_run; fi
      ;;
  esac

  args=(history --model-output "$model_path" --prediction-output "$prediction_path" --model-artifact-name "$MODEL_ARTIFACT" --run-id "$RUN_ID" --run-attempt "$RUN_ATTEMPT")
  if (( current_file_count > 0 )); then args+=("${current_inputs[@]}"); fi
  if [[ -n "$previous_model_path" && -n "$previous_prediction_path" ]]; then
    args+=(--previous-model "$previous_model_path" --previous-prediction "$previous_prediction_path" --previous-model-name "$previous_model_name")
  fi
  cli_result=$(nanoom "${args[@]}")
  if [[ "${MEASUREMENT_DOWNLOAD_OUTCOME:-success}" != success ]]; then
    cli_result=$(jq -c '.status="degraded" | .measurementDownloadDegraded=true' <<<"$cli_result")
  fi
  if [[ "$previous_model_unavailable" == true ]]; then
    cli_result=$(jq -c '.status="degraded" | .previousModelDegraded=true' <<<"$cli_result")
  fi
  scope_count=$(jq -r '.scopeCount // 0' <<<"$cli_result")
  publish=false
  if (( scope_count > 0 )); then
    publish=true
    echo "model-path=$model_path" >> "$GITHUB_OUTPUT"
    echo "model-name=$MODEL_ARTIFACT" >> "$GITHUB_OUTPUT"
    echo "prediction-path=$prediction_path" >> "$GITHUB_OUTPUT"
    echo "prediction-name=$HISTORY_ARTIFACT" >> "$GITHUB_OUTPUT"
  else
    echo 'no usable model states were produced; prediction publish marker will be skipped' >&2
  fi
  elapsed=$(( $(date +%s) - started ))
  result=$(jq -cn --argjson cli "$cli_result" --argjson current "$current_summary" --arg sourceRun "$source_run_id" --argjson elapsed "$elapsed" --arg artifactVersion "$artifact_version" --argjson publish "$publish" '{status:$cli.status,scheduler:"artifact",artifactVersion:$artifactVersion,publish:$publish,sourceRunId:(if $sourceRun == "" then null else ($sourceRun|tonumber) end),current:$current,cli:$cli,mergeMs:($elapsed*1000)}')
  echo "publish=$publish" >> "$GITHUB_OUTPUT"
  echo "result=$result" >> "$GITHUB_OUTPUT"
  if [[ "$publish" == true ]]; then echo "upload-started=$(date +%s)" >> "$GITHUB_OUTPUT"; fi
  printf 'Final JSON\n%s\n' "$result"
  { echo '### nanoom history'; echo; echo "PredictionState v3 compilation finished with status `$(jq -r .status <<<"$cli_result")`."; } >> "$GITHUB_STEP_SUMMARY"
  exit 0
fi
echo "result=$result" >> "$GITHUB_OUTPUT"
printf 'Final JSON\n%s\n' "$result"
{ echo '### nanoom history'; echo; echo "`$SCHEDULER` timing lifecycle completed."; } >> "$GITHUB_STEP_SUMMARY"
