#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cd "$tmp"
cat > package.json <<'JSON'
{"name":"focused-root","private":true,"packageManager":"pnpm@12.8.2","devDependencies":{"tool":"workspace:*"}}
JSON
printf 'packages:\n  - packages/*\nlegacyDirFiltering: true\n' > pnpm-workspace.yaml
printf '%s\n' '{"group":{"ci":{"tasks":["test"]}}}' > nanoom.config.json
for name in app shared tool tool-shared unrelated; do
  mkdir -p "packages/$name"
  jq -cn --arg name "$name" '{name:$name,version:"1.0.0",scripts:{test:"node -e '\''require(\"fs\").writeFileSync(\"executed\",\"ok\")'\''"}}' > "packages/$name/package.json"
done
jq '.dependencies={shared:"workspace:*"}' packages/app/package.json > app.json
mv app.json packages/app/package.json
jq '.dependencies={"tool-shared":"workspace:*"}' packages/tool/package.json > tool.json
mv tool.json packages/tool/package.json
pnpm install --lockfile-only > "$tmp/lock.log" 2>&1
NODE_ENV=production "$root/target/debug/nanoom" install --filter app --json > "$tmp/install.json"
jq -e '.scope.devDependencies == true and .scope.dependencyClosure == true' "$tmp/install.json" >/dev/null
# Root tooling and both dependency closures are usable from the selected app.
node - <<'JS'
const assert = require('node:assert/strict');
const fs = require('node:fs');
assert.equal(require('./node_modules/tool/package.json').name, 'tool');
assert.equal(require('./packages/tool/node_modules/tool-shared/package.json').name, 'tool-shared');
assert.equal(require('./packages/app/node_modules/shared/package.json').name, 'shared');
assert.equal(fs.existsSync('./node_modules/unrelated'), false);
assert.equal(fs.existsSync('./packages/unrelated/node_modules'), false);
JS
"$root/target/debug/nanoom" run ci test --all --filter app --json > "$tmp/run.json"
test -f packages/app/executed
test ! -f packages/unrelated/executed
printf '%s\n' 'native pnpm focused install preserves development tooling and both internal closures'

# npm needs explicit workspace closure: include-workspace-root alone links the
# root tool but does not install that tool's dependencies.
rm -rf node_modules packages/*/node_modules
node - <<'JS'
const fs = require('node:fs');
const root = JSON.parse(fs.readFileSync('package.json'));
root.packageManager = 'npm@11.16.0';
root.workspaces = ['packages/*'];
root.devDependencies.tool = '1.0.0';
fs.writeFileSync('package.json', JSON.stringify(root));
for (const name of ['app','tool']) {
 const path = `packages/${name}/package.json`;
 const pkg = JSON.parse(fs.readFileSync(path));
 for (const dep in pkg.dependencies) pkg.dependencies[dep] = '1.0.0';
 fs.writeFileSync(path, JSON.stringify(pkg));
}
JS
npm install --package-lock-only --ignore-scripts --no-audit --no-fund > "$tmp/npm-lock.log" 2>&1
NODE_ENV=production "$root/target/debug/nanoom" install --filter app --json > "$tmp/npm-install.json"
jq -e '.packageManager == "npm" and .scope.devDependencies == true' "$tmp/npm-install.json" >/dev/null
node - <<'JS'
const assert = require('node:assert/strict');
const fs = require('node:fs');
for (const name of ['app','shared','tool','tool-shared']) {
 assert.equal(require(`./node_modules/${name}/package.json`).name, name);
}
assert.equal(fs.existsSync('./node_modules/unrelated'), false);
JS
if "$root/target/debug/nanoom" install --filter absent --json > "$tmp/unknown.json" 2>/dev/null; then exit 1; fi
jq -e '.status == "failure"' "$tmp/unknown.json" >/dev/null
rm package-lock.json
if "$root/target/debug/nanoom" install --filter app --json > "$tmp/no-lock.json" 2>/dev/null; then exit 1; fi
jq -e '.status == "failure"' "$tmp/no-lock.json" >/dev/null
printf '%s\n' 'native npm focused install includes both closures and rejects unknown workspaces or missing lockfiles'

rm -rf node_modules packages/*/node_modules
node - <<'JS'
const fs = require('node:fs');
const root = JSON.parse(fs.readFileSync('package.json'));
root.packageManager = 'yarn@4.11.0';
root.devDependencies.tool = 'workspace:*';
fs.writeFileSync('package.json', JSON.stringify(root));
for (const name of ['app','tool']) {
 const path = `packages/${name}/package.json`;
 const pkg = JSON.parse(fs.readFileSync(path));
 for (const dep in pkg.dependencies) pkg.dependencies[dep] = 'workspace:*';
 fs.writeFileSync(path, JSON.stringify(pkg));
}
JS
printf 'nodeLinker: node-modules\n' > .yarnrc.yml
yarn install > "$tmp/yarn-lock.log" 2>&1
cp yarn.lock "$tmp/original-yarn.lock"
rm -rf node_modules packages/*/node_modules .yarn/install-state.gz packages/unrelated
YARN_ENABLE_IMMUTABLE_INSTALLS=true NODE_ENV=production "$root/target/debug/nanoom" install --filter app --json > "$tmp/yarn-install.json"
jq -e '.packageManager == "yarn" and .scope.devDependencies == true' "$tmp/yarn-install.json" >/dev/null
cmp yarn.lock "$tmp/original-yarn.lock"
node - <<'JS'
const assert = require('node:assert/strict');
const fs = require('node:fs');
for (const name of ['app','shared','tool','tool-shared']) {
 assert.equal(require(`./node_modules/${name}/package.json`).name, name);
}
assert.equal(fs.existsSync('./node_modules/unrelated'), false);
JS
printf '%s\n' 'native Yarn Berry focused install includes root and selected workspace closures in production'
