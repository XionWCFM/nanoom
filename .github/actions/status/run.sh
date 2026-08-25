#!/usr/bin/env bash
set -Eeuo pipefail
ACTION_NAME=status ACTION_CWD=. ACTION_PHASE=input-validation ACTION_COMMAND='built-in needs evaluation (no subprocess)'
source "$GITHUB_ACTION_PATH/../_setup/log.sh"; trap 'nanoom_fail "$?"' ERR
bold=$'\033[1m'; cyan=$'\033[36m'; reset=$'\033[0m'
printf '%s◆ nanoom status%s\n  Inputs\n    affected job: %s\n    matrix job: %s\n    group: %s\n    format: %s\n' "$bold$cyan" "$reset" "$AFFECTED" "$MATRIX" "$GROUP" "$FORMAT"
jq -e . >/dev/null <<<"$NEEDS"; [[ "$FORMAT" == text || "$FORMAT" == markdown ]] || { echo "format must be text or markdown, got '$FORMAT'" >&2; false; }
a=$(jq -r --arg n "$AFFECTED" '.[$n].result' <<<"$NEEDS"); m=$(jq -r --arg n "$MATRIX" '.[$n].result' <<<"$NEEDS"); h=$(jq -r --arg n "$AFFECTED" --arg g "$GROUP" '.[$n].outputs.groups // "{}" | fromjson | .[$g].hasChange // false' <<<"$NEEDS"); expected=$([[ "$h" == true ]] && echo success || echo skipped)
printf '  Resolved values\n    affected result: %s (%s.hasChange=%s)\n    matrix result: %s\n    expected matrix result: %s\n  Why\n    matrix status must agree with the authoritative affected output\n  Command\n    %s\n  Progress\n    ▶ evaluating workflow needs\n' "$a" "$GROUP" "$h" "$m" "$expected" "$ACTION_COMMAND"
ACTION_PHASE=status-evaluation
if [[ "$a" != success ]]; then reason='affected calculation failed'; elif [[ "$m" == "$expected" ]]; then reason=$([[ "$h" == true ]] && echo 'changes required a matrix and every entry succeeded' || echo 'no changes required work and the matrix was correctly skipped'); else reason='matrix result is inconsistent with affected output'; fi
result=$(jq -cn --arg affectedJob "$AFFECTED" --arg affectedResult "$a" --arg matrixJob "$MATRIX" --arg matrixResult "$m" --arg group "$GROUP" --argjson hasChange "$h" --arg expectedMatrixResult "$expected" --arg reason "$reason" '{affectedJob:$affectedJob, affectedResult:$affectedResult, matrixJob:$matrixJob, matrixResult:$matrixResult, group:$group, hasChange:$hasChange, expectedMatrixResult:$expectedMatrixResult,reason:$reason,status:(if $affectedResult == "success" and $matrixResult == $expectedMatrixResult then "success" else "failure" end)}')
echo "result=$result" >> "$GITHUB_OUTPUT"; status=$(jq -r .status <<<"$result"); symbol=$([[ "$status" == success ]] && echo '✓' || echo '✗')
printf '  Result\n    %s status=%s; %s\n  Action outputs\n    result=<same canonical JSON below>\n  Final JSON\n    %s\n' "$symbol" "$status" "$reason" "$result"
{ echo '### nanoom workflow status'; echo; echo '| Job | Result | Detail |'; echo '|---|---|---|'; echo "| $AFFECTED | $a | $GROUP.hasChange=$h |"; echo "| $MATRIX | $m | expected: $expected |"; echo; echo "Reason: $reason"; } >> "$GITHUB_STEP_SUMMARY"
if [[ "$status" == success ]]; then echo "::notice title=Workflow status::$reason"; else trap - ERR; echo "::error title=Inconsistent workflow result::$reason; $GROUP.hasChange=$h; matrix=$m; expected=$expected"; exit 1; fi
