#!/usr/bin/env bash

nanoom_fail() {
  local code=${1:-1}
  trap - ERR
  (( code != 0 )) || code=1
  local command=${ACTION_COMMAND:-not-started}
  local cwd=${ACTION_CWD:-.}
  local phase=${ACTION_PHASE:-initialization}
  local result
  result=$(jq -cn --arg action "$ACTION_NAME" --arg phase "$phase" --arg command "$command" --arg cwd "$cwd" --argjson exitCode "$code" '{status:"failure",action:$action,phase:$phase,command:$command,cwd:$cwd,exitCode:$exitCode,reason:("nanoom " + $action + " failed during " + $phase)}')
  echo "result=$result" >> "$GITHUB_OUTPUT"
  printf '  Result\n    ✗ failed during %s (exit %s)\n' "$phase" "$code"
  printf '  Action outputs\n    result=<same canonical JSON below>\n'
  printf '  Final JSON\n    %s\n' "$result"
  echo "::error title=nanoom $ACTION_NAME failed::phase=$phase; exit=$code; cwd=$cwd; command=$command"
  exit "$code"
}
