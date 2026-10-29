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

All frames carry `"v": 1`. Unknown versions and unknown types are refused with
an error frame; the connection stays open unless authentication failed.

### Pool → host: create_agent

```json
{"v": 1, "type": "create_agent", "executionId": "<stable id>", "prompt": "<text>"}
```

`executionId` is chosen by the pool and is the idempotency key. The host MUST
answer a repeat of the same `executionId` with the existing execution's state
(including completion) and MUST NOT run the prompt twice. The pool MAY resend
after a lost response, a reconnect, or a process restart.

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
time. A repeated completed frame for the same execution is accepted (it makes
replay safe).

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
`status: "interrupted"`. Interrupting an unknown or already-completed
execution is answered with the existing completion or a no-op error frame.

### Errors

```json
{"v": 1, "type": "error", "code": "invalid_request", "detail": "<text>"}
```

The pool uses the same `code` taxonomy as the hub API
(`docs/HUB_API_ERRORS.md`), including `daemon_not_connected` when dispatch is
attempted with no live socket. Protocol errors never include credentials.

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
