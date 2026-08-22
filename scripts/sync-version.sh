#!/usr/bin/env bash
# Syncs the Cargo.toml version with packages/cli/package.json (source of truth).
# Usage: scripts/sync-version.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

VERSION=$(node -p "require('$ROOT/packages/cli/package.json').version")

sed -i.bak \
  "s/^version = \".*\"/version = \"${VERSION}\"/" \
  "$ROOT/Cargo.toml"
rm -f "$ROOT/Cargo.toml.bak"

echo "Cargo.toml version synced to ${VERSION}"
