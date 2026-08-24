#!/usr/bin/env bash
# Validates the source package contracts for all release-binary platforms.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(node -p "require('$ROOT/packages/cli/package.json').version")"

for target in linux-x64 linux-arm64 macos-x64 macos-arm64 windows-x64; do
  case "$target" in
    linux-x64) os=linux; cpu=x64; binary=nanoom ;;
    linux-arm64) os=linux; cpu=arm64; binary=nanoom ;;
    macos-x64) os=darwin; cpu=x64; binary=nanoom ;;
    macos-arm64) os=darwin; cpu=arm64; binary=nanoom ;;
    windows-x64) os=win32; cpu=x64; binary=nanoom.exe ;;
  esac
  manifest="$ROOT/packages/cli-$target/package.json"
  test -f "$manifest"
  node -e '
    const fs = require("fs");
    const [file, name, version, os, cpu, binary] = process.argv.slice(1);
    const pkg = JSON.parse(fs.readFileSync(file, "utf8"));
    if (pkg.name !== `@nanoom/cli-${name}` || pkg.version !== version ||
        pkg.os?.[0] !== os || pkg.cpu?.[0] !== cpu || pkg.files?.[0] !== binary) process.exit(1);
  ' "$manifest" "$target" "$VERSION" "$os" "$cpu" "$binary"
done

node -e '
  const fs = require("fs");
  const [file, version] = process.argv.slice(1);
  const pkg = JSON.parse(fs.readFileSync(file, "utf8"));
  const mismatches = Object.entries(pkg.optionalDependencies || {}).filter(([, value]) => value !== version);
  if (mismatches.length !== 0) throw new Error(`wrapper dependency versions do not match ${version}`);
' "$ROOT/packages/cli/package.json" "$VERSION"

node "$ROOT/.yarn/releases/yarn-4.9.1.cjs" --cwd "$ROOT" workspaces list --json | \
  jq -s -e '[.[].name] | contains(["@nanoom/cli", "@nanoom/cli-linux-x64", "@nanoom/cli-linux-arm64", "@nanoom/cli-macos-x64", "@nanoom/cli-macos-arm64", "@nanoom/cli-windows-x64"])' >/dev/null

echo "Validated five Yarn Berry platform package contracts at version $VERSION."
