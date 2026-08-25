#!/usr/bin/env bash
set -Eeuo pipefail

ACTION_NAME=affected ACTION_CWD=$CWD ACTION_PHASE=input-resolution ACTION_COMMAND=not-started
source "$GITHUB_ACTION_PATH/../_setup/log.sh"
trap 'nanoom_fail "$?"' ERR
bold=$'\033[1m'; cyan=$'\033[36m'; reset=$'\033[0m'; started=$(date +%s)
printf '%s◆ nanoom affected%s\n' "$bold$cyan" "$reset"
printf '  Inputs\n    working directory: %s\n    config: %s\n    requested comparison: %s -> %s\n    selection: all configured groups\n' "$CWD" "$CONFIG" "${BASE:-event-derived}" "${HEAD:-event-derived}"
[[ -n "$BASE" ]] || BASE=$EVENT_BASE
[[ -n "$HEAD" ]] || HEAD=$EVENT_HEAD
[[ -n "$BASE" ]] || { echo 'affected could not resolve a base revision from inputs or the GitHub event' >&2; false; }
[[ -n "$HEAD" ]] || HEAD=HEAD
if ! git -C "$CWD" rev-parse --verify "$BASE^{commit}" >/dev/null 2>&1; then
  ACTION_PHASE=git-fetch
  if [[ "$EVENT" == pull_request && "$BASE" != refs/* ]]; then
    git -C "$CWD" fetch --no-tags --depth=100 origin "$BASE:refs/remotes/origin/$BASE"; BASE="refs/remotes/origin/$BASE"
  else
    git -C "$CWD" fetch --no-tags --depth=100 origin "$BASE"; BASE=FETCH_HEAD
  fi
fi
printf '  Resolved values\n    comparison refs: %s -> %s\n' "$BASE" "$HEAD"
printf '  Why\n    changed paths are expanded through dependencies and every configured group rule\n'
args=(-C "$CWD" -c "$CONFIG" affected --report --base "$BASE" --head "$HEAD")
printf -v ACTION_COMMAND '%q ' nanoom "${args[@]}"; ACTION_COMMAND=${ACTION_COMMAND% }
printf '  Command\n    cwd: %s\n    %s\n  Progress\n    ▶ setting up nanoom\n' "$CWD" "$ACTION_COMMAND"
ACTION_PHASE=setup; bash "$GITHUB_ACTION_PATH/../_setup/setup.sh"; export PATH="$RUNNER_TEMP/nanoom-bin:$PATH"
printf '    ▶ calculating affected workspaces\n'; ACTION_PHASE=affected-calculation
report=$(nanoom "${args[@]}")
matrix=$(jq -c .matrix <<<"$report")
groups=$(jq -c 'with_entries(.value = {hasChange: ((.value.include | length) > 0), matrix: .value})' <<<"$matrix")
has=$(jq -r 'any(to_entries[]; .value.include | length > 0)' <<<"$matrix")
count=$(jq '[to_entries[].value.include[]] | length' <<<"$matrix")
elapsed=$(( $(date +%s) - started ))
base_commit=$(jq -r .affected.diagnostics.comparison.baseCommit <<<"$report"); head_commit=$(jq -r .affected.diagnostics.comparison.headCommit <<<"$report"); mode=$(jq -r .affected.diagnostics.comparison.mode <<<"$report"); changed=$(jq '.affected.diagnostics.changedFiles | length' <<<"$report")
echo "has_change=$has" >> "$GITHUB_OUTPUT"; echo "groups=$groups" >> "$GITHUB_OUTPUT"; echo "result=$report" >> "$GITHUB_OUTPUT"
printf '  Result\n    ✓ has_change=%s; matrix entries=%s; elapsed=%ss\n    comparison: %s -> %s (%s)\n    changed files: %s\n' "$has" "$count" "$elapsed" "$base_commit" "$head_commit" "$mode" "$changed"
jq -r '.affected.diagnostics.changedFiles[] | "      · " + .' <<<"$report"
printf '    selected workspaces:\n'
jq -r '.affected.diagnostics.reasons | to_entries[] | "      · \(.key): " + (if .value.kind == "direct" then "direct change: " + (.value.changedFiles | join(", ")) elif .value.kind == "globalDependency" then "global dependency: " + (.value.changedFiles | join(", ")) else "transitive dependency: " + (.value.dependencyPath | join(" -> ")) end)' <<<"$report"
if (( count )); then
  jq -r 'to_entries[] as $group | $group.value.include[] | "      · \($group.key) / \(.name) / \(.task)" + (if .shard then " / shard \(.shard)/\(.totalShards)" else "" end) + (if .isolate then " / isolated" else "" end)' <<<"$matrix"
else
  printf '      · no workspace tasks matched; downstream matrix will be skipped\n'
fi
printf '  Action outputs\n    has_change=%s\n    groups=%s\n    result=<same canonical JSON below>\n  Final JSON\n    %s\n' "$has" "$groups" "$(jq -c . <<<"$report")"
echo "::notice title=Affected result::has_change=$has; $count matrix entries will run (${elapsed}s)"
{
  echo "### nanoom affected"; echo; echo "**Result:** \`has_change=$has\` — **$count** matrix entries across all groups"; echo; echo "Comparison (\`$mode\`): \`$base_commit\` -> \`$head_commit\`; **$changed** changed files."; echo; echo '| Group | Workspace | Task | Execution | Why |'; echo '|---|---|---|---|---|'
  jq -r --argjson reasons "$(jq -c .affected.diagnostics.reasons <<<"$report")" 'to_entries[] as $group | $group.value.include[] | . as $entry | "| \($group.key) | \(.name) | \(.task) | " + (if .shard then "shard \(.shard)/\(.totalShards)" elif .isolate then "isolated" else "standard" end) + " | " + ($reasons[$entry.name] | if .kind == "direct" then "direct: " + (.changedFiles | join(", ")) elif .kind == "globalDependency" then "global: " + (.changedFiles | join(", ")) else "transitive: " + (.dependencyPath | join(" -> ")) end) + " |"' <<<"$matrix"
  (( count )) || echo '| - | - | - | no work | no matching change |'
} >> "$GITHUB_STEP_SUMMARY"
