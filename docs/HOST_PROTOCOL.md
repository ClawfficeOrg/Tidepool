# Tidepool host protocol (v1)

The pool talks to an enrolled machine over one WebSocket. HTTP is used only to
enroll and to report capabilities; everything else is versioned JSON frames.
The protocol is framed by the pool, so Swaygent and the reference host agent
implement the same document unchanged.

## 1. Enroll

A host is registered by an operator (`POST /v1/hosts`) and gets an immutable
`id` and a mutable `slug`. An operator or an API key with `daemons:enroll`
mints `POST /v1/hosts/enrollment-tokens` for that host; the raw token is shown
once and expires after ten minutes.

The host generates a long-lived credential (at least 32 characters) and keeps
it. It exchanges the token once:

```http
POST /v1/hosts/enroll
Content-Type: application/json

{"token": "<public-id>.<secret>", "credential": "<host-generated>"}
```

The pool stores only an Argon2id hash of the credential. A host that already
has a credential is refused with 409. The token is single use; an expired or
revoked token fails with 410. Revoking the issuing API key revokes every
unconsumed token it minted.

## 2. Connect

```http
GET /ws/host
X-Host-Id: <host public id>
X-Host-Credential: <host-generated credential>
Upgrade: websocket
```

A successful upgrade marks the host `connected`; dropping the last connection
marks it `offline`. The pool never accepts a client-supplied presence field.

The host sends a heartbeat every 30 seconds:

```json
{"v": 1, "type": "heartbeat"}
```

The pool answers `{"type":"heartbeat.ack"}`. A host with no heartbeat for 90
seconds is marked offline, the socket is closed, and dispatch fails with
`daemon_not_connected`. Reconnect with the same credential; presence resumes.

## 3. Execution permission and capabilities

Enrollment and connection do **not** permit execution. `host.execute` is a
separate grant (`POST /v1/hosts/{id}/execution`). Until it is granted the host
is *connected-only*.

With the grant, the host reports what it can run:

```http
POST /v1/hosts/{id}/capabilities
X-Host-Id: <host public id>
X-Host-Credential: <credential>
Content-Type: application/json

{"capabilities": [
  {"provider": "anthropic", "model": "claude", "execution_mode": "default",
   "thinking_option": "high", "options": {"max_tokens": 8192}},
  {"provider": "local", "model": "echo", "execution_mode": "default"}
]}
```

A report replaces the previous set. `options` is opaque provider-native JSON.
Only providers in the pool's grantable set are offered for pooled execution.
Re-reporting after a model disappears is how the host withdraws it.

## 4. Frames

Frames are JSON text messages. The frames below carry `"v": 1`, and a host
should send it, but the pool reads only `type` and does not check `v` today.
The pool's own `heartbeat.ack` and `error` frames carry no `v`.

What the pool accepts from a host is exactly this list:

| Host → pool `type` | Pool action |
|---|---|
| `heartbeat` (or the bare text `heartbeat`) | Refresh presence; answer `{"type":"heartbeat.ack"}` |
| `execution_started` | Move the execution from `queued` to `running` |
| `execution_output` | Append `text` while the execution is `queued` or `running` |
| `execution_completed` | Record the terminal status and output; advance the run step |

Anything else, including a text message that is not JSON, a frame with no
`type`, and a host-sent `error` frame, is answered with the pool's `error`
frame (see Errors) and otherwise ignored. The connection stays open. Binary
messages are ignored and a ping is answered with a pong. A frame whose
`executionId` matches no execution changes nothing and is not answered.

### Pool → host: create_agent

```json
{"v": 1, "type": "create_agent", "executionId": "<stable id>", "prompt": "<text>"}
```

`executionId` is chosen by the pool and is the idempotency key. The host MUST
answer a repeat of the same `executionId` with the existing execution's state
(including completion) and MUST NOT run the prompt twice. The pool MAY resend
after a lost response, a reconnect, or a process restart.

Two optional fields may follow. Each is omitted when the pool has nothing to
send, so a frame without them is the frame above, and a host that ignores
unknown keys keeps working:

```json
{"v": 1, "type": "create_agent", "executionId": "<stable id>", "prompt": "<text>",
 "agent": {"provider": "anthropic", "model": "claude", "execution_mode": "default",
           "thinking_option": "high"},
 "project": "<name>"}
```

- `agent` is the selection the pool asks for, in the shape of a capability
  report entry without `options`; `thinking_option` may be absent. The
  operator route checks it against the host's last report before sending.
- `project` is a name of 1-128 ASCII letters, digits, `.`, `_` or `-` that does
  not start with `.`. The pool refuses anything else.

Both are requests, not commands. The host maps `agent` through its own
allowlist and resolves `project` against its own list of projects; it MUST NOT
treat either as a filesystem path or a command line. It refuses anything it
will not run as described under Refusals. The pool does not store the options,
and run dispatch and its replays do not send them yet.

### Host → pool: execution_started

```json
{"v": 1, "type": "execution_started", "executionId": "<id>"}
```

Optional, but recommended. The pool also marks the execution running when it
sends the intent.

### Host → pool: execution_output

```json
{"v": 1, "type": "execution_output", "executionId": "<id>", "text": "<delta>"}
```

Deltas are appended in arrival order. Duplicate deltas are not deduplicated;
the host should send each delta once and rely on `executionId` for the intent.

### Host → pool: execution_completed

```json
{"v": 1, "type": "execution_completed", "executionId": "<id>",
 "status": "succeeded|failed|interrupted", "output": "<final text>"}
```

Terminal. The pool records the status, optional final output and completion
time. A missing or unknown `status` is recorded as `failed`; a missing `output`
keeps the output appended so far. A repeated completed frame for the same
execution is accepted (it makes replay safe). The pool does not compare it with
the first one: it rewrites the execution's status and output, but a run step
that already finished stays as it was. A host therefore sends the same status
every time.

### Pool → host: interrupt_execution

```json
{"v": 1, "type": "interrupt_execution", "executionId": "<id>"}
```

An operator sends it with
`POST /v1/hosts/{id}/executions/{execution_id}/interrupt` (member token, a
viewer is refused). The execution must belong to that host. A running
execution is answered `202` with `"sent": true`; an execution that is already
terminal is answered `200` with `"sent": false` and nothing is sent; a host
with no live socket fails with `daemon_not_connected`. The grant is not
required, so work can still be stopped after `host.execute` is revoked. The
route does not change the recorded status: the host's `execution_completed`
does.

The host stops the execution and answers with `execution_completed` and
`status: "interrupted"`. For an execution that already completed, the host
answers with the existing completion and its recorded status, never a second,
different terminal state. For an execution it does not know, the host does
nothing: it MUST NOT answer with an `error` frame, because the pool has no
handler for one (see the table above).

### Refusals

A host that will not run a `create_agent` (its grant or local switch is off,
the selection or project is not on its allowlist, the prompt is invalid)
answers with a failed completion, not with an `error` frame:

```json
{"v": 1, "type": "execution_completed", "executionId": "<id>",
 "status": "failed", "output": "refused: <reason>"}
```

The pool records that as a failed execution and fails the run step. The host
should remember the refusal under the `executionId`, so a resent intent gets
the same answer.

### Errors

`error` is a pool → host frame only. The pool sends one when it receives a
frame it does not handle:

```json
{"type": "error", "code": "invalid_request", "detail": "unknown frame"}
```

The host should log it and carry on; the connection stays open. `code` uses
the hub API taxonomy (`docs/HUB_API_ERRORS.md`). Dispatch failures such as
`daemon_not_connected` are not frames: they are returned to the HTTP caller
that tried to dispatch. Protocol errors never include credentials.

## 5. Replay and recovery

- A resent `create_agent` with the same `executionId` returns the existing
  execution; the host does not run it again.
- The pool stores an execution row before sending, so a pool restart can resend
  the same intent after the host reconnects.
- A host that loses its socket reconnects with the same credential. It keeps
  its local execution table (the reference agent keeps it in memory) and answers
  the resent intent with completion.
- Presence is derived from connection + heartbeat; a disconnected host is
  offline immediately and dispatch fails with `daemon_not_connected` instead of
  queueing.

## 6. Reference agent

`crates/tidepool-host-agent` implements this document with a blocking HTTP +
WebSocket client. It can enroll, report a capability list, heartbeat, accept one
`create_agent`, emit output and complete; it deduplicates by `executionId` and
answers a replay with the stored completion. It exists to test the pool without
a full host, not as a production daemon: Swaygent is the suite's pool host and
implements this document in `crates/swaygent-core`.
