#!/usr/bin/env bash
set -euo pipefail

base_ref="${1:-main}"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

files=()
while IFS= read -r file; do
  files+=("$file")
done < <(git diff --name-only "$base_ref...HEAD")
if ((${#files[@]} == 0)); then
  echo "review-change: no changes against $base_ref"
  exit 0
fi

printf 'review-change: %d changed files against %s\n' "${#files[@]}" "$base_ref"
printf '%s\n' "${files[@]}"

has() { printf '%s\n' "${files[@]}" | grep -Fxq -- "$1"; }
any() { printf '%s\n' "${files[@]}" | grep -Eq -- "$1"; }

release_only=true
for file in "${files[@]}"; do
  case "$file" in
    .changeset/*.md | Cargo.lock | Cargo.toml | yarn.lock | packages/cli/CHANGELOG.md | packages/cli/package.json | packages/cli-*/package.json) ;;
    *) release_only=false ;;
  esac
done
if $release_only; then
  bash scripts/version-consistency.sh
  echo 'review-change: PASS (release versions and package contracts are consistent)'
  exit 0
fi

fail=0

if any '^src/|^packages/cli/(bin/|postinstall\.js|download-smoke\.js|smoke-test\.js|scripts/)' && ! any '(^|/)(tests?|__tests__|.*test.*|smoke-test\.js)'; then
  echo 'BLOCKED: CLI/source change has no changed regression test.'; fail=1
fi
if any '^\.github/actions/' && ! has 'scripts/action-contract.sh'; then
  echo 'BLOCKED: Action change must update/confirm scripts/action-contract.sh.'; fail=1
fi
if any '^\.github/(actions|workflows)/' && ! any '^docs/(content|adr)/'; then
  echo 'BLOCKED: public Action/workflow change has no docs or ADR change.'; fail=1
fi
if any '(package\.json|yarn\.lock|Cargo\.toml|Cargo\.lock|install|dependency)' && ! any '(^|/)(test|tests|fixtures|scripts)/|verify-install'; then
  echo 'BLOCKED: dependency/install change has no focused-install or dependency regression evidence.'; fail=1
fi

if ((fail)); then
  echo 'review-change: BLOCKED'
  exit 1
fi
echo 'review-change: PASS (heuristics passed; run semantic reviewer and applicable completion gates)'
