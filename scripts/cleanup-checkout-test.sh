#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/workspace/.nanoom/job" "$tmp/workspace/keep"

env CLEANUP_CHECKOUT=false CWD=.nanoom/job GITHUB_WORKSPACE="$tmp/workspace" bash "$root/.github/actions/run/cleanup.sh"
test -d "$tmp/workspace/.nanoom/job"

env CLEANUP_CHECKOUT=true CWD=.nanoom/job GITHUB_WORKSPACE="$tmp/workspace" bash "$root/.github/actions/run/cleanup.sh"
test ! -e "$tmp/workspace/.nanoom/job"
test -d "$tmp/workspace/keep"

mkdir -p "$tmp/workspace/.nanoom/absolute-job"
env CLEANUP_CHECKOUT=true CWD="$tmp/workspace/.nanoom/absolute-job" GITHUB_WORKSPACE="$tmp/workspace" bash "$root/.github/actions/run/cleanup.sh"
test ! -e "$tmp/workspace/.nanoom/absolute-job"

if env CLEANUP_CHECKOUT=true CWD=. GITHUB_WORKSPACE="$tmp/workspace" bash "$root/.github/actions/run/cleanup.sh" >/dev/null 2>&1; then
  echo 'cleanup accepted repository root' >&2
  exit 1
fi
if env CLEANUP_CHECKOUT=true CWD="$tmp/workspace/keep" GITHUB_WORKSPACE="$tmp/workspace" bash "$root/.github/actions/run/cleanup.sh" >/dev/null 2>&1; then
  echo 'cleanup accepted a path outside .nanoom' >&2
  exit 1
fi
test -d "$tmp/workspace/keep"
echo 'isolated checkout cleanup contract passed'
