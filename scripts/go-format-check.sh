#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
files=$(gofmt -l cmd internal schema.go)
if [[ -n "$files" ]]; then
  printf 'Go files need gofmt:\n%s\n' "$files" >&2
  exit 1
fi
