#!/usr/bin/env bash
set -euo pipefail
cd "$CWD"
# Corepack reads the packageManager declaration; the consumer does not repeat it.
manager=$(jq -r '.packageManager // ""' package.json)
if [[ -f yarn.lock || -f pnpm-lock.yaml || "$manager" == npm@* ]]; then
  if command -v corepack >/dev/null; then
    corepack_binary=corepack
  else
    corepack_home="$RUNNER_TEMP/nanoom-corepack"
    npm install --prefix "$corepack_home" --no-audit --no-fund --ignore-scripts corepack@0.36.0
    corepack_binary="$corepack_home/node_modules/.bin/corepack"
  fi
  shim_dir="$RUNNER_TEMP/nanoom-package-manager"
  mkdir -p "$shim_dir"
  shims=(yarn pnpm)
  # npm shims are opt-in in Corepack; respect an explicit npm version too.
  if [[ "$manager" == npm@* ]]; then shims+=(npm); fi
  "$corepack_binary" enable --install-directory "$shim_dir" "${shims[@]}"
  echo "$shim_dir" >> "$GITHUB_PATH"
fi
