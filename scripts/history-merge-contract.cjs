#!/usr/bin/env node
const assert = require("node:assert/strict");
const fs = require("node:fs");
const crypto = require("node:crypto");
const path = require("node:path");

async function main() {
  const [mode, server, token, repository, scope, batchPath, wrongScopePath] = process.argv.slice(2);
  if (mode === "responses") {
    const files = fs
      .readdirSync(path.join(server, "batches"))
      .filter((file) => file.endsWith(".json"));
    assert.ok(files.length > 0, "History Action emitted no batches");
    for (const file of files) {
      const batch = JSON.parse(fs.readFileSync(path.join(server, "batches", file)));
      const result = JSON.parse(fs.readFileSync(path.join(server, "merge-" + file)));
      const count =
        batch.aggregates.length +
        (batch.environmentBatches || []).reduce(
          (sum, child) => sum + child.batch.aggregates.length,
          0,
        );
      assert.equal(
        result.receivedAggregateCount,
        count,
        "merge response omitted environment aggregates",
      );
      assert.equal(result.modelVersion, 3);
      assert.equal(result.applied, true);
    }
    console.log(`History Action merge response counts passed: ${files.length} actual batches`);
    return;
  }
  const endpoint = `${server}/v1/repositories/${repository}/scopes/${scope}`;
  const bytes = fs.readFileSync(batchPath);
  const batch = JSON.parse(bytes);
  const digest = (data) => crypto.createHash("sha256").update(data).digest("hex");
  async function post(data, expected, key = digest(data), code) {
    const headers = { authorization: `Bearer ${token}`, "content-type": "application/json" };
    if (key !== null) headers["idempotency-key"] = key;
    const response = await fetch(endpoint + "/observations:merge", {
      method: "POST",
      headers,
      body: data,
      signal: AbortSignal.timeout(10000),
    });
    const body = await response.json();
    assert.equal(response.status, expected, JSON.stringify(body));
    if (code) assert.equal(body.code, code);
    return body;
  }
  async function snapshot() {
    const response = await fetch(endpoint + "/snapshot", {
      headers: { authorization: `Bearer ${token}` },
      signal: AbortSignal.timeout(10000),
    });
    return {
      status: response.status,
      body: await response.text(),
      etag: response.headers.get("etag"),
    };
  }
  if (mode === "empty") {
    await post(fs.readFileSync(wrongScopePath), 403, undefined, "scope_mismatch");
    await post(bytes, 400, null, "idempotency_key_mismatch");
    await post(bytes, 409, "0".repeat(64), "idempotency_key_mismatch");
    for (const producedAtMs of [Date.now() + 600_000, Date.now() - 604_800_001]) {
      const invalid = { ...batch, producedAtMs };
      await post(Buffer.from(JSON.stringify(invalid)), 422, undefined, "batch_expired");
    }
    assert.equal((await snapshot()).status, 404, "invalid requests created history");
  } else {
    assert.equal(mode, "retry");
    const applied = await post(bytes, 200);
    assert.equal(applied.applied, true);
    assert.equal(applied.outcome, "updated");
    assert.equal(applied.receivedAggregateCount, batch.aggregates.length);
    const before = await snapshot();
    assert.equal(before.status, 200);
    const repeated = await post(bytes, 200);
    assert.equal(repeated.applied, false);
    assert.equal(repeated.outcome, "unchanged");
    fs.writeFileSync(batchPath + ".response.json", JSON.stringify(applied));
    assert.deepEqual(await snapshot(), before, "duplicate retry changed history");
    batch.aggregates[0].totalDurationMs += 1;
    await post(Buffer.from(JSON.stringify(batch)), 409, undefined, "batch_conflict");
    assert.deepEqual(await snapshot(), before, "conflicting batch changed history");
    for (const condition of [before.etag, `W/${before.etag}`, "*"]) {
      const response = await fetch(endpoint + "/snapshot", {
        headers: { authorization: `Bearer ${token}`, "if-none-match": condition },
      });
      assert.equal(response.status, 304);
      assert.equal(await response.text(), "");
      assert.equal(response.headers.get("etag"), before.etag);
    }
  }
  console.log(`History merge contracts passed: ${mode}`);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
