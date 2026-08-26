#!/usr/bin/env bash
set -Eeuo pipefail
ACTION_NAME=run ACTION_CWD=$CWD ACTION_PHASE=input-validation ACTION_COMMAND=not-started
source "$GITHUB_ACTION_PATH/../_setup/log.sh"; trap 'nanoom_fail "$?"' ERR
bold=$'\033[1m'; cyan=$'\033[36m'; reset=$'\033[0m'; started=$(date +%s)
printf '%s◆ nanoom run%s\n  Inputs\n    matrix: %s\n    package manager: %s\n    monorepo tool: %s\n    cwd: %s\n' "$bold$cyan" "$reset" "$MATRIX" "$PM" "$TOOL" "$CWD"
entry=$(jq -ce '(.include[0] // .) | select(.group and .task and .name)' <<<"$MATRIX"); matrix_json=$(jq -c '{group, label, name, path, task, shard, totalShards, isolate} | with_entries(select(.value != null))' <<<"$entry"); group=$(jq -r .group <<<"$entry"); task=$(jq -r .task <<<"$entry"); name=$(jq -r .name <<<"$entry")
printf '  Resolved values\n    matrix: %s\n    workspace: %s\n    group/task: %s / %s\n' "$matrix_json" "$name" "$group" "$task"
[[ -n $(jq -r '.shard // empty' <<<"$entry") ]] && printf '    shard: %s/%s\n' "$(jq -r .shard <<<"$entry")" "$(jq -r .totalShards <<<"$entry")"
[[ $(jq -r '.isolate // false' <<<"$entry") == true ]] && printf '    isolation: enabled\n'
printf '  Why\n    the authoritative affected matrix selected this task\n'
args=(-C "$CWD" run "$group" "$task" --all --filter "$name" --json)
[[ -n $(jq -r '.shard // empty' <<<"$entry") ]] && args+=(--shard "$(jq -r .shard <<<"$entry")" --total-shards "$(jq -r .totalShards <<<"$entry")"); [[ $(jq -r '.isolate // false' <<<"$entry") == true ]] && args+=(--isolate)
if [[ "$TOOL" == turbo && ! -x "$CWD/node_modules/.bin/turbo" ]]; then args+=(--runner "$PM"); elif [[ "$TOOL" != auto ]]; then args+=(--runner "$TOOL"); fi
printf -v ACTION_COMMAND '%q ' nanoom "${args[@]}"; ACTION_COMMAND=${ACTION_COMMAND% }
printf '  Command\n    cwd: %s\n    %s\n  Progress\n    ▶ setting up nanoom\n' "$CWD" "$ACTION_COMMAND"
ACTION_PHASE=setup; bash "$GITHUB_ACTION_PATH/../_setup/setup.sh"; export PATH="$RUNNER_TEMP/nanoom-bin:$PATH"
printf '    ▶ running matrix task\n'; ACTION_PHASE=matrix-task; cli_result=$(nanoom "${args[@]}")
elapsed=$(( $(date +%s) - started )); result=$(jq -cn --argjson matrix "$matrix_json" --arg command "$ACTION_COMMAND" --arg cwd "$CWD" --argjson cli "$cli_result" --argjson elapsed "$elapsed" '{status:"success", reason:"executed the task selected by the authoritative affected matrix entry", matrix:$matrix, command:$command, cwd:$cwd, cli:$cli, elapsedSeconds:$elapsed}')
echo "result=$result" >> "$GITHUB_OUTPUT"
printf '  Result\n    ✓ %s / %s / %s succeeded; elapsed=%ss\n    runner: %s\n  Action outputs\n    result=<same canonical JSON below>\n  Final JSON\n    %s\n' "$group" "$name" "$task" "$elapsed" "$(jq -r .runner <<<"$cli_result")" "$result"
echo "::notice title=Matrix task complete::$group / $name / $task succeeded (${elapsed}s)"
{ echo '### nanoom run'; echo; echo "**Result:** \`$group / $name / $task\` succeeded in ${elapsed}s."; echo; echo 'Reason: this task was selected by the authoritative affected matrix entry.'; echo; echo "Command: \`$ACTION_COMMAND\`"; } >> "$GITHUB_STEP_SUMMARY"
