#!/usr/bin/env bash
set -Eeuo pipefail
ACTION_NAME=install ACTION_CWD=$CWD ACTION_PHASE=input-validation ACTION_COMMAND=not-started
source "$GITHUB_ACTION_PATH/../_setup/log.sh"; trap 'nanoom_fail "$?"' ERR
started=$(date +%s)
entry=$(jq -ce '(.include[0] // .)' <<<"$MATRIX")
if [[ $(jq -r '.mode // empty' <<<"$entry") == continuous ]]; then items='[]'; else items=$(jq -c 'if .items then .items elif .name then [.] else error("matrix entry must contain items or name") end' <<<"$entry"); fi
matrix_json=$(jq -c '{assignmentId,agentId,runId,mode,predictedDurationMs,items} | with_entries(select(.value != null))' <<<"$entry")
names=(); while IFS= read -r name; do names+=("$name"); done < <(jq -r '[.[].name] | unique[]' <<<"$items")
[[ "$PM" != npm || ${#names[@]} -eq 0 ]] || { echo 'npm cannot perform a focused workspace install; use Yarn Berry or pnpm' >&2; false; }
args=(-C "$CWD" install --package-manager "$PM"); for name in "${names[@]}"; do args+=(--filter "$name"); done; args+=(--json)
printf -v ACTION_COMMAND '%q ' nanoom "${args[@]}"; ACTION_COMMAND=${ACTION_COMMAND% }
printf '◆ nanoom install\n  Inputs\n    normalized assignment: %s\n    package manager: %s\n    cwd: %s\n  Command\n    %s\n' "$matrix_json" "$PM" "$CWD" "$ACTION_COMMAND"
ACTION_PHASE=focused-install; cli_result=$(nanoom "${args[@]}")
elapsed=$(( $(date +%s) - started ))
result=$(jq -cn --argjson matrix "$matrix_json" --arg command "$ACTION_COMMAND" --arg cwd "$CWD" --argjson cli "$cli_result" --argjson elapsed "$elapsed" '{status:"success",reason:(if ($matrix.mode == "continuous") then "installed the full workspace closure because future claims are unknown" else "installed the union of assignment workspace closures" end),matrix:$matrix,command:$command,cwd:$cwd,cli:$cli,elapsedSeconds:$elapsed}')
echo "result=$result" >> "$GITHUB_OUTPUT"
printf '  Result\n    ✓ workspaces=%s; elapsed=%ss\n  Final JSON\n    %s\n' "${#names[@]}" "$elapsed" "$result"
{ echo '### nanoom install'; echo; echo "**Result:** ${#names[@]} assignment workspaces installed in ${elapsed}s."; echo; echo "Command: \`$ACTION_COMMAND\`"; } >> "$GITHUB_STEP_SUMMARY"
