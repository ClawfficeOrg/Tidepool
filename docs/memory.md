# Playzoid-Server — Memory Index

Master memory file. High-level architectural decisions that apply across
milestones live here; per-milestone detail lives in the versioned files below.
Do not rewrite history — supersede entries with new dated ones.

## Versioned Memory Files

| Milestone | File | Status |
|-----------|------|--------|
| v0 — Foundation, Auth, Leaderboards | (this file) | 🔄 Active |

## Hard Rules

1. **Never delete files** without explicit instruction. `mv` to `/tmp/`
   (recycle) instead of `rm`.
2. **Never push directly to `main`** — all work goes through PRs
   (see `AGENTS.md`).
3. **No secrets in code or docs** — credentials live in `.env` / env vars.

## Architectural Decisions (Cross-Cutting)

### Argon2id for password hashing
Established 2026-04-30. All `password_hash` columns store PHC-format
Argon2id strings; bcrypt is not a dependency. Tuning parameters live in
`Config`. Full rationale: dated entry below.

### Docker dev stack is the live integration test target
Established 2026-05-27. Integration tests are `#[ignore]`d by default and run
against `docker compose -f config/docker-compose.dev.yml` (MySQL + Redis).
Override with `DATABASE_URL` / `REDIS_URL`.

### MySQL is canonical; PostgreSQL out of scope until 1.0.0
sqlx uses the `mysql` feature only. Migrations are MySQL-flavoured SQL.

---

# Decision Log

> Long-term decision record for the Playzoid-Server crate. New decisions are
> appended at the bottom with a date stamp, the alternatives considered, and
> the reasoning that won.

This file was previously `memory/projects/playzoid-server/project.md`; it moved
to `docs/memory.md` as part of the standard repo layout.

---

## Decisions

### 2026-04-30 — Use Argon2id (not bcrypt) for password hashing

- **Context:** Phase 0.2 introduces real authentication (`POST /auth/login`,
  `POST /auth/register`). The original `docs/PLAN.md` mentioned both bcrypt
  and Argon2 as candidates without picking one.
- **Options considered:**
  - `bcrypt` — battle-tested, simple, but capped at 72-byte passwords and
    older against modern GPU/ASIC attacks.
  - `argon2` (Argon2id variant) — winner of the 2015 Password Hashing
    Competition, memory-hard, current OWASP recommendation.
  - `scrypt` — also memory-hard but with less ecosystem momentum than
    Argon2id today.
- **Decision:** Use **Argon2id** via the `argon2` crate (already pinned in
  `Cargo.toml`). All `password_hash` columns store the PHC-format
  `$argon2id$...` string; verification uses `argon2::PasswordVerifier`.
- **Consequences:**
  - The `players.password_hash` column is sized `VARCHAR(255)` to fit the
    PHC encoding plus salt + hash.
  - Tuning parameters (`m_cost`, `t_cost`, `p_cost`) live in `Config` so we
    can ratchet them without a migration.
  - Bcrypt is **not** added as a dependency; legacy hashes are not in scope.

---

### 2026-05-27 — Docker dev stack is the live integration test target

- **Context:** Integration tests needed a live MySQL + Redis. The project already
  had a Docker Compose dev stack confirmed healthy from the Phase 0.2 smoke
  test session.
- **Decision:** Integration tests default to
  `mysql://playzoid:password@127.0.0.1:3306/playzoid_dev` and
  `redis://127.0.0.1:6379` (the Docker dev stack). Both URLs are overrideable
  via `DATABASE_URL` / `REDIS_URL` env vars for CI or alternate environments.
- **Pattern:** Tests are marked `#[ignore]` so `cargo test` skips them;
  `cargo test -- --ignored` runs the full suite when Docker is up.
- **Consequences:** No in-memory test DB; tests use UUID-prefixed unique
  usernames to avoid conflicts on repeated runs.

---

### 2026-08-25 — Game saves are own-only reads returning full blobs

- **Context:** Task 0.3.6 adds `GET /saves/{player_id}`. Profile reads
  (`GET /players/{id}`) are free reads for any authenticated user, but game
  saves are private per-player game state, and Talo's `game-saves.getAll`
  returns full save blobs.
- **Options considered:**
  - Free-read (any authenticated user may list anyone's saves) — matches
    profile reads but leaks private game state.
  - Own-only read with 403 for cross-player — treats saves like leaderboard
    entries; private by default.
  - Metadata-only list (no blobs) — smaller payloads but breaks parity with
    `game-saves.getAll` and forces an extra fetch per slot.
- **Decision:** Saves are **own-only** — a request whose `{player_id}`
  differs from the JWT identity returns 403 before any SQL. Lists return the
  full `SaveView` objects **including blobs**, newest first
  (`created_at DESC, updated_at DESC`). No pagination for this Small task; a
  player with no saves gets 200 `[]`. A parent may *not* read a subaccount's
  saves (open question pending product input).
- **Consequences:**
  - `/saves` gets no Redis cache (matches the leaderboard services; DB-only).
  - No internal `id` is selected in the list query — the ORDER BY uses
    `updated_at` explicitly as the tie-break, keeping `SaveRow` free of dead fields.
  - Single-save retrieval (0.3.8) and creation (0.3.7) reuse the same
    `SaveView` shape.

---

### 2026-08-25 — Save creation is own-only with an optional Talo-shaped `playerId`

- **Context:** Task 0.3.7 adds `POST /saves`. Talo's verified `CreateSaveRequest`
  carries a `playerId` field, but the 0.3.6 decision made saves private
  per-player (own-only reads).
- **Options considered:**
  - Trust a required `playerId` from the body — lets any caller create saves
    for arbitrary players.
  - Ignore `playerId` entirely — breaks Talo SDK client compatibility (they
    send it).
  - Optional `playerId` validated against the JWT — accept when it matches,
    403 when it differs, default to the JWT identity when absent.
- **Decision:** `playerId` stays optional and is validated against the JWT
  identity (403 on mismatch); `None` defaults to the caller, so creation can
  never target another player. Combined serialized `save` + `metadata` is
  capped at 32 KiB (`MAX_SAVE_BYTES`) — under InnoDB's 65,535-byte row limit
  and mirroring the `MAX_PROPS_BYTES` precedent. JSON `null` save blobs are
  rejected up-front so the NOT NULL column never surfaces a misleading 500.
  Create reuses the 0.3.6 `SaveView` projection; an insert `public_id` UUID
  collision retries once with a fresh UUID.
- **Consequences:**
  - The API layer raises body-validation 400s (unknown fields, empty/malformed
    name) before the ownership 403 check, which itself precedes the
    pool-unavailable 503; the service re-validates (trimmed name, null blob,
    size cap) before any SQL, so those 400s beat SQL-time failures (404
    player-not-found, 500) whenever a pool is present.
  - Delete (0.3.9) and single-save retrieval (0.3.8) follow the same own-only
    + `SaveView` conventions.

---

### 2026-08-25 — Single-save reads are scoped by internal player id (defense-in-depth)

- **Context:** Task 0.3.8 adds `GET /saves/{player_id}/{save_id}`. The API
  layer already rejects mismatched `{player_id}` with 403 pre-pool, but the
  service query could still have been `WHERE public_id = ?` alone.
- **Decision:** `get_save` resolves the owning player to its internal BIGINT
  id (active-only) and selects `WHERE public_id = ? AND player_id = <internal>`.
  A save belonging to another player — or an unknown save id — therefore
  surfaces as the same 404 as an unknown player, so clients cannot distinguish
  (and never see) another player's save ids. Same pattern applies to the
  0.3.9 delete.
- **Consequences:** No new error variant needed; `SaveServiceError::NotFound`
  already covers both unknown-player and unknown-save reads.

---

<!-- Append new decisions below this line. Use the dated heading format above. -->

## 2026-08-25 — WebSocket `/ws` gating and message envelope (task 0.3.10)

Established: socket auth is a one-shot ticket (`POST /v1/socket-tickets` →
`?ticket=` query param on the ws upgrade), matching the Talo socketToken
flow; actor stores the resolved `alias_id`. Missing/invalid tickets still
upgrade so the client gets the `v1.error` envelope (`INVALID_SOCKET_TOKEN`),
then the socket closes with policy code — no silent unauthenticated sessions.
Inbound envelope parsing was extracted to a pure `process_text_frame`
function so happy/error paths are unit-testable without a live socket; all
input failures return typed `v1.error` codes (`INVALID_JSON`,
`UNHANDLED_REQUEST`, `INVALID_INPUT`) instead of defaulting to 0/"".
Message ids are timestamp + atomic counter (unique within the same second).

---

## 2026-08-25 — Socket presence = single-process in-memory hub, register-on-identify (task 0.3.11)

**Context:** Task 0.3.11 adds the Talo `v1.players.presence.updated` broadcast
for player connect/disconnect. Upstream fires presence from the socket layer:
`player.setPresence(true)` when a player identifies, `setPresence(false)` when
their connection closes, envelope
`{ presence: { playerAlias, online, customStatus, lastSeenAt }, meta: { onlineChanged, customStatusChanged } }`
fanned out to all connected authed conns.

**Options considered:**
- Register on ticket-authenticated connect (earliest possible) — announces
  "online" for sockets that may never identify; deviates from Talo timing.
- Register on successful `v1.players.identify` (chosen, Talo parity) — a
  presence registration is armed only when the client actually claims its
  alias; the same boolean gates the `stopping()`-issued leave.
- Shared store / pub-sub (Redis) — multi-node correct but out of scope for v0
  and would force `main.rs` + Cargo wiring outside this task's owned path.

**Decision:** Single-process actix `PresenceHub` actor, process-global via a
`Lazy` accessor in `src/sockets/presence.rs`. Registry is
`alias → { conn_key → Recipient<PresenceChange> }`; `online` fires on the
alias's first identified connection, `offline` on the last disconnect, fanned
out to all registered recipients. `alias_id` in the registry always comes from
the server-resolved socket ticket — never a client-supplied id — so presence
cannot be spoofed. Connections get a unique `conn_key` (atomically assigned)
so a dropped socket is unregistered exactly once via `stopping()`.

**Consequences:**
- Single-node only: multi-instance deployments need a shared presence store
  (follow-up, tracked with the multi-node work). `customStatus` is accepted in
  the envelope shape but fixed to `null` / `customStatusChanged: false`
  (follow-up).
- Per-alias cap (256 conns) prunes dead recipients so a missed
  `LeavePresence` cannot grow the registry without bound.
- Tests exercise the hub directly (mock recipients) and the wire path through
  pure ws.rs helpers; no live stack required.

---

## 2026-08-25 — Socket channel join/leave: socket-driven extension over verified Talo response shapes (task 0.3.12)

**Context:** Task 0.3.12 adds the `v1.channels.player-joined` /
`v1.channels.player-left` broadcasts for WebSocket game-channel membership.
Upstream Talo (verified from `TaloDev/backend` + `TaloDev/docs`) fan-outs these
envelopes over sockets but has **no** `v1.channels.join` / `leave` request
token — membership is driven over HTTP, and sockets only receive the fan-out.
Playzoid v0 has no channel persistence or HTTP channel routes yet.

**Options considered:**
- Broker membership purely over the verified response side with no request
  trigger — nobody could ever join a channel, making the feature dead code.
- Add `v1.channels.join` / `v1.channels.leave` as documented Playzoid **request**
  extensions (chosen) — the only in-scope trigger; response envelopes stay
  Talo-verified, so clients that only consume broadcasts see identical shapes.
- Pull HTTP channel management into this task — scope creep into future
  tasks/owned paths.

**Decision:** `v1.channels.join` / `v1.channels.leave` are Playzoid socket
request extensions. Membership is tracked in a single-process in-memory
`ChannelHub` (structural mirror of `PresenceHub`): `channel → alias → conn_key
→ Recipient<ChannelChange>`, plus a reverse `conn_key → memberships` index so a
disconnect leaves all its channels in O(its channels). Alias ids always come
from the server-resolved socket ticket (never client-supplied). Joining an
already-member alias is idempotent (SDK-doc "already in channel → nothing
happens"). The joiner's own connection is included in the `player-joined`
fan-out (mirrors presence-hub insert-then-broadcast); the departed connection is
excluded from its own `player-left`. `meta.reason` is emitted as the numeric
`GameChannelLeavingReason::DEFAULT` (`0`), per the upstream TS numeric enum —
never a string. No `TEMPORARY_MEMBERSHIP` variant yet (belongs to subaccount /
temp-membership work).

**Consequences:**
- Single-node only, same as presence; multi-instance needs a shared store
      (tracked with the multi-node work).
    - `v1.channels.leave` is a socket-driven explicit leave; disconnect auto-leaves
      all channels via `stopping()` → `LeaveAllChannels`.
    - This is a **documented deviation** from upstream (request side only); if a
      reviewer rejects it the task is BLOCKED pending a human decision. Response
      envelopes remain Talo-verified.
    - Chat broadcast (task 0.3.13) will route through this membership registry.

---

## 2026-08-25 — Socket chat broadcast: joint-notification registry + Talo-parity envelope (task 0.3.13)

**Context:** Task 0.3.13 adds the `v1.channels.message` fan-out. Upstream Talo
(verified from
`TaloDev/backend/src/socket/listeners/gameChannelListeners.ts`) accepts
`{ channel: { id }, message: string }` and fans
`{ res: "v1.channels.message", data: { channel, message: <string>, playerAlias } }`
out to **every member socket, sender included**; a sender who is not a member
is rejected ("Player not in channel").

**Options considered:**
- Keep the 0.3.10-era echo shape (`data.message = { id, from: "server", message }`)
  and just broadcast that — preserves the earlier doc but diverges from the
  verified Talo fan-out (plain-string message + sender `playerAlias`).
- Register a second recipient per connection for chat alone — duplicates
  bookkeeping and doubles the fan-out registry.
- Re-type the single registry to a joint notification enum (chosen) — one
  `Recipient<ChannelNotification>` (`Change | Message`) per connection carries
  both membership changes and chat messages through the same fan-out paths.
- Reject non-member sends with Talo's "Player not in channel" envelope — needs
  a sender-reachable error channel that v0 sockets lack; instead a non-member
  send is a silent no-op (mirrors the existing non-member leave no-op).

**Decision:** `v1.channels.message` is a Playzoid **request** extension
(`channelId` field, same as the 0.3.12 join/leave extension; upstream nests
`channel.id`), while the response envelope stays Talo-verified. The channel
hub's membership registry stores `Recipient<ChannelNotification>` once per
connection; join/leave and chat messages fan out through that single registry.
The broadcast envelope is
`{ res, data: { channel: { id }, message: <string>, playerAlias: { id } } }` —
message is a plain string, the sender identity is always the server-resolved
socket-ticket alias (never a client-supplied `playerAliasId`), and the sender's
own connection receives its message (Talo parity). Inbound messages are
validated at the ws layer: identify-gated, integer `channelId`, non-empty
`message`, capped at `MAX_CHAT_MESSAGE_CHARS` (1000 chars). Sending as a
non-member, or into an unknown/empty channel, is a silent no-op for v0.

**Consequences:**
- The 0.3.10-documented counterpart-gated echo (`{ id, from: "server", message }`,
  timestamp+counter message ids) is dropped; envelopes changed. Recorded in
  `CHANGES.md`.
- Non-member sends produce no error to the sender (v0 trade-off); revisit when
  a socket error channel exists.
- Single-node only, same as presence / 0.3.12; multi-instance chat needs a
  shared store (tracked with the multi-node work).

---

## 2026-08-25 — Socket channel participation is group-keyed by parent_account_id (task 0.3.14)

**Context:** Task 0.3.14 adds subaccount participant support to WebSocket game
channels. `TaloRustServerPlan.md` docifies a "Subaccount Chat Extension" where
subaccounts appear as distinct users but share channel membership with their
parent. The verified upstream envelope set (`v1.channels.*`,
`v1.players.presence.updated`) carries per-alias `playerAlias` ids and never a
parent relationship, so the grouping must live server-side.

**Options considered:**
- Group-keyed participation at the registry level (chosen): a channel's
  participant set is its distinct groups with ≥1 live conn, where
  `group(alias) = players.parent_account_id` (server-resolved) or the alias
  itself for root accounts. First conn of a group announces `player-joined`
  (carrying the joining alias); last conn announces `player-left` (carrying the
  departing alias); chat fans to every conn across the channel's participant
  groups. Parent and subaccount conns therefore share membership and each
  other's chat.
- Parent id as an envelope field on broadcasts — breaks Talo parity; rejected
  (parity doctrine from 0.3.12/0.3.13). An **optional, additive**
  `parentAccountId` is instead surfaced in the `v1.players.identify.success`
  data (null for roots / degraded DB).
- Per-alias membership with parent-implied reads — requires walking parent
  trees per broadcast; no in-scope trigger and more state.

**Decision:** The single-process `ChannelHub` registry is rekeyed to
`channel → group → alias → conn_key → Recipient<ChannelNotification>` with a
reverse `conn_key → (channel, group, alias)` index (the alias is kept so a
group's last-conn `player-left` announces the departing alias). The group is
always derived server-side from the ticketed alias via a parameterized
`SELECT parent_account_id FROM players WHERE id = ? AND status <> 'deleted'`;
`resolve_parent_account_id` + pure `group_key` live in the new
`src/sockets/groups.rs`. Immediate parent only (one hop, mirrors the one-level
parent resolution used elsewhere); nested-subaccount roots are follow-up.
Envelopes keep the Talo-verified per-alias shape — grouping is visible only in
who shares a channel's membership. `v1.channels.join` / `leave` /
`message` remain Playzoid request extensions (0.3.12/0.3.13). Presence stays
per-alias, so subaccounts still appear as distinct users there.
**Degraded mode:** when the DB pool is absent, the alias is unknown, or the
lookup fails, `parent_account_id = None` and every alias groups as itself —
today's behavior; group resolution never fails a connection.

**Consequences:**
- `JoinChannel`/`LeaveChannel` carry the resolved `parent_account_id`; the hub
  computes the group key. `ChannelMessage` carries the sender's server-stamped
  `group`; the send gate is group-level membership (`group` must be a channel
  participant), so a subaccount of a participant parent may send.
- Spoof-proofing preserved: group always derives from the ticketed alias →
  `players.id`, never a client-supplied value (additive `parentAccountId` in
  `identify.success` is server data, not client input).
- A pre-existing v0 gap noted, not fixed (outside `src/sockets/` owned path):
  `POST /v1/socket-tickets` accepts any client-supplied alias id. A lookup miss
  just disables grouping for that alias — no privilege path, because the group
  remains server-derived.
- Single-node only, same as presence/chat; multi-instance needs a shared store
  (tracked with the multi-node work).

---

## 2026-08-26 — `/v1` route-prefix parity via dual-mount aliases (task 0.4.1)

**Decision:** the four gameplay route groups (`auth`, `players`, `leaderboards`,
`saves`) are mounted at both the canonical upstream-prefixed path
(`/v1/<group>`, matching Talo) and the legacy unprefixed path (`/<group>`)
during the transition. Each module builds its routes once in a private
`scoped(prefix)` helper and `config` registers the scope twice, so the two
mounts cannot drift; the public `config(&mut web::ServiceConfig)` signatures
are unchanged and `main.rs` is untouched.

**Rationale:** upstream parity (task 0.4.1) without breaking existing clients
in one step. The legacy alias removal is deliberately deferred to a later
hardening task — when it happens, only the second `.service(scoped(...))`
call per module plus the alias tests need deleting.

**Deliberately unprefixed (infra, not part of the upstream API surface):**
`/healthz` and `/ws`. `/v1/socket-tickets` already carried the prefix.

**Consequences:**
- The routed surface temporarily includes both spellings of every gameplay
  endpoint — documented intent for the transition window.
- Legacy-alias coverage lives in dedicated tests (unit + integration) so the
  eventual removal is a visible, test-driven change.
- `docs/TALO_API.md` still describes the pre-parity prefixes; its update is
  owned by task 0.4.13 (noted in `docs/ralph-log.md`).

---

## 2026-08-26 — Upstream domain-model struct lift (task 0.4.2)

**Context:** task 0.4.2 asks for full serde structs for `PlayerAlias`,
`PlayerAuth`, `GameChannel`, and the complete `LeaderboardEntry` (incl.
upstream `props`). `docs/TALO_API_STRUCTS.md` only catalogues request bodies,
so the domain shapes were re-verified against the live upstream docs:
docs.trytalo.com `/docs/sockets/responses` (canonical types: `Prop`,
`Player`, `PlayerAlias`, `GameChannel`, `GameChannelLeavingReason`),
`/docs/http/leaderboard-api` (entry samples: float score, 0-based
`position`, nested alias *without* timestamps), and
`/docs/http/game-channel-api` (`owner: null` for system channels,
`autoCleanup` / `private` flags, alias payloads *with* timestamps).

**Decisions:**
- New modules under `src/entities/`: `prop.rs` (shared key/value pair),
  `player_auth.rs`, `player_alias.rs` (with nested `PlayerRef`),
  `game_channel.rs`; `leaderboard.rs` gains `LeaderboardSortMode` plus a full
  upstream-parity `LeaderboardEntry` **alongside** — never replacing — our
  implemented `LeaderboardEntryView`, so wire compatibility is untouched.
- `GameChannel.owner` uses the full `Option<PlayerAlias>` rather than a
  summary struct: live samples show owner is a complete alias and there is
  no recursion cycle (alias → player → auth), so full parity costs nothing.
- Alias-level timestamps are `Option<DateTime<Utc>>`: channel payloads carry
  them but leaderboard-entry samples omit them; liberal acceptance keeps
  both real fixtures deserializable.
- `LeaderboardEntry.score` is `f64` (upstream samples are floats, e.g.
  `593.21`) while our persisted schema stays `BIGINT i64`; conversion
  belongs to the future service layer. `position: u64` mirrors upstream's
  0-based index; our views keep their 1-based `rank`.
- `GameChannelLeavingReason` gets hand-written integer serde: serde variant
  `rename` would emit JSON strings (`"0"`), upstream emits bare integers.
  Matches the manual-decode precedent set by `PlayerStatus`.
- Upstream's circular `Player.aliases` back-reference is omitted;
  `Player.groups` is not modelled until player-group persistence exists.

**Consequences:**
- Purely additive data structs; no handler/service/socket changes, so every
  existing response shape stays byte-compatible.
- Security invariant pinned in unit tests: neither `PlayerAuth` nor
  `PlayerAlias` can serialize credential material (`password_hash` remains
  on the internal `players` row only).
- When channel/analytics endpoints land later in 0.4, these structs are the
  ready-made parity targets instead of new ad-hoc shapes.

---

## 2026-08-26 — `game_settings`: opaque route id, no FK until a games table exists (task 0.4.3)

**Context:** task 0.4.3 adds the `game_settings` table backing
`GET/PUT /v1/games/{game_id}/settings`. Upstream addresses games by an opaque
route id, but Playzoid v0 has no `games` table yet, so the column cannot
foreign-key anywhere.

**Options considered:**
- Wait for a `games` table and add the FK then — blocks settings work behind
  an unscoped task.
- Invent a minimal `games` table in this task — scope creep beyond owned
  paths (`migrations/`) and guesses at a schema no task specifies.
- Store the route id directly with a unique constraint, no FK (chosen).

**Decision:** `game_id VARCHAR(64) NOT NULL UNIQUE` holds the opaque route
identifier, mirroring the leaderboards' `internal_name` convention; the
internal BIGINT id never leaves the server (players/game_saves precedent).
`config JSON NOT NULL` carries arbitrary per-game configuration, one row per
game via the unique constraint. Size capping is deliberately not in-schema —
MySQL JSON columns cannot be size-bounded — and belongs to the API layer
(task 0.4.4), same split as the leaderboards' props ≤ 4 KB rule.

**Consequences:**
- When a `games` table lands, adding the FK is a small follow-up migration;
  existing rows already key on route ids.
- PUT upsert semantics live in task 0.4.4.

---

## 2026-08-26 — Game settings endpoints: Playzoid extension, JWT-only writes, 32 KiB cap, upsert (task 0.4.4)

**Context:** task 0.4.4 adds `GET/PUT /v1/games/{game_id}/settings`. Upstream
Talo has only dashboard-managed global `GET /v1/game-config` KV — verified no
upstream per-game settings HTTP endpoint exists, so the task text plus the
0.4.3 schema decision are authoritative for the shape.

**Decisions:**
- **Documented Playzoid extension**, not upstream parity: request/response
  shape defined by this repo (`{ gameId, config, createdAt, updatedAt }`,
  camelCase like every other view). Recorded here so 0.4.13's TALO_API.md
  update marks it as an extension rather than upstream surface.
- **PUT upserts** (`INSERT … ON DUPLICATE KEY UPDATE config = ?`, fully
  parameterized, value bound twice): first PUT creates, later PUTs replace
  only `config`; `created_at` preserved, `updated_at` maintained by MySQL.
  Always returns 200 with the read-back view — no create/update status split,
  because clients address the row by id, not by server-assigned key.
- **Size cap 32 KiB** (`MAX_CONFIG_BYTES`), mirroring the saves
  `MAX_SAVE_BYTES` precedent; enforced pre-SQL alongside null-config and
  id-length validation so bad requests never touch the database.
- **No legacy alias mount**: the route is born after the 0.4.1 parity pass;
  only `/v1/games/{game_id}/settings` exists (`/v1/socket-tickets`
  precedent).
- **Auth trade-off:** both endpoints require a valid JWT, but there is no
  `games` table / ownership / admin scope yet, so any authenticated player
  can overwrite any game's config. v0 accepts this per the task spec
  ("auth-guarded"); revisit when `games` or scopes land. Rate limiting
  arrives in 0.4.8.

---

## 2026-08-26 — Analytics events: append-only, SET NULL player FK, generic name+JSON schema (task 0.4.5)

**Context:** task 0.4.5 adds the `analytics_events` table backing the batched
`POST /v1/events` ingest (0.4.6). Upstream Talo's analytics event shape is
undocumented in this repo (`TALO_API.md` has no events section;
`TALO_API_STRUCTS.md` has no Event struct) — verified by grep before
designing. `docs/memory.md` Open Question #6 already pins typed event
schema as deferred to Phase 1.0.

**Decisions:**
- **Append-only semantics encoded in schema:** no `updated_at` column
  (rows are never updated; its omission documents immutability intent)
  and no `public_id` (unlike saves/players, events are write-only
  fire-and-forget in v0 — clients never address a stored event).
- **`player_id` nullable + ON DELETE SET NULL**, not CASCADE: events may be
  emitted pre-identify (anonymous), and deleting a player must never erase
  their event history — CASCADE would silently rewrite history and break
  append-only guarantees.
- **Generic schema:** free-form `name VARCHAR(64)` event key + optional JSON
  `props`. No upstream-specific columns guessed from an undocumented shape;
  whatever 0.4.6's batched body needs fits. Typed enum/payload schema lands
  in Phase 1.0 when upstream shapes are verified.
- **Minimal indexing for a high-write log:** `(player_id)` for per-player
  queries, `(name, created_at)` for per-event-type time-range queries. No
  more — every index taxes ingest throughput.

---

## 2026-08-26 — `POST /v1/events`: bare-array fire-and-forget ingest, anonymous-on-miss attribution (task 0.4.6)

**Context:** task 0.4.6 implements the batched analytics ingest over the
generic `analytics_events` schema from 0.4.5. Upstream Talo's event shape
remains undocumented in this repo (no events section in `TALO_API.md`, no
Event struct in `TALO_API_STRUCTS.md`), so the 0.4.5 schema plus this task's
text ("batched array body, validate shape, fire-and-forget semantics") are
authoritative.

**Decisions:**
- **Body = bare JSON array** `[{"name", "props"?}, ...]` — no wrapper
  object, no client timestamps (`created_at` is DB-stamped; append-only
  log), and `deny_unknown_fields` per event so malformed telemetry surfaces
  as a clean 400 instead of silently dropping data.
- **Whole-batch atomic validation pre-SQL** (`validate_batch` pure fn):
  non-empty, ≤ `MAX_BATCH_EVENTS` = 100, trimmed `name` 1..=64 chars
  (`VARCHAR(64)`), serialized `props` ≤ `MAX_PROPS_BYTES` = 4 KiB
  (leaderboard props precedent). One invalid event rejects everything with
  400; nothing partial is ever written.
- **Fire-and-forget HTTP contract:** accepted batches answer
  `202 {"accepted": n}` after one batched multi-row INSERT; *any*
  post-validation database failure is logged (`tracing::error!` with batch
  size) and **still answered 202** — analytics loss is tolerable by
  definition and clients never block on DB health. Pool absent → 503
  (degraded-mode precedent). Rejected: 500 on DB failure (contradicts the
  contract); `tokio::spawn` writes (kills test determinism + backpressure).
- **Best-effort attribution:** JWT `public_id` resolves to the internal
  `players.id` once per request (`status <> 'deleted'`, saves precedent);
  an unknown/deleted caller — or even a failing resolution query — stores
  anonymous rows (`player_id NULL`) instead of failing. Deliberately unlike
  saves' 404: events address no player resource.
- **Batched INSERT via `sqlx::QueryBuilder::push_values`:** only `(?, ?, ?)`
  placeholder scaffolding repeated by the validated count, every value
  attached through `push_bind` — identical injection guarantee to the
  static bound queries elsewhere; 100×3 params ≪ MySQL's parameter limit.
  Explicit SQL-safety comment in code so reviewers don't misread it as
  interpolation.
- Caps bound abuse until rate limiting (0.4.8): worst case ~400 KiB/request.

**Consequences:**
- Swallowed DB errors create an observability gap until `/metrics` (0.4.9);
  errors are logged with batch size meanwhile.
- Typed event schema stays deferred to Phase 1.0 (Open Question #6); when it
  lands only `EventInput`/`validate_batch` change.

---

## 2026-08-26 — `POST /v1/feedback`: analytics-events sink reuse, honest 500 on DB failure (task 0.4.7)

**Context:** task 0.4.7 adds player feedback submission. Upstream Talo has no
feedback shape in this repo (no feedback section in `TALO_API.md`, no struct
in `TALO_API_STRUCTS.md`), the task's owned paths exclude `migrations/` and
`src/entities/`, and 0.4.5's `analytics_events` schema was built deliberately
generic (`name` + JSON `props`).

**Decisions:**
- **Store feedback in `analytics_events`:** one row per submission with
  `name = "feedback"` and `props = { "message": <trimmed> }`. No dedicated
  table/migration in v0; rows are never read back by any client-facing
  endpoint. A dedicated table (ratings, categories, moderation state) remains
  a Phase 1.0 candidate — if review rejects the sink reuse, the task reopens
  as a migration follow-up.
- **Honest failure, not fire-and-forget:** unlike `/v1/events`, feedback is
  user content — a post-validation database failure answers
  `500 {"error": "internal error"}` (details logged server-side only)
  instead of faking success. Rejected: swallowing errors like analytics does
  (silently loses player messages).
- **Body `{ "message": ... }` with `deny_unknown_fields`:** trimmed message
  must be 1..=`MAX_MESSAGE_CHARS` = 1000 chars (socket-chat gate precedent,
  task 0.3.13) *and* its JSON-encoded `props` ≤ `MAX_PROPS_BYTES` = 4 KiB
  (events precedent — escape-heavy control-char input can blow the encoded
  cap while staying length-valid). No client timestamps or rating fields;
  richer schema defers together with the dedicated-table decision.
- **Best-effort attribution** identical to events: JWT `public_id` resolves
  once (`status <> 'deleted'`, saves precedent); an unknown/deleted caller —
  or a failing resolution query — stores anonymous (`player_id NULL`) rows,
  never an error. Static `.bind()`-only INSERT.
- **Canonical-only route** (no legacy alias, post-0.4.1 precedent); guard
  order cheapest-first: auth 401 → pool 503 → validation 400 → insert.

**Consequences:**
- Feedback queries ride the existing `(name, created_at)` index; nothing new
  to migrate or reverse.
- Rate limiting (0.4.8) still required: caps bound per-request cost but not
  request frequency.

---

## 2026-08-26 — Redis fixed-window rate limiting: fail-open, Arc<S> middleware, monomorphic limiter (task 0.4.8)

**Context:** task 0.4.8 adds rate limiting on public routes. Redis is already a
dependency (WebSocket hub pre-0.4); the 0.3.x degraded-mode pattern (service
unavailable → pass-through, warn-logged) is the repo-wide precedent. The task
marks task 0.4.8 owned paths `src/middleware/` + `src/config.rs`.

**Decisions:**
- **Fixed windows in Redis, not an in-process token bucket:** one key per
  `(class, client ip, window_start)` (`rl:{class}:{ip}:{window_start}`),
  incremented by an atomic `EVAL` (`INCR` → first-hit `EXPIRE` → `TTL`). Crosses
  worker threads today and server instances when multi-node lands; token-bucket
  state would need the same shared store anyway. Two classes: `auth`
  (credential prefixes, tight budget — brute-force backstop) and `default`
  (other public prefixes). Rejected: in-memory counters (per-worker skew) and a
  simpler global bucket (one client starves everyone).
- **Fail-open degraded mode** matches the 0.3.x precedent: Redis down at boot →
  no `RateLimiter` app data → pass-through; mid-flight backend error → allow +
  warn-log. Availability over strictness for v0.
- **`X-Forwarded-For` trust is opt-in** (`RATE_LIMIT_TRUST_XFF=true`): the
  header is trivially spoofable without an overwriting proxy, so the default
  buckets by socket peer IP. Requests with no peer address pass through rather
  than sharing one unkeyed bucket.
- **Never clone the inner `HttpRequest` for the 429 path:** cloning a
  `ServiceRequest` bumps the `Rc` refcount; when the response is built from the
  clone, actix calls `match_info_mut()` on the shared `Rc` and panics. The 429
  is built by consuming the request directly — `req.into_response(...)` — in
  the same branch that would otherwise feed it to the downstream service.
- **Middleware future is `'static`, so the inner service is stored as `Arc<S>`:**
  `Box::pin(async move {...})` cannot capture `&self` (it dies with `call`).
  Cloning the service into the future needs `S: Clone`; wrapping the field in
  `Arc` needs only `S: 'static`, which actix services satisfy. Also requires
  `S::Future: 'static`.
- **`RateLimiter` is a single concrete type** (`Arc<dyn WindowCounter>`, not
  `RateLimiter<C>`): `app_data` downcasts on the *concrete* type, so a generic
  limiter silently defeats the middleware (tests registered
  `RateLimiter<MockCounter>` while the middleware looked up
  `RateLimiter<RedisWindowCounter>` and found nothing → everything passed
  through). The trait is object-safe with a `'static` boxed `CounterFuture`;
  implementations copy owned captures (connection clone, key `String`) up front.
- **Injectable clock** (`NowFn` on `RateLimiter`, test-only constructor):
  fixed-window rollover is deterministic in tests (429 holds at
  `Retry-After − 1`, passes after the window elapses).

**Consequences:**
- Rate limiting is scoped by path prefix, so future public routes just add a
  prefix to `RATE_LIMIT_PUBLIC_PREFIXES`.
- `Retry-After`/`X-RateLimit-*` headers give clients and proxies a standard
  backoff signal; 0.4.9 Prometheus metrics can report 429 counts per class.
- Fail-open means a total Redis outage costs request-frequency protection but
  never availability — consistent with `/healthz` never being limited.

---

## 2026-08-27 — Production hardening batch: global metrics, route-table OpenAPI, gated audit (tasks 0.4.9-0.4.13)

**Context:** closing Phase 0.4 with observability, an OpenAPI spec, gated
security scanning, live-stack rate-limit tests, and API docs. Three real
decisions worth recording.

**Decisions:**
- **Prometheus via a process-global `LazyLock<Metrics>` registry + middleware:**
  `MetricsMiddleware` wraps the app outermost (measures the whole pipeline
  including 429s) and records method+status counters and a latency histogram;
  the `/metrics` handler renders the registry plus live pool gauges sampled at
  scrape time (no static DB collectors — correct even when a pool appears after
  boot). WS connections move a gauge in `WsConn::started`/`stopping`. The
  `/metrics` route skips recording itself. Rejected: per-request registry
  instances (labels leak), and `actix-web-prom` (extra dep; manual is ~same
  code).
- **OpenAPI generated from a single route table, validated end-to-end in CI:**
  one `ROUTES` const array (method, path, tag, summary, security, body, success
  code) drives `utoipa` builders into an OpenAPI 3.0 document served at
  `/openapi.json`. A unit test asserts every table row exists in the doc, and a
  CI job boots the real server (degraded mode) and validates the served JSON.
  Rejected: `#[utoipa::path]` on every handler (blast radius + schema churn for
  ~22 handlers); the table gives one place to update and honest drift
  detection. Socket-tickets documented as un-authenticated (matches the
  handler).
- **`cargo audit` gates CI with a justified ignore list:** `validator` 0.18→0.20
  clears RUSTSEC-2024-0421 (`idna` 0.5). Two advisories cannot be fixed in
  place — `h2` (actix-http 3.x pins h2 ^0.3, no 0.3.x fix; needs actix-web 5)
  and `rsa` (no upstream fix; only reachable via sqlx-mysql password auth,
  mitigated by TLS to MySQL) — so they are ignored in `.cargo/audit.toml` with
  a mandatory rationale, not silently. Warnings (spin yanked, anyhow /
  event-listener unsound) are non-gating by default.

**Consequences:**
- New public routes must be added to the OpenAPI `ROUTES` table or CI fails —
  the spec cannot silently drift.
- Adding a route to `RATE_LIMIT_PUBLIC_PREFIXES` does not change `/metrics` or
  `/openapi.json` exposure (neither is a configured prefix).
- The h2 advisory will keep failing `cargo audit` if the ignore is ever removed
  without an actix-web 5 migration — tracked as a follow-up.

---

## 2026-10-12 — Bounded Redis handshake: degraded mode must actually degrade (0.4.13 follow-up)

**Context:** re-enabling the CI triggers (previously `workflow_dispatch`-only)
exposed a failing `openapi` job: the server never served `/openapi.json` even
though the job boots it precisely so it *can* run without a database ("degraded
mode"). Reproduced locally: the process stayed alive but never bound a socket.

**Root cause:** `redis::aio::ConnectionManager::new` (redis 0.27) has no default
connect timeout, and its internal retry loop re-arms indefinitely when the host
is unreachable. `main` awaited that handshake *after* the DB pool, so an
unreachable `REDIS_URL` hung startup forever — `HttpServer::bind` was never
reached. An unparseable URL was unaffected (`Client::open` fails fast), which is
why the "degraded mode" contract looked correct in earlier testing.

**Decision:** all optional-infrastructure connections must carry an explicit,
process-owned timeout. New `src/services/redis.rs` exposes
`connect(&str) -> Result<ConnectionManager, String>` which wraps
`ConnectionManager::new` in a 3s `tokio::time::timeout` and maps every failure
(invalid URL, connect error, timeout) to a `String` for the caller to warn-log
and degrade, matching `db::build_pool`'s explicit `acquire_timeout` (5s).
`main` now calls it and, on `Err`, continues with Redis-backed caching and rate
limiting disabled.

**Consequences:**
- Startup is bounded: worst case ≈ 5s (MySQL) + 3s (Redis) before binding, so
  liveness probes and CI boots are deterministic.
- Rate limiting and caching are both off when Redis is down; the existing
  fail-open middleware path is unchanged and now actually reachable.
- New public routes must still be added to the OpenAPI `ROUTES` table — the job
  that enforces it (and would have caught this) runs again.
- Follow-up: the CI `openapi` job's unreachable-DB config previously hid the
  Redis hang only because the whole workflow never ran; keep it enabled.

---

## 2026-10-12 — Repository governance: rulesets repaired (MainProtect, ReleaseBranchProtect, TagProtect)

**Context:** while reviewing the Phase 0.4 release PR, every push to
`release/v0.4` reported "Bypassed rule violations … Cannot update this
protected ref", yet `GET /branches/release/v0.4/protection` returned 404. The
enforcement was a **ruleset** (`MainProtect`, id 24756876), not classic branch
protection.

**Findings:**
- `MainProtect` was named for `main` but matched `ref_name.include: ["~ALL"]`
  and carried the `update` **and** `creation` rules. Absent bypass actors, that
  combination forbids pushing to *and* creating *any* branch — the ruleset was
  mis-scoped and effectively only produced the bypass warning.
- Its `bypass_actors` listed the org-admin actor plus DeployKey, repository
  roles 2/4/5 and one team, so nothing was enforced for anyone with write
  access. It had no `pull_request` and no `required_status_checks`, meaning the
  `AGENTS.md` contract ("never push directly to `main`", "CI green plus one
  human reviewer") was social only — not machine-enforced. This mattered
  because CI had been dead for ~4 months and nothing blocked red merges.
- `v0.4.0` was a **lightweight** tag, unlike the annotated `v0.2.0`/`v0.3.0`,
  and sat 5 commits behind the branch tip.

**Decision:**
- **Tags:** cut annotated `v0.4.1` at `447a49b` for the follow-up fixes and
  keep `v0.4.0` as the phase-completion marker. Rejected re-pointing `v0.4.0`:
  rewriting a published tag is worse than an extra patch tag, and semver treats
  post-release bug fixes as a patch anyway.
- **MainProtect:** re-scope to `~DEFAULT_BRANCH`; add `pull_request` (1
  approval, `dismiss_stale_reviews_on_push`, `required_review_thread_resolution`)
  and `required_status_checks` (`Lint & Test`, `cargo-audit`, `OpenAPI drift
  check`, `sqlx migrate (mysql)`); trim bypass actors to the Organization-admin
  actor only. `non_fast_forward` + `deletion` retained.
- **ReleaseBranchProtect (new, id 24958920):** `deletion` +
  `non_fast_forward` for `refs/heads/release/**`, preserving the force-push
  protection that re-scoping `MainProtect` would otherwise drop.
- **TagProtect (new, id 24958921):** `deletion` + `update` for `refs/tags/v*`.

**Consequences:**
- "CI green" is now enforced by GitHub, not convention; a red PR cannot merge
  to `main`, and direct pushes/PR-less merges to `main` are blocked.
- The single Organization-admin bypass is deliberate: it is the escape hatch,
  and it is also why the author does not strictly need a second human to merge.
  `AGENTS.md` still requires a human reviewer — treat the bypass as an
  emergency path, not a workflow.
- **Deadlock risk to watch:** requiring 1 approval plus "never self-approve"
  needs a second human with write access; today the admin bypass is doing that
  work. Add a second reviewer (or a bot account) if the bypass is ever removed.
- Creating a new `release/**` branch is unaffected (`creation` applies to the
  default branch only now). Tags are create-only: `v*` can be added but not
  moved or deleted.

---

## 2026-10-12 — SECURITY: `.env` (JWT signing key) leaked in public repo history

**Incident:** `.env` was committed in `a778497` and has been present in every
branch and all four tags ever since. The repository is **public**, and the file
contained a real `JWT_SECRET` (a 64-hex-character HS256 signing key) — i.e. the
key that signed all auth tokens. Anyone with that value could forge a valid JWT
for any player id. The value has since been rotated and is not reproduced here.
GitHub secret scanning was **disabled** on this repository, so nothing alerted.

**Blast radius:** committed on `main`, `release/v0.3`, `release/v0.4` and all
`task/*` branches; contained in tags `v0.2.0`, `v0.3.0`, `v0.4.0`, `v0.4.1`.
The adding commit is #97 of 144, so any history rewrite touches ~96 commits and
every ref.

**Decision (stage 1 — done):**
- Rotate the key. This is the only actual remediation: the value is already
  public, so removing it from history does not un-leak it. Documented in
  `.env.example` that the secret must be generated per environment and never
  committed.
- `git rm --cached .env` + `.gitignore` entries (`.env`, `.env.*`,
  `!.env.example`) so the file cannot be re-committed.
- Enable GitHub secret scanning + push protection so this class of mistake is
  caught at push time instead of by a human reading `git ls-files`.

**Decision (stage 2 — done 2026-10-12):** `.env` purged from history with
`git filter-repo --path .env --invert-paths` on a mirror clone (backup kept
beforehand). All 11 branches force-pushed and all four tags deleted + recreated
from filtered history. Verified: 0 commits touch `.env`, 0 blobs contain the
secret, 0 trees list the path; commit count unchanged (147); `release/v0.4`'s
**tree hash is byte-identical**, so prior build/test validation still holds —
only `.env` was removed. `TagProtect` was temporarily disabled for the tag
delete/recreate and restored immediately after.

**⚠️ Residual exposure — GitHub `refs/pull/*/head` are immutable.** After the
rewrite, a fresh clone *still* contained `.env` via `refs/pull/{14..20}/head`,
which GitHub pins to the original pre-rewrite commits (PR 14 was the commit
that introduced it, `a778497`). Pushing a deletion is rejected:
`! [remote rejected] refs/pull/14/head (deny updating a hidden ref)`. These
refs cannot be removed by git or by API from a normal account; only GitHub
Support can purge them by garbage-collecting unreachable objects and closing
the PRs. Until that happens the secret remains retrievable **even though the
key has been rotated, so it can no longer be used**. Any future secret leak on
this repository has the same escape route: branch/tag rewrites are not
sufficient while the offending PRs exist.

**Consequences:**
- `JWT_SECRET` committed in this repo is permanently compromised regardless of
  history edits; treat any token signed with it before rotation as untrusted.
- There is no known production deployment of Playzoid-Server (the VPS skill
  covers ZoidMatter, a different project), so the practical exposure is a dev
  instance — but the key must still be rotated.
- History rewriting changed every commit SHA from `a778497` onward; open PR #21
  was retargeted to the rewritten `release/v0.4` and remains MERGEABLE. Any
  existing clone must be re-cloned or hard-reset, not pulled.
- Force-pushing protected branches required the Organization-admin bypass in
  `MainProtect`/`ReleaseBranchProtect`; both rulesets remain active with
  `non_fast_forward`, so this was a one-off administrative action, not a
  routine workflow.

---

### 2026-10-19 — Playzoid becomes Tidepool; TideTable becomes the plan

**Context:** the project was a Talo-compatible game backend named Playzoid-Server.
The suite's own design system (clawffice-design) had already assigned this
product the name **Tidepool** (Hub and game server; pools federate with other
pools; product hue `tide`, current `water`, creature the rock pool, part
`Rhumb`), and the work that comes next — the hub above the game server — is the
work that name describes. Separately, the roadmap lived in markdown todo files
whose ids are positional and whose prerequisites are prose; it could not express
"hosts before dispatch" or say which files two concurrent tasks both own.

**Options considered:**
- Keep the name and add the hub beside it — two names for one product, and a
  suite that disagrees with its own design system.
- Rename later, when the hub lands — every new hub file, metric, image tag and
  doc written under a name that is about to change; the rename would then touch
  the hub too, not just the game API.
- Rename now, with the plan change (chosen) — one disruptive pass while the
  surface is one binary and 22 routes, and the hub is written under its real
  name from the first file.

**Decision:**
- **Name:** the repository becomes `ClawfficeOrg/Tidepool`, the crate and binary
  `tidepool`, and the Prometheus prefix `tidepool_*`. GitHub keeps a redirect
  from the old repository name. `docs/todo-v1.json` tasks 1.0.1–1.0.3, 1.0.5
  carry each part of the rename; the game API surface does not change.
- **Plan:** TideTable JSON is the plan of record from here on.
  `docs/todo-v1.json` (v1, ships `0.5.0`) is the rename and the hub;
  `docs/todo-v2.json` (v2, ships `0.6.0`) is the clients. `docs/todo-v0.md` and
  `docs/todo-v1.md` are frozen as the record of releases 0.1–0.4, and
  `docs/todo.md` is the index. Task ids, the `task/vX.Y.Z` branch pattern and
  the green gate are unchanged.
- **Hub scope:** servers first (v1), clients second (v2). v1 is identity, hosts,
  connections, triggers, workflows, dispatch, activity, chatrooms, the session
  relay and self-hosting. v2 is the machine-readable contract, the TypeScript,
  Rust and Python SDKs with one conformance suite, the `tidepool` CLI, the
  Svelte control plane, the ratatui console and the Zoid reference client.
  Federation (Rhumb), the hosted pool and mobile are deferred to v3.

**Reference:** Paseo Hub (<https://paseo.sh/docs/hub>) was read on 2026-10-19 and
mapped capability by capability in [`HUB_CAPABILITIES.md`](HUB_CAPABILITIES.md).
It is read for product shape only: no Paseo code, schema, configuration format,
database or wire protocol is copied or ported, and Tidepool does not speak the
Paseo protocol or read a Paseo home directory. The same rule Wheelhouse records
in its `docs/paseo-notes.md`.

**Consequences:**
- Renaming the metric prefix is observable by anyone scraping; it is recorded as
  a breaking change in `CHANGES.md` rather than dropped silently.
- Only `.env`/environment secrets are configuration; nothing in the rename moves
  a credential into the repository, and the standing rule (never hardcode a
  secret) is unchanged.
- Cross-file prerequisites do not validate in TideTable, so `todo-v2.json`
  records its v1 dependencies as `requires_v1` — documentation, not scheduling.
  A supervisor must not start a v2 task while those entries are pending.
- The suite's other repos still refer to the old codename in places; each is
  renamed by its own plan, not this one.

---

## 2026-10-19 — Pool identity is single-tenant in v1

**Decision:** one `organizations` row per pool. The table carries a constant
`singleton` column with a `UNIQUE` key, and the identity service refuses setup
when an organization already exists. There is no organization-id in any v1
route: the pool *is* the tenant.

**Why:** every later phase authorizes against one owner and one org; inventing
tenant scoping now would add a column to every future table for a case nobody
has asked to run. Multi-tenancy is deferred, not rejected: a future migration
drops the singleton constraint and adds `organization_id` to the remaining hub
tables.

**Bootstrap semantics:** `TIDEBOOT_*` is applied once at startup by `main`.
`bootstrap_from_env` only inserts — it returns `AlreadyBootstrapped` when the
organization exists and never touches the operator row. The temporary password
is hashed with Argon2id exactly like any other; `password_change_required` is
true until `POST /v1/pool/password` replaces the hash and clears the flag. A
restart cannot re-apply the password, and after the first change the old
password fails verification because the hash changed. The bootstrap secret is
never logged; `PoolBootstrapConfig`'s `Debug` prints `<redacted>`.

**Operator tokens:** operators use the same JWT issuer and secret as players;
the subject is `pool_users.public_id`. Pool handlers resolve that subject to an
operator row, so a player token is refused with 401. Task 1.1.4's problem
contract and 1.1.2's session invalidation will build on this; if a token kind
becomes necessary it is an additive claim, not a second secret.

**Security:** the bootstrap credential is a privileged secret and is only safe
over TLS. Any deployment that exposes setup or operator sign-in must terminate
TLS in front of the pool; the compose/deploy task (1.6.x) owns that.

**Team roles and invitations:** owners manage members and invitations, admins
will change configuration, members operate, viewers read. Role checks live in
`OperatorAuth` (`require_mutating` / `require_owner`) and read the role from the
database on every request, so a demotion is effective immediately. Invitation
tokens are `<public_id>.<secret>`; only an Argon2id hash of the secret is
stored, so the database cannot be used to accept an invitation and the raw
token is shown once. Single use is `accepted_at`, checked under `FOR UPDATE`.
Removing a member deletes the row, which makes every old JWT fail the next
operator lookup; there is no separate session table in v1.

**API keys and audit:** keys are `<prefix>.<secret>`; the prefix is the lookup
half and only an Argon2id hash of the secret is stored, so list/whoami can show
a prefix without ever re-exposing a usable key. Scopes are an allow-list of
named capabilities, and authentication (401) is kept separate from
authorization (403). Revocation flips `status` and authentication selects only
active rows, so it is immediate. The audit table records one row per hub
mutation with `member`, `api_key` or `system` as actor; v1 writes it after the
mutation rather than in the same transaction, and a write failure is logged.
Daemon enrollment tokens are not part of this task; the future table must join
back to `api_keys.status` so revoking a key kills its unconsumed tokens.

**Hub errors and request ids:** hub failures use RFC 9457 problem details with
`code` and `requestId`; the game API keeps its `{"error": ...}` bodies because
clients depend on them. Handlers emit specific codes, and a hub-only middleware
is the safety net for extractor/404/405/429/5xx failures. The model is additive:
new hub routes must document `x-auth` and, for API keys, `x-required-scope`.
`requestId` comes from middleware inserted before the handler; incoming ids are
reused only when they match a conservative charset/length so log injection and
header abuse stay impossible. The Rust drift test parses the hub modules'
`web::scope(...).route(...)` registrations and compares them with `ROUTES`, so
a route cannot be added to code without the table.

**Host identity:** a host has an immutable generated `public_id` and a mutable
`slug` derived from its hostname. Slugs are lowercase ASCII kebab-case, capped
at 63 characters, and collision-suffixed (`-2`, `-3`) on registration; explicit
renames collide with 409 rather than silently re-suffixing. Revocation is a
status, not a delete: the row stays for audit and every live lookup filters
`status <> 'revoked'`. Configuration activation resolves a slug through
`resolve_for_activation` and names the `host_slug` field on failure, so a
missing/revoked host fails at save time instead of dispatch time.

**Host enrollment:** a ten-minute token (`<public_id>.<secret>`) is minted only
for a registered, unenrolled host. The host generates its own long-lived
credential; the pool stores only the Argon2id hash and no read endpoint returns
it. `host_credentials.host_id` is UNIQUE, so one host has one relationship.
Tokens are single-use and carry the issuing API-key id, so `revoke_api_key`
expires every unconsumed token it minted. The enrollment exchange is public but
requires the one-time token; presence and protocol sessions will authenticate
with the relationship credential via `authenticate_host`.

**Host presence:** the live process keeps a connection/heartbeat registry;
`hosts.status` is a durable projection, not the source of truth. A host is
online only while it has a connection *and* a heartbeat within 90 seconds, so a
client cannot declare presence. Startup marks leftover `connected` rows offline
because no connection survives a restart. Dispatch calls `require_connected`;
an offline host fails with `daemon_not_connected` and is not queued. The
`tidepool_hosts_connected` gauge is updated on transitions and resynced when
stale rows are swept.

**Host execution is a second grant:** enrollment (identity) and the WebSocket
connection (presence) never permit the pool to start agents. `host.execute` is a
separate `execution_granted` flag, default false. A capability report is a flat
list of provider/model/mode/thinking variants plus opaque provider-native
options; re-reporting replaces the set. Validation happens at save time and
names the first unavailable field. The pool only offers providers in the
grantable allow-list for pooled execution, even if a host reports others.

**Host protocol executions:** `executionId` is chosen by the pool and is the
idempotency key. The pool stores the execution before sending `create_agent`
and resends the same intent on reconnect/lost response; the host must answer a
replay with the existing result. Terminal replays are served from the ledger
even when the host is offline. Output is appended only while queued/running, so
a late `execution_output` after completion cannot duplicate the final text. The
reference agent keeps completions in memory; Limpet must persist them.

**Connections and credentials:** a connection stores `credential_ref`, never a
secret. Resolution happens at validation time from the environment/secret
store; reads expose only `credential_configured`. Provider behaviour lives
behind a synchronous object-safe `ConnectionProvider` trait so save-time
validation cannot depend on provider availability and adding a kind is one
implementation. `TIDEPOOL_CONNECTIONS` is an overlay: environment entries win
on slug, are labelled and are immutable through the API; the shadowed database
row is preserved.

**GitHub webhooks:** signature verification happens before parsing/matching and
uses the raw body; a bad signature is 401 and records nothing. Dedup is a unique
`(connection_public_id, delivery_id)` insert with `INSERT IGNORE`, so GitHub
retries are acknowledged once. Repository ownership is case-insensitive and
resolved against the connection's `repository` resources; an unknown repository
is acknowledged but `accepted:false`, while configuration validation fails with
a named `repository` field. `issue_comment` on a PR normalises to the PR comment
event.

**Slack has two transports and one ingest path:** everything after
verification — normalization, workspace resolution, channel filters, the
delivery record — is shared, so the webhook route and socket mode cannot drift.
Webhook verification is the Slack `v0` HMAC over `v0:<timestamp>:<body>` with a
five-minute replay window, checked before parsing; socket mode is verified by
the `app_token` that dialled the socket. Both dedup on Slack's `event_id`
(falling back to `channel:event_ts`), so a retry *and* the same event arriving
on both transports collapse to one `webhook_deliveries` row. The socket-mode
session is deliberately transport-agnostic: it takes frames as text and returns
the frames to send, which keeps the protocol (hello / envelope / ack /
disconnect) unit-testable and leaves dialling the WebSocket — and performing
`chat.postMessage` — to dispatch (1.4.6), which owns the outbound client. The
mention is stripped during normalization rather than at prompt-build time, so
`text` is prompt-safe by construction and `raw_text` keeps the original.

**Declared outputs are enforced per step, not per provider:**
`services::outputs` holds the capability registry (`slack.reply` now,
`discord.reply` next) and `OutputLedger` enforces all three rules in one place
— undeclared capabilities cannot be emitted, `maximum` is a hard count, and
`finish()` fails when a `required` capability was never emitted, so a step that
must answer is recorded as failed rather than silently complete. A reply is
constructed from the event it answers (`ReplyTarget` has no public
constructor), which makes a misaddressed reply unrepresentable instead of
merely checked.

**Discord's dedup window belongs to the session, not the socket (1.3.4):** the
gateway's resume replays everything after the sequence the pool acknowledged,
and it may replay an event already handled, so dedup state must outlive the
connection that produced the duplicate. `DiscordGatewaySession` therefore holds
the `(session_id, seq)` pair *and* the last 1024 message snowflakes, and the
caller reuses one session across redials — the alternative, state on the
socket, would make every reconnect a source of double runs. The in-memory
window is bounded, so the unique `(connection_public_id, delivery_id)` key on
`webhook_deliveries` remains the backstop across a restart. `op 9` with
`d: false` clears the session id and sequence so the next handshake identifies
rather than resuming a session the server dropped.

**The gateway session holds no token:** frames needing the bot token are built
as a `GatewayCommand` naming the credential field, and the value is injected
only by `frame_with_token` at send time — the same split as Slack's
`SlackApiRequest`, so the pool decides *what* to send and dispatch (1.4.6)
decides *with which secret*. A consequence worth keeping: the ingest context
cannot leak a token through `Debug`, because it never has one.

**Discord threads need no thread field:** a thread is a channel in Discord, so
a message in one carries the thread's id as its `channel_id`. Reply addressing
is therefore the same code for a channel and a thread, and "replies land in the
originating thread" holds by construction rather than by a branch. Replies also
pin `allowed_mentions: {"parse": []}`, so agent-authored text cannot ping a
user, a role or `@everyone` whatever it contains.

## 2026-10-21 — Projects and configuration revisions (task 1.4.1)

**Configuration is an authored file bundle, not a normalized document.** A
project's configuration arrives as an ordered set of `{ path, content }` files.
One file is the manifest (`tidepool.json`, JSON); the rest are referenced by it
(prompt partials point at a file path). The authored bytes are stored verbatim
in `configuration_revision_files` — the manifest is parsed only to drive
validation, and nothing is rewritten into a canonical form.

**Revisions are append-only, and a failed install is still a revision.** Every
install inserts one `configuration_revisions` row (version, `active` flag,
`status` ENUM `valid`/`invalid`, and the JSON `validation_errors`). There is no
`updated_at` and no update path; the only field that ever changes is `active`,
flipped off on the previous active revision when a newer one activates. A
failed install therefore leaves a durable, immutable record of what was
attempted *and* the field errors that rejected it, while the previous active
revision keeps serving — this is the "install-then-fail-then-still-active"
contract and it is what the `configuration:install` response reports
(`activated: false`, `status: invalid`, per-issue `{ field, revision, message }`).

**Version numbers are per-project and derived under a lock.** The next version
is `COUNT(*) + 1` for the project, computed inside a transaction that first
`SELECT ... FOR UPDATE`s the project row. Revisions are only ever deleted by
project cascade, so count and max version agree, and the row lock serializes
concurrent installs for the same project so the unique `(project_id, version)`
key cannot race.

**Reference resolution is split pure/Db.** `resolve_references` is a pure
function over the manifest, the bundle's file-path set, and two pre-computed
resolved-slug sets (active connections, live hosts); the database half is two
thin lookups (`find_effective_by_slug`, `resolve_for_activation`). Every
unresolvable reference yields a named `FieldIssue` (field + revision + message),
so connection, host, environment, named-agent and partial failures are each
unit-testable without a database and each names the field and the revision.
Full agent-capability checking (1.4.2) and the workflow/environment schemas
(1.4.4/1.4.5) build on this resolver and the stored definitions.

**Scoping:** projects and revisions carry `organization_id` (via the project's
own), and every query filters on it; the API resolves it from the credential
(member token or API key) rather than the path.

---

## Open Questions / Assumptions

These mirror the open questions in [`docs/todo.md`](todo.md). Resolve
them here when answered.

1. **Database engine** — MySQL is canonical (see `migrations/` and the sqlx
   `mysql` feature flag). PostgreSQL parity is *out of scope* until 1.0.0.
2. **JWT secret rotation** — static env-var secret for v1 (see `Config::from_env`
   validation in `src/config.rs`). Revisit after 1.0.0.
3. **Talo TypeScript reference** — see `docs/TALO_API.md` for the verified
   subset of upstream shapes; gaps tracked under "Remaining TODOs" in that file.
4. **Auth identifier** — `identifier` (matches Talo upstream — accepts
   email | username | numeric id). `username` alone is *not* canonical.
5. **Subaccount limits** — *open*; pending product input.
6. **Analytics event schema** — *open*; deferred to Phase 1.0.
