#!/usr/bin/env bash
set -Eeuo pipefail
ACTION_NAME=prepare ACTION_CWD="$CWD" ACTION_PHASE=sparse-checkout ACTION_COMMAND=git-sparse-checkout-set
source "$GITHUB_ACTION_PATH/../_setup/log.sh"; trap 'nanoom_fail "$?"' ERR
source "$GITHUB_ACTION_PATH/../_setup/assignment.sh"

nanoom_validate_assignment_file "$ASSIGNMENT_FILE" "$CWD"
[[ -f "$PATHS_FILE" ]] || { echo "checkout paths file does not exist: $PATHS_FILE" >&2; false; }
path_count=$(wc -l < "$PATHS_FILE" | tr -d ' ')
printf '◆ nanoom prepare checkout\n  cwd: %s\n  head: %s\n  planned paths: %s\n' "$CWD" "$GITHUB_SHA" "$path_count"
git -C "$CWD" sparse-checkout set --cone --stdin < "$PATHS_FILE"
actual_head=$(git -C "$CWD" rev-parse --verify 'HEAD^{commit}')
[[ "$actual_head" == "$GITHUB_SHA" ]] || {
  echo "sparse checkout HEAD mismatch: expected $GITHUB_SHA, got $actual_head" >&2
  false
}
while IFS= read -r path; do
  [[ -d "$CWD/$path" ]] || {
    echo "planned sparse checkout path is missing at $GITHUB_SHA: $path" >&2
    false
  }
done < "$PATHS_FILE"

printf 'head=%s\n' "$actual_head" >> "$GITHUB_OUTPUT"
printf 'result={"status":"success","head":"%s","checkoutPathCount":%s}\n' \
  "$actual_head" "$(jq -r '.checkoutPaths | length' "$ASSIGNMENT_FILE")" >> "$GITHUB_OUTPUT"
{ echo '### nanoom prepare'; echo; echo "**Result:** checked out $actual_head with $(jq -r '.checkoutPaths | length' "$ASSIGNMENT_FILE") planned cone paths."; } >> "$GITHUB_STEP_SUMMARY"
