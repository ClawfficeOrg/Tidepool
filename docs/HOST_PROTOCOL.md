# Tidepool host protocol (v1, v2 jobs)

The pool talks to an enrolled machine over one WebSocket. HTTP is used only to
enroll and to report capabilities; everything else is versioned JSON frames.
The protocol is framed by the pool, so Swaygent and the reference host agent
implement the same document unchanged. Version 2 (section 5a) adds typed
`tkrun` jobs; it is negotiated and v1 hosts are unaffected.

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

## 5a. Version 2: negotiation and typed jobs (phase 1.7)

v1 is unchanged. A host that never sends `hello` is a v1 host and receives
only v1 frames. A v2 host opts in with its first frame after connect:

```json
{"v": 2, "type": "hello", "versions": [1, 2], "features": ["jobs.tkrun"],
 "agent": "tidepool-host-agent/0.1.0"}
```

The pool answers with the intersection it supports:

```json
{"v": 2, "type": "hello.ack", "version": 2, "features": ["jobs.tkrun"]}
```

Unknown features are dropped from the answer, never refused. The pool sends
`start_job` only to a host that negotiated `jobs.tkrun`; otherwise dispatch
fails with `409` naming the missing feature. The negotiated set lasts for that
socket only and is re-negotiated on every connect.

A job is a **typed** request. The pool never sends a command line, a path or a
secret: the host builds a fixed argv from the typed fields and validates each
one against its own allowlist. v2 `create_agent` and v1 frames are not changed.

### Pool -> host: start_job

```json
{"v": 2, "type": "start_job", "jobId": "<uuid>", "kind": "tkrun",
 "project": {"slug": "chiton", "server": "https://tk.example"},
 "checkout": "chiton-main",
 "options": {"hours": 2, "minutes": 0, "tasks": ["1.3.2"], "jobs": 1,
             "dryRun": false}}
```

- `kind` is `tkrun` (feature `jobs.tkrun`). The host runs
  `tidetable --server <project.server> tkrun run <slug> --repo <dir> ...` with
  `<dir>` taken from its local checkout allowlist for `checkout`.
- `project.slug` matches `^[a-z0-9][a-z0-9-]{0,62}$`; `checkout` matches
  `^[a-z0-9][a-z0-9_-]{0,62}$`.
- `options`: `hours` 0-12, `minutes` 0-59, `jobs` 1-4, `tasks` each matching
  `^\d+\.\d+\.\d+$` (at most 16), `dryRun` boolean. Unknown keys refuse.
- The TideTable token is never in this frame. The host supplies it from its own
  local store and passes it only to the child process, as `TIDETABLE_TOKEN`.

### Pool -> host: stop_job

```json
{"v": 2, "type": "stop_job", "jobId": "<uuid>", "mode": "stop|kill"}
```

`stop` asks the run to stop after the current step (`tkrun stop`); `kill`
terminates it (`tkrun kill`). The host answers with `job_completed`.

### Host -> pool: job_accepted, job_refused

```json
{"v": 2, "type": "job_accepted", "jobId": "<uuid>", "pid": 1234,
 "owner": "tkrun-2026-10-31@HOST"}
{"v": 2, "type": "job_refused", "jobId": "<uuid>",
 "code": "checkout_not_allowed|tidetable_forbidden|tidetable_unreachable|job_already_running|tkrun_missing|invalid_request",
 "detail": "<text>"}
```

`owner` is the TKRun owner string (`tkrun.py`), which joins the job to the
hub's claims in a later phase. `detail` never contains a credential.

The host refuses before spawning anything. For `tidetable_forbidden` and
`tidetable_unreachable` the host checks the hub first with a read request
(`GET /api/<slug>/ready` with its token): `401` and `403` map to
`tidetable_forbidden`, a transport failure to `tidetable_unreachable`.

### Host -> pool: job_output, job_completed

```json
{"v": 2, "type": "job_output", "jobId": "<uuid>", "seq": 17, "text": "<line>"}
{"v": 2, "type": "job_completed", "jobId": "<uuid>",
 "status": "succeeded|failed|stopped|killed", "exitCode": 0,
 "summary": "<last log lines>"}
```

`seq` is per job, starting at 1 and increasing by one per line. The pool keeps
the highest `seq` it stored and drops any line at or below it, so a replay after
a reconnect does not duplicate output. `job_completed` is terminal and is
repeated on request for the same `jobId`.

### Pool -> host: start_job replay

A repeated `start_job` with the same `jobId` is answered with the existing
job's `job_accepted` or `job_completed` (or `job_refused`), never a second
process. This is the same rule as `create_agent` (section 4).

### Host -> pool: jobs_snapshot

Sent right after `hello.ack` on every connect:

```json
{"v": 2, "type": "jobs_snapshot", "jobs": [
  {"jobId": "<uuid>", "state": "running", "pid": 1234, "exitCode": null},
  {"jobId": "<uuid>", "state": "completed", "pid": 1200, "exitCode": 0}]}
```

A job that kept running while the socket was down is re-adopted: the pool
keeps its row live and resumes output from the stored `seq`.

### Jobs are not queued

Dispatch to a host that is not connected is refused with `daemon_not_connected`
(409) and nothing is stored. A job row is written before `start_job` is sent, so
a lost frame shows as a job that never reaches `job_accepted`, not as a silent
gap.

## 6. Reference agent

`crates/tidepool-host-agent` implements this document with a blocking HTTP +
WebSocket client. It can enroll, report a capability list, heartbeat, accept one
`create_agent`, emit output and complete; it deduplicates by `executionId` and
answers a replay with the stored completion. It exists to test the pool without
a full host, not as a production daemon: Swaygent is the suite's pool host and
implements this document in `crates/swaygent-core`.
