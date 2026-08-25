#!/usr/bin/env bash
set -Eeuo pipefail
ACTION_NAME=status ACTION_CWD=. ACTION_PHASE=input-validation ACTION_COMMAND='built-in needs evaluation (no subprocess)'
source "$GITHUB_ACTION_PATH/../_setup/log.sh"; trap 'nanoom_fail "$?"' ERR
bold=$'\033[1m'; cyan=$'\033[36m'; reset=$'\033[0m'
printf '%s◆ nanoom status%s\n  Inputs\n    needs: %s\n' "$bold$cyan" "$reset" "$NEEDS"
[[ -n "$NEEDS" ]] || { echo 'needs must contain at least one job result' >&2; false; }
jq -e 'type == "object" and length > 0 and all(.[]; (.result | type) == "string")' >/dev/null <<<"$NEEDS"

jobs=$(jq -c '[to_entries[] | {name: .key, result: .value.result}] | sort_by(.name)' <<<"$NEEDS")
invalid=$(jq -r '[.[] | select(.result != "success" and .result != "skipped")] | length' <<<"$jobs")
status=success
reason='all needed jobs succeeded or were skipped'
if (( invalid )); then
  status=failure
  reason=$(jq -r '[.[] | select(.result != "success" and .result != "skipped") | "\(.name)=\(.result)"] | "invalid job results: " + join(", ")' <<<"$jobs")
fi

printf '  Resolved values\n    jobs: %s\n  Why\n    %s\n  Command\n    %s\n  Progress\n    ▶ evaluating every needs result\n' "$jobs" "$reason" "$ACTION_COMMAND"
ACTION_PHASE=status-evaluation
result=$(jq -cn --argjson jobs "$jobs" --arg status "$status" --arg reason "$reason" '{jobs:$jobs,status:$status,reason:$reason}')
echo "result=$result" >> "$GITHUB_OUTPUT"; status=$(jq -r .status <<<"$result"); symbol=$([[ "$status" == success ]] && echo '✓' || echo '✗')
printf '  Result\n    %s status=%s; %s\n  Action outputs\n    result=<same canonical JSON below>\n  Final JSON\n    %s\n' "$symbol" "$status" "$reason" "$result"
{
  echo '### nanoom workflow status'
  echo
  echo '| Job | Result |'
  echo '|---|---|'
  jq -r '.[] | "| \(.name) | \(.result) |"' <<<"$jobs"
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
