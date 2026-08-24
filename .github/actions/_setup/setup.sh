#!/usr/bin/env bash
set -euo pipefail

requested=${REQUESTED:-latest}
repository=${REPOSITORY:-XionWCFM/nanoom}
api=${API:-https://api.github.com}
server=${SERVER:-https://github.com}
bin_dir="$RUNNER_TEMP/nanoom-bin"
mkdir -p "$bin_dir"

if [[ "$requested" == local ]]; then
  binary=${BINARY:?NANOOM_LOCAL_BINARY must point to an executable file}
  test -x "$binary"
  cp "$binary" "$bin_dir/${binary##*/}"
  echo "$bin_dir" >> "$GITHUB_PATH"
  echo "::notice title=nanoom setup::Using local binary $binary"
  exit 0
fi

if [[ "$requested" == latest ]]; then
  curl_args=(-fsSL)
  [[ -n ${TOKEN:-} ]] && curl_args+=(-H "Authorization: Bearer $TOKEN")
  version=$(curl "${curl_args[@]}" "$api/repos/$repository/releases/latest" | jq -er .tag_name)
else
  version=$requested
fi

case "$(uname -s)" in
  Darwin) os=macos; extension=tar.gz; executable=nanoom ;;
  Linux) os=linux; extension=tar.gz; executable=nanoom ;;
  MINGW*|MSYS*|CYGWIN*) os=windows; extension=zip; executable=nanoom.exe ;;
  *) echo "Unsupported runner: $(uname -s)" >&2; exit 2 ;;
esac
case "$(uname -m)" in
  x86_64|amd64) arch=x64 ;;
  aarch64|arm64) arch=arm64 ;;
  *) echo "Unsupported architecture: $(uname -m)" >&2; exit 2 ;;
esac

file="nanoom-$os-$arch.$extension"
download_dir="$RUNNER_TEMP/nanoom-download"
mkdir -p "$download_dir"
url="$server/$repository/releases/download/$version/$file"
curl -fsSL "$url" -o "$download_dir/$file"
curl -fsSL "$url.sha256" -o "$download_dir/$file.sha256"
expected=$(awk '{print $1}' "$download_dir/$file.sha256")
actual=$( (command -v sha256sum >/dev/null && sha256sum "$download_dir/$file" || shasum -a 256 "$download_dir/$file") | awk '{print $1}')
[[ "$expected" == "$actual" ]] || { echo "Checksum mismatch for $file" >&2; exit 1; }

if [[ "$extension" == zip ]]; then
  powershell -NoProfile -Command "Expand-Archive -Force '$download_dir/$file' '$bin_dir'"
else
  tar -xzf "$download_dir/$file" -C "$bin_dir"
  chmod +x "$bin_dir/$executable"
fi
test -x "$bin_dir/$executable" || [[ "$os" == windows && -f "$bin_dir/$executable" ]]
echo "$bin_dir" >> "$GITHUB_PATH"
echo "::notice title=nanoom setup::Installed $repository@$version for $os/$arch (checksum verified)"
