# tidepool-client

Typed Rust client for the **Tidepool API v1** and the hub's resource surface:
hosts and enrolment, presence and capabilities, connections, projects and
configuration revisions, triggers, dispatch, runs and activity, chatrooms with
cursor paging and the live stream, the session relay, API keys, members and the
pool's Talo-compatible game routes.

This is the Rust binding of the same contract and the same resource shape as
the TypeScript SDK (`sdk/ts`). It is the client the ratatui console, the
reference host agent and the Zoid client build on.

## API version

| Item | Value |
|---|---|
| API major version | **v1** |
| Contract | `docs/api/openapi-v1.json` |
| Contract version | **1.0.0** (`tidepool_client::CONTRACT_VERSION`) |

The contract is the only input to the generated modules under
`src/generated/`. A contract edit without a regeneration fails the crate's
tests; `node scripts/generate.mjs --check` fails the same way in CI.

## Features

| Feature | Default | Adds |
|---|---|---|
| *(none)* | — | contract types, the route table, typed requests, errors |
| `blocking` | yes | `blocking::Client` over `ureq`; no async runtime |
| `async` | no | `asynchronous::Client` over `reqwest` (needs Tokio) |

A no-feature build has no networking dependency beyond `serde`. The blocking
facade never asks the caller to write a runtime:

```rust
use tidepool_client::blocking::Client;
use tidepool_client::endpoints::GetV1Hosts;

let pool = Client::from_base_url("https://pool.example.com")?.with_token(token);
for host in pool.call(&GetV1Hosts::new())?.hosts {
    println!("{} {} may run agents: {}", host.slug, host.status, host.execution_enabled);
}
```

The async facade is the same surface with `.await`; the caller supplies the
runtime (`#[tokio::main]`, `Runtime::block_on`, …):

```rust
use tidepool_client::asynchronous::Client;
use tidepool_client::endpoints::GetV1Hosts;

let pool = Client::from_base_url("https://pool.example.com")?.with_token(token);
let hosts = pool.call(&GetV1Hosts::new()).await?;
```

## Every contract route is reachable and typed

Each operation is a type in `tidepool_client::endpoints`:

```rust
use tidepool_client::blocking::Client;
use tidepool_client::endpoints::{GetV1Activity, GetV1ActivityQuery, PostV1Runs};
use tidepool_client::generated::types::ManualDispatchRequest;

let page = pool.call(
    &GetV1Activity::new().with_query(GetV1ActivityQuery {
        project: Some("proj_01".to_owned()),
        limit: Some(50),
        ..GetV1ActivityQuery::default()
    }),
)?;

let dispatched = pool.call(&PostV1Runs::new(ManualDispatchRequest {
    actor: "operator_01".to_owned(),
    delivery_key: "delivery-1".to_owned(),
    input: Some("run the nightly build".to_owned()),
    project: "proj_01".to_owned(),
    revision: None,
    trigger: "nightly".to_owned(),
}))?;
```

The generated namespace wrappers mirror the TypeScript SDK's resource shape:

```rust
use tidepool_client::generated::resources::blocking_api as api;

let hosts = api::hosts::list(&pool)?;
let host = api::hosts::get(&pool, "host_01")?;
let page = api::activity::list(&pool, Default::default())?;
```

There is also a generic path keyed by operation id, for a caller that prefers
the route table:

```rust
let host = pool.request_json("get_v1_hosts_id", &[("id", "host_01")], &[], None)?;
```

`tidepool_client::describe("post_v1_runs")` returns the route's method, path,
auth model, required scope and role. `tidepool_client::allows(...)` answers
whether a principal may call it before sending anything.

## Errors

Every non-2xx response becomes `Error::Problem(ProblemError)` with `status`,
`code`, `detail`, `title`, `request_id` (body or `x-request-id`), `attempts`
and `retry_after_ms`. An unknown code is preserved verbatim, as the contract
requires.

```rust
if let Some(problem) = error.problem() {
    if problem.code() == "not_found" {
        eprintln!("missing, requestId {:?}", problem.request_id());
    }
}
```

## Retries and refresh

`429` is retried with exponential backoff (250 ms, 500 ms, capped at 4 s) or the
server's `Retry-After`, and never more than `max_retries` (default 2). A
`Retry-After` longer than 60 s is surfaced instead of slept through
(`ProblemError::retry_after_ms`). A `401` consults the refresh hook at most once
per request; a hook returning `None` lets the 401 surface, so a stale credential
cannot loop.

```rust
let pool = Client::from_base_url(base_url)?
    .with_token(token)
    .with_refresh(std::sync::Arc::new(|| {
        // Sign in again on a background thread or with a stored credential.
        Ok(Some(tidepool_client::TokenUpdate::new(new_token)))
    }));
```

Per call: `CallOptions::new().with_request_id(..).with_header(..)
.with_timeout(..).without_retry()`.

## Cursor paging

`tidepool_client::paging` follows `next`/`nextCursor` until the server stops
offering one:

```rust
use tidepool_client::paging::{PageOptions, activity, chat_messages};

for message in chat_messages(&pool, "room_01", PageOptions::new().after(0)) {
    println!("{}", message?.body);
}
```

## Live chatroom stream

`GET /v1/chatrooms/{room}/stream` upgrades to a WebSocket. The handle carries
the operator token in the URL because the pool expects it there; treat the
whole URL as a secret.

```rust
use tidepool_client::endpoints::GetV1ChatroomsRoomStream;

let stream = pool.stream(&GetV1ChatroomsRoomStream::new("room_01"))?;
for frame in stream.connect()? {
    match frame? {
        tidepool_client::stream::ChatFrame::Message { message, .. } => println!("{}", message.body),
        tidepool_client::stream::ChatFrame::Current { cursor, .. } => println!("live from {cursor:?}"),
        _ => {}
    }
}
```

The async facade returns the same typed handle and leaves the socket to the
caller's WebSocket library, so it does not force a second runtime dependency.
`ChatFrame::parse` turns one text payload into a typed frame.

## Generation

```sh
cd sdk/rust
node scripts/generate.mjs          # regenerate src/generated/*
node scripts/generate.mjs --check  # fail if a committed artifact is stale
```

`scripts/generate.mjs` holds the one explicit `operationId → namespace.method`
map and the query-parameter table the contract does not publish yet
(`docs/API_CONTRACT.md` §5.1). A route without a mapping fails generation
rather than shipping untyped.

## Tests

```sh
cargo test                 # blocking facade + contract parity
cargo test --all-features  # also the async facade
cargo clippy --all-targets --all-features -- -D warnings
cargo build --no-default-features
cargo build --no-default-features --features async
```

All tests are hermetic: a scripted `TcpListener` answers the HTTP exchanges and
a local WebSocket handshake drives the stream tests, so no pool, MySQL or Redis
is needed. The live-stack test is opt-in and ignored by default:

```sh
TIDEPOOL_BASE_URL=http://localhost:8080 TIDEPOOL_TOKEN=... \
  cargo test --test live -- --ignored --nocapture
```

## Not covered

The async facade has no iterator over pages and no socket driver; the blocking
facade has both. The query parameters remain outside the published contract
until the route table gains them, so that table is the review point for a change
to paging.
