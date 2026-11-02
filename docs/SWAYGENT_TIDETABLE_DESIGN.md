# Tidepool, TideTable and Swaygent: one connected system

> Design pass, 2026-10-30. No code changed. Every claim about current behaviour
> cites the file and line it was read from; section 9 lists what was not
> verified and where the brief did not match the code.

## 0. Summary

- **Tidepool is the place you look and the place that decides to run.** It
  owns projects (as bindings), hosts, checkouts, schedules and the job record.
- **TideTable stays the source of truth for tasks, claims, leases and
  outcomes.** Tidepool reads it; it never writes a todo file and never holds a
  write token.
- **Swaygent is the hands.** It connects to Tidepool over the documented host
  socket and, when asked, runs `tidetable tkrun run <slug> --repo <checkout>`
  as a local process with a TideTable token that never leaves the machine.

Recommended execution model: option (a), a typed **tkrun job** sent to a host
over host protocol v2, not per-task prompts.

## 1. The model

### 1.1 Entities

| Entity | Fields | Owner of the fact | Where it lives |
|---|---|---|---|
| **TideTable project** | slug, name, repo_url, todo_path, default_branch | TideTable | hub registry (`deploy/registry.json` shape, read by `load_registry`, `tidetable/serve.py:161-162`); the VPS copy holds `chiton` |
| **Tasks, claims, leases, done, changelog, events** | per task | TideTable | the todo file on the hub; writes only through `POST /api/<slug>/start|done|…` (`serve.py:49-55`) |
| **Gate, agent routing, prompt rules** | `.tidetable/tkrun.json`, `.tidetable/agents.json` | the repository (committed) | e.g. Chiton `.tidetable/tkrun.json` (gate = clippy + test) |
| **Tidepool project** | slug, name, active configuration revision | Tidepool | `projects` table (`migrations/20261021000001_create_projects.up.sql`) |
| **Binding** (new) | Tidepool project -> TideTable `{server, slug}` | Tidepool (authored) | new `tidetable` block in the manifest `tidepool.json` (`src/services/configuration.rs:42`) |
| **Host** | id, slug, credential hash, presence, `host.execute`, capabilities | Tidepool | hosts tables (task 1.2.x) |
| **Checkout** (new) | name, host, absolute directory | Tidepool (authored), confirmed by the host | a named `checkouts` entry in the manifest; same validation shape as an environment (`src/services/environments.rs:280-283`) but no worktree: TKRun owns its worktree |
| **Schedule** (new) | name, cron, timezone, checkout, job options, overlap policy | Tidepool | `schedules` block in the manifest |
| **Job** (new) | id, project, checkout, host, options, origin (manual/schedule), status, exit code, owner string, output tail | Tidepool owns the record; the host owns the process and the worktree | new `jobs` table |
| **Task outcome inside a job** | claimed, done, partial, failed | TideTable (claims) + the host's run directory (`.tkrun/log.md`, `tkrun.py:628`) | Tidepool joins them, never stores them as truth |

A **project** in the user's sense ("slug + repo + todo path + gate + hub") is
therefore a join, not one row: the Tidepool project, its TideTable binding
(slug, hub, and via the hub's registry the repo and todo path), the gate from
the repo's committed config, and one or more checkouts on hosts.

### 1.2 Why this split

- **TideTable keeps tasks and claims** because it is already the single
  writer with locking and lease enforcement (`integration-guide.md:283-293`),
  and TKRun already speaks it (`tkrun.py:824-835`). A second store of task
  state in Tidepool would need a sync protocol and would be wrong whenever the
  hub and the pool disagree. TideTable's own rule is that its Postgres is "a
  read projection ... never a scheduler" (`integration-guide.md:292-293`), so
  there is no scheduler to compete with.
- **Tidepool owns schedules and the job record** because deciding *when* and
  *on which machine* is exactly the hub layer it already is: hosts, presence,
  dispatch with idempotent ids and the no-queue rule
  (`docs/HUB_CAPABILITIES.md:101-103`). TideTable has no notion of a machine
  beyond advisory heartbeats.
- **The repo owns the gate.** Gate commands are code the repo runs; letting
  Tidepool author them would make the pool a remote-code channel. Tidepool
  shows the gate as the host reports it (step names plus a hash of
  `.tidetable/tkrun.json`), never edits it.
- **Tidepool does not copy the registry.** The binding stores only the slug
  and the hub URL. Activation asks the hub that the slug exists
  (`GET /api/projects`, `serve.py:379-381`); repo_url and todo_path are shown
  live from the hub. Registering a new slug stays a TideTable operation (there
  is no route for it; the registry is a file).

### 1.3 How it maps

```
            operator (control plane / console / CLI)
                         |
                  Tidepool pool  -- reads (read token) -->  TideTable hub (VPS)
   project, binding, checkouts,                              tasks, claims, leases,
   schedules, jobs, hosts                                    events, sessions
                         | /ws/host, protocol v2                   ^
                         v                                         | claims/done (write token,
                Swaygent on a machine                              | scoped to this host's slugs)
       spawns: tidetable --server <hub> tkrun run <slug> --repo <checkout>
                         |
                 git worktree, agent, gate, commits (stay local)
```

## 2. Where runs execute

### 2.1 What exists today

- TKRun runs as a local process; in hub mode the todo argument is the slug and
  `--repo` is the checkout (`tkrun.py:2067-2088`, `integration-guide.md:18-28`).
  It does claims, gate, review, repair, commit and `done` itself, makes its
  own tidetable calls with the token (`tkrun.py:831-832`) and hides that token
  from the agent (`tkrun.py:92-93`, `1234`).
- Tidepool dispatch computes a directory and branch for each step
  (`src/services/dispatch.rs:776-793`) but `create_execution` sends only the
  prompt (`src/services/runs.rs:711-717`, frame built at
  `src/services/host_protocol.rs:86-93`). **The directory never reaches the
  host.** Any prompt dispatch today runs wherever the host's process happens to
  be.
- Swaygent's dispatch speaks HTTP polling routes
  (`POST /v1/hosts/{h}/sessions/next`, `.../status`, `.../output`,
  `GET .../control`; Swaygent `crates/swaygent-core/src/hub/dispatch.rs:13-29`).
  **Tidepool has no such routes** (only `/v1/sessions/{s}/relay`,
  `src/api/relay.rs:3-7`). Swaygent's presence is a 60 s
  `POST /v1/hosts/{id}/capabilities` with
  `{harnesses, providers, skill_count, agent_count}` (Swaygent
  `crates/swaygent-core/src/hub.rs:592-603`); Tidepool's handler takes
  `{capabilities: [...]}` with `deny_unknown_fields`
  (`src/api/hosts.rs:224-230`) and derives presence only from the WebSocket
  heartbeat (`src/sockets/host_ws.rs:105-112`, `248`). As read, **Swaygent
  cannot currently be dispatched to by Tidepool, and would show offline.**
  (Not run; inferred from the code.)

### 2.2 Options

| | (a) Host runs `tidetable tkrun run` | (b) Tidepool sends per-task prompts via `create_agent` | (c) Swaygent-native task loop |
|---|---|---|---|
| Claims, leases, gate, review, repair, commit | reused, already tested | Tidepool or the host must reimplement all of it | Swaygent reimplements it in Rust |
| Credential invariant | already enforced for agents (`tkrun.py:92`, `1234`) | the agent process gets the prompt only, but someone still has to claim/done and run the gate | must be re-proven |
| Code stays local | yes (`integration-guide.md:25-26`) | yes, but the pool must know about worktrees and branches | yes |
| Stop / kill | `tkrun stop|kill --repo` work without being the parent (`tkrun.md:147-150`) | `interrupt_execution` | new |
| Survives host restart | yes if spawned detached; pid file in `.tkrun/` (`tkrun.py:631`, `1995-1998`) | no: an execution is lost and recorded interrupted (`HUB_CAPABILITIES.md:103`) | new |
| Progress granularity | job-level now; per-task with a small TKRun addition (`--events`) | per task, natively | per task |
| New surface | 5 frames, 1 table, 4 routes | gate/claim engine in the pool | a second runner to maintain |

(c) becomes attractive only after TideTable's Rust core port (TideTable
`docs/rust-port.md`, phase 5.0), when Swaygent could link the runner instead of
spawning Python. Not before.

### 2.3 Recommendation: (a), as a typed job

Tidepool sends a **structured** `start_job` frame naming a job kind
(`tkrun`), a checkout and bounded options. The host builds a **fixed argv**
from it; the pool never sends a command line, a path or a secret. Keep
`create_agent` for what it is good at (one prompt, one harness), and fix it in
v2 to carry `cwd` and `branch`, which Swaygent's assignment parser already
accepts (`dispatch.rs:98-154` in Swaygent).

### 2.4 Changes to `docs/HOST_PROTOCOL.md`

v1 stays byte-for-byte valid: a host that never sends `hello` is a v1 host and
receives only v1 frames (`HOST_PROTOCOL.md:79-80` already refuses unknown
versions without closing). New sections:

1. **Negotiation.** First host frame on connect:
   `{"v":2,"type":"hello","versions":[1,2],"features":["jobs.tkrun","agent.cwd"],"agent":"swaygent/0.3.0"}`.
   Pool answers `{"v":2,"type":"hello.ack","version":2,"features":[...]}` (the
   intersection). Frames then carry the negotiated `v`.
2. **Jobs** (v2, feature `jobs.tkrun`), below.
3. **Checkouts report.** `POST /v1/hosts/{id}/checkouts` (host-authenticated
   like capabilities) lists `{name, directory, exists, git_head,
   tkrun_config_sha256, gate_steps[], tidetable: {server, token_present,
   slugs_verified[]}}`. The token itself is never sent.
4. **Swaygent conformance note**: the WebSocket is the only presence and
   dispatch path; the HTTP session routes Swaygent speaks today are not part of
   the protocol.

## 3. Protocol and API additions

### 3.1 Frames (v2)

Pool -> host:

```json
{"v":2,"type":"start_job","jobId":"<uuid>","kind":"tkrun",
 "project":{"slug":"chiton","server":"https://tk.example"},
 "checkout":"chiton-main",
 "options":{"hours":2,"minutes":0,"tasks":["1.3.2"],"jobs":1,"dryRun":false,"base":null}}
{"v":2,"type":"stop_job","jobId":"<uuid>","mode":"stop|kill"}
{"v":2,"type":"job_query","jobIds":["<uuid>"]}
```

Host -> pool:

```json
{"v":2,"type":"job_accepted","jobId":"…","pid":1234,"owner":"tkrun-2026-10-30@HOST",
 "worktree":"C:/…/Chiton-tkrun-2026-10-30","branch":"tkrun/2026-10-30"}
{"v":2,"type":"job_refused","jobId":"…","code":"checkout_not_allowed|tidetable_forbidden|tidetable_unreachable|job_already_running|tkrun_missing","detail":"…"}
{"v":2,"type":"job_output","jobId":"…","seq":17,"text":"<line>"}
{"v":2,"type":"job_event","jobId":"…","seq":18,"event":{"task":"1.3.2","phase":"claimed|gate|done|partial|failed|skipped"}}
{"v":2,"type":"job_completed","jobId":"…","status":"succeeded|failed|stopped|killed","exitCode":0,"summary":"<last log lines>"}
{"v":2,"type":"jobs_snapshot","jobs":[{"jobId":"…","state":"running|completed","pid":1234,"exitCode":null}]}
```

Rules, mirroring v1's execution rules (`HOST_PROTOCOL.md:88-91`, `142-153`):

- `jobId` is the idempotency key. A repeat `start_job` gets the existing
  job's `job_accepted` or `job_completed`, never a second process.
- `seq` is per job and lets the pool drop duplicate output after a reconnect
  (v1 does not deduplicate deltas, `HOST_PROTOCOL.md:108-109`; jobs are long
  enough to need it).
- The host sends `jobs_snapshot` right after `hello.ack` on every connect, so a
  job that kept running while the socket was down is re-adopted.
- `job_event` is optional; phase 1 sends only output and completion.

### 3.2 Routes (hub API, RFC 9457 errors as today)

| Route | Scope | Purpose |
|---|---|---|
| `GET /v1/projects/{id}/tidetable` | `projects:read` | binding plus live hub facts (repo_url, todo_path, default_branch, ready/in-progress counts) |
| `POST /v1/projects/{id}/jobs` | `runs:dispatch` | manual job: `{checkout, options, deliveryKey}`; refused `daemon_not_connected` when the host is offline (no queue) |
| `GET /v1/projects/{id}/jobs`, `GET /v1/jobs/{id}` | `projects:read` | job list and detail (status, output tail, owner, linked claims) |
| `POST /v1/jobs/{id}/stop` | `runs:dispatch` | `{mode: stop|kill}` |
| `GET /v1/projects/{id}/status` | `projects:read` | the one page: binding, checkouts and their hosts' presence, live jobs, TideTable in-progress claims with owners, next schedule fire |
| `POST /v1/hosts/{id}/jobs-grant`, `DELETE` same | operator | per host and project grant for `jobs.tkrun` (section 6) |
| `POST /v1/hosts/{id}/checkouts` | host credential | checkouts report (section 2.4) |

Existing scopes are reused (`src/entities/api_key.rs:55-71`). Jobs get their
own table rather than `runs`, because `runs.trigger_id` is `NOT NULL` and
`origin` is `event|manual` (`migrations/20261021000003_create_runs.up.sql:23`,
`28`); a schedule-fired job is neither a trigger run nor a step execution.

### 3.3 Manifest additions (`tidepool.json`)

```json
{
  "hosts": ["mikes-pc"],
  "tidetable": {"server": "https://tk.example", "slug": "chiton"},
  "checkouts": [{"name": "chiton-main", "host": "mikes-pc",
                 "directory": "C:/Users/hippo/git_repos/clawffice/Chiton"}],
  "schedules": [{"name": "nightly", "cron": "0 1 * * 1-5", "timezone": "America/New_York",
                 "checkout": "chiton-main", "options": {"hours": 6, "jobs": 1},
                 "overlap": "skip"}]
}
```

The manifest struct has no `deny_unknown_fields` (`configuration.rs:97-118`),
so today these keys would be **silently ignored**; the task that adds them must
make them first-class and validated (unknown checkout host, missing slug on the
hub, cron that does not parse: each a named field issue and a kept previous
revision, per 1.4.1).

### 3.4 Status, joined

`GET /v1/projects/{id}/status` answers "what is running" by joining three
sources, each the owner of its part: Tidepool jobs (host, pid, since), the hub's
`GET /api/<slug>/list` claims (owner, lease), and the hub's
`GET /api/<slug>/sessions` heartbeats. The join key is the TKRun owner string,
which in hub mode is `tkrun-<date>@<hostname>` (`tkrun.py:617`); the host
reports it in `job_accepted`. A claim whose owner matches no live job is shown
as "claimed outside Tidepool", not hidden.

## 4. Auth

| Component | Holds | Scope | Never holds |
|---|---|---|---|
| Operator | member session or API key | existing scopes | — |
| Tidepool pool | its own TideTable machine token, from env (e.g. `TIDEPOOL_TIDETABLE_TOKEN`) | `slugs` = every bound slug, `scopes = ["read"]` | any write token; `TK_API_TOKEN` |
| Host (Swaygent) | Tidepool host credential (keychain, already: Swaygent `hub.rs:61-70`) | its own host | anything hub-wide |
| Host (Swaygent) | one TideTable machine token **per machine**, in the OS keychain | `slugs` = projects this host has checkouts for, `scopes = ["read","write"]`, expiry 90 days (default in `tokens.py:110`) | `TK_API_TOKEN`; a token with `*` or extra slugs |
| `tidetable tkrun` process | that token via `TIDETABLE_TOKEN` in its env only | inherited | — |
| Agent, reviewer, repair agent | nothing | — | all four names (`tkrun.py:92-93`) |
| TideTable hub (VPS) | `TK_API_TOKEN` (break-glass, full access, never 403s: `serve.py:112-114`) | — | — |

Decisions:

- **Tokens are minted on the VPS by the operator** (there is no mint route or
  CLI; `tokens.mint` at `tokens.py:309` is a function) and installed on the
  host by hand (`swaygent pool tidetable-token set`, stdin only, like
  `pool enroll`). **Tidepool never carries a TideTable token over the wire.**
  A compromised pool can then misdirect jobs but cannot hand out write access.
- The token goes to the TKRun child through its environment, never `--token`
  (that shows in process listings; the TUI doc warns the same,
  `integration-guide.md:352-354`). The host must not export it into its own
  process environment, so harness sessions it starts never inherit it.
- Slug and scope are enforced by the hub: a GET needs `read`, a POST `write`,
  a mismatch is 403 (`serve.py:105-140`, `950-963`).
- **Gap found:** `/api/projects` and `/api/stream` carry no slug, so the slug
  gate is skipped (`serve.py:953`) and any valid machine token reads the whole
  registry (`serve.py:379-381`) and, unless it passes a slug filter, every
  project's events. A Chiton token can see every project's repo URL. TideTable
  should filter both by the token's slugs (task 1.7.7).

## 5. Failure modes

| Case | What happens | Design response |
|---|---|---|
| **Host offline mid-run** | TKRun is a local process; nothing about the socket stops it. Claims carry a lease (`lease_minutes` 480, `tkrun.md:113`). | Host spawns TKRun detached and never kills jobs on disconnect. Pool marks the job `host_offline` (not failed). On reconnect `jobs_snapshot` re-adopts it or reports completion read from the exit file. Offline > lease: the pool shows the claim as stale; TideTable lets anyone reclaim. |
| **Hub unreachable** | Preflight `GET /api/<slug>/ready` fails. Mid-run behaviour of TKRun when `tidetable start/done` fails was **not verified**. | Before start: `job_refused` `tidetable_unreachable`, nothing claimed. Mid-run: job stays running; the pool's status view marks hub facts stale with their age. Pool-side: activation of a binding fails closed when the hub cannot confirm the slug, keeping the previous revision. Schedules that fire while the hub is down are recorded `missed`. |
| **Duplicate dispatch** | Same `jobId` resent after a lost frame or pool restart. Two schedules or a manual click on top of a schedule. Two hosts on one slug. | `jobId` idempotency on the host; pool stores the job row before sending (as v1, `HOST_PROTOCOL.md:146-147`). A unique live-job key per **slug** in phase 1 (`job_already_running`); schedules use `overlap: skip` and delivery key `schedule:<name>:<fire-time>`. TKRun's pid file also refuses a second run in one checkout (`tkrun.py:1995-1998`). Across hosts, TideTable claims stop two runners taking one task, but TKRun's `touches` disjointness is per run, not cross-host, hence one job per slug. |
| **Run outlives its host** | The host app restarts or is uninstalled; Swaygent's session handles kill their tree on drop (Swaygent `dispatch.rs:1048-1049`), which would kill a child TKRun mid-task and leave the task `in_progress` with uncommitted work (`tkrun.md:172-173`). Host revoked while a job runs. | TKRun jobs are **not** session handles: spawned detached, recorded in host state (job id, pid, checkout, run dir). Stop and kill go through `tidetable tkrun stop|kill --repo`, which need no parentage. Revoking a host marks its live jobs `orphaned`; the pool cannot stop them, and says so with the checkout path and `tkrun kill` command for a person. |
| **403: token scoped to other slugs** | Hub answers `403 {"error":"forbidden for this project/scope"}` (`serve.py:958-963`). | Host preflight turns this into `job_refused` `tidetable_forbidden` naming the slug and the scope that failed; checkouts report shows `slugs_verified` so the pool flags it before anyone dispatches. Fix is a re-minted token; the pool never retries a 403. A read-only token fails on the first `start` POST the same way. |
| **Repo config changed under a schedule** | `.tidetable/tkrun.json` edited on base | The checkouts report carries its SHA-256; a change since the grant marks the checkout `needs_review` and schedules on it pause until an operator re-approves (section 6). |

## 6. Security

**What a host may run.** Only the job kinds it advertises, built as a fixed
argv: `tidetable --server <bound server> tkrun run <slug> --repo <checkout
directory> [--hours H] [--minutes M] [--task ID]... [--jobs N] [--dry-run]`.
The host validates each option (task ids against `^\d+\.\d+\.\d+$`, hours
0-12, jobs 1-4), resolves the checkout name against **its own local allowlist**
(Swaygent settings: checkout name -> directory -> slug), and refuses a name
not on it. The pool's manifest naming a directory is a request, not an
authority.

**What that actually permits.** TKRun executes the repo's gate commands and
`agent_commands` (`tkrun.md:93-123`), and agents "are not sandboxed beyond the
tool allowlists" (`tkrun.md:203-205`). So granting a TKRun job on a checkout
means: *whoever can push to that repo's base branch can run code on this
machine.* `host.execute` is not a sandbox (`HUB_CAPABILITIES.md:158-159`) and
neither is this grant.

**Chiton specifically.** Its agent command is
`python .tidetable/oarlock/oarlock-dispatch.py run ... --approve-all` with no
`opencode_permissions`, so TKRun's shell allow/deny lists do not constrain it
(`tkrun.md:114`), and the agent's CWD is the worktree (`tkrun.py:799`), where
it can edit that same script for the next task. Recommend
`"deny_edit": [".tidetable/**"]` in Chiton's config before an unattended run.

**Credential invariant.** TKRun drops the four names from agent processes
(`tkrun.py:92-93`, `1232-1236`). **Gap found:** gate steps run with the full
runner environment (`run_proc(cmd, cwd, ..., env=self.job_env or None)` at
`tkrun.py:1379`; `run_proc` merges `os.environ` and drops only when asked,
`tkrun.py:281-285`). A gate runs code the agent wrote (`cargo test`), so an
agent can exfiltrate `TIDETABLE_TOKEN` through a test. `merge_resolve`
(`tkrun.py:1867`) has the same shape. TideTable must pass
`drop=AGENT_ENV_DROP + agent_env_drop` to gate and merge-resolve processes
before any Tidepool-dispatched run is considered safe (task 1.7.6).

**What the operator must approve** (each recorded with actor and time):

1. `host.execute` for the host (exists; granted by an operator through
   `POST /v1/hosts/{id}/execution`, `src/api/hosts.rs:233-256`).
2. A `jobs.tkrun` grant for that host **and** project, pinned to the
   checkout's current `tkrun_config_sha256`; re-approval when it changes.
3. On the machine: adding the checkout to the host's local allowlist and
   installing the TideTable token. Two keys, two places: a pool admin alone
   cannot start code on a machine.
4. Each schedule, as part of the configuration revision that adds it.

## 7. Phasing

Proposed as two phases appended to `docs/todo-v1.json` (server-side work,
which is v1's scope), with cross-repo tasks prefixed by repository as the v2
file already does for client repos. Phase 1.7 is the smallest slice: Chiton,
the VPS hub, one host, one TKRun run. Tasks 1.7.1-1.7.5 are testable in CI with
no game API, no control plane and no real agent: a loopback `tidetable serve`
with a temporary registry and a minted token, and the reference host agent.

```json
{
  "phases": [
    {
      "id": "1.7",
      "title": "Projects meet TideTable: one TKRun job end to end",
      "goal": "A Tidepool project bound to a TideTable slug can ask one connected host to run `tidetable tkrun run <slug> --repo <checkout>`, see it start, stream its output, stop it, and record its exit, with no TideTable token on the pool wire and none reachable by an agent or a gate.",
      "prerequisites": ["1.4"],
      "tasks": [
        {"id": "1.7.1", "title": "Host protocol v2: hello and job frames",
         "status": "pending",
         "goal": "Add hello negotiation and the start_job, stop_job, job_query, job_accepted, job_refused, job_output (with seq), job_event, job_completed and jobs_snapshot frames. v1 hosts keep working unchanged. create_agent v2 carries optional cwd and branch from the resolved environment.",
         "touches": ["docs/HOST_PROTOCOL.md", "src/services/host_protocol.rs", "src/sockets/host_ws.rs", "src/services/runs.rs"],
         "success": ["A v1 host that never sends hello receives only v1 frames and its tests pass unchanged",
                     "A resent start_job with the same jobId yields the existing job, never a second",
                     "Duplicate job_output seq values are dropped",
                     "create_agent v2 carries the directory dispatch already computes"],
         "tests": "Unit tests per frame and for negotiation; replay and reconnect tests against an in-process host.",
         "prerequisites": [], "complexity": "High", "agent": "pro_dev_agent"},
        {"id": "1.7.2", "title": "TideTable binding and checkouts in the manifest",
         "status": "pending",
         "goal": "tidepool.json gains a validated `tidetable {server, slug}` block and named `checkouts {name, host, directory}`. Activation confirms the slug through the hub's GET /api/projects with the pool's read-only token and fails closed, keeping the previous revision, when the slug is unknown or the hub unreachable. GET /v1/projects/{id}/tidetable shows the binding plus live repo_url, todo_path and default_branch.",
         "touches": ["src/services/configuration.rs", "src/services/tidetable_client.rs", "src/entities/checkout.rs", "src/api/projects.rs", "src/config.rs"],
         "success": ["An unknown slug or checkout host is a named field issue and the previous revision stays active",
                     "The pool's TideTable token is read-only and never appears in a response, log or error",
                     "repo_url and todo_path are read from the hub, not stored"],
         "tests": "Unit tests for each validation rule; integration test against a loopback tidetable serve.",
         "prerequisites": [], "complexity": "Medium", "agent": "mid_dev_agent",
         "security_notes": "The pool holds only a read-scoped TideTable token. Never TK_API_TOKEN."},
        {"id": "1.7.3", "title": "Jobs: record, manual dispatch, stop, grant",
         "status": "pending",
         "goal": "A jobs table and POST /v1/projects/{id}/jobs, GET list and detail, POST /v1/jobs/{id}/stop, plus the per host and project jobs.tkrun grant pinned to the checkout's tkrun config hash. One live job per slug. Offline host refuses with daemon_not_connected and queues nothing.",
         "touches": ["migrations/", "src/entities/job.rs", "src/services/jobs.rs", "src/api/jobs.rs", "src/api/hosts.rs"],
         "success": ["A second dispatch for a slug with a live job is refused with job_already_running",
                     "Dispatch without the jobs.tkrun grant is 403 naming the grant",
                     "A host_offline job becomes running again after jobs_snapshot",
                     "The job row exists before the frame is sent"],
         "tests": "Unit tests for state transitions; integration tests for offline dispatch, duplicate dispatch, grant refusal and restart replay.",
         "prerequisites": ["1.7.1", "1.7.2"], "complexity": "High", "agent": "pro_dev_agent"},
        {"id": "1.7.4", "title": "Reference host agent runs tkrun jobs",
         "status": "pending",
         "goal": "crates/tidepool-host-agent speaks v2 and implements jobs.tkrun: local checkout allowlist, fixed argv, token from a local file or env passed only to the child, preflight GET /api/<slug>/ready mapping 401/403/unreachable to job_refused codes, detached spawn, stop and kill via tkrun stop|kill, jobs_snapshot on reconnect.",
         "touches": ["crates/tidepool-host-agent/"],
         "success": ["A checkout not on the local allowlist is refused before anything spawns",
                     "The child receives TIDETABLE_TOKEN and the agent's own environment is never printed or sent",
                     "Killing the agent's socket does not stop the child, and reconnecting re-adopts it"],
         "tests": "Integration tests with a stub `tidetable` executable that records argv and env and prints scripted lines.",
         "prerequisites": ["1.7.1"], "complexity": "High", "agent": "pro_dev_agent"},
        {"id": "1.7.5", "title": "End-to-end slice in CI",
         "status": "pending",
         "goal": "One test: pool, reference agent and a loopback `tidetable serve` with a temporary registry holding slug chiton-fixture and a token minted with tokens.mint_token for that slug. Dispatch a `--dry-run` job and a `--task` job against a tiny fixture repo whose agent_commands is a script that prints TKRUN_RESULT: DONE. Also the 403 case with a token scoped to another slug.",
         "touches": ["tests/e2e_tkrun_job.rs", "tests/fixtures/tkrun/"],
         "success": ["The task job ends succeeded and the hub shows the task done with the job's owner",
                     "A token scoped to another slug ends in job_refused tidetable_forbidden and claims nothing",
                     "Nothing in the test touches the game API"],
         "tests": "This task is the test.",
         "prerequisites": ["1.7.3", "1.7.4"], "complexity": "High", "agent": "pro_dev_agent"},
        {"id": "1.7.6", "title": "TideTable: gate and merge-resolve drop runner credentials",
         "status": "pending",
         "goal": "Repository TideTable. Pass AGENT_ENV_DROP plus agent_env_drop to gate and merge_resolve processes, as already done for agents.",
         "touches": ["TideTable:tidetable/tkrun.py", "TideTable:tests/"],
         "success": ["A gate step that prints its environment shows none of TIDETABLE_TOKEN, TIDETABLE_SERVER, TASKERKEEPER_TOKEN, TK_API_TOKEN"],
         "tests": "Unit test with a gate step that dumps env.",
         "prerequisites": [], "complexity": "Low", "agent": "basic_dev_agent",
         "security_notes": "Blocks 1.7.9: gates run agent-written code."},
        {"id": "1.7.7", "title": "TideTable: whoami and slug-filtered projects and stream",
         "status": "pending",
         "goal": "Repository TideTable. GET /api/whoami returns the presented machine token's slugs, scopes and expiry (never the secret). /api/projects and /api/stream return only the token's slugs for machine tokens.",
         "touches": ["TideTable:tidetable/serve.py", "TideTable:tests/"],
         "success": ["A chiton-only token lists only chiton in /api/projects", "whoami lets a host fill slugs_verified without a write"],
         "tests": "Route tests for each token shape.",
         "prerequisites": [], "complexity": "Low", "agent": "mid_dev_agent"},
        {"id": "1.7.8", "title": "Swaygent: /ws/host v2 and tkrun jobs",
         "status": "pending",
         "goal": "Repository Swaygent. Replace the HTTP session-polling routes with the documented WebSocket (presence, v1 create_agent with cwd, v2 jobs.tkrun), send capabilities in Tidepool's shape, keep the TideTable token in the keychain, spawn tkrun detached and re-adopt it after a restart.",
         "touches": ["Swaygent:crates/swaygent-core/src/hub.rs", "Swaygent:crates/swaygent-core/src/hub/", "Swaygent:crates/swaygent-cli/src/cli.rs"],
         "success": ["Swaygent shows connected in GET /v1/hosts", "A capability report is accepted by Tidepool",
                     "Restarting Swaygent during a job does not stop tkrun and the job is re-adopted"],
         "tests": "Headless tests over the existing Http and SessionRunner seams plus a scripted socket.",
         "prerequisites": ["1.7.1"], "complexity": "Very High", "agent": "flagship"},
        {"id": "1.7.9", "title": "Chiton on the VPS hub, one real task",
         "status": "pending",
         "goal": "Operator mints a chiton read/write token, installs it on the Swaygent host, binds the Tidepool project to slug chiton, adds the checkout, grants jobs.tkrun and dispatches `--task <one ready id>`.",
         "touches": ["docs/memory.md"],
         "success": ["The task is claimed and closed on the hub by owner tkrun-<date>@<host>", "The job record shows the exit code and the branch",
                     "The agent transcript in .tkrun/<date>/ contains no TideTable credential"],
         "tests": "Manual acceptance; a person reviews the branch before merging.",
         "prerequisites": ["1.7.3", "1.7.6", "1.7.8"], "complexity": "Medium", "agent": "mid_dev_agent",
         "human_look": "Needs the VPS, a real token and a real agent run."}
      ]
    },
    {
      "id": "1.8",
      "title": "Schedules and one status page",
      "goal": "Schedules fire jobs without a person, and one Tidepool view answers what is bound, what is running where, and what TideTable says about it.",
      "prerequisites": ["1.7"],
      "tasks": [
        {"id": "1.8.1", "title": "Schedules", "status": "pending",
         "goal": "Manifest `schedules` with cron, timezone, checkout, options and overlap skip. Each fire has delivery key schedule:<name>:<time>; an offline host or unreachable hub records a missed fire and queues nothing; a checkout whose tkrun config hash changed pauses its schedules.",
         "touches": ["src/services/schedules.rs", "src/services/configuration.rs", "migrations/"],
         "success": ["A fire is dispatched at most once per key across pool restarts", "A missed fire is recorded with its reason"],
         "tests": "Clock-injected unit tests; restart test.", "prerequisites": [], "complexity": "High", "agent": "pro_dev_agent"},
        {"id": "1.8.2", "title": "Project status join", "status": "pending",
         "goal": "GET /v1/projects/{id}/status joins jobs, host presence, the hub's list claims and sessions by owner string, marking hub facts with their age and claims with no live job as outside Tidepool.",
         "touches": ["src/services/project_status.rs", "src/api/projects.rs"],
         "success": ["A claim from a manual tkrun on another machine is shown, labelled outside Tidepool", "Hub unreachable returns the pool's facts with a stale marker, not an error"],
         "tests": "Integration tests with a loopback hub.", "prerequisites": [], "complexity": "Medium", "agent": "mid_dev_agent"},
        {"id": "1.8.3", "title": "TideTable: tkrun --events and run id", "status": "pending",
         "goal": "Repository TideTable. `tkrun run --events PATH --run-id ID` writes one JSON line per task transition and posts the run id in its hub heartbeat, so hosts can send job_event and the hub can show which job holds a claim.",
         "touches": ["TideTable:tidetable/tkrun.py", "TideTable:docs/tkrun.md"],
         "success": ["Every claim, gate result and outcome appears once in the events file"],
         "tests": "Runner tests with stub agents.", "prerequisites": [], "complexity": "Medium", "agent": "mid_dev_agent"}
      ]
    }
  ]
}
```

Control plane follow-through (v2, no new task): 2.1.5 gains the binding,
checkouts and schedules editor; 2.1.7 gains jobs; 2.1.3 gains the checkouts
report and the `jobs.tkrun` grant.

## 8. Open questions

Decided 2026-10-30: all six recommendations below are accepted as written.
Item 3's `opus/1.2.5` question is still open (section 9).

1. **Swaygent transport: WebSocket, or does Tidepool add HTTP polling?**
   Recommend the WebSocket. It is the documented protocol
   (`HOST_PROTOCOL.md:5-6`), presence already depends on it, and the polling
   `sessions/next` shape implies a queue, which the no-queue rule forbids
   (`HUB_CAPABILITIES.md:101`). Swaygent's own memory expects to adapt
   (Swaygent `docs/memory.md:13-14`).
2. **Who mints and installs the per-host TideTable token?** Recommend: the
   operator on the VPS, installed on the host by hand; Tidepool never handles
   it. The alternative (a TideTable admin route Tidepool calls) puts a
   hub-admin credential in the pool.
3. **Swaygent main is frozen: where does 1.7.8 land?** Recommend a branch
   `feat/pool-ws-jobs` off Swaygent main, with the reference agent (1.7.4)
   carrying phase 1 until it merges. Please also confirm which branch you meant
   by `opus/1.2.5` (section 9).
4. **One live TKRun job per slug, or per checkout?** Recommend per slug for
   now; TKRun's `touches` check does not span machines.
5. **Phase placement: 1.7/1.8 in `todo-v1.json`, or a v2 phase?** Recommend
   v1: it is server-side and protocol work, and v2's rule that no v2 task
   starts before v1 completes (`todo-v2.json:55`) would block it behind 1.6.4,
   which still needs a human look.
6. **Should Tidepool ever register new TideTable slugs?** Recommend no for
   now: the registry is a file on the VPS and stays TideTable's; Tidepool binds
   only to slugs the hub already lists.

## 9. Not verified, and where the brief differed from the code

- **Swaygent branch.** No branch `opus/1.2.5` exists in the Swaygent repo
  (`Zoid/examples/swaygent`, `git branch -a`), and there are no
  `swaygent-opus-1.2.5` or `swaygent-1.2.8` worktrees (worktrees present:
  `swaygent` on main at 7c0d6fc, `Swaygent-tui-mouse`, `Swaygent-win-pkg`).
  Commit 11a7b04 is on main and is Swaygent's own task 1.2.5, "sync --from and
  drift resolution", unrelated to Tidepool's host protocol. Swaygent's pool
  work is its `docs/todo-v2.json` tasks 2.1.1-2.1.6 (2.1.3 and 2.1.6 are
  `needs_human_look`). This document describes Swaygent from main.
- **Grant naming.** `HUB_CAPABILITIES.md:88` says `hub.execute` "granted from
  the machine"; the protocol and code say `host.execute`
  (`HOST_PROTOCOL.md:54`, `src/api/hosts.rs:409`) and the grant is an operator
  route (`src/api/hosts.rs:233-256`). This document uses `host.execute`.
- **Chiton on the hub.** The repo's `deploy/registry.json` lists only
  `tidetable`; the VPS registry entry for `chiton` and how its token (if any)
  was minted were not inspected. Chiton also has a local `docs/todo.json`; the
  hub copy is taken as the truth and the two can drift.
- **TKRun when the hub drops mid-run** was not traced; the failure-mode row
  says what Tidepool does, not what TKRun does.
- **Swaygent vs Tidepool incompatibility** (section 2.1) is read from code, not
  observed against a running pool.
- No code was run, built or tested; nothing on the VPS was touched.
