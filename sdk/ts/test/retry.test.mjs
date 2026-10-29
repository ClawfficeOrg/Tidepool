//! Retry and token-refresh behaviour.
//!
//! A 429 is retried with backoff that honours `Retry-After`; the retry count is
//! bounded by `maxRetries`, so no request is retried indefinitely.

import assert from "node:assert/strict";
import { test } from "node:test";

import { TidepoolClient } from "../src/index.js";
import { ProblemError } from "../src/problem.js";
import { RecordedServer } from "./support/recorded-server.mjs";

const BASE = "https://pool.example.com";

/** Problem body for a rate limit. */
function rateLimited(requestId = "req-429") {
  return {
    type: "https://tidepool.dev/problems/rate_limited",
    title: "Too many requests",
    status: 429,
    code: "rate_limited",
    detail: "rate limit exceeded",
    requestId,
  };
}

/** A client over a list of steps, capturing sleeps instead of performing them. */
function harness(steps, options = {}) {
  const server = new RecordedServer(steps);
  const sleeps = [];
  const client = new TidepoolClient({
    baseUrl: BASE,
    token: "jwt",
    fetch: server.fetch,
    sleep: async (ms) => {
      sleeps.push(ms);
    },
    ...options,
  });
  return { client, server, sleeps };
}

test("a 429 with Retry-After is retried after exactly that delay", async () => {
  const { client, server, sleeps } = harness([
    {
      request: { method: "GET", path: "/v1/hosts" },
      response: { status: 429, headers: { "content-type": "application/problem+json", "retry-after": "1" }, body: rateLimited() },
    },
    { request: { method: "GET", path: "/v1/hosts" }, response: { status: 200, body: { hosts: [] } } },
  ]);

  const page = await client.hosts.list();
  assert.deepEqual(page, { hosts: [] });
  assert.deepEqual(sleeps, [1000]);
  assert.equal(server.calls.length, 2);
});

test("exponential backoff doubles between attempts when no Retry-After is sent", async () => {
  const { client, sleeps, server } = harness([
    {
      request: { method: "GET", path: "/v1/hosts" },
      response: { status: 429, headers: { "content-type": "application/problem+json" }, body: rateLimited() },
      times: 2,
    },
    { request: { method: "GET", path: "/v1/hosts" }, response: { status: 200, body: { hosts: [] } } },
  ]);

  await client.hosts.list();
  assert.deepEqual(sleeps, [250, 500]);
  assert.equal(server.calls.length, 3);
});

test("retries are bounded and the final 429 surfaces with the attempt count", async () => {
  const { client, sleeps, server } = harness([
    {
      request: { method: "GET", path: "/v1/hosts" },
      response: { status: 429, headers: { "content-type": "application/problem+json", "retry-after": "0" }, body: rateLimited("req-final") },
      times: 10,
    },
  ]);

  const error = await client.hosts.list().catch((caught) => caught);
  assert.ok(error instanceof ProblemError);
  assert.equal(error.status, 429);
  assert.equal(error.code, "rate_limited");
  assert.equal(error.rateLimited, true);
  assert.equal(error.attempts, 3, "one initial attempt plus maxRetries=2");
  assert.equal(error.requestId, "req-final");
  assert.deepEqual(sleeps, [0, 0], "backoff uses Retry-After: 0 both times");
  assert.equal(server.calls.length, 3, "no fourth request");

  const bounded = await new TidepoolClient({
    baseUrl: BASE,
    fetch: new RecordedServer([
      {
        request: { method: "GET", path: "/v1/hosts" },
        response: { status: 429, headers: { "content-type": "application/problem+json" }, body: rateLimited() },
        times: 20,
      },
    ]).fetch,
    sleep: async () => {},
    retry: { maxRetries: 0 },
  })
    .hosts.list()
    .catch((caught) => caught);
  assert.equal(bounded.attempts, 1);
});

test("a Retry-After longer than the policy ceiling is surfaced, not slept through", async () => {
  const { client, sleeps, server } = harness([
    {
      request: { method: "GET", path: "/v1/hosts" },
      response: { status: 429, headers: { "content-type": "application/problem+json", "retry-after": "3600" }, body: rateLimited() },
      times: 5,
    },
  ]);

  const error = await client.hosts.list().catch((caught) => caught);
  assert.ok(error instanceof ProblemError);
  assert.equal(error.attempts, 1);
  assert.equal(error.retryAfterMs, 3_600_000);
  assert.deepEqual(sleeps, []);
  assert.equal(server.calls.length, 1);
});

test("an HTTP-date Retry-After is honoured", async () => {
  const now = Date.parse("2026-10-29T05:00:00Z");
  const { client, sleeps } = harness(
    [
      {
        request: { method: "GET", path: "/v1/hosts" },
        response: {
          status: 429,
          headers: { "content-type": "application/problem+json", "retry-after": "Thu, 29 Oct 2026 05:00:02 GMT" },
          body: rateLimited(),
        },
      },
      { request: { method: "GET", path: "/v1/hosts" }, response: { status: 200, body: { hosts: [] } } },
    ],
    { now: () => now },
  );

  await client.hosts.list();
  assert.deepEqual(sleeps, [2000]);
});

test("retry can be disabled per call and per client", async () => {
  const steps = [
    {
      request: { method: "GET", path: "/v1/hosts" },
      response: { status: 429, headers: { "content-type": "application/problem+json" }, body: rateLimited() },
      times: 4,
    },
  ];
  const first = harness(steps).client;
  const perCall = await first.hosts.list({ retry: false }).catch((caught) => caught);
  assert.equal(perCall.attempts, 1);

  const second = harness(steps).client;
  const configured = await second.hosts.list({ retry: { maxRetries: 1 } }).catch((caught) => caught);
  assert.equal(configured.attempts, 2);
});

test("a 401 refreshes the token once and replays the request", async () => {
  const { client, server } = harness(
    [
      {
        request: { method: "GET", path: "/v1/pool/me" },
        response: { status: 401, headers: { "content-type": "application/problem+json" }, body: { type: "about:blank", title: "Unauthorized", status: 401, code: "unauthorized", requestId: "r1" } },
      },
      { request: { method: "GET", path: "/v1/pool/me" }, response: { status: 200, body: { user: { id: "operator_01" }, organization: { id: "org_01" } } } },
    ],
    {
      refreshToken: async () => "refreshed-jwt",
    },
  );

  const me = await client.pool.me();
  assert.equal(me.user.id, "operator_01");
  assert.equal(server.calls.length, 2);
  assert.equal(server.calls[0].headers.authorization, "Bearer jwt");
  assert.equal(server.calls[1].headers.authorization, "Bearer refreshed-jwt");
});

test("a refresh that keeps failing is not retried again", async () => {
  const { client, server, sleeps } = harness(
    [
      {
        request: { method: "GET", path: "/v1/pool/me" },
        response: { status: 401, headers: { "content-type": "application/problem+json" }, body: { type: "about:blank", title: "Unauthorized", status: 401, code: "unauthorized", requestId: "r1" } },
        times: 5,
      },
    ],
    {
      refreshToken: async () => "still-bad-jwt",
    },
  );

  const error = await client.pool.me().catch((caught) => caught);
  assert.ok(error instanceof ProblemError);
  assert.equal(error.status, 401);
  assert.equal(server.calls.length, 2, "one replay after one refresh, then stop");
  assert.deepEqual(sleeps, []);
});

test("a refresh hook that returns null leaves the 401 to surface", async () => {
  const { client, server } = harness(
    [
      {
        request: { method: "GET", path: "/v1/pool/me" },
        response: { status: 401, headers: { "content-type": "application/problem+json" }, body: { type: "about:blank", title: "Unauthorized", status: 401, code: "unauthorized", requestId: "r2" } },
        times: 3,
      },
    ],
    { refreshToken: async () => null },
  );

  const error = await client.pool.me().catch((caught) => caught);
  assert.equal(error.status, 401);
  assert.equal(error.requestId, "r2");
  assert.equal(server.calls.length, 1);
});

test("429 retries keep the request body and method intact", async () => {
  const { client, server } = harness([
    {
      request: { method: "POST", path: "/v1/runs" },
      response: { status: 429, headers: { "content-type": "application/problem+json", "retry-after": "0" }, body: rateLimited() },
    },
    {
      request: { method: "POST", path: "/v1/runs" },
      response: { status: 200, body: { deduplicated: true, run: { id: "run_01" } } },
    },
  ]);

  const body = { project: "proj_01", trigger: "nightly", actor: "operator_01", deliveryKey: "key-1" };
  const response = await client.runs.dispatch(body);
  assert.equal(response.deduplicated, true);
  assert.equal(server.calls.length, 2);
  assert.deepEqual(server.calls[1].body, body);
  assert.equal(server.calls[1].method, "POST");
});
