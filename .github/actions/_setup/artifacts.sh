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

nanoom_run_artifacts() {
  local run_id=$1
  local page=1 response count all='[]'
  while :; do
    response=$(curl --fail --silent --show-error \
      -H "Authorization: Bearer $TOKEN" \
      -H 'Accept: application/vnd.github+json' \
      "$API/repos/$REPOSITORY/actions/runs/$run_id/artifacts?per_page=100&page=$page")
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

nanoom_artifact_exists() {
  local artifacts=$1 mode=$2 pattern=$3
  if [[ "$mode" == exact ]]; then
    jq -e --arg pattern "$pattern" 'any(.artifacts[]?; (.expired | not) and .name == $pattern)' >/dev/null <<<"$artifacts"
  else
    jq -e --arg pattern "$pattern" 'any(.artifacts[]?; (.expired | not) and (.name | startswith($pattern)))' >/dev/null <<<"$artifacts"
  fi
}
