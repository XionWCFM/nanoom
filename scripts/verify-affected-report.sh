#!/usr/bin/env bash
set -euo pipefail

jq -e '[.ci.matrix.include[].name] | sort == ["@fixture/app", "@fixture/core", "@fixture/shared", "@fixture/shared"]' <<<"$GROUPS" >/dev/null
jq -e '.ci.hasChange == true' <<<"$GROUPS" >/dev/null
jq -e '[.ci.matrix.include[] | select(.name == "@fixture/shared" and .shard == 1)] | length == 1' <<<"$GROUPS" >/dev/null
jq -e '[.ci.matrix.include[] | select(.name == "@fixture/shared" and .shard == 2)] | length == 1' <<<"$GROUPS" >/dev/null
jq -e '[.ci.matrix.include[] | select(.name == "@fixture/app" and .isolate == true)] | length == 1' <<<"$GROUPS" >/dev/null
jq -e '(.affected.diagnostics.comparison.baseCommit | length) == 40 and (.affected.diagnostics.comparison.headCommit | length) == 40 and .affected.diagnostics.reasons["@fixture/shared"].kind == "direct" and .affected.diagnostics.reasons["@fixture/core"].dependencyPath == ["@fixture/core", "@fixture/shared"] and .affected.diagnostics.reasons["@fixture/app"].dependencyPath == ["@fixture/app", "@fixture/core", "@fixture/shared"]' <<<"$RESULT" >/dev/null

echo "affected matrix contract: 4 entries, direct shared change, core/app transitive paths"
