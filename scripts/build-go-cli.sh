#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
output=${1:-}
if [[ -z "$output" ]]; then
  output=dist/nanoom
  if [[ $(go env GOOS) == windows ]]; then output+=.exe; fi
fi
version=$(node -p 'require("./packages/cli/package.json").version')
mkdir -p "$(dirname "$output")"
CGO_ENABLED=0 go build -trimpath -ldflags "-s -w -X main.Version=$version" -o "$output" ./cmd/nanoom
