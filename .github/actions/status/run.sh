#!/usr/bin/env bash
set -Eeuo pipefail
ACTION_NAME=status ACTION_CWD=. ACTION_PHASE=input-validation ACTION_COMMAND='built-in needs evaluation (no subprocess)'
source "$GITHUB_ACTION_PATH/../_setup/log.sh"; trap 'nanoom_fail "$?"' ERR
bold=$'\033[1m'; cyan=$'\033[36m'; reset=$'\033[0m'
if [[ -n "${RESULTS:-}" ]]; then
  NEEDS=$(jq -Rsc '
    split("\n") | map(select(length > 0) | split("=")) |
    if any(.[]; length != 2 or .[0] == "" or .[1] == "")
    then error("results must contain non-empty job=result lines")
    else map({key: .[0], value: {result: .[1]}}) | from_entries
    end
  ' <<<"$RESULTS")
fi
printf '%s◆ nanoom status%s\n  Inputs\n    needs: %s\n' "$bold$cyan" "$reset" "${NEEDS:-}"
[[ -n "${NEEDS:-}" ]] || { echo 'needs must contain at least one job result' >&2; false; }
jq -e 'type == "object" and length > 0 and all(.[]; (.result | type) == "string")' >/dev/null <<<"$NEEDS"
REQUIRED_JOBS=${REQUIRED_JOBS:-[]}
jq -e 'type == "array" and all(.[]; type == "string" and length > 0) and length == (unique | length)' >/dev/null <<<"$REQUIRED_JOBS" || {
  echo 'requiredJobs must be a JSON array of unique non-empty job IDs' >&2
  false
}

jobs=$(jq -c '[to_entries[] | {name: .key, result: .value.result}] | sort_by(.name)' <<<"$NEEDS")
invalid=$(jq -r '[.[] | select(.result != "success" and .result != "skipped")] | length' <<<"$jobs")
required_jobs=$(jq -c 'sort' <<<"$REQUIRED_JOBS")
required_failures=$(jq -cnr --argjson jobs "$jobs" --argjson required "$required_jobs" '
  [$required[] as $name | ([$jobs[] | select(.name == $name)]) as $found |
    if ($found | length) == 0 then "\($name)=missing"
    elif $found[0].result != "success" then "\($name)=\($found[0].result)"
    else empty end
  ] | join(", ")
')
status=success
reason='all needed jobs succeeded or were skipped'
if [[ -n "$required_failures" ]]; then
  status=failure
  reason="required jobs must succeed: $required_failures"
fi
if (( invalid )); then
  status=failure
  invalid_reason=$(jq -r '[.[] | select(.result != "success" and .result != "skipped") | "\(.name)=\(.result)"] | "invalid job results: " + join(", ")' <<<"$jobs")
  if [[ -n "$required_failures" ]]; then reason="$reason; $invalid_reason"; else reason=$invalid_reason; fi
fi

printf '  Resolved values\n    jobs: %s\n    required jobs: %s\n  Why\n    %s\n  Command\n    %s\n  Progress\n    ▶ evaluating every needs result\n' "$jobs" "$required_jobs" "$reason" "$ACTION_COMMAND"
ACTION_PHASE=status-evaluation
result=$(jq -cn --argjson jobs "$jobs" --argjson requiredJobs "$required_jobs" --arg status "$status" --arg reason "$reason" '{jobs:$jobs,requiredJobs:$requiredJobs,status:$status,reason:$reason}')
echo "result=$result" >> "$GITHUB_OUTPUT"; status=$(jq -r .status <<<"$result"); symbol=$([[ "$status" == success ]] && echo '✓' || echo '✗')
printf '  Result\n    %s status=%s; %s\n  Action outputs\n    result=<same canonical JSON below>\n  Final JSON\n    %s\n' "$symbol" "$status" "$reason" "$result"
{
  echo '### nanoom workflow status'
  echo
  echo '| Job | Result | Required |'
  echo '|---|---|---|'
  jq -r --argjson required "$required_jobs" '.[] | .name as $name | "| \($name) | \(.result) | \(($required | index($name)) != null) |"' <<<"$jobs"
  echo
  echo "Reason: $reason"
} >> "$GITHUB_STEP_SUMMARY"
if [[ "$status" == success ]]; then
  echo "::notice title=Workflow status::$reason"
else
  trap - ERR
  echo "::error title=Workflow status failed::$reason"
  exit 1
fi
