#!/usr/bin/env bash

nanoom_validate_assignment_file() {
  local assignment_file=${1:?assignment file is required}
  local cwd=${2:?checkout cwd is required}
  local expected_reference=${3:-${PLAN:-}}
  local metadata_dir plan_file reference_file group assignment_id current_head expected_head

  [[ -f "$assignment_file" ]] || {
    echo "assignment file does not exist: $assignment_file" >&2
    return 1
  }
  metadata_dir=$(cd "$(dirname "$assignment_file")" && pwd -P)
  plan_file="$metadata_dir/plan-v1.json"
  reference_file="$metadata_dir/plan-reference.json"
  [[ -f "$plan_file" && -f "$reference_file" ]] || {
    echo "assignment metadata must include plan-v1.json and plan-reference.json beside assignment.json" >&2
    return 1
  }
  [[ -n "$expected_reference" ]] || {
    echo 'assignment validation requires the original Plan reference output from nanoom affected' >&2
    return 1
  }

  local workspace workspace_real cwd_path cwd_real
  workspace=${GITHUB_WORKSPACE:?GITHUB_WORKSPACE is required}
  workspace_real=$(cd "$workspace" && pwd -P)
  if [[ "$cwd" == /* ]]; then
    cwd_path=$cwd
  else
    cwd_path="$workspace_real/$cwd"
  fi
  [[ -d "$cwd_path" ]] || {
    echo "assignment checkout does not exist: $cwd_path" >&2
    return 1
  }
  cwd_real=$(cd "$cwd_path" && pwd -P)
  [[ "$cwd_real" == "$workspace_real/.nanoom/"* ]] || {
    echo "assignment checkout must be isolated below $workspace_real/.nanoom: $cwd_real" >&2
    return 1
  }

  [[ -n ${REPOSITORY:-} && -n ${WORKFLOW_REF:-} && -n ${RUN_ID:-} && -n ${RUN_ATTEMPT:-} && -n ${GITHUB_SHA:-} ]] || {
    echo "assignment validation requires repository, workflow, run, attempt, and head identity" >&2
    return 1
  }
  local normalized_expected
  normalized_expected=$(jq -cS --argjson attempt "$RUN_ATTEMPT" '.current.attempt=$attempt' <<<"$expected_reference") || return 1
  [[ "$normalized_expected" == "$(jq -cS . "$reference_file")" ]] || {
    echo 'assignment Plan reference no longer matches the original affected output' >&2
    return 1
  }
  jq -e \
    --arg repository "$REPOSITORY" \
    --arg workflow "$WORKFLOW_REF" \
    --arg run "$RUN_ID" \
    --arg attempt "$RUN_ATTEMPT" \
    --arg head "$GITHUB_SHA" \
    '.version == 1
      and .current.repository == $repository
      and .current.workflow == $workflow
      and .current.runId == $run
      and (.current.attempt | tostring) == $attempt
      and .current.head == $head
      and .provenance.head == $head
      and (.items | type == "array" and length > 0)' \
    "$assignment_file" >/dev/null || {
    echo "assignment file does not match the current run identity or has no planned items" >&2
    return 1
  }
  current_head=$(git -C "$cwd_real" rev-parse --verify 'HEAD^{commit}')
  expected_head=$(jq -er '.current.head' "$assignment_file")
  [[ "$current_head" == "$expected_head" ]] || {
    echo "assignment checkout HEAD mismatch: expected $expected_head, got $current_head" >&2
    return 1
  }

  group=$(jq -er '.group | select(type == "string" and length > 0)' "$assignment_file")
  assignment_id=$(jq -er '.assignmentId | select(type == "string" and length > 0)' "$assignment_file")
  local validation_dir
  validation_dir=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/nanoom-assignment-check.XXXXXX")
  if nanoom plan select \
    --input "$plan_file" \
    --reference "$reference_file" \
    --group "$group" \
    --assignment "$assignment_id" \
    --output-dir "$validation_dir/selected" >/dev/null &&
    cmp -s "$assignment_file" "$validation_dir/selected/assignment.json"; then
    rm -rf -- "$validation_dir"
    return 0
  fi
  rm -rf -- "$validation_dir"
  echo "assignment file failed Plan digest, schema, provenance, or selection validation" >&2
  return 1
}
