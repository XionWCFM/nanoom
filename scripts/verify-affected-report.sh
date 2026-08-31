#!/usr/bin/env bash
set -euo pipefail

jq -e '[.ci.matrix.include[].name] | sort == ["@fixture/app", "@fixture/core", "@fixture/shared", "@fixture/shared"]' <<<"$GROUPS" >/dev/null
jq -e '.ci.hasChange == true' <<<"$GROUPS" >/dev/null
! jq -e '[.ci.matrix.include[] | has("group") or has("label") or has("path")] | any' <<<"$GROUPS" >/dev/null
jq -e '[.ci.matrix.include[] | select(.name == "@fixture/shared" and .shard == 1)] | length == 1' <<<"$GROUPS" >/dev/null
jq -e '[.ci.matrix.include[] | select(.name == "@fixture/shared" and .shard == 2)] | length == 1' <<<"$GROUPS" >/dev/null
jq -e '[.ci.matrix.include[] | select(.name == "@fixture/app" and .task == "test")] | length == 1' <<<"$GROUPS" >/dev/null
jq -e '.status == "success" and .hasChange and (.comparison.baseCommit | length) == 40 and (.comparison.headCommit | length) == 40 and .groups.ci == {hasChange:true,entryCount:4}' <<<"$RESULT" >/dev/null

echo "affected matrix contract: 4 entries, direct shared change, core/app transitive paths"
