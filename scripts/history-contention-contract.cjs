#!/usr/bin/env node
const assert = require("node:assert/strict");
const fs = require("node:fs");
const crypto = require("node:crypto");
const path = require("node:path");

function entryCounts(state) {
  return Object.fromEntries(
    state.entries.map((entry) => [
      entry.keyId,
      entry.buckets.reduce((sum, bucket) => sum + bucket[1], 0),
    ]),
  );
}
function environmentHistory(states) {
  return states.map(({ environment, state }) => ({
    environment,
    scope: state.scope,
    entries: state.entries,
    receipts: state.receipts,
  }));
}

async function main() {
  if (process.argv[2] === "verify") {
    const directory = process.argv[3];
    const expected = JSON.parse(fs.readFileSync(path.join(directory, "contention-expected.json")));
    const rows = JSON.parse(fs.readFileSync(path.join(directory, "after-contention.json"))).flatMap(
      (page) => page.results,
    );
    const state = JSON.parse(
      rows.find((row) => row.storage_key === expected.storageKey).model_json,
    );
    assert.deepEqual(
      entryCounts(state),
      expected.counts,
      "concurrent merges lost observations or counted duplicate retries",
    );
    assert.equal(state.receipts.length, expected.receiptCount);
    assert.deepEqual(
      environmentHistory(state.environmentStates),
      expected.environmentStates,
      "legacy concurrency changed environment-specific history",
    );
    console.log(
      "D1 contention integrity passed: all observations retained and exactly twelve receipts added",
    );
    return;
  }
  const [server, token, repository, scope, batchPath, directory] = process.argv.slice(2);
  const endpoint = `${server}/v1/repositories/${repository}/scopes/${scope}`;
  const original = JSON.parse(fs.readFileSync(batchPath));
  const digest = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
  function canonical(value) {
    if (Array.isArray(value)) return value.map(canonical);
    if (value && typeof value === "object")
      return Object.fromEntries(
        Object.keys(value)
          .sort()
          .map((key) => [key, canonical(value[key])]),
      );
    return value;
  }
  function batch(attempt, aggregates = original.aggregates) {
    return {
      ...original,
      runAttempt: attempt,
      batchId: digest(JSON.stringify([scope, original.runId, attempt])),
      producedAtMs: Date.now(),
      aggregates,
    };
  }
  async function post(batch) {
    const bytes = JSON.stringify(canonical(batch));
    const response = await fetch(endpoint + "/observations:merge", {
      method: "POST",
      headers: {
        authorization: `Bearer ${token}`,
        "content-type": "application/json",
        "idempotency-key": digest(bytes),
      },
      body: bytes,
      signal: AbortSignal.timeout(30000),
    });
    return { status: response.status, body: await response.json() };
  }
  async function snapshot() {
    const response = await fetch(endpoint + "/snapshot", {
      headers: { authorization: `Bearer ${token}` },
      signal: AbortSignal.timeout(10000),
    });
    assert.equal(response.status, 200);
    return { body: await response.text(), etag: response.headers.get("etag") };
  }
  const before = await snapshot();
  const at = Date.now();
  const aggregates = Array.from({ length: 12000 }, (_, index) => ({
    keyId: digest(`capacity-workspace-${index}`),
    utcEpochDay: Math.floor(at / 86400000),
    observationCount: 1,
    totalDurationMs: 100,
    lastObservedAtMs: at,
  })).sort((a, b) => a.keyId.localeCompare(b.keyId));
  const capacity = await post(batch(1000, aggregates));
  assert.equal(capacity.status, 409, JSON.stringify(capacity));
  assert.equal(capacity.body.code, "model_capacity_exceeded");
  assert.deepEqual(await snapshot(), before, "rejected capacity update changed persisted history");

  const batches = Array.from({ length: 12 }, (_, index) => batch(100 + index));
  let retries = 0;
  async function merge(value) {
    for (let attempt = 0; attempt < 8; attempt++) {
      const result = await post(value);
      if (result.status === 200) return result.body;
      assert.equal(result.status, 503, JSON.stringify(result));
      assert.equal(result.body.code, "cas_retries_exhausted");
      retries++;
      await new Promise((resolve) => setTimeout(resolve, 25 * (attempt + 1)));
    }
    throw new Error("concurrent merge did not recover within eight client retries");
  }
  const results = await Promise.all(
    batches.map(async (value) => {
      const duplicate = await Promise.all([merge(value), merge(value)]);
      assert.equal(
        duplicate.filter((result) => result.applied).length,
        1,
        "concurrent retry applied the same batch twice",
      );
      return duplicate;
    }),
  );
  assert.equal(results.length, 12);
  const stored = JSON.parse(fs.readFileSync(path.join(directory, "rows.json")))
    .flatMap((page) => page.results)
    .find((row) => row.storage_key.includes(scope));
  const state = JSON.parse(stored.model_json);
  const counts = entryCounts(state);
  for (const aggregate of original.aggregates)
    counts[aggregate.keyId] = (counts[aggregate.keyId] || 0) + 12 * aggregate.observationCount;
  fs.writeFileSync(
    path.join(directory, "contention-expected.json"),
    JSON.stringify({
      storageKey: stored.storage_key,
      counts,
      receiptCount: state.receipts.length + 12,
      environmentStates: environmentHistory(state.environmentStates),
    }),
  );
  console.log(
    `History contention passed: oversized model rejected unchanged; 12 concurrent batches with duplicate retries applied exactly once (${retries} client retries)`,
  );
}
main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
