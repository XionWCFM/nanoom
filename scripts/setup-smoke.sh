#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/payload"
printf '#!/usr/bin/env bash\nexit 0\n' > "$tmp/payload/nanoom"
chmod +x "$tmp/payload/nanoom"
tar -czf "$tmp/nanoom-linux-x64.tar.gz" -C "$tmp/payload" nanoom
sha256sum "$tmp/nanoom-linux-x64.tar.gz" > "$tmp/nanoom-linux-x64.tar.gz.sha256"

cat > "$tmp/bin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
args=" $* "
printf '%s\n' "$args" >> "$FIXTURE_REQUESTS"
if [[ "$args" == *'/releases/latest '* ]]; then
  [[ "$args" == *' -H Authorization: Bearer fixture-token '* ]]
  printf '{"tag_name":"v0.0.0"}\n'
  exit
fi
out=
for ((i=1; i<=$#; i++)); do
  [[ ${!i} == -o ]] && { next=$((i + 1)); out=${!next}; break; }
done
[[ -n "$out" ]]
case "$out" in
  *.sha256) cp "$FIXTURE_ARCHIVE.sha256" "$out" ;;
  *) cp "$FIXTURE_ARCHIVE" "$out" ;;
esac
EOF
chmod +x "$tmp/bin/curl"

PATH="$tmp/bin:$PATH" RUNNER_TEMP="$tmp/runner" GITHUB_PATH="$tmp/github-path" \
  TOKEN=fixture-token FIXTURE_ARCHIVE="$tmp/nanoom-linux-x64.tar.gz" FIXTURE_REQUESTS="$tmp/requests" REQUESTED=v0.0.0 RELEASE_BASE_URL=https://github.example.test \
  bash "$root/.github/actions/_setup/setup.sh"
grep -q '/nanoom-bin$' "$tmp/github-path"
grep -Fq 'https://github.example.test/XionWCFM/nanoom/releases/download/v0.0.0/nanoom-' "$tmp/requests"
"$tmp/runner/nanoom-bin/nanoom"
echo 'setup authentication smoke passed'
