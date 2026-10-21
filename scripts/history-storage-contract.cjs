#!/usr/bin/env node
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const assert = require("node:assert/strict");

async function main() {
  const [mode, directory, server, token, repository, scope] = process.argv.slice(2);
  if (mode === "prepare") {
    const rows = JSON.parse(fs.readFileSync(path.join(directory, "rows.json"))).flatMap(
      (page) => page.results,
    );
    const row = rows.find((row) => row.storage_key.includes(scope));
    assert.ok(row, "the authorized scope was not stored in D1");
    const state = JSON.parse(row.model_json);
    const response = JSON.parse(fs.readFileSync(path.join(directory, "response.json")));
    const entries = state.entries.concat(
      ...(state.environmentStates || []).map((child) => child.state.entries),
    );
    assert.equal(response.retainedKeyCount, entries.length);
    assert.equal(response.receiptCount, state.receipts.length);
    const quote = (value) => "'" + value.replaceAll("'", "''") + "'";
    const write = (name, json, digest) =>
      fs.writeFileSync(
        path.join(directory, name + ".sql"),
        `UPDATE prediction_state SET model_json=${quote(json)}, model_sha256=${quote(digest)} WHERE storage_key=${quote(row.storage_key)};`,
      );
    const sha = (json) => crypto.createHash("sha256").update(json).digest("hex");
    write("restore", row.model_json, row.model_sha256);
    const padding = 1_900_000 - Buffer.byteLength(row.model_json);
    assert.ok(padding > 0);
    // Workerd limits printf's width to 100,000; concatenate bounded chunks.
    const chunks = [];
    for (let remaining = padding; remaining > 0; remaining -= 100_000)
      chunks.push(`printf('%*s', ${Math.min(remaining, 100_000)}, '')`);
    fs.writeFileSync(
      path.join(directory, "boundary.sql"),
      `UPDATE prediction_state SET model_json=model_json || ${chunks.join(" || ")}, model_sha256=${quote(sha(row.model_json + " ".repeat(padding)))} WHERE storage_key=${quote(row.storage_key)};`,
    );
    write("checksum", row.model_json, "0".repeat(64));
    const schema = JSON.stringify({ ...state, unexpected: true });
    write("schema", schema, sha(schema));
    function changeScope(state) {
      state.scope.ref.ref = "refs/heads/foreign-storage";
      for (const child of state.environmentStates || []) changeScope(child.state);
    }
    changeScope(state);
    const foreign = JSON.stringify(state);
    write("scope", foreign, sha(foreign));
  } else if (mode === "boundary") {
    const rows = JSON.parse(fs.readFileSync(path.join(directory, "boundary-size.json"))).flatMap(
      (page) => page.results,
    );
    assert.equal(rows.find((row) => row.storage_key.includes(scope)).bytes, 1_900_000);
    console.log("D1 row boundary passed: exactly 1,900,000 bytes of valid model JSON");
  } else {
    assert.equal(mode, "reject");
    const response = await fetch(
      `${server}/v1/repositories/${repository}/scopes/${scope}/snapshot`,
      {
        headers: { authorization: `Bearer ${token}` },
        signal: AbortSignal.timeout(10000),
      },
    );
    const body = await response.json();
    assert.equal(response.status, 503, JSON.stringify(body));
    assert.equal(body.code, "storage_corrupt");
    assert.equal(response.headers.get("retry-after"), "1");
    assert.equal(response.headers.get("cache-control"), "no-store");
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
