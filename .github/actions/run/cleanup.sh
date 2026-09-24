#!/usr/bin/env bash
set -Eeuo pipefail
[[ "$CLEANUP_CHECKOUT" == true ]] || exit 0
[[ "$CWD" != *..* ]] || {
  echo "cleanupCheckout refused path containing '..': '$CWD'" >&2
  exit 1
}
workspace=$(cd "$GITHUB_WORKSPACE" && pwd -P)
case "$CWD" in
  .nanoom/*) target_path="$GITHUB_WORKSPACE/$CWD" ;;
  "$GITHUB_WORKSPACE"/.nanoom/*) target_path="$CWD" ;;
  "$workspace"/.nanoom/*) target_path="$CWD" ;;
  *)
    echo "cleanupCheckout requires cwd below $workspace/.nanoom/, got '$CWD'" >&2
    exit 1
    ;;
esac
target=$(cd "$target_path" && pwd -P)
[[ "$target" == "$workspace/.nanoom/"* ]] || {
  echo "cleanupCheckout refused path outside $workspace/.nanoom" >&2
  exit 1
}
rm -rf -- "$target"
