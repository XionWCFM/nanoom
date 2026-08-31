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
fail=0

# Dependency-only updates reuse the repository's existing regression and
# release-contract gates. Requiring a touched test or docs file encourages
# meaningless edits and makes Dependabot permanently red.
dependency_only=1
for file in "${files[@]}"; do
  case "$file" in
    Cargo.toml|Cargo.lock|package.json|yarn.lock|pnpm-lock.yaml|packages/*/package.json) ;;
    .github/workflows/*.yml|.github/workflows/*.yaml)
      if git diff --unified=0 "$base_ref...HEAD" -- "$file" |
        grep -E '^[+-][^+-]' |
        grep -Ev '^[+-][[:space:]]*uses:[[:space:]]+[^[:space:]]+@[^[:space:]]+[[:space:]]*$' |
        grep -q .; then
        dependency_only=0
      fi
      ;;
    *) dependency_only=0 ;;
  esac
done

if ((dependency_only)); then
  echo 'review-change: dependency-only update; existing regression and release-contract gates are authoritative'
fi

if ((!dependency_only)) && any '^(src/|packages/cli/)' && ! any '(^|/)(tests?|__tests__|.*test.*|smoke-test\.js)'; then
  echo 'BLOCKED: CLI/source change has no changed regression test.'; fail=1
fi
if ((!dependency_only)) && any '^\.github/actions/' && ! has 'scripts/action-contract.sh'; then
  echo 'BLOCKED: Action change must update/confirm scripts/action-contract.sh.'; fail=1
fi
if ((!dependency_only)) && any '^\.github/(actions|workflows)/' && ! any '^docs/(content|adr)/'; then
  echo 'BLOCKED: public Action/workflow change has no docs or ADR change.'; fail=1
fi
if ((!dependency_only)) && any '(package\.json|yarn\.lock|Cargo\.toml|Cargo\.lock|install|dependency)' && ! any '(^|/)(test|tests|fixtures|scripts)/|verify-install'; then
  echo 'BLOCKED: dependency/install change has no focused-install or dependency regression evidence.'; fail=1
fi
if ((!dependency_only)) && any '(^|/)(Cargo\.toml|package\.json|src/|\.github/actions/)' && ! any '^docs/(content|adr)/'; then
  echo 'BLOCKED: public implementation change has no documentation update.'; fail=1
fi

if ((fail)); then
  echo 'review-change: BLOCKED'
  exit 1
fi
echo 'review-change: PASS (heuristics passed; run semantic reviewer and applicable completion gates)'
