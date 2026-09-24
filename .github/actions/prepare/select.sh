#!/usr/bin/env bash
set -Eeuo pipefail
ACTION_NAME=prepare ACTION_CWD="$GITHUB_WORKSPACE" ACTION_PHASE=plan-validation ACTION_COMMAND=not-started
source "$GITHUB_ACTION_PATH/../_setup/log.sh"; trap 'nanoom_fail "$?"' ERR

plan_dir=${PLAN_DIR:?PLAN_DIR is required}
[[ "$RUN_ID" =~ ^[0-9]+$ && "$RUN_ATTEMPT" =~ ^[0-9]+$ && "$MATRIX_INDEX" =~ ^[0-9]+$ && "$GITHUB_JOB" =~ ^[A-Za-z0-9_-]+$ ]] || {
  echo 'run, attempt, matrix index, and job identity are not safe checkout path segments' >&2
  false
}
[[ -f "$plan_dir/plan-v1.json" && -f "$plan_dir/plan-reference.json" ]] || {
  echo 'downloaded Plan artifact must contain plan-v1.json and plan-reference.json' >&2
  false
}

reference=$(jq -ce . <<<"$PLAN")
artifact_reference=$(jq -ce . "$plan_dir/plan-reference.json")
jq -e \
  --arg repository "$REPOSITORY" \
  --arg workflow "$WORKFLOW_REF" \
  --arg run "$RUN_ID" \
  --arg attempt "$RUN_ATTEMPT" \
  --arg head "$GITHUB_SHA" \
  --arg group "$GROUP" \
  --arg assignment "$ASSIGNMENT_ID" \
  --argjson expected "$reference" \
  '. == $expected
    and .version == 1
    and .provenance.repository == $repository
    and .provenance.workflow == $workflow
    and .provenance.runId == $run
    and (.provenance.producerAttempt | tonumber) <= ($attempt | tonumber)
    and .current.repository == $repository
    and .current.workflow == $workflow
    and .current.runId == $run
    and .current.head == $head
    and .provenance.head == $head
    and .current.attempt == .provenance.producerAttempt
    and (.artifactName | type == "string" and length > 0)
    and ($group | length > 0)
    and ($assignment | length > 0)' <<<"$artifact_reference" >/dev/null || {
  echo 'Plan artifact reference does not match this repository, workflow, run, attempt, head, or selected assignment' >&2
  false
}

consumer_reference="$plan_dir/selected-reference.json"
jq --argjson attempt "$RUN_ATTEMPT" '.current.attempt=$attempt' "$plan_dir/plan-reference.json" > "$consumer_reference"
selected_dir="$plan_dir/selected"
nanoom plan select \
  --input "$plan_dir/plan-v1.json" \
  --reference "$consumer_reference" \
  --group "$GROUP" \
  --assignment "$ASSIGNMENT_ID" \
  --output-dir "$selected_dir" >/dev/null
cp "$plan_dir/plan-v1.json" "$selected_dir/plan-v1.json"
cp "$consumer_reference" "$selected_dir/plan-reference.json"

head=$(jq -er '.current.head' "$selected_dir/assignment.json")
checkout_path=".nanoom/$RUN_ID/$RUN_ATTEMPT/$GITHUB_JOB/$MATRIX_INDEX"
printf 'assignment-file=%s\n' "$selected_dir/assignment.json" >> "$GITHUB_OUTPUT"
printf 'paths-file=%s\n' "$selected_dir/paths.txt" >> "$GITHUB_OUTPUT"
printf 'head=%s\n' "$head" >> "$GITHUB_OUTPUT"
printf 'cwd=%s/%s\n' "${GITHUB_WORKSPACE%/}" "$checkout_path" >> "$GITHUB_OUTPUT"
printf 'checkout-path=%s\n' "$checkout_path" >> "$GITHUB_OUTPUT"
printf 'group=%s\n' "$GROUP" >> "$GITHUB_OUTPUT"
printf 'assignment-id=%s\n' "$ASSIGNMENT_ID" >> "$GITHUB_OUTPUT"
