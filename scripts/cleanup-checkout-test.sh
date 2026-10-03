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

mkdir -p "$tmp/workspace/.nanoom/nested-job/nested app"
git -C "$tmp/workspace/.nanoom/nested-job" init -q
touch "$tmp/workspace/.nanoom/nested-job/checkout-metadata"
env CLEANUP_CHECKOUT=true CWD="$tmp/workspace/.nanoom/nested-job/nested app" GITHUB_WORKSPACE="$tmp/workspace" bash "$root/.github/actions/run/cleanup.sh"
test ! -e "$tmp/workspace/.nanoom/nested-job"

if env CLEANUP_CHECKOUT=true CWD=. GITHUB_WORKSPACE="$tmp/workspace" bash "$root/.github/actions/run/cleanup.sh" >/dev/null 2>&1; then
  echo 'cleanup accepted repository root' >&2
  exit 1
fi
if env CLEANUP_CHECKOUT=true CWD="$tmp/workspace/keep" GITHUB_WORKSPACE="$tmp/workspace" bash "$root/.github/actions/run/cleanup.sh" >/dev/null 2>&1; then
  echo 'cleanup accepted a path outside .nanoom' >&2
  exit 1
fi
test -d "$tmp/workspace/keep"
# A directory below the main checkout must never resolve to a deletable Git root.
git -C "$tmp/workspace" init -q
mkdir -p "$tmp/workspace/.nanoom/not-a-checkout"
if env CLEANUP_CHECKOUT=true CWD=.nanoom/not-a-checkout GITHUB_WORKSPACE="$tmp/workspace" bash "$root/.github/actions/run/cleanup.sh" >/dev/null 2>&1; then
  echo 'cleanup accepted the main Git checkout through a nested directory' >&2
  exit 1
fi
test -d "$tmp/workspace/.nanoom/not-a-checkout"
echo 'isolated checkout cleanup contract passed'
