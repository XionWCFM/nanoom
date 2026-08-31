#!/usr/bin/env bash
set -Eeuo pipefail
started=$(date +%s)
if [[ "$SCHEDULER" == http ]]; then
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
  current_inputs=(); while IFS= read -r path; do current_inputs+=(--input "$path"); done < <(find "$sample_dir" -type f -name '*.json' -print | sort)
  (( ${#current_inputs[@]} > 0 )) || { echo 'no successful timing samples were downloaded' >&2; false; }
  previous="$RUNNER_TEMP/nanoom-previous-history.zip"; previous_json="$RUNNER_TEMP/nanoom-previous-history.json"
  set +e
  artifacts=$(curl --fail --silent --show-error -H "Authorization: Bearer $TOKEN" -H 'Accept: application/vnd.github+json' "$API/repos/$REPOSITORY/actions/artifacts?name=$HISTORY_ARTIFACT&per_page=1")
  url=$(jq -r '.artifacts | map(select(.expired | not)) | first | .archive_download_url // empty' <<<"$artifacts" 2>/dev/null)
  [[ -n "$url" ]] && curl --fail --silent --show-error -L -H "Authorization: Bearer $TOKEN" "$url" -o "$previous" && unzip -p "$previous" history.json > "$previous_json" && jq -e '.samples | type == "array"' "$previous_json" >/dev/null
  previous_ok=$?; set -e
  inputs=(); (( previous_ok == 0 )) && inputs+=(--input "$previous_json"); inputs+=("${current_inputs[@]}")
  cli_result=$(nanoom history "${inputs[@]}" --output "$output")
  elapsed=$(( $(date +%s) - started )); result=$(jq -cn --argjson cli "$cli_result" --argjson elapsed "$elapsed" '{status:"success",scheduler:"artifact",cli:$cli,mergeMs:($elapsed*1000)}')
  echo "history-path=$output" >> "$GITHUB_OUTPUT"
  echo "upload-started=$(date +%s)" >> "$GITHUB_OUTPUT"
fi
echo "result=$result" >> "$GITHUB_OUTPUT"
printf 'Final JSON\n%s\n' "$result"
{ echo '### nanoom history'; echo; echo "\`$SCHEDULER\` timing lifecycle completed."; } >> "$GITHUB_STEP_SUMMARY"
