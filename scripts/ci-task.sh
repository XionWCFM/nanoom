#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TERM_COLOR=always RUSTFLAGS='-D warnings'
case "${1:-}" in
  test)
    cargo test --locked --all --all-features
    node packages/cli/smoke-test.js
    cargo build --locked --release
    ;;
  check)
    rustup component add rustfmt clippy llvm-tools-preview
    command -v cargo-llvm-cov >/dev/null || cargo install cargo-llvm-cov --locked
    bash scripts/verify-completion.sh --local
    bash scripts/history-server-e2e-test.sh
    cargo build --locked --release
    ;;
  *) echo 'expected check or test' >&2; exit 2 ;;
esac
