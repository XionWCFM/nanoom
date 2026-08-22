#!/usr/bin/env bash
# Creates a small JS monorepo fixture used by action-test.yml.
# Usage: setup-fixture.sh [--shards] [--isolate] [--change]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"   # repo root
FIXTURE="$PWD/.fixture"
mkdir -p "$FIXTURE/packages/app" "$FIXTURE/packages/lib"

cat > "$FIXTURE/package.json" << 'EOF'
{
  "name": "fixture-root",
  "private": true,
  "workspaces": ["packages/*"]
}
EOF

cat > "$FIXTURE/packages/app/package.json" << 'EOF'
{
  "name": "@fixture/app",
  "version": "1.0.0",
  "dependencies": { "@fixture/lib": "^1.0.0" },
  "scripts": { "test": "echo app-test" }
}
EOF

cat > "$FIXTURE/packages/lib/package.json" << 'EOF'
{
  "name": "@fixture/lib",
  "version": "1.0.0",
  "scripts": { "test": "echo lib-test" }
}
EOF

SHARD_BLOCK=""
ISOLATE_BLOCK=""
CHANGE=false
for arg in "$@"; do
  case "$arg" in
    --shards) SHARD_BLOCK=', "rules": [{ "name": "@fixture/lib", "shard": [{ "task": "test", "shard": 2 }] }]' ;;
    --isolate) ISOLATE_BLOCK=', "rules": [{ "name": "@fixture/app", "isolate": ["test"] }]' ;;
    --change) CHANGE=true ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

RULES=""
if [ -n "$SHARD_BLOCK" ]; then RULES="$SHARD_BLOCK"; fi
if [ -n "$ISOLATE_BLOCK" ]; then
  if [ -n "$RULES" ]; then
    RULES=', "rules": [{ "name": "@fixture/lib", "shard": [{ "task": "test", "shard": 2 }] }, { "name": "@fixture/app", "isolate": ["test"] }]'
  else
    RULES="$ISOLATE_BLOCK"
  fi
fi

if [ -n "$RULES" ]; then
  SHARD_BLOCK="$RULES"
else
  SHARD_BLOCK=""
fi

cat > "$FIXTURE/nanoom.config.json" << EOF
{
  "group": {
    "ci": { "tasks": ["test"], "concurrency": 4${SHARD_BLOCK} }
  },
  "globalDependencies": ["*.lock"]
}
EOF

cd "$FIXTURE"
git init -q -b main
git config user.email fixture@example.com
git config user.name fixture
git add .
git commit -q -m init --no-gpg-sign
if [ "$CHANGE" = true ]; then
  git -C "$FIXTURE" switch -q -c feature
  printf 'fixture change\n' > "$FIXTURE/packages/lib/changed.txt"
  git -C "$FIXTURE" add packages/lib/changed.txt
  git -C "$FIXTURE" commit -q -m change --no-gpg-sign
fi
echo "Fixture ready at $FIXTURE"
