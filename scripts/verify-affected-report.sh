#!/usr/bin/env bash
set -euo pipefail

plan=${1:?usage: verify-affected-report.sh plan-v1.json}

jq -e '.ci.include | length == 2' <<<"$GROUPS" >/dev/null
jq -e '[.ci.include[].assignmentId] | sort == ["ci-1", "ci-2"]' <<<"$GROUPS" >/dev/null
jq -e '[.ci.include[] | has("assignmentId") and has("predictedDurationMs") and has("predictionSources") and has("predictionReason") and has("runnerLabels") and has("timingEnvironment")] | all' <<<"$GROUPS" >/dev/null
jq -e '[.ci.include[] | .runnerLabels == ["ubuntu-latest"] and (.timingEnvironment | startswith("runner-labels:"))] | all' <<<"$GROUPS" >/dev/null

jq -e '
  .version == 1
  and .hasChange == true
  and .taskRunner == "yarn"
  and .assignmentCount == 2
  and .itemCount == 4
  and ([.groups.ci.assignments[].items[].name] | sort) == ["@fixture/app", "@fixture/core", "@fixture/shared", "@fixture/shared"]
  and ([.groups.ci.assignments[].items[] | select(.name == "@fixture/shared") | .shard] | sort) == [1, 2]
  and ([.groups.ci.assignments[] as $assignment | $assignment.items[] as $item | select(($assignment.checkoutPaths | index($item.path)) == null)] | length) == 0
' "$plan" >/dev/null

jq -e '
  .status == "success"
  and .hasChange == true
  and .historyNeeded == true
  and .groupCount == 1
  and .assignmentCount == 2
  and .itemCount == 4
  and .timingRunner == "yarn"
  and (.historyStatus == "loaded" or .historyStatus == "fallback" or .historyStatus == "disabled")
  and .scheduling.historyBackend == "artifact"
  and .scheduling.historyStatus == .historyStatus
' <<<"$RESULT" >/dev/null

echo "affected Plan contract: 4 work items in 2 validated assignments"
