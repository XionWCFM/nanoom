#!/usr/bin/env bash
set -Eeuo pipefail
[[ "$CLEANUP_CHECKOUT" == true ]] || exit 0
[[ "$CWD" == .nanoom/* && "$CWD" != *..* && "$CWD" != /* ]] || {
  echo "cleanupCheckout requires cwd below .nanoom/, got '$CWD'" >&2
  exit 1
}
workspace=$(cd "$GITHUB_WORKSPACE" && pwd -P)
target=$(cd "$GITHUB_WORKSPACE/$CWD" && pwd -P)
[[ "$target" == "$workspace/.nanoom/"* ]] || {
  echo "cleanupCheckout refused path outside $workspace/.nanoom" >&2
  exit 1
}
rm -rf -- "$target"
