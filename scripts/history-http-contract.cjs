#!/usr/bin/env node
const assert = require("node:assert/strict");

async function main() {
  const [server, token, repository, scope] = process.argv.slice(2);
  assert.ok(
    server && token && repository && scope,
    "server, token, repository and scope are required",
  );
  const base = `/v1/repositories/${repository}/scopes/${scope}`;
  const headers = { authorization: `Bearer ${token}` };
  const otherScope = (scope[0] === "a" ? "b" : "a") + scope.slice(1);
  const cases = [
    ["unknown-route", "/missing", {}, 404, "not_found"],
    ["missing-credential", `${base}/snapshot`, {}, 401, "unauthenticated"],
    [
      "wrong-credential",
      `${base}/snapshot`,
      { headers: { authorization: `Bearer ${"x".repeat(40)}` } },
      401,
      "unauthenticated",
    ],
    [
      "wrong-scheme",
      `${base}/snapshot`,
      { headers: { authorization: `Basic ${token}` } },
      401,
      "unauthenticated",
    ],
    [
      "forbidden-scope",
      `/v1/repositories/${repository}/scopes/${otherScope}/snapshot`,
      { headers },
      403,
      "forbidden",
    ],
    ["snapshot-method", `${base}/snapshot`, { method: "POST", headers }, 405, "method_not_allowed"],
    ["merge-method", `${base}/observations:merge`, { headers }, 405, "method_not_allowed"],
    [
      "missing-media",
      `${base}/observations:merge`,
      { method: "POST", headers },
      415,
      "unsupported_media_type",
    ],
    [
      "wrong-media",
      `${base}/observations:merge`,
      { method: "POST", headers: { ...headers, "content-type": "text/plain" }, body: "{}" },
      415,
      "unsupported_media_type",
    ],
    [
      "wrong-charset",
      `${base}/observations:merge`,
      {
        method: "POST",
        headers: { ...headers, "content-type": "application/json; charset=iso-8859-1" },
        body: "{}",
      },
      415,
      "unsupported_media_type",
    ],
    [
      "compressed-media",
      `${base}/observations:merge`,
      {
        method: "POST",
        headers: { ...headers, "content-type": "application/json", "content-encoding": "gzip" },
        body: "{}",
      },
      415,
      "unsupported_media_type",
    ],
    [
      "invalid-json",
      `${base}/observations:merge`,
      {
        method: "POST",
        headers: { ...headers, "content-type": "application/json" },
        body: "{invalid}",
      },
      400,
      "invalid_request",
    ],
    [
      "duplicate-json",
      `${base}/observations:merge`,
      {
        method: "POST",
        headers: { ...headers, "content-type": "application/json" },
        body: '{"a":1,"a":2}',
      },
      400,
      "invalid_request",
    ],
    [
      "invalid-batch-schema",
      `${base}/observations:merge`,
      {
        method: "POST",
        headers: {
          ...headers,
          "content-type": 'Application/JSON; charset="utf-8"',
          "content-encoding": "identity",
        },
        body: "{}",
      },
      422,
      "invalid_history",
    ],
  ];
  for (const [id, route, options, status, code] of cases) {
    const response = await fetch(server + route, { ...options, signal: AbortSignal.timeout(5000) });
    const body = await response.json();
    assert.equal(response.status, status, `${id}: ${JSON.stringify(body)}`);
    assert.equal(body.status, status, id);
    assert.equal(body.code, code, id);
    assert.equal(response.headers.get("content-type"), "application/problem+json", id);
    assert.equal(response.headers.get("cache-control"), "no-store", id);
    assert.equal(response.headers.get("retry-after"), null, id);
    assert.equal(response.headers.get("x-request-id"), body.requestId, id);
    assert.ok(body.requestId && body.detail && body.title, id);
    if (status === 401) assert.equal(response.headers.get("www-authenticate"), "Bearer", id);
    if (status === 405)
      assert.equal(response.headers.get("allow"), route.endsWith("snapshot") ? "GET" : "POST", id);
  }
  const health = await fetch(server + "/health", {
    headers: { "cf-ray": "mutation-health-check" },
  });
  assert.equal(health.status, 200);
  assert.equal(health.headers.get("retry-after"), null);
  assert.deepEqual(await health.json(), { status: "ok" });
  assert.equal(health.headers.get("cache-control"), "no-store");
  assert.equal(health.headers.get("x-request-id"), "mutation-health-check");
  for (const ray of ["", "x".repeat(129)]) {
    const response = await fetch(server + "/health", { headers: { "cf-ray": ray } });
    assert.equal(response.status, 200);
    assert.match(response.headers.get("x-request-id"), /^local-\d+$/);
  }
  const limit = 16 * 1024 * 1024;
  for (const [size, status, code] of [
    [limit, 422, "invalid_history"],
    [limit + 1, 413, "payload_too_large"],
  ]) {
    const response = await fetch(server + base + "/observations:merge", {
      method: "POST",
      headers: { ...headers, "content-type": "application/json" },
      body: '"' + "x".repeat(size - 2) + '"',
      signal: AbortSignal.timeout(10000),
    });
    assert.equal(response.status, status, `body size ${size}`);
    assert.equal((await response.json()).code, code);
  }
  const snapshot = await fetch(server + base + "/snapshot", { headers });
  assert.equal(snapshot.status, 404, "invalid requests must not create history");
  assert.equal((await snapshot.json()).code, "history_not_found");
  console.log(
    `History HTTP contracts passed: ${cases.length} invalid requests, public health and no storage side effects`,
  );
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
