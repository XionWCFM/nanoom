#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
repo="$tmp/producer"
consumer="$tmp/consumer"
mkdir -p "$repo/packages/pkg-a" "$repo/packages/pkg-shared" "$repo/packages/pkg-b" "$repo/tools/always" "$repo/tools/unrelated" "$tmp/bin" "$consumer"
cat > "$repo/package.json" <<'JSON'
{"name":"root","private":true,"packageManager":"pnpm@9.1.0","workspaces":["packages/*"]}
JSON
printf 'lockfileVersion: 9\n' > "$repo/pnpm-lock.yaml"
cat > "$repo/nanoom.config.json" <<'JSON'
{"workspace":{"include":["packages/*"]},"checkout":{"always":["tools/always"]},"group":{"ci":{"tasks":["test"]}}}
JSON
cat > "$repo/packages/pkg-a/package.json" <<'JSON'
{"name":"pkg-a","version":"1.0.0","dependencies":{"pkg-shared":"workspace:*"},"scripts":{"test":"node -e \"require('fs').writeFileSync(process.env.RUNNER_TEMP + '/pkg-a-ran', 'yes')\""}}
JSON
cat > "$repo/packages/pkg-shared/package.json" <<'JSON'
{"name":"pkg-shared","version":"1.0.0","scripts":{"test":"node -e \"console.log('pkg-shared test ran')\""}}
JSON
cat > "$repo/packages/pkg-b/package.json" <<'JSON'
{"name":"pkg-b","version":"1.0.0","scripts":{"test":"node -e \"console.log('pkg-b test ran')\""}}
JSON
touch "$repo/tools/always/keep.txt" "$repo/tools/unrelated/keep.txt"
git -C "$repo" init -q
git -C "$repo" config user.email test@example.com
git -C "$repo" config user.name test
git -C "$repo" add .
git -C "$repo" commit -qm base
base=$(git -C "$repo" rev-parse HEAD)
printf 'change\n' > "$repo/packages/pkg-a/change.txt"
git -C "$repo" add .
git -C "$repo" commit -qm change
head=$(git -C "$repo" rev-parse HEAD)

cat > "$tmp/bin/pnpm" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$FAKE_PNPM_CALLS"
if [[ ${1:-} == run ]]; then exec npm run "$2"; fi
exit 0
SH
chmod +x "$tmp/bin/pnpm"
ln -s "$root/target/debug/nanoom" "$tmp/bin/nanoom"
export PATH="$tmp/bin:$PATH" FAKE_PNPM_CALLS="$tmp/pnpm-calls"
export RUNNER_TEMP="$tmp/runner-temp" GITHUB_WORKSPACE="$consumer" GITHUB_STEP_SUMMARY="$tmp/summary"
export GITHUB_OUTPUT="$tmp/affected-output" GITHUB_ACTION_PATH="$root/.github/actions/affected"
export ACTION_NAME=affected ACTION_CWD="$repo" CWD="$repo" CONFIG=nanoom.config.json
export BASE="$base" HEAD="$head" EVENT=push EVENT_BASE="$base" EVENT_HEAD="$head"
export REF_NAME=feature HISTORY_REF=feature WORKFLOW_REF=owner/repo/.github/workflows/ci.yml@refs/heads/feature
export SCHEDULER=artifact TIMING_RUNNER=auto TIMING_ENVIRONMENT=linux-x64 COORDINATOR_URL='' COORDINATOR_TOKEN=''
export PACKAGE_MANAGER=auto
export REPOSITORY=owner/repo RUN_ID=8675309 RUN_ATTEMPT=1 GITHUB_SHA="$head" GITHUB_JOB=affected
export API=https://api.github.com TOKEN=test-token HISTORY_ARTIFACT=nanoom-timing-history RELEASE_BASE_URL=https://github.com
mkdir -p "$RUNNER_TEMP"
bash "$GITHUB_ACTION_PATH/run.sh" > "$tmp/affected.log"

plan_ref=$(sed -n 's/^plan=//p' "$GITHUB_OUTPUT")
groups=$(sed -n 's/^groups=//p' "$GITHUB_OUTPUT")
test -n "$plan_ref"
jq -e '.artifactName == "nanoom-plan-v1-8675309-1-affected" and .provenance.head == $head' --arg head "$head" <<<"$plan_ref" >/dev/null
jq -e '.ci.include | length == 1 and .[0].group == "ci" and (.[] | has("items") | not)' <<<"$groups" >/dev/null
artifact_dir="$RUNNER_TEMP/nanoom-plan-8675309-1-affected"
jq -e '.packageManager == "pnpm" and .packageManagerVersion == "9.1.0" and .installMode == "focused" and (.lockfileDigest | test("^[0-9a-f]{64}$"))' "$artifact_dir/preparation-context.json" >/dev/null
test ! -s "$FAKE_PNPM_CALLS"
plan_sha=$(sha256sum "$artifact_dir/plan-v1.json" | awk '{print $1}')
jq -e --arg sha "$plan_sha" '.sha256 == $sha' "$artifact_dir/plan-reference.json" >/dev/null

# Select on a later attempt to prove the consumer fills current.attempt.
export GITHUB_ACTION_PATH="$root/.github/actions/prepare" PLAN_DIR="$artifact_dir"
export PLAN="$plan_ref" GROUP=ci ASSIGNMENT_ID="$(jq -r '.ci.include[0].assignmentId' <<<"$groups")"
export RUN_ATTEMPT=2 GITHUB_JOB=prepare MATRIX_INDEX=0 GITHUB_OUTPUT="$tmp/select-output"
export WORKFLOW_REF=owner/repo/.github/workflows/ci.yml@refs/heads/feature
bash "$GITHUB_ACTION_PATH/select.sh" > "$tmp/select.log"
assignment_file=$(sed -n 's/^assignment-file=//p' "$GITHUB_OUTPUT")
paths_file=$(sed -n 's/^paths-file=//p' "$GITHUB_OUTPUT")
cwd=$(sed -n 's/^cwd=//p' "$GITHUB_OUTPUT")
test "$(jq -r '.current.attempt' "$assignment_file")" -eq 2
test "$(jq -r '.provenance.producerAttempt' "$assignment_file")" -eq 1
jq -e 'all(.checkoutPaths[]; . != "packages/pkg-b") and (.checkoutPaths | index("packages/pkg-a") != null) and (.checkoutPaths | index("packages/pkg-shared") != null) and (.checkoutPaths | index("tools/always") != null)' "$assignment_file" >/dev/null
cp "$artifact_dir/plan-reference.json" "$tmp/original-reference.json"
jq '.sha256="0000000000000000000000000000000000000000000000000000000000000000"' "$artifact_dir/plan-reference.json" > "$tmp/tampered-reference.json"
cp "$tmp/tampered-reference.json" "$artifact_dir/plan-reference.json"
if bash "$GITHUB_ACTION_PATH/select.sh" > "$tmp/tamper.log" 2>&1; then echo 'tampered Plan digest unexpectedly selected' >&2; exit 1; fi
grep -q 'Plan artifact reference does not match' "$tmp/tamper.log"
cp "$tmp/original-reference.json" "$artifact_dir/plan-reference.json"

# The exact head starts in root-only non-cone mode, then prepare switches to
# the validated assignment's cone paths. Unrelated workspaces stay absent.
git clone -q --depth=1 --no-checkout "file://$repo" "$cwd"
git -C "$cwd" checkout -q --detach "$head"
test "$(git -C "$cwd" rev-list --count HEAD)" -eq 1
git -C "$cwd" sparse-checkout set --no-cone '/*' '!/*/'
test -f "$cwd/package.json"
test ! -e "$cwd/packages/pkg-a"
export GITHUB_ACTION_PATH="$root/.github/actions/prepare" ASSIGNMENT_FILE="$assignment_file" PATHS_FILE="$paths_file" CWD="$cwd"
export GITHUB_OUTPUT="$tmp/checkout-output" GITHUB_STEP_SUMMARY="$tmp/checkout-summary" GITHUB_SHA="$head"
export REPOSITORY=owner/repo RUN_ID=8675309 RUN_ATTEMPT=2 GITHUB_JOB=prepare WORKFLOW_REF=owner/repo/.github/workflows/ci.yml@refs/heads/feature
bash "$GITHUB_ACTION_PATH/checkout.sh" > "$tmp/checkout.log"
test "$(git -C "$cwd" rev-parse HEAD)" = "$head"
test -f "$cwd/packages/pkg-a/package.json"
test -f "$cwd/packages/pkg-shared/package.json"
test -f "$cwd/tools/always/keep.txt"
test ! -e "$cwd/packages/pkg-b"
test ! -e "$cwd/tools/unrelated"

# Focused install includes the root and assignment workspace closure.
export GITHUB_ACTION_PATH="$root/.github/actions/install" GITHUB_OUTPUT="$tmp/install-output"
export ASSIGNMENT_FILE="$assignment_file" CWD="$cwd" PM=pnpm MATRIX='' GITHUB_SHA="$head"
bash "$GITHUB_ACTION_PATH/run.sh" > "$tmp/install.log"
grep -q -- '--filter .' "$FAKE_PNPM_CALLS"
grep -q -- '--filter pkg-a...' "$FAKE_PNPM_CALLS"

# Nanoom and the Action execute the planned item through the installed runner.
export GITHUB_ACTION_PATH="$root/.github/actions/run" GITHUB_OUTPUT="$tmp/run-output"
export ASSIGNMENT_FILE="$assignment_file" CWD="$cwd" PM=pnpm TOOL=auto GROUP=ci
export SCHEDULER=off ARTIFACT_VERSION=v4 TIMING_ENVIRONMENT=linux-x64
export COORDINATOR_URL='' COORDINATOR_TOKEN=''
bash "$GITHUB_ACTION_PATH/run.sh" > "$tmp/run.log"
run_result=$(sed -n 's/^result=//p' "$GITHUB_OUTPUT")
jq -e '.status == "success" and .plannedItemCount == 1 and .executedItemCount == 1 and has("detailFile")' <<<"$run_result" >/dev/null
test -f "$RUNNER_TEMP/pkg-a-ran"

echo 'Plan producer, rerun selection, sparse checkout, focused install, and run contracts passed'
