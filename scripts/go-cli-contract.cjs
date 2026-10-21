#!/usr/bin/env node
const assert = require("node:assert/strict"),
  fs = require("node:fs"),
  path = require("node:path"),
  { spawnSync } = require("node:child_process");
const [rust, go, tmp, root] = process.argv.slice(2);
function invoke(binary, args) {
  const r = spawnSync(binary, args, { encoding: "utf8", cwd: tmp });
  assert.equal(r.error, undefined);
  return { code: r.status, stdout: r.stdout, stderr: r.stderr };
}
function compare(args) {
  const before = invoke(rust, args),
    after = invoke(go, args);
  assert.equal(after.code, before.code, `exit: ${args}: ${after.stderr}`);
  assert.deepEqual(JSON.parse(after.stdout), JSON.parse(before.stdout), `JSON: ${args}`);
  return after;
}
compare(["version", "--json"]);
for (const status of ["success", "skipped", "failure", "cancelled"])
  compare(["status", "build,test", "--results", `build=success,test=${status}`, "--json"]);
for (const name of [
  "nanoom.config.json",
  "package.json",
  "pnpm-workspace.yaml",
  ".npmrc",
  ".yarnrc.yml",
  "pnpm-lock.yaml",
  "yarn.lock",
  "package-lock.json",
  "npm-shrinkwrap.json",
]) {
  const selected = path.join(tmp, name);
  fs.writeFileSync(selected, "before\n");
  compare([
    "cache-key",
    "--cwd",
    tmp,
    "--runner",
    "npm",
    "--task",
    "test",
    "--filter",
    "@fixture/web",
    "--json",
  ]);
  fs.writeFileSync(selected, "after\n");
  compare(["cache-key", "--cwd", tmp, "--runner", "turbo", "--task", "build", "--json"]);
  fs.unlinkSync(selected);
}
const corpus = JSON.parse(
  fs.readFileSync(path.join(root, "internal/prediction/testdata/rust-v3.json")),
);
for (const index of [0, 6]) {
  const scenario = corpus.scenarios[index];
  const args = [
    "history",
    "--model-artifact-name",
    "go-port-comparison",
    "--run-id",
    scenario.measurements[0].runId,
    "--run-attempt",
    "1",
    "--now-ms",
    String(scenario.now),
  ];
  scenario.measurements.forEach((m, i) => {
    const input = path.join(tmp, `input-${index}-${i}.json`);
    fs.writeFileSync(input, JSON.stringify(m));
    args.push("--input", input);
  });
  const paths = (binary) => [
    "--model-output",
    `${tmp}/${binary}-model.json`,
    "--prediction-output",
    `${tmp}/${binary}-prediction.json`,
    "--batch-output-dir",
    `${tmp}/${binary}-batches`,
  ];
  const before = invoke(rust, [...args, ...paths("rust")]),
    after = invoke(go, [...args, ...paths("go")]);
  assert.equal(before.code, 0, before.stderr);
  assert.equal(after.code, 0, after.stderr);
  assert.deepEqual(JSON.parse(after.stdout), JSON.parse(before.stdout));
  for (const type of ["model", "prediction"])
    assert.equal(
      fs.readFileSync(`${tmp}/go-${type}.json`, "utf8"),
      fs.readFileSync(`${tmp}/rust-${type}.json`, "utf8"),
      `${type} bytes differ`,
    );
  const scope = corpus.scopeId;
  assert.equal(
    fs.readFileSync(`${tmp}/go-batches/${scope}.json`, "utf8"),
    fs.readFileSync(`${tmp}/rust-batches/${scope}.json`, "utf8"),
  );
  for (const invalid of ["invalid-json", "invalid-run"]) {
    const file = `${tmp}/${invalid}.json`;
    const m = structuredClone(scenario.measurements[0]);
    m.runId = "999999";
    fs.writeFileSync(file, invalid === "invalid-json" ? "{}" : JSON.stringify(m));
    const r = invoke(rust, [...args, "--input", file, ...paths("rust")]),
      g = invoke(go, [...args, "--input", file, ...paths("go")]);
    assert.equal(g.code, r.code);
    assert.deepEqual(JSON.parse(g.stdout), JSON.parse(r.stdout));
  }
}

// Exercise Plan consumption across the language boundary with an actual Git checkout.
const repo = path.join(tmp, "repository");
fs.mkdirSync(repo);
function git(...args) {
  const r = spawnSync("git", ["-C", repo, ...args], { encoding: "utf8" });
  assert.equal(r.status, 0, r.stderr);
  return r.stdout.trim();
}
function write(name, value) {
  const file = path.join(repo, name);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, JSON.stringify(value));
}
write("package.json", {
  name: "root",
  private: true,
  packageManager: "npm@11.16.0",
  workspaces: ["packages/*"],
  devDependencies: { tools: "1.0.0" },
});
write("nanoom.config.json", { group: { ci: { tasks: ["test"] } } });
write("packages/app/package.json", {
  name: "app",
  version: "1.0.0",
  dependencies: { shared: "1.0.0" },
});
write("packages/shared/package.json", { name: "shared", version: "1.0.0" });
write("packages/tools/package.json", {
  name: "tools",
  version: "1.0.0",
  dependencies: { shared: "1.0.0" },
});
write("packages/unrelated/package.json", { name: "unrelated", version: "1.0.0" });
git("init");
git("config", "user.email", "migration@example.com");
git("config", "user.name", "Migration contract");
git("add", ".");
git("commit", "-qm", "base", "--no-gpg-sign");
const base = git("rev-parse", "HEAD");
fs.writeFileSync(path.join(repo, "packages/app/source.js"), "changed");
git("add", ".");
git("commit", "-qm", "changed", "--no-gpg-sign");
const head = git("rev-parse", "HEAD");
// Compare the planner itself, including diagnostics and the no-change path.
for (const selectedBase of [base, head]) {
  compare(["affected", "--cwd", repo, "--base", selectedBase, "--head", head, "--json"]);
}
write("context.json", {
  repository: "owner/repo",
  workflow: ".github/workflows/ci.yml",
  runId: "12345",
  producerAttempt: 1,
  planningJob: "affected",
  base,
  head,
  taskRunner: "npm",
});
const planned = invoke(rust, [
  "affected",
  "--cwd",
  repo,
  "--base",
  base,
  "--head",
  head,
  "--plan-context",
  path.join(repo, "context.json"),
  "--plan-output",
  path.join(repo, "plan.json"),
]);
assert.equal(planned.code, 0, planned.stderr);
const compact = JSON.parse(planned.stdout);
write("reference.json", compact.plan);
const goPlanned = invoke(go, [
  "affected",
  "--cwd",
  repo,
  "--base",
  base,
  "--head",
  head,
  "--plan-context",
  path.join(repo, "context.json"),
  "--plan-output",
  path.join(repo, "go-plan.json"),
]);
assert.equal(goPlanned.code, 0, goPlanned.stderr);
assert.deepEqual(
  JSON.parse(goPlanned.stdout),
  compact,
  "Go compact Plan and digest must match Rust",
);
assert.deepEqual(
  JSON.parse(fs.readFileSync(path.join(repo, "go-plan.json"))),
  JSON.parse(fs.readFileSync(path.join(repo, "plan.json"))),
  "authoritative Plan must match Rust",
);
const row = compact.groups.ci.include[0];
for (const [binary, label] of [
  [rust, "rust"],
  [go, "go"],
]) {
  const result = invoke(binary, [
    "plan",
    "select",
    "--cwd",
    repo,
    "--input",
    "plan.json",
    "--reference",
    "reference.json",
    "--group",
    "ci",
    "--assignment",
    row.assignmentId,
    "--output-dir",
    label,
  ]);
  assert.equal(result.code, 0, result.stderr);
}
assert.deepEqual(
  JSON.parse(fs.readFileSync(path.join(repo, "go/assignment.json"))),
  JSON.parse(fs.readFileSync(path.join(repo, "rust/assignment.json"))),
);
assert.equal(
  fs.readFileSync(path.join(repo, "go/paths.txt"), "utf8"),
  fs.readFileSync(path.join(repo, "rust/paths.txt"), "utf8"),
);
// npm ci itself clears previous installs; verify the Go path in a clean dependency tree.
const lock = spawnSync(
  "npm",
  ["install", "--package-lock-only", "--ignore-scripts", "--no-audit", "--no-fund"],
  { cwd: repo, encoding: "utf8" },
);
assert.equal(lock.status, 0, lock.stderr);
compare(["install", "--cwd", repo, "--package-manager", "npm", "--filter", "app", "--json"]);
for (const name of ["app", "shared", "tools"])
  assert.equal(
    JSON.parse(fs.readFileSync(path.join(repo, "node_modules", name, "package.json"))).name,
    name,
  );
assert.equal(fs.existsSync(path.join(repo, "node_modules/unrelated")), false);
const absent = invoke(go, [
  "install",
  "--cwd",
  repo,
  "--package-manager",
  "npm",
  "--filter",
  "absent",
  "--json",
]);
assert.equal(absent.code, 1);
assert.match(absent.stderr, /Workspace not found/);
console.log(
  "Go CLI matches Rust: version, all four status outcomes, nine cache inputs, history model/prediction/batch bytes, invalid measurement degradation, authoritative Plan selection, real npm focused install and unrelated-workspace exclusion",
);
