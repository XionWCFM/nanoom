#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
expected=${1:-}
package_version=$(node -p "require('$root/packages/cli/package.json').version")
cargo_version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$root/Cargo.toml" | head -1)
[[ "$package_version" == "$cargo_version" ]]
[[ -z "$expected" || "$expected" == "v$package_version" ]]
bash "$root/scripts/platform-package-smoke.sh"
grep -Fqx "version = \"$package_version\"" "$root/Cargo.lock"
echo "version contract passed: v$package_version"
