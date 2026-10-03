#!/usr/bin/env bash

# The default template exposes no preparation clock step. GitHub's job start
# includes checkout and Node setup, unlike the start of focused installation.
nanoom_job_preparation_start() {
  [[ ${API:-} == https://* && -n ${TOKEN:-} && -n ${REPOSITORY:-} && ${RUN_ID:-} =~ ^[0-9]+$ && ${RUN_ATTEMPT:-} =~ ^[0-9]+$ ]] || return 1
  local page=1 response count starts='[]' remaining
  nanoom_history_budget_start 3 1048576
  while :; do
    remaining=$(nanoom_history_remaining) || return 1
    response=$(curl --fail --silent --show-error --max-time "$remaining" --max-filesize 1048576 \
      -H "Authorization: Bearer $TOKEN" -H 'Accept: application/vnd.github+json' \
      "$API/repos/$REPOSITORY/actions/runs/$RUN_ID/attempts/$RUN_ATTEMPT/jobs?per_page=100&page=$page") || return 1
    nanoom_history_charge_bytes "$(LC_ALL=C printf '%s' "$response" | wc -c | tr -d ' ')" || return 1
    starts=$(nanoom_history_timeout jq -c --arg marker "[$ASSIGNMENT_ID]" --argjson starts "$starts" \
      '$starts + [.jobs[]? | select(.status == "in_progress" and (.name | contains($marker))) | .started_at | fromdateiso8601 * 1000]' <<<"$response") || return 1
    count=$(nanoom_history_timeout jq '.jobs | length' <<<"$response") || return 1
    (( count == 100 )) || break
    page=$((page + 1))
  done
  nanoom_history_timeout jq -er 'select(length == 1) | .[0] | select(. > 0)' <<<"$starts"
}
