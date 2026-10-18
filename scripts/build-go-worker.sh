#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
output=${1:-$root/crates/history-worker/build-go}
# workers-assets-gen replaces this directory; reject source/root destinations.
case "$output" in
  */build|*/build-go) ;;
  *) echo 'Go Worker output must end in /build or /build-go' >&2; exit 1 ;;
esac
if [[ -L "$output" ]]; then echo 'Go Worker output must not be a symlink' >&2; exit 1; fi
cd "$root"
go run github.com/syumai/workers-go/cmd/workers-assets-gen -mode=go -o "$output"
GOOS=js GOARCH=wasm CGO_ENABLED=0 go build -trimpath -ldflags='-s -w' -o "$output/app.wasm" ./cmd/history-worker
