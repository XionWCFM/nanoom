#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/source" "$tmp/runner"
printf '%s\n' '{"version":3,"observations":[]}' > "$tmp/source/measurements.json"
(cd "$tmp/source" && zip -q "$tmp/measurement.zip" measurements.json)
size=$(wc -c < "$tmp/measurement.zip" | tr -d ' ')
jq -cn --argjson size "$size" '{artifacts:[
  {id:1,name:"nanoom-measurement-v3-99-2-ci-1",expired:false,size_in_bytes:$size},
  {id:2,name:"nanoom-measurement-v3-99-1-ci-1",expired:false,size_in_bytes:$size},
  {id:3,name:"nanoom-plan-v1-99-2-plan",expired:false,size_in_bytes:$size},
  {id:4,name:"nanoom-measurement-v3-99-2-ci-2",expired:true,size_in_bytes:$size}]}' > "$tmp/artifacts.json"
cat > "$tmp/bin/curl" <<'CURL'
#!/usr/bin/env bash
output=''; url=''
while (($#)); do
  case "$1" in
    -o) output=$2; shift 2 ;;
    http*) url=$1; shift ;;
    *) shift ;;
  esac
done
printf '%s\n' "$url" >> "$REQUESTS"
case "$url" in
  */runs/99/artifacts*) cat "$ARTIFACTS" ;;
  */artifacts/1/zip) cp "$ARCHIVE" "$output" ;;
  *) exit 22 ;;
esac
CURL
chmod +x "$tmp/bin/curl"
export PATH="$tmp/bin:$PATH" GITHUB_ACTION_PATH="$root/.github/actions/history-ghes"
export API=https://ghes.example/api/v3 REPOSITORY=owner/repo TOKEN=test RUN_ID=99 RUN_ATTEMPT=2 RUNNER_TEMP="$tmp/runner"
export MEASUREMENT_DIR="$tmp/download" REQUESTS="$tmp/requests" ARTIFACTS="$tmp/artifacts.json" ARCHIVE="$tmp/measurement.zip"
for action in history-ghes history status; do
  export GITHUB_ACTION_PATH="$root/.github/actions/$action"
  : > "$REQUESTS"
  bash "$GITHUB_ACTION_PATH/../_setup/download-measurements.sh"
  test "$(grep -c '/artifacts/.*/zip' "$REQUESTS")" -eq 1
done
cmp "$tmp/source/measurements.json" "$MEASUREMENT_DIR/nanoom-measurement-v3-99-2-ci-1/measurements.json"
test "$(grep -c '/artifacts/.*/zip' "$REQUESTS")" -eq 1
! grep -q '/artifacts/[234]/zip' "$REQUESTS"
# No measurements is a valid no-change attempt, with no archive requests.
printf '%s\n' '{"artifacts":[]}' > "$ARTIFACTS"
: > "$REQUESTS"
bash "$GITHUB_ACTION_PATH/../_setup/download-measurements.sh"
! grep -q '/artifacts/.*/zip' "$REQUESTS"
# An oversized selected archive fails before downloading, so history degrades.
printf '%s\n' '{"artifacts":[{"id":1,"name":"nanoom-measurement-v3-99-2-ci-1","expired":false,"size_in_bytes":4194305}]}' > "$ARTIFACTS"
if bash "$GITHUB_ACTION_PATH/../_setup/download-measurements.sh"; then exit 1; fi
! grep -q '/artifacts/.*/zip' "$REQUESTS"
printf '%s\n' 'current-attempt measurement selection and bounded downloads passed'
