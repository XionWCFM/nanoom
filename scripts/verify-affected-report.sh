#!/usr/bin/env bash
set -euo pipefail

jq -e '.ci.hasChange == true and (.ci.matrix.include | length) == 2' <<<"$GROUPS" >/dev/null
jq -e '[.ci.matrix.include[].items[].name] | sort == ["@fixture/app", "@fixture/core", "@fixture/shared", "@fixture/shared"]' <<<"$GROUPS" >/dev/null
jq -e '[.ci.matrix.include[].items[] | select(.name == "@fixture/shared") | .shard] | sort == [1,2]' <<<"$GROUPS" >/dev/null
jq -e '[.ci.matrix.include[] | has("assignmentId") and has("predictedDurationMs") and has("reason")] | all' <<<"$GROUPS" >/dev/null
jq -e '[.ci.matrix.include[].items[] | has("path") or has("label")] | any | not' <<<"$GROUPS" >/dev/null
jq -e '.affected.has_change and (.affected.diagnostics.comparison.baseCommit | length) == 40 and (.affected.diagnostics.comparison.headCommit | length) == 40 and .affected.group.ci.totalWorkspaces == 4 and .affected.group.ci.affectedWorkspaces == 3 and .affected.group.ci.affectedPercent == 75 and .affected.group.ci.distribution == {name:"full",maxAffectedPercent:100,concurrency:2} and .groups.ci == {hasChange:true,assignmentCount:2}' <<<"$RESULT" >/dev/null

echo "affected assignment contract: 4 work items in 2 deterministic buckets"
