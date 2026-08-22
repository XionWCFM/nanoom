#!/usr/bin/env bash
# Validate release artifacts without publishing or mutating a repository.
set -euo pipefail

ARTIFACT_DIR="${1:?usage: release-smoke.sh <artifact-directory>}"
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT_DIR/Cargo.toml" | head -1)"

expected=(
  "nanoom-linux-x64.tar.gz"
  "nanoom-linux-arm64.tar.gz"
  "nanoom-macos-x64.tar.gz"
  "nanoom-macos-arm64.tar.gz"
  "nanoom-windows-x64.zip"
)

for artifact in "${expected[@]}"; do
  file="$ARTIFACT_DIR/$artifact"
  checksum="$file.sha256"
  test -f "$file" || { echo "missing artifact: $artifact" >&2; exit 1; }
  test -f "$checksum" || { echo "missing checksum: $artifact.sha256" >&2; exit 1; }

  if command -v sha256sum >/dev/null 2>&1; then
    (cd "$ARTIFACT_DIR" && sha256sum -c "$(basename "$checksum")")
  else
    actual="$(shasum -a 256 "$file" | awk '{print $1}')"
    expected_hash="$(awk '{print $1}' "$checksum")"
    test "$actual" = "$expected_hash" || { echo "checksum mismatch: $artifact" >&2; exit 1; }
  fi

  case "$artifact" in
    *.tar.gz)
      listing="$(tar -tzf "$file")"
      printf '%s\n' "$listing" | grep -qx 'nanoom'
      mkdir -p "$ARTIFACT_DIR/.smoke/$artifact"
      tar -xzf "$file" -C "$ARTIFACT_DIR/.smoke/$artifact"
      test -x "$ARTIFACT_DIR/.smoke/$artifact/nanoom"
      "$ARTIFACT_DIR/.smoke/$artifact/nanoom" --version | grep -Fqx "nanoom $VERSION"
      ;;
    *.zip)
      command -v unzip >/dev/null 2>&1 || { echo "unzip is required for Windows archive validation" >&2; exit 1; }
      unzip -Z1 "$file" | grep -qx 'nanoom.exe'
      mkdir -p "$ARTIFACT_DIR/.smoke/$artifact"
      unzip -q "$file" -d "$ARTIFACT_DIR/.smoke/$artifact"
      test -f "$ARTIFACT_DIR/.smoke/$artifact/nanoom.exe"
      ;;
  esac
done

echo "release artifact smoke test passed for version $VERSION"
