#!/usr/bin/env bash
# Syncs Go and reference Cargo versions with packages/cli/package.json (source of truth).
# Usage: scripts/sync-version.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

VERSION=$(node -p "require('$ROOT/packages/cli/package.json').version")

sed -i.bak \
  "s/^version = \".*\"/version = \"${VERSION}\"/" \
  "$ROOT/Cargo.toml"
rm -f "$ROOT/Cargo.toml.bak"

for manifest in "$ROOT"/packages/cli-*/package.json; do
  node -e '
    const fs = require("fs");
    const [file, version] = process.argv.slice(1);
    const pkg = JSON.parse(fs.readFileSync(file, "utf8"));
    pkg.version = version;
    fs.writeFileSync(file, `${JSON.stringify(pkg, null, 2)}\n`);
  ' "$manifest" "$VERSION"
done

node -e '
  const fs = require("fs");
  const [file, version] = process.argv.slice(1);
  const pkg = JSON.parse(fs.readFileSync(file, "utf8"));
  for (const name of Object.keys(pkg.optionalDependencies || {})) {
    pkg.optionalDependencies[name] = version;
  }
  fs.writeFileSync(file, `${JSON.stringify(pkg, null, 2)}\n`);
' "$ROOT/packages/cli/package.json" "$VERSION"

sed -i.bak "s/^var Version = .*/var Version = \"${VERSION}\"/" "$ROOT/cmd/nanoom/main.go"
rm -f "$ROOT/cmd/nanoom/main.go.bak"
echo "Go, reference Cargo and npm package versions synced to ${VERSION}"
