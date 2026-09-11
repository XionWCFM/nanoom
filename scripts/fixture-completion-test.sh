#!/usr/bin/env bash
set -euo pipefail

jobs='{"name":"affected","conclusion":"success"}
{"name":"run (scale-1)","conclusion":"success"}
{"name":"run (scale-2)","conclusion":"success"}
{"name":"run (scale-3)","conclusion":"success"}
{"name":"status","conclusion":"success"}'

jq -se -f scripts/fixture-completion.jq <<<"$jobs" >/dev/null
two_runs=$(jq -c 'select(.name != "run (scale-3)")' <<<"$jobs")
! jq -se -f scripts/fixture-completion.jq <<<"$two_runs" >/dev/null
echo 'fixture completion contract passed'
