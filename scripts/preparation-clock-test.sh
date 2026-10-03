#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin"
cat > "$tmp/bin/curl" <<'CURL'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$REQUESTS"
cat "$JOBS"
CURL
chmod +x "$tmp/bin/curl"
export PATH="$tmp/bin:$PATH" API=https://api.example TOKEN=test REPOSITORY=owner/repo RUN_ID=99 RUN_ATTEMPT=2 ASSIGNMENT_ID=ci-1
export REQUESTS="$tmp/requests" JOBS="$tmp/jobs.json"
source "$root/.github/actions/_setup/artifacts.sh"
source "$root/.github/actions/run/preparation.sh"
printf '%s\n' '{"jobs":[{"name":"Run affected work (app · test · [ci-1])","status":"in_progress","started_at":"2026-10-03T01:02:03Z"},{"name":"other [ci-10]","status":"in_progress","started_at":"2026-10-03T00:00:00Z"}]}' > "$JOBS"
expected=$(jq -n '"2026-10-03T01:02:03Z" | fromdateiso8601 * 1000')
test "$(nanoom_job_preparation_start)" = "$expected"
grep -q '/runs/99/attempts/2/jobs?per_page=100&page=1' "$REQUESTS"
# A duplicate name cannot identify this job reliably; omit telemetry.
jq '.jobs += [.jobs[0]]' "$JOBS" > "$tmp/duplicate.json"
mv "$tmp/duplicate.json" "$JOBS"
if nanoom_job_preparation_start; then exit 1; fi
printf '%s\n' '{"jobs":[]}' > "$JOBS"
if nanoom_job_preparation_start; then exit 1; fi
printf '%s\n' '{"jobs":[{"name":"[ci-1]","status":"in_progress","started_at":"invalid"}]}' > "$JOBS"
if nanoom_job_preparation_start 2>/dev/null; then exit 1; fi
: > "$REQUESTS"
TOKEN=''
if nanoom_job_preparation_start; then exit 1; fi
test ! -s "$REQUESTS"
printf '%s\n' 'native job preparation clock and ambiguous/unavailable fallback passed'
