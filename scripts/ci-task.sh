#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
case "${1:-}" in
  test)
    yarn format:check
    go vet ./...
    go test -race ./...
    bash scripts/build-go-cli.sh
    export NANOOM_TEST_BINARY=$(node -p 'require("path").resolve("dist", process.platform === "win32" ? "nanoom.exe" : "nanoom")')
    node packages/cli/smoke-test.js
    ;;
  check)
    bash scripts/verify-completion.sh --local
    NANOOM_TEST_BINARY="$PWD/dist/nanoom" HISTORY_E2E_WORKER_LANGUAGE=go bash scripts/history-server-e2e-test.sh
    ;;
  *) echo 'expected check or test' >&2; exit 2 ;;
esac
