#!/usr/bin/env bash
set -Eeuo pipefail
started=$(date +%s)
source "$GITHUB_ACTION_PATH/../_setup/artifacts.sh"
artifact_version=$(nanoom_artifact_version "$SCHEDULER" "${ARTIFACT_VERSION:-}")
history_backend=${HISTORY_BACKEND:-artifact}
HISTORY_SERVER_URL=${HISTORY_SERVER_URL:-}
HISTORY_SERVER_TOKEN=${HISTORY_SERVER_TOKEN:-}
[[ "$history_backend" =~ ^(artifact|server)$ ]] || { echo 'historyBackend must be artifact or server' >&2; false; }
if [[ "$history_backend" == server && "$SCHEDULER" != artifact ]]; then
  echo 'historyBackend=server requires scheduler=artifact so successful task measurements are available' >&2
  false
fi
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

  if [[ "$history_backend" == artifact ]]; then
    nanoom_history_budget_start 60 25165824
  nanoom_try_previous_model() {
    local candidate_run=$1 candidate_dir=$2 candidate_artifacts pointer
    [[ -n "$candidate_run" ]] || return 1
    candidate_artifacts=$(nanoom_run_artifacts "$candidate_run") || return 1
    nanoom_download_artifact_bounded "$candidate_artifacts" "$HISTORY_ARTIFACT" "$candidate_dir/prediction" 8388608 8388608 || return 1
    previous_prediction_path=$(find "$candidate_dir/prediction" -maxdepth 1 -type f -name '*.json' -print | sort | head -n 1)
    [[ -n "$previous_prediction_path" ]] || return 1
    pointer=$(nanoom_history_timeout jq -er '.predictions[0].modelArtifact | select(.name and .sha256)' "$previous_prediction_path" 2>/dev/null) || return 1
    previous_model_name=$(nanoom_history_timeout jq -er .name <<<"$pointer") || return 1
    nanoom_download_artifact_bounded "$candidate_artifacts" "$previous_model_name" "$candidate_dir/model" 16777216 16777216 || {
      previous_model_unavailable=true
      return 1
    }
    previous_model_path=$(find "$candidate_dir/model" -maxdepth 1 -type f -name '*.json' -print | sort | head -n 1)
    [[ -n "$previous_model_path" ]]
  }
  nanoom_try_previous_model_for_event() {
    local branch=$1 event=$2 pr_number=$3 head_repository_id=$4 candidate_run
    while IFS= read -r candidate_run; do
      [[ -n "$candidate_run" ]] || continue
      if nanoom_try_previous_model "$candidate_run" "$RUNNER_TEMP/nanoom-prev-$candidate_run"; then
        source_run_id=$candidate_run
        return 0
      fi
    done < <(nanoom_previous_successful_runs_for_event "$WORKFLOW_REF" "$branch" "$RUN_ID" "$event" "$pr_number" "$head_repository_id")
    return 1
  }

  case "${GITHUB_EVENT_NAME:-}" in
    pull_request|pull_request_target)
      if ! nanoom_try_previous_model_for_event "$PR_HEAD_REF" pull_request "$PR_NUMBER" "$PR_HEAD_REPOSITORY_ID"; then
        nanoom_try_previous_model_for_event "$PR_BASE_REF" push '' '' || true
      fi
      ;;
    *)
      nanoom_try_previous_model_for_event "$HISTORY_REF" push '' '' || true
      ;;
  esac
  fi

  args=(history --model-output "$model_path" --prediction-output "$prediction_path" --model-artifact-name "$MODEL_ARTIFACT" --run-id "$RUN_ID" --run-attempt "$RUN_ATTEMPT")
  batch_dir="$output_dir/batches"
  if [[ "$history_backend" == server ]]; then
    args+=(--batch-output-dir "$batch_dir")
  fi
  if (( current_file_count > 0 )); then args+=("${current_inputs[@]}"); fi
  if [[ -n "$previous_model_path" && -n "$previous_prediction_path" ]]; then
    args+=(--previous-model "$previous_model_path" --previous-prediction "$previous_prediction_path" --previous-model-name "$previous_model_name")
  fi
  compile_failed=false
  if [[ "$history_backend" == server ]]; then
    nanoom_history_budget_start 60 25165824
    if ! cli_result=$(nanoom_history_timeout nanoom "${args[@]}"); then
      compile_failed=true
      cli_result='{"status":"degraded","historyBatchCompileFailed":true,"scopeCount":0,"emittedBatchCount":0}'
      echo 'could not compile History Server batches; task CI remains successful in degraded mode' >&2
    fi
  else
    cli_result=$(nanoom "${args[@]}")
  fi
  if [[ "${MEASUREMENT_DOWNLOAD_OUTCOME:-success}" != success ]]; then
    cli_result=$(jq -c '.status="degraded" | .measurementDownloadDegraded=true' <<<"$cli_result")
  fi
  if [[ "$history_backend" == server ]]; then
    applied_count=0; duplicate_count=0; batch_count=0; server_degraded=$compile_failed
    if [[ "${MEASUREMENT_DOWNLOAD_OUTCOME:-success}" != success ]]; then
      server_degraded=true
    fi
    if [[ -d "$batch_dir" ]]; then
      while IFS= read -r batch_path; do
        batch_count=$((batch_count + 1))
        scope_id=${batch_path##*/}
        scope_id=${scope_id%.json}
        [[ "$scope_id" =~ ^[a-f0-9]{64}$ ]] || { server_degraded=true; continue; }
        if command -v sha256sum >/dev/null 2>&1; then
          idempotency_key=$(sha256sum "$batch_path" | awk '{print $1}')
        else
          idempotency_key=$(shasum -a 256 "$batch_path" | awk '{print $1}')
        fi
        repository_key=$(jq -er '.scope.repositoryKey' "$batch_path") || { server_degraded=true; continue; }
        if ! nanoom_history_server_trusted_event; then
          server_degraded=true
          echo 'History Server write skipped for an event that may run untrusted code' >&2
          break
        fi
        if [[ -z "$HISTORY_SERVER_TOKEN" ]] || ! nanoom_history_server_url_valid "$HISTORY_SERVER_URL"; then
          server_degraded=true
          echo 'History Server URL or credential is unavailable; measurements were not merged' >&2
          break
        fi
        [[ "$repository_key" =~ ^[a-z0-9][a-z0-9._-]{0,63}$ ]] || { server_degraded=true; continue; }
        history_server_base=${HISTORY_SERVER_URL%/}
        response_path="$output_dir/merge-$scope_id.json"
        merged=false
        for attempt in 1 2 3; do
          remaining=$(nanoom_history_remaining) || break
          status_code=$(curl --silent --show-error --max-time "$remaining" --max-filesize 1048576 \
            -X POST \
            -H "Authorization: Bearer $HISTORY_SERVER_TOKEN" \
            -H 'Content-Type: application/json' \
            -H "Idempotency-Key: $idempotency_key" \
            -H 'Accept: application/json' \
            --data-binary "@$batch_path" \
            -o "$response_path" -w '%{http_code}' \
            "$history_server_base/v1/repositories/$repository_key/scopes/$scope_id/observations:merge") || status_code=000
          if [[ "$status_code" == 200 ]]; then
            applied=$(jq -r '.applied | select(type == "boolean")' "$response_path") || break
            [[ "$applied" == true || "$applied" == false ]] || break
            if [[ "$applied" == true ]]; then
              applied_count=$((applied_count + 1))
            else
              duplicate_count=$((duplicate_count + 1))
            fi
            merged=true
            break
          fi
          [[ "$status_code" == 000 || "$status_code" =~ ^5 ]] || break
        done
        if [[ "$merged" != true ]]; then
          server_degraded=true
          echo "History Server merge failed for scope $scope_id; continuing task CI in degraded mode" >&2
        fi
      done < <(find "$batch_dir" -type f -name '*.json' -print 2>/dev/null | sort)
    fi
    if [[ "$batch_count" -gt 0 && "$server_degraded" == false ]]; then
      server_status=merged
    elif [[ "$batch_count" -eq 0 && "$server_degraded" == false ]]; then
      server_status=no_observations
    else
      server_status=degraded
      cli_result=$(jq -c '.status="degraded" | .historyServerDegraded=true' <<<"$cli_result")
    fi
    if [[ "$server_degraded" == true && "$(jq -r '.historyServerDegraded // false' <<<"$cli_result")" != true ]]; then
      cli_result=$(jq -c '.status="degraded" | .historyServerDegraded=true' <<<"$cli_result")
    fi
    elapsed=$(( $(date +%s) - started ))
    result=$(jq -cn --argjson cli "$cli_result" --argjson current "$current_summary" --arg artifactVersion "$artifact_version" --arg serverStatus "$server_status" --argjson batches "$batch_count" --argjson applied "$applied_count" --argjson duplicates "$duplicate_count" --argjson elapsed "$elapsed" '{status:$cli.status,scheduler:"artifact",historyBackend:"server",artifactVersion:$artifactVersion,publish:false,sourceRunId:null,current:$current,cli:$cli,serverMerge:{status:$serverStatus,batchCount:$batches,appliedBatchCount:$applied,duplicateBatchCount:$duplicates},mergeMs:($elapsed*1000)}')
    echo 'publish=false' >> "$GITHUB_OUTPUT"
    echo "result=$result" >> "$GITHUB_OUTPUT"
    printf 'Final JSON\n%s\n' "$result"
    { echo '### nanoom history'; echo; echo "History Server batch merge finished with status \`$server_status\` ($applied_count applied, $duplicate_count duplicate)."; } >> "$GITHUB_STEP_SUMMARY"
    exit 0
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
  { echo '### nanoom history'; echo; printf 'PredictionState v3 compilation finished with status `%s`.\n' "$(jq -r .status <<<"$cli_result")"; } >> "$GITHUB_STEP_SUMMARY"
  exit 0
fi
echo "result=$result" >> "$GITHUB_OUTPUT"
printf 'Final JSON\n%s\n' "$result"
{ echo '### nanoom history'; echo; printf '`%s` timing lifecycle completed.\n' "$SCHEDULER"; } >> "$GITHUB_STEP_SUMMARY"
