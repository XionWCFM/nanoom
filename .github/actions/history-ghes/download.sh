#!/usr/bin/env bash
set -Eeuo pipefail
source "$GITHUB_ACTION_PATH/../_setup/artifacts.sh"
[[ "$RUN_ID" =~ ^[0-9]+$ && "$RUN_ATTEMPT" =~ ^[0-9]+$ ]]
# Reuse the bounded, single-JSON archive reader; never fetch unrelated artifacts.
nanoom_history_budget_start 60 25165824
artifacts=$(nanoom_run_artifacts "$RUN_ID")
prefix="nanoom-measurement-v3-$RUN_ID-$RUN_ATTEMPT-"
names=$(nanoom_history_timeout jq -r --arg prefix "$prefix" \
  '.artifacts[]? | select((.expired | not) and (.name | startswith($prefix))) | .name' <<<"$artifacts")
while IFS= read -r name; do
  [[ -n "$name" ]] || continue
  [[ "$name" =~ ^[A-Za-z0-9._-]+$ ]]
  nanoom_download_artifact_bounded "$artifacts" "$name" "$MEASUREMENT_DIR/$name" 4194304 8388608
done <<<"$names"
