#!/usr/bin/env bash

nanoom_artifact_version() {
  local scheduler=$1 version=${2:-v4}
  [[ "$scheduler" != artifact || "$version" =~ ^(v3|v4)$ ]] || {
    echo "artifactVersion must be v4 or v3" >&2
    return 1
  }
  printf '%s' "$version"
}

nanoom_workflow_file() {
  local value=${1#*/}
  value=${value#*/}
  printf '%s' "${value%@*}"
}

nanoom_prediction_identity() {
  local event=$1 workflow_ref=$2 repository_id=$3 server_url=$4 git_ref=$5
  local pr_number=${6:-} pr_head_repository_id=${7:-} pr_head_ref=${8:-} pr_base_ref=${9:-}
  local repository_key workflow_path ref_json server_hash
  [[ "$repository_id" =~ ^[0-9]+$ && -n "$server_url" ]] || return 1
  workflow_path=$(nanoom_workflow_file "$workflow_ref")
  [[ -n "$workflow_path" ]] || return 1
  if [[ "$server_url" == https://github.com ]]; then
    repository_key="github-$repository_id"
  else
    if command -v sha256sum >/dev/null 2>&1; then
      server_hash=$(printf '%s' "$server_url" | sha256sum | cut -c1-12)
    else
      server_hash=$(printf '%s' "$server_url" | shasum -a 256 | cut -c1-12)
    fi
    repository_key="ghes-$server_hash-$repository_id"
  fi
  case "$event" in
    pull_request|pull_request_target)
      [[ "$pr_number" =~ ^[1-9][0-9]*$ && "$pr_head_repository_id" =~ ^[0-9]+$ && -n "$pr_head_ref" && -n "$pr_base_ref" ]] || return 1
      ref_json=$(jq -cn --arg number "$pr_number" --arg repo "$pr_head_repository_id" --arg head "$pr_head_ref" --arg base "$pr_base_ref" '{kind:"pull_request",number:($number|tonumber),headRepositoryId:$repo,headRef:("refs/heads/"+$head),baseRef:("refs/heads/"+$base)}')
      ;;
    *)
      [[ "$git_ref" == refs/heads/* ]] || return 1
      ref_json=$(jq -cn --arg ref "$git_ref" '{kind:"push",ref:$ref}')
      ;;
  esac
  jq -cn --arg repositoryKey "$repository_key" --arg workflowPath "$workflow_path" --argjson ref "$ref_json" '{repositoryKey:$repositoryKey,workflowPath:$workflowPath,ref:$ref}'
}

nanoom_previous_successful_run() {
  local workflow_ref=$1 branch=$2 current_run=$3
  local workflow encoded_workflow encoded_branch response
  workflow=$(nanoom_workflow_file "$workflow_ref")
  [[ -n "$workflow" && -n "$branch" ]] || return 0
  encoded_workflow=$(jq -rn --arg value "$workflow" '$value | @uri')
  encoded_branch=$(jq -rn --arg value "$branch" '$value | @uri')
  response=$(curl --fail --silent --show-error \
    -H "Authorization: Bearer $TOKEN" \
    -H 'Accept: application/vnd.github+json' \
    "$API/repos/$REPOSITORY/actions/workflows/$encoded_workflow/runs?branch=$encoded_branch&status=success&per_page=100")
  jq -r --arg current "$current_run" \
    '[.workflow_runs[]? | select(.status == "completed" and .conclusion == "success") | select((.id | tostring) != $current)] | sort_by(.created_at, .id) | .[-1].id // empty' \
    <<<"$response"
}

nanoom_now_ms() {
  local now
  now=$(date +%s%3N 2>/dev/null || true)
  if [[ "$now" =~ ^[0-9]+$ ]]; then
    printf '%s' "$now"
  elif command -v python3 >/dev/null 2>&1; then
    python3 -c 'import time; print(time.monotonic_ns() // 1_000_000)'
  else
    printf '%s' "$((SECONDS * 1000))"
  fi
}

nanoom_history_budget_start() {
  NANOOM_HISTORY_DEADLINE_MS=$(($(nanoom_now_ms) + ${1:-3} * 1000))
  NANOOM_HISTORY_BYTES=0
  NANOOM_HISTORY_MAX_BYTES=${2:-8388608}
}

nanoom_history_remaining() {
  local remaining_ms=$((NANOOM_HISTORY_DEADLINE_MS - $(nanoom_now_ms)))
  (( remaining_ms > 0 )) || return 1
  printf '%d.%03d' "$((remaining_ms / 1000))" "$((remaining_ms % 1000))"
}

nanoom_history_timeout() {
  local remaining
  remaining=$(nanoom_history_remaining) || return 124
  if command -v timeout >/dev/null 2>&1; then
    timeout --kill-after=0.1s "${remaining}s" "$@"
  elif command -v python3 >/dev/null 2>&1; then
    python3 -c 'import subprocess, sys
try:
    result = subprocess.run(sys.argv[2:], timeout=float(sys.argv[1]), check=False)
    sys.exit(result.returncode)
except subprocess.TimeoutExpired:
    sys.exit(124)' "$remaining" "$@"
  else
    return 124
  fi
}

nanoom_history_charge_bytes() {
  local bytes=$1
  (( bytes >= 0 && NANOOM_HISTORY_BYTES + bytes <= NANOOM_HISTORY_MAX_BYTES )) || return 1
  NANOOM_HISTORY_BYTES=$((NANOOM_HISTORY_BYTES + bytes))
}

nanoom_previous_successful_run_for_event() {
  local workflow_ref=$1 branch=$2 current_run=$3 event=$4 pr_number=${5:-} head_repository_id=${6:-}
  local workflow encoded_workflow encoded_branch response remaining
  workflow=$(nanoom_workflow_file "$workflow_ref")
  [[ -n "$workflow" && -n "$branch" ]] || return 0
  remaining=$(nanoom_history_remaining) || return 0
  encoded_workflow=$(jq -rn --arg value "$workflow" '$value | @uri')
  encoded_branch=$(jq -rn --arg value "$branch" '$value | @uri')
  response=$(curl --fail --silent --show-error --max-time "$remaining" --max-filesize 1048576 \
    -H "Authorization: Bearer $TOKEN" \
    -H 'Accept: application/vnd.github+json' \
    "$API/repos/$REPOSITORY/actions/workflows/$encoded_workflow/runs?branch=$encoded_branch&event=$event&status=success&per_page=20") || return 0
  nanoom_history_charge_bytes "$(LC_ALL=C printf '%s' "$response" | wc -c | tr -d ' ')" || return 0
  jq -r --arg current "$current_run" --arg event "$event" --arg branch "$branch" \
    --arg pr "$pr_number" --arg headRepository "$head_repository_id" \
    '[.workflow_runs[]? | select(.status == "completed" and .conclusion == "success") | select((.id | tostring) != $current) | select((.created_at | fromdateiso8601) >= (now - 2592000)) | select(if $event == "pull_request" then .head_branch == $branch and ((.head_repository.id // "") | tostring) == $headRepository and any(.pull_requests[]?; (.number | tostring) == $pr) else true end)] | sort_by(.created_at, .id) | .[-1].id // empty' \
    <<<"$response" || return 0
}

nanoom_run_artifacts() {
  local run_id=$1
  local page=1 response count all='[]' remaining
  while :; do
    local budget_options=()
    if [[ -n ${NANOOM_HISTORY_DEADLINE_MS:-} ]]; then
      remaining=$(nanoom_history_remaining) || return 1
      budget_options+=(--max-time "$remaining" --max-filesize 1048576)
    fi
    response=$(curl --fail --silent --show-error \
      "${budget_options[@]}" \
      -H "Authorization: Bearer $TOKEN" \
      -H 'Accept: application/vnd.github+json' \
      "$API/repos/$REPOSITORY/actions/runs/$run_id/artifacts?per_page=100&page=$page")
    nanoom_history_charge_bytes "$(LC_ALL=C printf '%s' "$response" | wc -c | tr -d ' ')" || return 1
    all=$(jq -cn --argjson all "$all" --argjson page "$(jq -c '.artifacts // []' <<<"$response")" '$all + $page')
    count=$(jq '.artifacts // [] | length' <<<"$response")
    (( count == 100 )) || break
    page=$((page + 1))
  done
  jq -cn --argjson artifacts "$all" '{artifacts:$artifacts}'
}

nanoom_download_artifacts() {
  local artifacts=$1 mode=$2 pattern=$3 destination=$4
  local matches artifact_id artifact_name archive
  mkdir -p "$destination"
  if [[ "$mode" == exact ]]; then
    matches=$(jq -c --arg pattern "$pattern" '.artifacts[]? | select(.expired | not) | select(.name == $pattern)' <<<"$artifacts")
  else
    matches=$(jq -c --arg pattern "$pattern" '.artifacts[]? | select(.expired | not) | select(.name | startswith($pattern))' <<<"$artifacts")
  fi
  [[ -n "$matches" ]] || return 1
  while IFS= read -r artifact; do
    artifact_id=$(jq -r .id <<<"$artifact")
    artifact_name=$(jq -r .name <<<"$artifact")
    archive="$RUNNER_TEMP/${artifact_name}-${artifact_id}.zip"
    curl --fail --silent --show-error -L \
      -H "Authorization: Bearer $TOKEN" \
      -H 'Accept: application/vnd.github+json' \
      "$API/repos/$REPOSITORY/actions/artifacts/$artifact_id/zip" \
      -o "$archive"
    while IFS= read -r entry; do
      [[ "$entry" == */ ]] && continue
      [[ ! -e "$destination/$entry" ]] || {
        echo "artifact '$artifact_name' would overwrite '$entry' in '$destination'" >&2
        return 1
      }
    done < <(unzip -Z1 "$archive")
    unzip -oq "$archive" -d "$destination"
  done <<<"$matches"
}

nanoom_download_artifact_bounded() {
  local artifacts=$1 name=$2 destination=$3 archive_limit=$4 json_limit=$5
  local artifact size artifact_id archive actual entry output remaining
  artifact=$(jq -ce --arg name "$name" '.artifacts[]? | select((.expired | not) and .name == $name)' <<<"$artifacts") || return 1
  size=$(jq -er '.size_in_bytes | select(type == "number" and . >= 0)' <<<"$artifact") || return 1
  (( size <= archive_limit && NANOOM_HISTORY_BYTES + size <= NANOOM_HISTORY_MAX_BYTES )) || return 1
  remaining=$(nanoom_history_remaining) || return 1
  artifact_id=$(jq -er '.id | select(type == "number" or type == "string")' <<<"$artifact") || return 1
  mkdir -p "$destination"
  archive="$RUNNER_TEMP/nanoom-prediction-$artifact_id.zip"
  curl --fail --silent --show-error -L --max-time "$remaining" --max-filesize "$archive_limit" \
    -H "Authorization: Bearer $TOKEN" \
    -H 'Accept: application/vnd.github+json' \
    "$API/repos/$REPOSITORY/actions/artifacts/$artifact_id/zip" -o "$archive" || return 1
  actual=$(wc -c < "$archive" | tr -d ' ')
  (( actual <= archive_limit && NANOOM_HISTORY_BYTES + actual <= NANOOM_HISTORY_MAX_BYTES )) || return 1
  NANOOM_HISTORY_BYTES=$((NANOOM_HISTORY_BYTES + actual))
  entry=$(unzip -Z1 "$archive") || return 1
  [[ "$entry" =~ ^[A-Za-z0-9._-]+\.json$ ]] || return 1
  output="$destination/$(basename "$entry")"
  set +e
  nanoom_history_timeout unzip -p "$archive" "$entry" | head -c "$((json_limit + 1))" > "$output"
  local -a unzip_status=("${PIPESTATUS[@]}")
  set -e
  (( unzip_status[0] == 0 || unzip_status[0] == 141 )) || return 1
  actual=$(wc -c < "$output" | tr -d ' ')
  (( actual <= json_limit )) || return 1
  [[ -s "$output" ]]
}

nanoom_download_prediction_artifact() {
  nanoom_download_artifact_bounded "$1" "$2" "$3" 4194304 8388608
}

nanoom_artifact_exists() {
  local artifacts=$1 mode=$2 pattern=$3
  if [[ "$mode" == exact ]]; then
    jq -e --arg pattern "$pattern" 'any(.artifacts[]?; (.expired | not) and .name == $pattern)' >/dev/null <<<"$artifacts"
  else
    jq -e --arg pattern "$pattern" 'any(.artifacts[]?; (.expired | not) and (.name | startswith($pattern)))' >/dev/null <<<"$artifacts"
  fi
}
