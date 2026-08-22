#!/usr/bin/env bash
# Publishes the @nanoom/cli wrapper and every platform-specific package.
# Platform packages download their binary from GitHub Releases during publish.
set -euo pipefail

VERSION=$(node -p "require('./package.json').version")
REPOSITORY="${GITHUB_REPOSITORY:-$(node -p "(typeof require('./package.json').repository === 'string' ? require('./package.json').repository : require('./package.json').repository.url).match(/github\\.com[/:]([^/]+\\/[^/.]+?)(?:\\.git)?$/)[1]")}"
REPO_URL="https://github.com/${REPOSITORY}/releases/download/v${VERSION}"

TARGETS=(linux-x64 linux-arm64 macos-x64 macos-arm64 windows-x64)
ARTIFACTS=(nanoom-linux-x64.tar.gz nanoom-linux-arm64.tar.gz nanoom-macos-x64.tar.gz nanoom-macos-arm64.tar.gz nanoom-windows-x64.zip)

WORKDIR=$(mktemp -d)
trap 'rm -rf "$WORKDIR"' EXIT

for index in "${!TARGETS[@]}"; do
  platform="${TARGETS[$index]}"
  artifact="${ARTIFACTS[$index]}"
  pkg="@nanoom/cli-${platform}"
  dir="$WORKDIR/$platform"
  mkdir -p "$dir"

  cat > "$dir/package.json" << EOF
{
  "name": "$pkg",
  "version": "$VERSION",
  "description": "nanoom binary for $platform",
  "license": "MIT",
  "os": ["$(echo "$platform" | cut -d- -f1 | sed 's/macos/darwin/;s/windows/win32/')"],
  "cpu": ["$(echo "$platform" | cut -d- -f2 | sed 's/x64/x64/;s/arm64/arm64/')"]
}
EOF

  curl -fsSL "$REPO_URL/$artifact" -o "$dir/$artifact"

  case "$artifact" in
    *.tar.gz) tar -xzf "$dir/$artifact" -C "$dir" ;;
    *.zip)
      if command -v unzip > /dev/null; then
        unzip -q "$dir/$artifact" -d "$dir"
      else
        node -e "const {execSync}=require('child_process');execSync('powershell -Command Expand-Archive -Path \'$dir/$artifact\' -DestinationPath \'$dir\'')"
      fi
      ;;
  esac

  cd "$dir"
  npm publish --access public
  cd - > /dev/null
done

npm publish --access public
echo "Published @nanoom/cli@$VERSION and all platform packages."
