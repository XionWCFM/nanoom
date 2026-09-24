#!/usr/bin/env bash
set -Eeuo pipefail
ACTION_NAME=install ACTION_CWD=$CWD ACTION_PHASE=input-validation ACTION_COMMAND=not-started
source "$GITHUB_ACTION_PATH/../_setup/log.sh"; trap 'nanoom_fail "$?"' ERR
started=$(date +%s)
continuous=false
if [[ -n ${ASSIGNMENT_FILE:-} ]]; then
  if [[ -z "$CWD" || "$CWD" == . ]]; then
    CWD="$GITHUB_WORKSPACE/.nanoom/$RUN_ID/$RUN_ATTEMPT/$GITHUB_JOB/${MATRIX_INDEX:-0}"
    ACTION_CWD=$CWD
  fi
  source "$GITHUB_ACTION_PATH/../_setup/assignment.sh"
  nanoom_validate_assignment_file "$ASSIGNMENT_FILE" "$CWD"
  group=$(jq -er .group "$ASSIGNMENT_FILE")
  assignment_id=$(jq -er .assignmentId "$ASSIGNMENT_FILE")
  planned_count=$(jq -er '.items | select(type == "array" and length > 0) | length' "$ASSIGNMENT_FILE")
  filter_file="$RUNNER_TEMP/nanoom-install-filters-$RUN_ID-$RUN_ATTEMPT-$GITHUB_JOB.json"
  jq -c '[.items[].name] | unique' "$ASSIGNMENT_FILE" > "$filter_file"
  name_count=$(jq -r length "$filter_file")
  ((name_count > 0)) || { echo 'static assignment install requires at least one workspace' >&2; false; }
  matrix_json=$(jq -cn --arg group "$group" --arg assignmentId "$assignment_id" --argjson itemCount "$planned_count" --argjson predictedDurationMs "$(jq -c '.predictedDurationMs // 0' "$ASSIGNMENT_FILE")" '{group:$group,assignmentId:$assignmentId,itemCount:$itemCount,predictedDurationMs:$predictedDurationMs}')
  args=(-C "$CWD" install --package-manager "$PM" --filter-file "$filter_file" --json)
elif [[ -n ${MATRIX:-} ]]; then
  entry=$(jq -ce '(.include[0] // .)' <<<"$MATRIX")
  [[ $(jq -r '.mode // empty' <<<"$entry") == continuous ]] || {
    echo 'static assignment install requires a validated assignment-file from nanoom prepare' >&2
    false
  }
  continuous=true
  name_count=0
  matrix_json=$(jq -c '{assignmentId,agentId,runId,mode,predictedDurationMs,items} | with_entries(select(.value != null))' <<<"$entry")
  args=(-C "$CWD" install --package-manager "$PM" --json)
else
  echo 'install requires assignmentFile; only scheduler=http continuous agents may use matrix' >&2
  false
fi
[[ "$PM" != npm || "$name_count" -eq 0 ]] || { echo 'npm cannot perform a focused workspace install; use Yarn Berry or pnpm' >&2; false; }
printf -v ACTION_COMMAND '%q ' nanoom "${args[@]}"; ACTION_COMMAND=${ACTION_COMMAND% }
printf '◆ nanoom install\n  Inputs\n    normalized assignment: %s\n    package manager: %s\n    cwd: %s\n  Command\n    %s\n' "$matrix_json" "$PM" "$CWD" "$ACTION_COMMAND"
ACTION_PHASE=focused-install
if cli_result=$(nanoom "${args[@]}"); then
  :
else
  cli_status=$?
  printf 'Nanoom CLI result: %s\n' "${cli_result:-<empty>}" >&2
  nanoom_fail "$cli_status"
fi
elapsed=$(( $(date +%s) - started ))
resolved_pm=$(jq -r '.packageManager // empty' <<<"$cli_result")
resolved_pm_version=''
if [[ "$resolved_pm" =~ ^(pnpm|yarn|npm)$ ]]; then
  set +e
  resolved_pm_version=$("$resolved_pm" --version 2>/dev/null)
  version_status=$?
  set -e
  if (( version_status == 0 )); then
    resolved_pm_version=${resolved_pm_version%%$'\n'*}
    resolved_pm_version=${resolved_pm_version:0:128}
  else
    resolved_pm_version=''
  fi
fi
result=$(jq -cn --argjson matrix "$matrix_json" --arg command "$ACTION_COMMAND" --arg cwd "$CWD" --argjson cli "$cli_result" --argjson elapsed "$elapsed" --argjson continuous "$continuous" --arg packageManager "$resolved_pm" --arg packageManagerVersion "$resolved_pm_version" '{status:"success",reason:(if $continuous then "installed the full workspace closure because future claims are unknown" else "installed the union of assignment workspace closures" end),assignment:$matrix,command:$command,cwd:$cwd,cli:$cli,elapsedSeconds:$elapsed} + (if $packageManager == "" then {} else {packageManager:$packageManager,installMode:(if $continuous then "full" else "focused" end)} + (if $packageManagerVersion == "" then {} else {packageManagerVersion:$packageManagerVersion} end) end)')
echo "result=$result" >> "$GITHUB_OUTPUT"
printf '  Result\n    ✓ workspaces=%s; elapsed=%ss\n  Final JSON\n    %s\n' "$name_count" "$elapsed" "$result"
{ echo '### nanoom install'; echo; echo "**Result:** $name_count assignment workspaces installed in ${elapsed}s."; echo; echo "Command: \`$ACTION_COMMAND\`"; } >> "$GITHUB_STEP_SUMMARY"
