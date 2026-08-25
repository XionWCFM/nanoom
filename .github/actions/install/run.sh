#!/usr/bin/env bash
set -Eeuo pipefail
ACTION_NAME=install ACTION_CWD=$CWD ACTION_PHASE=input-validation ACTION_COMMAND=not-started
source "$GITHUB_ACTION_PATH/../_setup/log.sh"; trap 'nanoom_fail "$?"' ERR
bold=$'\033[1m'; cyan=$'\033[36m'; reset=$'\033[0m'; started=$(date +%s)
printf '%s◆ nanoom install%s\n  Inputs\n    matrix: %s\n    package manager: %s\n    cwd: %s\n' "$bold$cyan" "$reset" "$MATRIX" "$PM" "$CWD"
entry=$(jq -ce '(.include[0] // .) | select(.name)' <<<"$MATRIX"); matrix_json=$(jq -c '{group, label, name, path, task, shard, totalShards, isolate} | with_entries(select(.value != null))' <<<"$entry"); name=$(jq -er '.name | select(type == "string" and length > 0)' <<<"$entry")
[[ "$PM" != npm ]] || { echo 'npm cannot perform a focused workspace install; use Yarn Berry or pnpm' >&2; false; }
printf '  Resolved values\n    matrix: %s\n    workspace: %s\n  Why\n    root development dependencies plus the selected workspace dependency closure are required\n' "$matrix_json" "$name"
args=(-C "$CWD" install --package-manager "$PM" --filter "$name" --json); printf -v ACTION_COMMAND '%q ' nanoom "${args[@]}"; ACTION_COMMAND=${ACTION_COMMAND% }
printf '  Command\n    cwd: %s\n    %s\n  Progress\n    ▶ setting up nanoom\n' "$CWD" "$ACTION_COMMAND"
ACTION_PHASE=setup; bash "$GITHUB_ACTION_PATH/../_setup/setup.sh"; export PATH="$RUNNER_TEMP/nanoom-bin:$PATH"
printf '    ▶ installing focused dependencies\n'; ACTION_PHASE=focused-install; cli_result=$(nanoom "${args[@]}")
elapsed=$(( $(date +%s) - started )); result=$(jq -cn --argjson matrix "$matrix_json" --arg command "$ACTION_COMMAND" --arg cwd "$CWD" --argjson cli "$cli_result" --argjson elapsed "$elapsed" '{status:"success", reason:"installed root development dependencies plus the selected workspace dependency closure", matrix:$matrix, command:$command, cwd:$cwd, cli:$cli, elapsedSeconds:$elapsed}')
echo "result=$result" >> "$GITHUB_OUTPUT"
printf '  Result\n    ✓ workspace=%s; status=success; elapsed=%ss\n    child command: %s\n  Action outputs\n    result=<same canonical JSON below>\n  Final JSON\n    %s\n' "$name" "$elapsed" "$(jq -r .command <<<"$cli_result")" "$result"
echo "::notice title=Focused install complete::$name dependencies are ready (${elapsed}s)"
{ echo '### nanoom install'; echo; echo "**Result:** focused dependencies for \`$name\` installed in ${elapsed}s."; echo; echo 'Reason: root development dependencies and the selected workspace dependency closure are required by the matrix entry.'; echo; echo "Command: \`$ACTION_COMMAND\`"; } >> "$GITHUB_STEP_SUMMARY"
