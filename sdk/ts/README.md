# @clawffice/tidepool-sdk

Typed TypeScript client for the Tidepool **hub** (`/v1/hosts`, `/v1/projects`,
`/v1/runs`, `/v1/chatrooms`, the session relay, …) and the Talo-compatible
**game** surface.

One typed method per contract route, one typed error shape, a browser build with
no Node-only dependency, bounded 429 retries that honour `Retry-After`, token
refresh, and a live chatroom stream.

The client is generated from, and checked against,
[`docs/api/openapi-v1.json`](../../docs/api/openapi-v1.json) — the committed
release copy of the contract described in
[`docs/API_CONTRACT.md`](../../docs/API_CONTRACT.md).

## Install

```sh
npm install @clawffice/tidepool-sdk
```

Node 20+ or any browser that has `fetch`. The package has **no runtime
dependencies**.

## Use

```ts
import { createClient, deliveryKey } from "@clawffice/tidepool-sdk";

const pool = createClient({ baseUrl: "https://pool.example.com" });

// Operator sign-in; the token and its lifetime are adopted automatically.
await pool.login({ email: "owner@example.com", password: process.env.TIDEPOOL_PASSWORD! });

// Hosts and enrolment.
const host = await pool.hosts.create({ display_name: "Build box", hostname: "build-box.local" });
const { token } = await pool.hosts.mintEnrollmentToken({ host_id: host.id });
const enrolled = await pool.hosts.enroll({ token, credential: "host-generated-secret" });

// Dispatch a run; the delivery key makes the retry idempotent.
const { run, deduplicated } = await pool.runs.dispatch({
  project: "proj_01",
  trigger: "nightly",
  actor: "operator_01",
  deliveryKey: deliveryKey(),
  input: "run the nightly build",
});
```

Namespaces mirror the contract: `pool` (identity, members, invitations),
`apiKeys`, `hosts` (presence, capabilities, executions), `connections`,
`projects` (configuration, revisions), `triggers`, `runs`, `activity`,
`chatrooms`, `relay`, `webhooks`, `setup`, `system`, and `game.*` for the
Talo surface. Every method is also reachable generically:

```ts
const page = await pool.request("get_v1_chatrooms_room_messages", {
  pathParams: { room: "room_01" },
  query: { after: 42, limit: 100 },
});
```

### Cursor paging

```ts
import { iterateChatMessages, iterateRelayEvents, iterateActivity } from "@clawffice/tidepool-sdk";

for await (const message of iterateChatMessages(pool, "room_01", { after: 0 })) {
  console.log(message.sequence, message.body);
}
```

### Live chatroom stream

`GET /v1/chatrooms/{room}/stream` upgrades to a WebSocket. The server expects the
operator JWT in the query string (a browser socket cannot set `Authorization`).

```ts
const stream = pool.chatrooms.stream("room_01", { after: lastSequence });
for await (const frame of stream.frames()) {
  if (frame.type === "chat.message") console.log(frame.message.body);
  if (frame.type === "chat.current") console.log("live, resume from", frame.cursor);
}
```

### Errors

Every non-2xx response becomes a `ProblemError` with `status`, `code`,
`detail`, `requestId` (from the problem body or the `x-request-id` header),
`attempts` and `retryAfterMs`. Unknown `code` values stay opaque strings, as the
contract requires.

```ts
import { ProblemError } from "@clawffice/tidepool-sdk";

try {
  await pool.projects.get("proj_missing");
} catch (error) {
  if (error instanceof ProblemError && error.code === "not_found") {
    console.warn("missing project, requestId", error.requestId);
  }
}
```

### Retries

`429` responses are retried with exponential backoff (250 ms, 500 ms, … capped
at 4 s) or with the server's `Retry-After` when present, and **never** more than
`retry.maxRetries` times (default 2). A `Retry-After` longer than
`maxRetryAfterMs` (default 60 s) is surfaced as an error instead of being slept
through:

```ts
const pool = createClient({
  baseUrl: "https://pool.example.com",
  retry: { maxRetries: 3, baseDelayMs: 200, maxDelayMs: 2_000 },
});
```

### Token refresh

Supply a refresh hook; it is called at most once per request, either proactively
when the cached expiry passed or reactively on a `401`. A hook that returns
`null` lets the `401` surface, so a stale credential cannot loop.

```ts
const pool = createClient({
  baseUrl: "https://pool.example.com",
  token: process.env.TIDEPOOL_TOKEN,
  refreshToken: async () => {
    const { token, expires_in } = await signInAgain();
    return { token, expiresIn: expires_in };
  },
});
```

### Permissions before sending

```ts
pool.allows("post_v1_runs", { scopes: ["runs:dispatch"] }); // true
pool.allows("get_v1_api_keys", { role: "admin" }); // false, owner only
pool.describe("post_v1_runs"); // auth model, scope, role, statuses
```

### Browser and Node builds

`package.json` exports conditions select the build:

| Condition | Entry | Adds |
|---|---|---|
| `browser` | `src/browser.js` | nothing — fetch and `WebSocket` only |
| `node` | `src/node.js` | `readTokenFile`, `createNodeClient` (`node:fs`) |
| default | `src/index.js` | the browser-safe core |

`node:fs` is imported by `src/node.js` alone; `test/browser.test.mjs` walks the
import graph from the browser entry and fails on any `node:` or bare import.

## Development

```sh
npm run generate     # regenerate src/generated/* and test/generated/conformance.ts
npm run check        # fail if the committed artifacts are stale
npm test             # node:test suites over a recorded HTTP transport
npm run typecheck    # tsc over the declarations and the generated conformance file
npm run smoke        # npm pack, fresh install, enrol a host and dispatch a run
```

`npm run typecheck` expects `tsc` (TypeScript 5.8) on `PATH`; the package declares
it as a dev dependency but ships no compiled output, so consumers do not need it.

The contract is the only input. Regeneration is a pure function of
`docs/api/openapi-v1.json`, and the drift test compares the committed artifacts
byte for byte. `scripts/lib/contract.mjs` holds the one explicit
`operationId → namespace.method` map; a new route without a mapping fails
generation rather than silently shipping untyped.

### Smoke test against a dev stack

```sh
TIDEPOOL_TOKEN=... TIDEPOOL_PROJECT=proj_01 TIDEPOOL_TRIGGER=nightly \
  node scripts/smoke.mjs --live http://localhost:8080
```

The default `npm run smoke` run is hermetic: it still packs, installs and
executes the published tarball, but the transport is a recorded set of
contract-shaped exchanges from `test/fixtures/`, so no server is required.

## Not covered

The contract currently documents no query parameters; the SDK types the ones the
handlers read (`after`, `limit`, `cursor`, `project`, `provider`, `trigger`,
`status`, `kind`, `page`, `per_page`) and records the gap in
`docs/API_CONTRACT.md` §5.1.
