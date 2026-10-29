# tidepool-client (Python)

Typed Python client for the **Tidepool API v1** and the hub's resource surface:
hosts and enrolment, presence and capabilities, connections, projects and
configuration revisions, triggers, dispatch, runs and activity, chatrooms with
cursor paging and the live stream, the session relay, API keys, members and the
pool's Talo-compatible game routes.

This is the Python binding of the same contract and the same resource shape as
the Rust SDK (`sdk/rust`) and the TypeScript SDK (`sdk/ts`), and it has the same
retry and error behaviour:

* one client per resource, plus a generic call keyed by operation id;
* typed models for every contract schema, tolerant of unknown fields and open
  enum values;
* `ProblemError` carrying `status`, `code`, `detail`, `request_id`, `attempts`
  and `retry_after_ms`, with unknown codes preserved verbatim;
* bounded 429 retries with exponential backoff that honour `Retry-After`, and
  refresh-once on a 401;
* a synchronous client and an asynchronous client over the *same* route table.

The package has **no runtime dependencies** — the sync client uses
`http.client`, the async client uses `asyncio` streams, so both work from a
plain wheel with no optional extra.

## API version

| Item | Value |
|---|---|
| API major version | **v1** |
| Contract | `docs/api/openapi-v1.json` |
| Contract version | **1.0.0** (`tidepool_client.CONTRACT_VERSION`) |

## Install

```sh
pip install tidepool-client
```

Python 3.9 or newer. From a checkout: `pip install .` at the repository root.

## Use

```python
import os

from tidepool_client import SyncClient

pool = SyncClient("https://pool.example.com", token=os.environ["TIDEPOOL_TOKEN"])

host = pool.hosts.create(body={"display_name": "Build box", "hostname": "build-box.local"})
minted = pool.hosts.mint_enrollment_token(body={"host_id": host.id})
enrolled = pool.hosts.enroll(body={"token": minted.token, "credential": credential})

activity = pool.activity.list(limit=50)
for entry in activity.items:
    print(entry.id, entry.kind)
```

The async client is the same surface as coroutines:

```python
from tidepool_client import AsyncClient

pool = AsyncClient("https://pool.example.com", token=token)
hosts = await pool.hosts.list()
```

Namespaces mirror the contract: `system`, `setup`, `pool`, `api_keys`, `hosts`,
`connections`, `projects`, `triggers`, `runs`, `activity`, `chatrooms`, `relay`,
`webhooks`, and `game.*` for the Talo surface. Every method is also reachable
generically:

```python
page = pool.call(
    "get_v1_chatrooms_room_messages",
    path_params={"room": "room_01"},
    query={"after": 42, "limit": 100},
)
raw = pool.request_json("get_v1_hosts")
```

`tidepool_client.ROUTES` describes every operation (method, path, auth model,
required scope and role), so a caller can check permission before sending:

```python
from tidepool_client import route

info = route("post_v1_runs")
if info.scope == "runs:dispatch":
    ...
```

## Typed models

`tidepool_client.models` holds one class per contract schema. `from_dict` and
`to_dict` round-trip a contract-shaped body, unknown members are preserved, and
an unknown open-enum value stays an opaque string:

```python
from tidepool_client.models import HostView

host = HostView.from_dict(body)
assert host.to_dict() == body
```

## Errors

Every non-2xx response becomes `ProblemError`:

```python
from tidepool_client import ProblemError

try:
    pool.projects.get("proj_missing")
except ProblemError as error:
    if error.code == "not_found":
        print("missing project, requestId", error.request_id)
```

`attempts` says how many HTTP attempts produced the error. A `429` with a
`Retry-After` above 60 seconds is not slept through: `RetryAfterTooLongError`
is raised instead, carrying `requested_ms` and `limit_ms`.

## Retries and refresh

```python
from tidepool_client import RetryPolicy, SyncClient

pool = SyncClient(
    "https://pool.example.com",
    retry=RetryPolicy(max_retries=3, base_delay_ms=200, max_delay_ms=2_000),
    refresh=lambda: fresh_token(),  # called at most once per request
)
```

A refresh hook may return a token string, a `TokenUpdate`, a mapping with a
`token`, or `None`. Returning `None` lets the 401 surface, so a stale credential
cannot loop. The async client accepts a coroutine hook.

Per call: `CallOptions().with_request_id(..).with_header(..).with_timeout(..)
.without_retry()`.

## Cursor paging

```python
from tidepool_client import iter_activity, iter_chat_messages, iter_relay_events

for message in iter_chat_messages(pool, "room_01", after=0):
    print(message.sequence, message.body)

for entry in iter_activity(pool, project="proj_01", limit=50):
    print(entry.kind)
```

`aiter_activity`, `aiter_chat_messages` and `aiter_relay_events` are the async
forms.

## Live chatroom stream

`GET /v1/chatrooms/{room}/stream` upgrades to a WebSocket. The handle carries the
operator token in its URL because the pool expects it there; treat the whole URL
as a secret.

```python
from tidepool_client import ChatFrame

stream = pool.chatrooms.stream("room_01", after=last_sequence)
# Hand stream.url to your WebSocket library, then:
frame = ChatFrame.parse(payload)
if frame.kind == "chat.message":
    print(frame.message.body)
elif frame.kind == "chat.current":
    print("live, resume from", frame.cursor)
```

## Example

[`examples/enroll_and_activity.py`](examples/enroll_and_activity.py) is the short
end-to-end example: it creates a host, mints an enrolment token, enrols the host
and reads the first page of activity.

```sh
python sdk/python/examples/enroll_and_activity.py \
  --base-url "$TIDEPOOL_BASE_URL" --token "$TIDEPOOL_TOKEN"
```

## Generation

The committed modules under `tidepool_client/_generated/` are produced from
`docs/api/openapi-v1.json` and nothing else:

```sh
python sdk/python/scripts/generate.py          # regenerate
python sdk/python/scripts/generate.py --check  # fail if a committed artifact is stale
```

The `operationId → namespace.method` map is explicit, so a route added to the
contract without a mapping fails generation rather than shipping untyped.

## Tests

```sh
python -m pytest sdk/python/tests
```

The suite is hermetic: a recorded local HTTP server answers scripted exchanges
and the route-coverage test calls every contract route on both clients. No pool,
MySQL or Redis is needed. The dev-stack checks are opt-in:

```sh
TIDEPOOL_BASE_URL=http://localhost:8080 TIDEPOOL_TOKEN=... \
  python -m pytest sdk/python/tests/test_live.py
```

The packaging test builds a wheel with `pip wheel --no-build-isolation
--no-deps`, asserts it has no dependencies or extras, and imports it directly
through `zipimport` — it never installs anything.
