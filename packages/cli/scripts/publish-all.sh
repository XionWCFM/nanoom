#!/usr/bin/env bash
# Publishes the @nanoom/cli wrapper and every platform-specific package with
# Yarn Berry. Release assets are copied into the source platform workspaces.
set -euo pipefail

VERSION=$(node -p "require('./package.json').version")
REPOSITORY="${GITHUB_REPOSITORY:-$(node -p "(typeof require('./package.json').repository === 'string' ? require('./package.json').repository : require('./package.json').repository.url).match(/github\\.com[/:]([^/]+\\/[^/.]+?)(?:\\.git)?$/)[1]")}"
REPO_URL="https://github.com/${REPOSITORY}/releases/download/v${VERSION}"

TARGETS=(linux-x64 linux-arm64 macos-x64 macos-arm64 windows-x64)
ARTIFACTS=(nanoom-linux-x64.tar.gz nanoom-linux-arm64.tar.gz nanoom-macos-x64.tar.gz nanoom-macos-arm64.tar.gz nanoom-windows-x64.zip)

for index in "${!TARGETS[@]}"; do
  platform="${TARGETS[$index]}"
  artifact="${ARTIFACTS[$index]}"
  pkg="@nanoom/cli-${platform}"
  dir="../cli-$platform"

  mkdir -p "$dir"
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

  yarn workspace "$pkg" npm publish --access public --tolerate-republish
done

yarn workspace @nanoom/cli npm publish --access public --tolerate-republish
echo "Published @nanoom/cli@$VERSION and all platform packages."
