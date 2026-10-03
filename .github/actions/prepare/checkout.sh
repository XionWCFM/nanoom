#!/usr/bin/env bash
set -Eeuo pipefail
ACTION_NAME=prepare ACTION_CWD="$CWD" ACTION_PHASE=sparse-checkout ACTION_COMMAND=git-sparse-checkout-set
source "$GITHUB_ACTION_PATH/../_setup/log.sh"; trap 'nanoom_fail "$?"' ERR
source "$GITHUB_ACTION_PATH/../_setup/assignment.sh"

nanoom_validate_assignment_file "$ASSIGNMENT_FILE" "$CWD"
[[ -f "$PATHS_FILE" ]] || { echo "checkout paths file does not exist: $PATHS_FILE" >&2; false; }
path_count=$(wc -l < "$PATHS_FILE" | tr -d ' ')
printf '◆ nanoom prepare checkout\n  cwd: %s\n  head: %s\n  planned paths: %s\n' "$CWD" "$GITHUB_SHA" "$path_count"
checkout_root=$(git -C "$CWD" rev-parse --show-toplevel)
git -C "$checkout_root" sparse-checkout set --cone --stdin < "$PATHS_FILE"
actual_head=$(git -C "$CWD" rev-parse --verify 'HEAD^{commit}')
[[ "$actual_head" == "$GITHUB_SHA" ]] || {
  echo "sparse checkout HEAD mismatch: expected $GITHUB_SHA, got $actual_head" >&2
  false
}
while IFS= read -r path; do
  [[ -d "$checkout_root/$path" ]] || {
    echo "planned sparse checkout path is missing at $GITHUB_SHA: $path" >&2
    false
  }
done < "$PATHS_FILE"

# A selected configuration is a file, never a reason to checkout its whole parent.
config_path=$(jq -r '.configPath // empty' "$ASSIGNMENT_FILE")
if [[ -n "$config_path" ]]; then
  ACTION_PHASE=configuration-checkout
  prefix=$(git -C "$CWD" rev-parse --show-prefix)
  [[ $(git -C "$CWD" cat-file -t "$actual_head:$prefix$config_path") == blob ]] || {
    echo 'planned configuration must select a tracked file' >&2; false
  }
  git --literal-pathspecs -C "$CWD" restore --ignore-skip-worktree-bits --source="$actual_head" --worktree -- "$config_path"
fi

printf 'head=%s\n' "$actual_head" >> "$GITHUB_OUTPUT"
printf 'result={"status":"success","head":"%s","checkoutPathCount":%s}\n' \
  "$actual_head" "$(jq -r '.checkoutPaths | length' "$ASSIGNMENT_FILE")" >> "$GITHUB_OUTPUT"
{ echo '### nanoom prepare'; echo; echo "**Result:** checked out $actual_head with $(jq -r '.checkoutPaths | length' "$ASSIGNMENT_FILE") planned cone paths."; } >> "$GITHUB_STEP_SUMMARY"
