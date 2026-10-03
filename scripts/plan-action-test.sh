#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
repo="$tmp/producer"
consumer="$tmp/consumer"
mkdir -p "$repo/packages/pkg-a" "$repo/packages/pkg-shared" "$repo/packages/pkg-b" "$repo/packages/root-tool" "$repo/packages/root-shared" "$repo/.yarn/releases" "$repo/.yarn/plugins" "$repo/tools/always" "$repo/tools/unrelated" "$tmp/bin" "$consumer"
cat > "$repo/package.json" <<'JSON'
{"name":"root","private":true,"packageManager":"pnpm@9.1.0","workspaces":["packages/*"],"devDependencies":{"root-tool":"workspace:*"}}
JSON
printf 'lockfileVersion: 9\n' > "$repo/pnpm-lock.yaml"
printf 'yarnPath: .yarn/releases/yarn.cjs\nplugins:\n  - path: .yarn/plugins/plugin.cjs\n' > "$repo/.yarnrc.yml"
touch "$repo/.yarn/releases/yarn.cjs" "$repo/.yarn/plugins/plugin.cjs"
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
printf '%s\n' '{"name":"root-tool","version":"1.0.0","dependencies":{"root-shared":"workspace:*"}}' > "$repo/packages/root-tool/package.json"
printf '%s\n' '{"name":"root-shared","version":"1.0.0"}' > "$repo/packages/root-shared/package.json"
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
matrix=$(sed -n 's/^matrix=//p' "$GITHUB_OUTPUT")
test "$(jq -c .include <<<"$matrix")" = "$(jq -c '[.[] | .include[]]' <<<"$groups")"
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

# The four-step template checks out directly in the job workspace using the
# producer's non-cone patterns, then install selects the authoritative Plan.
direct="$tmp/direct-checkout"
git clone -q --depth=1 --no-checkout "file://$repo" "$direct"
jq -r '.ci.include[0].checkout.sparseCheckout' <<<"$groups" |
  git -C "$direct" sparse-checkout set --no-cone --stdin
git -C "$direct" checkout -q --detach "$(jq -r '.ci.include[0].checkout.ref' <<<"$groups")"
test -f "$direct/package.json"
test -f "$direct/pnpm-lock.yaml"
test -f "$direct/packages/pkg-a/change.txt"
test -f "$direct/packages/pkg-shared/package.json"
test -f "$direct/packages/root-tool/package.json"
test -f "$direct/packages/root-shared/package.json"
test -f "$direct/.yarn/releases/yarn.cjs"
test -f "$direct/.yarn/plugins/plugin.cjs"
test -f "$direct/tools/always/keep.txt"
test ! -e "$direct/packages/pkg-b"
test ! -e "$direct/tools/unrelated"
export GITHUB_WORKSPACE="$direct" SELECT_CWD="$direct" GITHUB_JOB=run
cat > "$tmp/bin/corepack" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" > "$RUNNER_TEMP/corepack-call"
SH
chmod +x "$tmp/bin/corepack"
export CWD="$direct" GITHUB_PATH="$tmp/package-manager-path"
bash "$root/.github/actions/install/activate.sh"
grep -q 'enable --install-directory .* yarn pnpm' "$RUNNER_TEMP/corepack-call"
test "$(cat "$GITHUB_PATH")" = "$RUNNER_TEMP/nanoom-package-manager"
# Declared npm versions also activate the opt-in npm Corepack shim.
mkdir -p "$tmp/npm-source"
printf '%s\n' '{"name":"npm-root","packageManager":"npm@11.16.0"}' > "$tmp/npm-source/package.json"
CWD="$tmp/npm-source" GITHUB_PATH="$tmp/npm-path" bash "$root/.github/actions/install/activate.sh"
grep -q 'enable --install-directory .* yarn pnpm npm' "$RUNNER_TEMP/corepack-call"
test "$(cat "$tmp/npm-path")" = "$RUNNER_TEMP/nanoom-package-manager"
# An undeclared npm repository uses Node's bundled npm without Corepack.
printf '%s\n' '{"name":"npm-root"}' > "$tmp/npm-source/package.json"
CWD="$tmp/npm-source" GITHUB_PATH="$tmp/default-npm-path" bash "$root/.github/actions/install/activate.sh"
test ! -e "$tmp/default-npm-path"
export GITHUB_ACTION_PATH="$root/.github/actions/install" GITHUB_OUTPUT="$tmp/direct-selection"
bash "$root/.github/actions/prepare/select.sh" > "$tmp/direct-select.log"
export ASSIGNMENT_FILE="$(sed -n 's/^assignment-file=//p' "$GITHUB_OUTPUT")" CWD="$direct"
export GITHUB_OUTPUT="$tmp/direct-install"
bash "$GITHUB_ACTION_PATH/run.sh" > "$tmp/direct-install.log"
export INSTALL_RESULT="$(sed -n 's/^result=//p' "$GITHUB_OUTPUT")"
export GITHUB_ACTION_PATH="$root/.github/actions/run" GITHUB_OUTPUT="$tmp/direct-run" CWD=''
bash "$GITHUB_ACTION_PATH/run.sh" > "$tmp/direct-run.log"
jq -e '.status == "success" and .executedItemCount == 1' <(sed -n 's/^result=//p' "$GITHUB_OUTPUT") >/dev/null

# The current checkout must still match the exact Plan source.
git -C "$direct" -c user.name=test -c user.email=test@example.com commit --allow-empty -qm tampered
export GITHUB_ACTION_PATH="$root/.github/actions/install" CWD="$direct"
if bash "$GITHUB_ACTION_PATH/run.sh" > "$tmp/direct-tamper.log" 2>&1; then
  echo 'wrong source SHA unexpectedly installed' >&2; exit 1
fi
grep -q 'HEAD mismatch' "$tmp/direct-tamper.log"

# A custom configuration outside root files is restored from the planned SHA
# during manifest-only planning, then carried through the authoritative Plan.
mkdir -p "$repo/settings"
git -C "$repo" mv nanoom.config.json settings/custom.json
printf '%s\n' '{"group":{"custom":{"tasks":["test"],"rules":[{"name":"pkg-b","ignore":true},{"name":"pkg-shared","ignore":true},{"name":"root-tool","ignore":true},{"name":"root-shared","ignore":true}]}}}' > "$repo/settings/custom.json"
git -C "$repo" add .
git -C "$repo" -c user.name=test -c user.email=test@example.com commit -qm custom --no-gpg-sign
custom_head=$(git -C "$repo" rev-parse HEAD)
planner="$tmp/custom-planner"
git clone -q --depth=1 --no-checkout "file://$repo" "$planner"
git -C "$planner" sparse-checkout set --no-cone '/*' '!/*/' '**/package.json'
git -C "$planner" checkout -q --detach "$custom_head"
test ! -e "$planner/settings/custom.json"
export GITHUB_ACTION_PATH="$root/.github/actions/affected" CWD="$planner" CONFIG=settings/custom.json
export BASE="$head" HEAD="$custom_head" EVENT_HEAD="$custom_head" GITHUB_SHA="$custom_head" RUN_ATTEMPT=1 RUN_ID=8675310 GITHUB_JOB=affected SCHEDULER=off
export GITHUB_OUTPUT="$tmp/custom-affected"
bash "$GITHUB_ACTION_PATH/run.sh" > "$tmp/custom-affected.log"
test -f "$planner/settings/custom.json"
plan_ref=$(sed -n 's/^plan=//p' "$GITHUB_OUTPUT")
groups=$(sed -n 's/^groups=//p' "$GITHUB_OUTPUT")
artifact_dir="$RUNNER_TEMP/nanoom-plan-$RUN_ID-$RUN_ATTEMPT-$GITHUB_JOB"
jq -e '.configPath == "settings/custom.json" and .itemCount == 1' "$artifact_dir/plan-v1.json" >/dev/null
custom="$tmp/custom-consumer"
git clone -q --depth=1 --no-checkout "file://$repo" "$custom"
jq -r '.custom.include[0].checkout.sparseCheckout' <<<"$groups" | git -C "$custom" sparse-checkout set --no-cone --stdin
git -C "$custom" checkout -q --detach "$custom_head"
test -f "$custom/settings/custom.json"
test ! -e "$custom/nanoom.config.json"
test ! -e "$custom/packages/pkg-b"
export GITHUB_ACTION_PATH="$root/.github/actions/install" GITHUB_WORKSPACE="$custom" SELECT_CWD="$custom" CWD="$custom" PLAN="$plan_ref" PLAN_DIR="$artifact_dir" GROUP=custom ASSIGNMENT_ID="$(jq -r '.custom.include[0].assignmentId' <<<"$groups")" GITHUB_JOB=run
export GITHUB_OUTPUT="$tmp/custom-selection"
bash "$root/.github/actions/prepare/select.sh" > "$tmp/custom-select.log"
export ASSIGNMENT_FILE="$(sed -n 's/^assignment-file=//p' "$GITHUB_OUTPUT")" GITHUB_OUTPUT="$tmp/custom-install"
bash "$GITHUB_ACTION_PATH/run.sh" > "$tmp/custom-install.log"
export INSTALL_RESULT="$(sed -n 's/^result=//p' "$GITHUB_OUTPUT")" GITHUB_ACTION_PATH="$root/.github/actions/run" GITHUB_OUTPUT="$tmp/custom-run" CWD=''
bash "$GITHUB_ACTION_PATH/run.sh" > "$tmp/custom-run.log"
jq -e '.status == "success" and .executedItemCount == 1 and .group == "custom"' <(sed -n 's/^result=//p' "$GITHUB_OUTPUT") >/dev/null
# Planning hashes the same npm lockfile that npm ci prioritizes.
jq '.packageManager="npm@11.16.0"' "$repo/package.json" > "$tmp/npm-package.json"
cp "$tmp/npm-package.json" "$repo/package.json"
printf '%s\n' 'package lock' > "$repo/package-lock.json"
printf '%s\n' 'shrinkwrap lock' > "$repo/npm-shrinkwrap.json"
git -C "$repo" add .
git -C "$repo" -c user.name=test -c user.email=test@example.com commit -qm npm --no-gpg-sign
npm_head=$(git -C "$repo" rev-parse HEAD)
export GITHUB_ACTION_PATH="$root/.github/actions/affected" CWD="$repo" CONFIG=settings/custom.json
export BASE="$custom_head" HEAD="$npm_head" EVENT_HEAD="$npm_head" GITHUB_SHA="$npm_head" GITHUB_JOB=affected RUN_ID=8675311 SCHEDULER=artifact
export GITHUB_OUTPUT="$tmp/npm-affected"
bash "$GITHUB_ACTION_PATH/run.sh" > "$tmp/npm-affected.log"
shrinkwrap_digest=$(sha256sum < "$repo/npm-shrinkwrap.json" | awk '{print $1}')
jq -e --arg digest "$shrinkwrap_digest" '.packageManager == "npm" and .packageManagerVersion == "11.16.0" and .lockfileDigest == $digest' "$RUNNER_TEMP/nanoom-plan-$RUN_ID-$RUN_ATTEMPT-$GITHUB_JOB/preparation-context.json" >/dev/null
echo 'Plan producer, rerun selection, sparse checkout, focused install, and run contracts passed'
