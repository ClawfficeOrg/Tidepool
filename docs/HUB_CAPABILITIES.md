# HUB_CAPABILITIES.md — Paseo Hub, mapped to Tidepool

> Read on 2026-10-19 from <https://paseo.sh>, its `/docs`, `/hub` and the
> `docs/hub/*` pages (overview, concepts, daemons, workflows, triggers,
> configuration, activity, api, self-hosting, security).
>
> **Purpose.** Paseo Hub is the closest thing to the layer Tidepool needs above
> the game server, so it is read for product shape and nothing else. This file
> records each capability that was read, what Tidepool's equivalent is, and the
> task that builds it. It is the task plan's reference, not a specification:
> the specification is the plan (`docs/todo-v1.json`, `docs/todo-v2.json`).
>
> **Paseo is inspiration only.** No Paseo code, schema, configuration format,
> database or wire protocol is copied or ported. Tidepool does not speak the
> Paseo protocol, does not read a Paseo home directory, and does not talk to a
> Paseo daemon. Where Paseo names a concept, Tidepool defines its own word for
> it below.

---

## 1. What Paseo Hub is

Paseo is an agentic development environment: agents run on your machines, and
desktop, mobile, web and CLI clients let you work from anywhere. A **daemon**
runs agents on one machine. **Hub** is the optional layer above the daemons:
you register your daemons with it, and it gives them capabilities they do not
have on their own.

What Hub adds, in Paseo's own words:

- Agents that start on their own, from activity in GitHub, Slack and Discord.
- Triggers you can keep in a repository and deploy from the CLI.
- A record of everything that arrived, what it matched and what ran.
- One place for a team to see all of it.

It ships no agent of its own: it runs the providers, subscriptions and skills
already configured on the daemons. It is offered self-hosted (open source) and
hosted (free tier and a paid per-seat tier).

## 2. What Tidepool is, and how it differs

Tidepool is the Clawffice suite's hub **and** its game server. That is the one
structural difference that matters, and it is deliberate: the pool is one
binary with one palette and one API surface, a Talo-compatible game backend
(auth, players, leaderboards, saves, channels, analytics, feedback) with the hub
layer beside it. Everything in `todo-v1.json` is additive to that surface; no
hub task changes an existing game endpoint.

Differences worth keeping in view while reading the mapping:

| | Paseo Hub | Tidepool |
|---|---|---|
| Deployment | self-hosted or hosted, with paid seats | self-hosted first; hosted is deferred to v3 |
| Storage | embedded database or PostgreSQL | MySQL, per the repo's standing decision (recorded in `docs/memory.md`) |
| Machine daemon | the Paseo daemon, already built and shipped | the pool host protocol (task 1.2.5), implemented by Swaygent, the suite's host |
| Clients | desktop, mobile, web, CLI, TypeScript SDK, MCP | Svelte control plane, ratatui console, Zoid reference client, TypeScript/Rust/Python SDKs and a CLI (`todo-v2.json`) |
| Rooms | not a Hub feature | chatrooms and an opt-in session relay are Tidepool's own surface (phase 1.5) |
| Federation | not offered | Rhumb, pool-to-pool, deferred to v3 |
| Design | Paseo's own | clawffice-design: tide hue, water current, rock-pool creature |

## 3. Capability mapping

Each row is one thing Hub does, the Tidepool equivalent, and where it is built.
Task ids are `todo-v1.json` unless marked `v2.`

### 3.1 The pieces

| Paseo Hub | Tidepool | Task |
|---|---|---|
| A **connection** lets Hub receive events from GitHub, Slack or Discord | `connections`: one per provider account, with its own transport, verification and resource identity, behind a provider trait | 1.3.1 |
| A **daemon** is a registered machine running the Paseo daemon | a **host**: one enrolled machine, immutable id, friendly slug, its own relationship credential | 1.2.1, 1.2.2 |
| A **project** groups one configuration with the connections and daemons it uses | a **project**, holding one active configuration revision | 1.4.1 |
| An **environment** names where a step runs: a daemon, its working directory and an optional worktree | the same shape, with the same three parts | 1.4.5 |
| A **trigger** says which provider event can start a workflow and which events are allowed through | the same, authorable as a form or as YAML, with the same checks | 1.4.2, 1.4.3 |
| A **workflow** is the ordered set of steps that runs after a trigger matches | the same: ordered steps, conditions, finite outputs, deadlines | 1.4.4 |
| A **step** starts one agent execution with its own prompt, agent selection, credentials, reply capabilities and limits | the same, including declared output capabilities with a maximum and a required flag | 1.3.3, 1.4.4 |
| A project has one active **configuration revision** at a time | the same, immutably stored, with the previous revision kept active when a new one fails validation | 1.4.1 |

### 3.2 Identity and access

| Paseo Hub | Tidepool | Task |
|---|---|---|
| An **organization** owns everything a credential can reach | one organization per pool in v1; multi-tenancy deferred | 1.1.1 |
| The dashboard has operator accounts; unattended installs bootstrap the first operator from the environment | the same, with the password-change-on-first-use rule | 1.1.1 |
| Seats are Hub members and pending invitations | members with roles (owner, admin, member, viewer) and expiring single-use invitations | 1.1.2 |
| **API keys** with selectable scopes, secret shown once, prefix and last-used retained, revocable | the same, with the same five scopes | 1.1.3 |
| A **daemon enrollment token**, short-lived and single-use, minted by the API key; the daemon then keeps its own credential | the same two-step exchange, with the credential stored only as a hash | 1.2.2 |
| `hub.execute` is a separate permission, granted from the machine, defaulting to no | the same, so enrolling a host never, by itself, lets the pool run agents on it | 1.2.4 |
| API failures use RFC 9457 problem details with `code` and `requestId` | the same shape for every hub route, with a documented code taxonomy | 1.1.4 |

### 3.3 From event to agent

| Paseo Hub | Tidepool | Task |
|---|---|---|
| A provider sends an event; Hub verifies it and identifies its project resource | the same, per provider transport | 1.3.2, 1.3.3, 1.3.4 |
| Hub evaluates triggers and their filters, including the required `from_users` allowlist | the same, and a trigger without a non-empty `from_users` is a hard rejection | 1.4.2 |
| A matching trigger creates a workflow run from the active revision | the same, with one run per delivery key | 1.4.6, 1.4.7 |
| A step with a false `if` is skipped; a true one starts on its daemon | the same, over inputs, values and earlier step output | 1.4.4, 1.4.6 |
| Hub records replies, structured output, status and completion | the same, in the activity log and the run record | 1.4.7 |
| A later step sees the earlier step's output | the same, and only earlier steps are readable | 1.4.4 |
| Delivering to an **offline daemon** fails with `daemon_not_connected`; nothing is queued | the same code, the same no-queue rule, so the trigger must fire again | 1.2.3, 1.4.6 |
| Hub owns creation, reconnect recovery, output observation and completion for the runs it started; a lost create response or a restart **resends the same create intent with the same execution id** and the daemon answers with the existing agent | the same idempotent replay, specified in the host protocol | 1.2.5, 1.4.6 |
| An agent that closed or errored is recorded as **interrupted**, never silently restarted | the same | 1.4.6 |
| **Continuation**: messages in the same Slack or Discord thread, or events on the same GitHub issue or PR, continue the existing agent, and a new prompt **steers** it if it is busy | the same, with each arrival keeping its own output limits and completion status | 1.4.7 |

### 3.4 Configuration as code

| Paseo Hub | Tidepool | Task |
|---|---|---|
| Configuration lives in the repository as `.paseo/hub.yml` plus `.paseo/workflows/*.yml` and partials | the same idea in Tidepool's own format and paths, stored as the exact authored files | 1.4.1, 1.4.4 |
| `deploy --dry-run` validates; `deploy` validates everything first, then installs each trigger | a validate endpoint that changes nothing, and an install endpoint that changes nothing on failure | 1.4.2, 1.4.3 |
| Activation resolves every reference (repo, workspace, guild, daemon, named agent, partial) and rejects an unresolvable one | the same, naming the field | 1.4.1, 1.4.4 |
| A failed activation keeps the previous active revision and shows the validation error | the same | 1.4.1 |
| Prompt partials are literal files, resolved at the exact commit | the same, with the bundle caps below | 1.4.4 |
| Bundle limits: 100 files, 512-character paths, 1,000,000 characters per file, 5,000,000 bytes of partials | the same limits | 1.4.4 |
| A step may route from a classifier whose finite output is bounded by `enum` or `const` | the same rule: only a finite output can bound a later choice | 1.4.4 |

### 3.5 Operator surfaces

| Paseo Hub | Tidepool | Task |
|---|---|---|
| A dashboard: home, daemons, projects, configuration, activity, apps, keys | the Svelte control plane: hosts, connections, projects and the configuration editor, triggers, runs and activity, rooms, members and keys, settings | 2.1.\* |
| The agent selectors ask the chosen **daemon** which providers, models, modes and thinking options it offers, and only those are offered; an authored value the daemon no longer reports is **marked unavailable without being replaced** | the same, and the same refusal at save time naming the field | 1.2.4, 2.1.6 |
| A terminal-facing way to run all of it | the ratatui console, with a headless mode that prints the CLI's JSON | 2.2.\*, 2.2.5 |
| Mobile and desktop apps | the Zoid reference client: menu bar and taskbar, sessions, rooms, runs | 2.3.\* |
| A CLI (`paseo hub …`) and a TypeScript SDK | the `tidepool` CLI and TypeScript, Rust and Python SDKs with one conformance suite | 2.0.2, 2.0.3, 2.0.4, 2.0.5, 2.0.6 |
| MCP tools so an agent inside Paseo can orchestrate other agents | not in v1 or v2; the CLI and the SDKs are the automation surface. Revisit when a harness needs it | — |

### 3.6 Running it

| Paseo Hub | Tidepool | Task |
|---|---|---|
| Self-host with one command, embedded database by default, PostgreSQL for a durable deployment | one compose stack with MySQL and Redis; the embedded-database option is explicitly not offered | 1.6.1, 1.6.2 |
| Migrations run automatically at startup; Hub does not listen when a migration fails | the same, forward-only | 1.6.2 |
| A public origin setting, and provider callback URLs that must change with it | the same, with the control plane showing which URLs are affected | 1.6.1, 2.1.4 |
| Provider apps configured in the dashboard, or entirely from the environment, with the environment taking precedence and being labelled | the same precedence and labelling | 1.3.1 |
| Bootstrap the first operator from environment variables for an unattended deployment, then remove the password | the same | 1.1.1, 1.6.4 |
| Back up the database before an upgrade; upgrades are forward-only | the same, documented and rehearsed | 1.6.2, 1.6.4 |
| Activity and daemon state visible enough to answer "is this working?" | hub metrics: runs by outcome, dispatch failures by code, connected hosts, connected viewers, webhook deliveries accepted, deduplicated and rejected | 1.6.3 |

### 3.7 Security boundaries Paseo states, and Tidepool keeps

- **A required allowlist, with no default**, because a stranger's comment on a
  public issue must not start an agent on someone's machine — task 1.4.2.
- **An allowlist is one layer and does not make a permitted account
  trustworthy**, and does not make prompt injection harmless — recorded in
  1.4.2's security notes and in `docs/security` guidance as the plan writes it.
- **Everyone who can change the active configuration can choose which
  connections, hosts and agent capabilities a project uses**, so the
  configuration repository is itself a protected asset — 1.4.1, and stated in
  `docs/API_CONTRACT.md` by task 2.0.1.
- **Credentials are never implied.** In Paseo a step holds GitHub authority
  only when it declares a `github` block. Tidepool's equivalent rule: a step's
  capabilities are only what it declares, and an output capability must be
  declared to be granted — 1.4.4.
- **Enrolling is not executing.** Identity and presence are granted by
  enrollment; running agents is a separate grant made by the machine — 1.2.4.
- **A permission grant is not a sandbox.** Stated in 1.2.4's security notes so
  no task later mistakes the grant for isolation.

## 4. What Tidepool does not take from Paseo

- **The wire protocol, schemas, database and configuration format.** Read for
  shape only. Tidepool's configuration is its own, its API is its own, and its
  clients speak only Tidepool.
- **The hosted tier and its pricing model.** Tidepool ships self-hosted; a
  hosted multi-tenant pool is deferred to v3 and is a product decision, not a
  task.
- **A bundled agent.** Like Hub, the pool ships no agent of its own. It runs
  what a host reports it can run (1.2.4); in the suite that is Oarlock and the
  ACP harnesses.
- **Telemetry.** Paseo's stated philosophy is no telemetry, tracking or forced
  login. Tidepool keeps that: analytics in the game API is a per-game
  developer surface, and the hub sends nothing anywhere by default. The one
  path that moves session data off a machine is the opt-in relay (1.5.5), off
  by default and per-session.
- **Embedded-database mode.** Recorded as a decision in `docs/memory.md`
  (task 1.6.2) rather than silently skipped.

## 5. Naming, and why the plan keeps neutral words

The plan deliberately uses neutral hub vocabulary — host, trigger, workflow,
run, activity, connection — because those words appear verbatim in the API,
the OpenAPI document and every SDK, and a client author should not have to
learn a metaphor to call an endpoint.

Two Tidepool-flavoured names already exist and are used:

- **Rhumb** — the pool-to-pool federation link (clawffice-design, Tidepool's
  `parts`). Deferred to v3 with the rest of federation.
- **Pool** — the Tidepool deployment itself, which is why the plan says *the
  pool* and not *the hub* wherever the running thing is meant.

A flavour pass over the remaining words was considered and is **not** in the
plan, because it is a decision for a human, not an agent: renaming `trigger`,
`workflow` and `run` after the SDKs ship is a breaking change to three
languages. If it is wanted, it should be decided before task 2.0.1 freezes the
contract. Suggestions that fit the water family, none of them approved:
`hook` for trigger, `rig` for workflow, `catch` for run, `logbook` for
activity. `Trawl`, `Tacklebox`, `Fathom` and `Capstan` were scanned and dropped
as names already (clawffice-design README, "Open → Name checks") and are not
available.

## 6. Reading list, with what each page contributed

| Page | Used for |
|---|---|
| `/hub` (marketing) | The value proposition and the hosted tiers; why Hub is a separate service from the daemon |
| `/docs/hub` (overview) | What Hub adds, and the triggers-in-your-repository idea |
| `/docs/hub/concepts` | The piece inventory (connection, daemon, project, environment, trigger, workflow, step), the event-to-agent flow, activation, and the security boundary about the configuration repository |
| `/docs/hub/daemons` | Enrollment, the two identities (immutable id, friendly slug), `hub.execute`, statuses, what Hub owns for a run, and the offline-daemon rule |
| `/docs/hub/triggers` | Events, filters, the required allowlist, save-time agent checking, continuation, and reply capabilities |
| `/docs/hub/workflows` | Steps, environments, named versus inline agents, classifier routing with finite outputs, prompt and partials, conditions, deadlines |
| `/docs/hub/api` | Scopes, the problem-details shape, trigger validate and install, project list, manual dispatch with delivery keys, enrollment tokens, and key audit |
| `/docs/hub/self-hosting` | Local data, the public origin, PostgreSQL versus embedded, app configuration and its precedence, bootstrap from the environment, compose, HTTPS, and backup before upgrade |
| `/docs` and `/docs/orchestration` | The client spread (desktop, mobile, web, CLI, SDK), profiles, schedules and heartbeats — the last two are inspiration for the deferred work, not v1 or v2 tasks |
