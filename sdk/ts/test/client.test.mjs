//! Client behaviour: URLs, credentials, problem mapping, refresh and
//! permission checks. Every exchange runs against a recorded transport.

import assert from "node:assert/strict";
import { test } from "node:test";

import { TidepoolClient, createClient, deliveryKey } from "../src/index.js";
import { ProblemError, retryAfterMs } from "../src/problem.js";
import { loadFixture, RecordedServer } from "./support/recorded-server.mjs";

const BASE = "https://pool.example.com";

/** A client over one recorded fixture. */
function clientFor(fixtureName, options = {}) {
  const server = new RecordedServer(loadFixture(fixtureName).steps);
  const client = new TidepoolClient({
    baseUrl: BASE,
    token: "operator-jwt",
    fetch: server.fetch,
    sleep: async () => {},
    ...options,
  });
  return { client, server };
}

test("auth headers follow the ambient credential precedence", async () => {
  const server = new RecordedServer([
    {
      request: { method: "GET", path: "/v1/pool/me" },
      response: { status: 200, body: { user: {}, organization: {} } },
    },
  ]);
  const client = new TidepoolClient({
    baseUrl: BASE,
    token: "jwt-1",
    apiKey: "tp_live_abc.secret",
    hostCredential: "host-secret",
    fetch: server.fetch,
  });
  await client.pool.me();
  const call = server.calls[0];
  assert.equal(call.headers.authorization, "Bearer jwt-1");
  assert.equal(call.headers["x-api-key"], "tp_live_abc.secret");
  assert.equal(call.headers["x-host-credential"], "host-secret");
  assert.equal(call.headers.accept, "application/json");
});

test("path parameters are encoded and query values are typed and dropped when empty", () => {
  const client = createClient({ baseUrl: `${BASE}/`, fetch: async () => new Response(null, { status: 204 }) });
  const url = client.urlFor("get_v1_projects_id_revisions_revision_id", {
    id: "proj one",
    revision_id: "rev/2",
  });
  assert.equal(url, `${BASE}/v1/projects/proj%20one/revisions/rev%2F2`);

  const paged = client.urlFor("get_v1_chatrooms_room_messages", { room: "room_01" }, { after: 7, limit: undefined });
  assert.equal(paged, `${BASE}/v1/chatrooms/room_01/messages?after=7`);

  assert.throws(
    () => client.urlFor("get_v1_hosts_id", {}),
    /missing path parameter "id"/,
  );
});

test("a 204 response resolves to undefined", async () => {
  const server = new RecordedServer([
    { request: { method: "DELETE", path: "/v1/hosts/host_01" }, response: { status: 204 } },
  ]);
  const client = new TidepoolClient({ baseUrl: BASE, token: "t", fetch: server.fetch });
  assert.equal(await client.hosts.remove("host_01"), undefined);
});

test("a problem-details body maps to a typed error with requestId", async () => {
  const { client, server } = clientFor("problem-details");
  const error = await client.projects.get("proj_missing").catch((caught) => caught);
  assert.ok(error instanceof ProblemError);
  assert.equal(error.name, "ProblemError");
  assert.equal(error.status, 404);
  assert.equal(error.code, "not_found");
  assert.equal(error.detail, "no such project in this organization");
  assert.equal(error.requestId, "req_body_01");
  assert.equal(error.operationId, "get_v1_projects_id");
  assert.equal(error.attempts, 1);
  assert.equal(error.rateLimited, false);
  assert.match(error.message, /GET \/v1\/projects\/\{id\} -> 404 not_found/);
  assert.match(error.message, /requestId req_body_01/);

  const scope = await client.apiKeys.whoami().catch((caught) => caught);
  assert.equal(scope.code, "missing_scope");
  assert.equal(scope.requestId, "req_header_02", "requestId falls back to x-request-id");
  assert.equal(server.unmatched.length, 1);
});

test("a game-surface error without requestId still yields a typed error", async () => {
  // The fixture's third exchange is the game error (no `requestId` in body).
  const server = new RecordedServer(loadFixture("problem-details").steps.slice(2));
  const client = new TidepoolClient({ baseUrl: BASE, token: "t", fetch: server.fetch });
  const error = await client.game.players.get("player_01").catch((caught) => caught);
  assert.ok(error instanceof ProblemError);
  assert.equal(error.code, "unauthorized");
  assert.equal(error.detail, "invalid token");
  assert.equal(error.requestId, "req_header_03");
  assert.equal(error.title, null);
});

test("an unknown problem code is preserved as an opaque string", async () => {
  const server = new RecordedServer([
    {
      request: { method: "GET", path: "/v1/projects" },
      response: {
        status: 418,
        headers: { "content-type": "application/problem+json" },
        body: { type: "about:blank", title: "Teapot", status: 418, code: "brand_new_code", requestId: "r1" },
      },
    },
  ]);
  const client = new TidepoolClient({ baseUrl: BASE, fetch: server.fetch });
  const error = await client.projects.list().catch((caught) => caught);
  assert.equal(error.code, "brand_new_code");
  assert.deepEqual(
    { status: error.status, code: error.code, requestId: error.requestId },
    { status: 418, code: "brand_new_code", requestId: "r1" },
  );
  assert.equal(JSON.parse(JSON.stringify(error)).code, "brand_new_code");
});

test("a caller-supplied requestId is sent as x-request-id", async () => {
  const server = new RecordedServer([
    { request: { method: "GET", path: "/v1/hosts" }, response: { status: 200, body: { hosts: [] } } },
  ]);
  const client = new TidepoolClient({ baseUrl: BASE, fetch: server.fetch });
  await client.hosts.list({ requestId: "req-local-01" });
  assert.equal(server.calls[0].headers["x-request-id"], "req-local-01");
});

test("request() dispatches by operation id with a single object", async () => {
  const server = new RecordedServer([
    {
      request: { method: "POST", path: "/v1/hosts/enroll" },
      response: { status: 201, body: { id: "host_01", slug: "h", display_name: "H", hostname: "h", status: "connected", execution_enabled: true, created_at: "2026-10-29T00:00:00Z" } },
    },
  ]);
  const client = new TidepoolClient({ baseUrl: BASE, fetch: server.fetch });
  const host = await client.request("post_v1_hosts_enroll", {
    body: { token: "t", credential: "c" },
    requestId: "req-2",
  });
  assert.equal(host.id, "host_01");
  assert.deepEqual(server.calls[0].body, { token: "t", credential: "c" });
  assert.equal(server.calls[0].headers["x-request-id"], "req-2");
});

test("describe exposes the access model the contract declares", () => {
  const client = createClient({ baseUrl: BASE, fetch: async () => new Response(null, { status: 204 }) });
  const route = client.describe("post_v1_runs");
  assert.equal(route.auth, "member_or_api_key");
  assert.equal(route.scope, "runs:dispatch");
  assert.equal(route.role, null);
  assert.equal(client.describe("get_v1_api_keys").role, "owner");
  assert.equal(client.describe("post_v1_hosts_enroll").auth, "public");
});

test("allows answers the contract's auth matrix without a request", () => {
  const client = createClient({ baseUrl: BASE, fetch: async () => new Response(null, { status: 204 }) });
  // Public route, no principal.
  assert.equal(client.allows("post_v1_hosts_enroll", {}), true);
  // Owner-only reads.
  assert.equal(client.allows("get_v1_api_keys", { role: "owner" }), true);
  assert.equal(client.allows("get_v1_api_keys", { role: "admin" }), false);
  // Reads for any role; writes refused for a viewer.
  assert.equal(client.allows("get_v1_projects", { role: "viewer" }), true);
  assert.equal(client.allows("post_v1_projects", { role: "viewer" }), false);
  assert.equal(client.allows("post_v1_projects", { role: "member" }), true);
  // API key with the matching scope; without it, no.
  assert.equal(client.allows("post_v1_runs", { scopes: ["runs:dispatch"] }), true);
  assert.equal(client.allows("post_v1_runs", { scopes: ["projects:read"] }), false);
  assert.equal(client.allows("get_v1_api_keys_whoami", { scopes: ["projects:read"] }), true);
});

test("login adopts the token and its reported lifetime", async () => {
  const server = new RecordedServer([
    {
      request: { method: "POST", path: "/v1/pool/login" },
      response: {
        status: 200,
        body: {
          token: "fresh-jwt",
          expires_in: 900,
          must_change_password: false,
          user: {
            id: "operator_01",
            email: "owner@example.com",
            display_name: "Owner",
            role: "owner",
            password_change_required: false,
            created_at: "2026-10-01T00:00:00Z",
          },
        },
      },
    },
  ]);
  let now = 1_000_000;
  const client = new TidepoolClient({ baseUrl: BASE, fetch: server.fetch, now: () => now });
  const login = await client.login({ email: "owner@example.com", password: "correct-horse-battery" });
  assert.equal(login.token, "fresh-jwt");
  assert.equal(client.token, "fresh-jwt");
  assert.equal(client.tokenExpiresAt, now + 900_000);
  assert.equal(client.tokenExpired, false);
  now += 900_000;
  assert.equal(client.tokenExpired, true);
  assert.equal(client.setToken(null).token, null);
});

test("an expired token is refreshed once before the call", async () => {
  const server = new RecordedServer([
    { request: { method: "GET", path: "/v1/pool/me" }, response: { status: 200, body: { user: {}, organization: {} } } },
  ]);
  let refreshCalls = 0;
  let now = 0;
  const client = new TidepoolClient({
    baseUrl: BASE,
    token: "old-jwt",
    refreshToken: async () => {
      refreshCalls += 1;
      return { token: "new-jwt", expiresIn: 60 };
    },
    fetch: server.fetch,
    now: () => now,
  });
  client.setToken("old-jwt", { expiresIn: 1 });
  now += 5_000;
  await client.pool.me();
  assert.equal(refreshCalls, 1);
  assert.equal(server.calls[0].headers.authorization, "Bearer new-jwt");
  assert.equal(client.tokenExpiresAt, now + 60_000);
});

test("retry-after parsing accepts seconds and HTTP dates", () => {
  assert.equal(retryAfterMs("5"), 5000);
  assert.equal(retryAfterMs("0"), 0);
  assert.equal(retryAfterMs(null), null);
  assert.equal(retryAfterMs("soon"), null);
  const now = Date.parse("2026-10-29T05:00:00Z");
  assert.equal(retryAfterMs("Thu, 29 Oct 2026 05:00:03 GMT", () => now), 3000);
  assert.equal(retryAfterMs("Thu, 29 Oct 2026 04:59:00 GMT", () => now), 0);
});

test("deliveryKey returns distinct opaque ids", () => {
  const first = deliveryKey();
  const second = deliveryKey();
  assert.equal(typeof first, "string");
  assert.ok(first.length >= 8);
  assert.notEqual(first, second);
});
