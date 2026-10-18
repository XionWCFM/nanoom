#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cd "$root"
cargo build --locked --bin nanoom
go build -trimpath -o "$tmp/nanoom-go" ./cmd/nanoom
node "$root/scripts/go-cli-contract.cjs" "$root/target/debug/nanoom" "$tmp/nanoom-go" "$tmp" "$root"
