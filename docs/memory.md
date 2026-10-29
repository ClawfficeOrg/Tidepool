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
reference agent keeps completions in memory; a real host (Swaygent) must
persist them.

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

## 2026-10-21 — Trigger schema and validation (task 1.4.2)

**The trigger is one document, and `from_users` is a hard-required allowlist.**
A trigger declares the provider `event` it listens for (`{ provider, type }`),
its `filters`, and the `run` it starts (`{ host, agent }` where `agent` is the
same `provider`/`model`/`execution_mode`/`thinking_option` selection the
host-capability check already accepts). `filters` is required and
`from_users` must be present and non-empty; a missing or empty allowlist is a
named `filters.from_users` rejection, never a silent default. This is the
security boundary from `HUB_CAPABILITIES.md` — the allowlist is what keeps a
stranger's comment on a public issue from starting an agent on someone's
machine.

**Filters are a conjunction, not a disjunction.** The filter dimensions are the
provider resource (`resource.kind` + `resource.id`), the connection
(`connection` slug), the invoking user (`from_users`), a `channel` or `label`,
and a `contains` or `pattern` guard. Every present condition must pass; there
is no "any" mode. In 1.4.2 the `channel`/`label`/`contains`/`pattern` guards are
validated for shape only — matching an event against them is dispatch's job
(1.4.6).

**Validation is read-only and reports every failure at once.** `POST
/v1/triggers/validate` (`configuration:validate`, or a non-viewer member)
resolves three things by name — the connection (slug → active connection whose
provider matches the event), the resource (an active connection of the event
provider delivers for `kind` + `id`), and the host (slug → live host, then the
host must hold `host.execute` and be able to run `run.agent` via
`host_capability::validate_agent_for_host`). Each failure is a named
`{ field, message }` issue; the response is `{ name, valid, errors }`. Nothing
is written: no audit row, no revision, no presence change.

**Resolution is split pure/Db, mirroring 1.4.1.** `structural_issues` (shape)
and `resolve_issues` (a pure function over a `TriggerResolution` snapshot of the
resolved facts) carry every rejection, so each filter predicate and each
rejection is unit-testable without a database; the DB half is three thin
lookups (`find_effective_by_slug`, `resolve_resource_owner`,
`resolve_for_activation` + `validate_agent_for_host`).

**An offline host still validates.** Validation resolves a host by slug and its
capability report; presence (connected/offline) is a dispatch-time concern and
`daemon_not_connected` remains dispatch-only (1.2.3/1.4.6). Trigger install
(1.4.3) will build on this resolver and the document shape.

---

## 2026-10-21 — Environment model (task 1.4.5)

**An environment is one complete named object, never a base plus overrides.**
`Environment` is `{ host, directory, worktree? }`; the document sets
`deny_unknown_fields`, so an `extends`, `override` or partial-worktree key is a
parse error, and there is no code path that combines two environments. A step
selects exactly one by name, or through a finite input whose every declared
choice names a complete environment and must resolve at activation. Selection
returns a borrow of the whole object, which is what makes "never merged or
overridden by a workflow" structural rather than a convention.

**The host slug is resolved at activation, the directory is declared there.**
`configuration::validate`/`install` resolves `environments.<name>.host` against
live hosts, so a host that was renamed or revoked fails activation with a named
field instead of failing at dispatch. The directory is validated as a
declaration: required, ≤4096 characters, absolute (POSIX or Windows drive) and
canonical (no `.`/`..`, repeated separators, trailing separator or control
characters). The pool has no filesystem access to a host and never clones,
creates or rewrites the directory, so a remote `stat` is not part of activation;
the host is the authority on existence when it prepares the execution. A
missing declaration or a non-canonical path is caught at activation, which is
the failure this task owns.

**Worktree branch templates embed the execution id, and only that.** Only
`${{ paseo.execution.id }}` (and the Tidepool spelling
`${{ tidepool.execution.id }}`) is accepted in `newBranch`; any other expression
is refused at the authored field. Activation renders the template with a sample
UUID, so a template that only works for a short id still fails before dispatch.
At dispatch the pool renders the real execution id and validates the resulting
Git ref, which is what gives every execution its own branch and lets a retry or
recovery reuse it. `checkout-branch` takes a literal branch and no expression.
Execution ids are sanitised to `[A-Za-z0-9._-]` before they touch a ref, so a
host or client supplied id cannot escape the branch namespace.

**Dispatch is a pure decision plus one presence lookup.** `resolve_dispatch`
takes the authored environments and host-availability facts and returns a
`DispatchTarget` (environment, host public id, directory, rendered branch). An
offline host yields `EnvironmentError::DaemonNotConnected`, whose `code()` is
the documented `daemon_not_connected`; there is no queue, so the trigger must
fire again. `require_for_dispatch` is the thin database wrapper
(`resolve_for_activation` + `host_presence::require_connected`) around that pure
function, which is what keeps the offline-rejection path unit-testable without
MySQL.

---

## 2026-10-21 — Workflow schema (task 1.4.4)

**A workflow is one trigger and an ordered step list, validated against the
same revision.** `Workflow` is `{ name, trigger, maxRuntime, inputs, steps }`;
each `Step` is `{ id, environment, agent, prompt, if?, output?, outputs,
maxRuntime?, idleTimeout?, reads? }`. Steps run in authored order and the
authored JSON is stored verbatim in the revision (no normalized workflow
document is written). `configuration::parse_workflows` returns the authored
list for dispatch (1.4.6).

**Only earlier steps are readable, enforced at every read site.** A `reads`
entry, any condition operand naming a step, and any `${{ steps.<id>... }}`
prompt expression must name a step that appears before the reader. A later
step, the reader itself or an unknown id is a named rejection, so dispatch
never needs to reason about forward references. Step ids are unique and limited
to `[A-Za-z0-9][A-Za-z0-9_-]*`; dots are excluded so the prompt expression
parser can separate an id from its output property unambiguously.

**An agent is a name or one complete static inline configuration, never a
dynamic inline object.** `AgentRef` has `Named(String)`, `Inline(InlineAgent)`
and a catch-all `Dynamic(map)`. The catch-all exists so an authored
`{ "input": ... }` object parses and receives a named validation issue instead
of turning the whole bundle into malformed JSON; it is never accepted.
`InlineAgent` denies unknown keys and refuses `${{ ... }}` values, which is
what keeps an inline agent complete and static.

**A finite output schema can bound a later choice; an unbounded one cannot.**
An output property is finite when it declares `const` or a non-empty `enum`.
When a condition compares a declared input to an earlier step's output property
with `in`/`eq`, every declared input choice must be inside the finite set. A
property without `enum`/`const` yields no bound, so the later choice stays
unconstrained. `finite_choice_bounds` exposes the intersection for dispatch;
the validation error names the input and the offending choice.

**Deadlines nest; the remaining workflow time is the outer bound.** A
workflow's `maxRuntime` defaults to one hour and is capped at seven days. A
step's `maxRuntime` defaults to the workflow's remaining time, and both that
value and the step's `idleTimeout` are capped by the remaining workflow time
(the idle timeout is also capped by the step's maximum runtime).
`Workflow::effective_deadlines` is pure so 1.4.6 can apply it at each step
start.

**Bundle caps are structural, named, and reject extras.** The 100-file,
512-character-path, 1,000,000-character-file and 5,000,000-byte-partial caps
each report the offending path. Paths must be relative and canonical: no
absolute or drive prefix, no backslash, no `.`/`..`, repeated or trailing
separator, control character or padding. Duplicate paths are refused, and any
bundle file other than `tidepool.json` that the manifest does not reference as
a partial is an "extra file" and fails activation. The partial byte cap sums
the referenced partial files, not every file in the bundle.

---

## 2026-10-21 — Trigger installation and revisions (task 1.4.3)

**A trigger's identity is per project, and its name is the replace key.** The
`triggers` row holds `(project_id, name)` as a unique key and a stable
`public_id`; installing a document whose name already exists replaces that row's
document rather than inserting a second trigger. Two projects in one
organization may each own a trigger called `auto-fix-issues`, which is what lets
a repository's configuration be deployed per project. `updated_at` moves on a
replace; `public_id` never does, so dispatch (1.4.6) and the activity log
(1.4.7) can refer to one trigger across every version of it.

**Revisions are append-only, so history stays readable.** A successful install
inserts one `trigger_revisions` row — per-trigger `version`, the exact installed
document as JSON, `active` — and flips `active` off on the trigger's previously
active row in the same transaction. Nothing is updated in place and nothing is
deleted (only a project cascade removes them), so `version N` always means the
same document. Version is `max(version) + 1`, not `count(*) + 1`, so a swept row
cannot make the next install collide on the unique `(trigger_id, version)` key.

**An invalid document changes nothing, because it never reaches the writer.**
Install validates first and then decides: `decide_install` returns
`TriggerError::Invalid` with the named issues and *no plan*, so there is
nothing for the database half to execute — no trigger row, no revision, no
deactivation, no audit entry. This is the "a later failure does not undo an
earlier success" contract from the task, and it is a property of the plan type
rather than a convention. The HTTP result is 422 `configuration_invalid`
naming every failing field. Note the deliberate asymmetry with configuration
1.4.1: a rejected *bundle* is still stored as an `invalid` revision for the
record, while a rejected *trigger* install is not stored at all, because the
task says it changes nothing.

**The bookkeeping is pure; only the write is not.** `RevisionPlan`,
`ExistingTrigger`, `plan_install` and `decide_install` take the trigger's
current state and return what will be written, so create-then-replace, version
numbering, identity retention and the invalid-document rule are all unit-tested
without a database. `install_trigger` then `SELECT ... FOR UPDATE`s the trigger
row (or the project row when creating one) and applies the plan, so two
concurrent installs of the same name serialize and cannot pick the same
version.

**The stored document is the canonical JSON of the authored one.** Installation
serializes the parsed `TriggerDocument`, so dispatch reads back exactly the
filters and run that were installed, with `deny_unknown_fields` having already
rejected anything unrecognized. The install body itself is the document plus a
`project` key and is parsed explicitly rather than with `#[serde(flatten)]`,
because flattening silently disables `deny_unknown_fields` — install must
accept and refuse exactly the documents validate does.

---

## 2026-10-21 — Dispatch and run lifecycle (task 1.4.6)

**A run is a snapshot, not a live revision pointer.** Creation copies the
workflow, the environment definitions and the prompt partial contents onto the
run, and every step stores its resolved environment, host, directory, rendered
branch and execution id. Recovery therefore evaluates what was dispatched even
after a newer revision is installed, and it needs no repository or manifest
read. The activity log (1.4.7) can read the same immutable record.

**Deduplication is per trigger and happens before any other work.** The unique
key is `(trigger_id, delivery_key)`. A resubmission returns the existing run
before environments are resolved, so a duplicate never fails because a host went
offline in between. The database key is the concurrency arbiter: the insert
loser reloads the winner instead of surfacing a 500. Manual dispatch reports
`deduplicated: true` and never claims exactly-once delivery — the promise is
that one key names one run.

**Offline means "write nothing", and preflight is the enforcement.** Every
step's environment is resolved through `environments::require_for_dispatch`
before the run transaction opens. `daemon_not_connected` therefore leaves zero
rows, matching the documented error contract. The presence check is not a
queue: the trigger must fire again later. If a host vanishes between preflight
and the first frame, the just-created run is deleted, preserving the same rule.

**The execution id is deterministic and stored before the frame.** A step's id
is `sha256(run_public_id + step_id)` shaped like a UUID. A crash between the
row write and the send, a lost response and a restart all resend the same
`create_agent`; the host answers with the existing execution. Recovery applies
one decision table: absent → send, queued/running → replay, succeeded → record
the ledger's completion, failed/interrupted → record interrupted and never send
the prompt again. Startup recovery runs while hosts are still offline (so most
intents defer); reconnect recovery per host replays them once the socket is
live.

**Conditions skip; deadlines stop.** A false condition marks only that step
`skipped` and continues with the next step, so an optional branch cannot fail a
run. The run deadline is the workflow runtime from creation; each step's
effective maximum is capped by the remaining run time. A deadline check runs
before every step start and again on completion; failing it marks the run
`failed` and every later pending step `skipped` with `run_deadline`. Idle
timeouts are stored on the workflow but not enforced yet: they need per-output
timestamps, which belong to the activity/observation record in 1.4.7.

**Manual input parsing is a prefix rule.** Only leading `name=value` tokens
whose name is a declared input are consumed, each at most once; the first other
token begins the request text, whitespace preserved after it. Defaults are
applied before required checks. An unknown key at the start is prose, not an
error, so a request that happens to contain `=` is never rejected.

**Trigger pattern guards are globs, not regexes.** The schema has no regex
dependency, so `*` and `?` are the only metacharacters and everything else is
literal. That keeps an authored filter from becoming a regex denial-of-service
and is pinned by unit tests.

---

## 2026-10-21 — Activity log, receipts and continuation (task 1.4.7)

**A receipt is written before matching, so "arrived but matched nothing" is a
recorded outcome.** The unique key is `(organization, provider, connection,
delivery key)`. `INSERT IGNORE` is the arbiter: a retry increments
`duplicate_count` on the same row and returns the run ids stored on it instead
of dispatching again. The receipt is then finished as `matched`, `unmatched` or
`rejected`, so a delivery that errors during matching is still visible. Manual
dispatches keep their own `(trigger, delivery_key)` dedup rule and do not write
provider receipts, because they are not provider deliveries.

**The activity read is a union, not a second log to keep in sync.** Receipts,
runs, run steps and executions are already durable; the read API unions them,
orders by observation time and id, and filters the union. Every branch carries
its own `organization_id` predicate, and project/provider/trigger/status are
applied after the union, so combining filters cannot widen the scope. The
trade-off is that a new record type must be added to the union, not just
written to a table.

**Continuation is decided before the run exists and recorded on it.** A
conversation key is the provider plus resource plus the channel/thread/issue
id the event carries. An existing conversation with an agent is continued; a
busy agent is also marked `steered`. A conversation whose agent is gone is
restored with a new agent. The run stores the decision, the reused execution
id and its own resolved output limits, so each arrival's limits and completion
status stay independent even when the conversation is shared. The host
protocol has no separate steer frame yet, so steering is recorded as intent on
the arrival; the create-intent replay remains the transport mechanism.

**MySQL is the only store; there is no embedded-database mode.** The pool
targets MySQL 8 (`migrations/`, sqlx `mysql` feature) and treats it as the
system of record for identity, connection credentials, configuration
revisions, runs and chatrooms. SQLite or any embedded engine is deliberately
not offered: multi-writer behaviour, `ENUM`/`CHECK` semantics and the
`information_schema` diagnostics the migrations rely on differ enough that a
second engine would double the schema surface for no self-hosting gain. The
standalone decision also keeps backup and restore a single documented
`mysqldump` procedure. Postgres parity remains out of scope until after 1.0.

**Migrations are forward-only and gate startup.** `db::prepare_database` is
the only supported boot path: it builds the pool, applies the embedded
migrations and returns an error on failure, and `main` exits rather than
listening against a partially migrated schema. There is no degraded mode for
the database.

**Execution ids are never shared across runs.** Conversation continuation
records *intent* (`created`, `continued`, `restored`, `steered`), not a reused
execution: `run_steps.execution_id` is unique per row and each arrival
dispatches its own executions. Carrying a message into a live agent needs a
message/steer frame the host protocol does not have yet, so actual agent reuse
belongs to the session relay (1.5.5). Recording an unused
`reused_execution_id` implied a reuse the lifecycle never performed, so the
column and its API field were removed rather than left as dead state.

**Output limits are snapshotted per arrival.** They are resolved from the
run's workflow snapshot into `output_limits_json` at creation, alongside the
existing workflow/environment/partial snapshots. A later revision cannot
retroactively loosen or tighten what an earlier arrival was allowed to emit.

---

## 2026-10-29 — Hub API contract: one route+schema table, versioned artifacts, resource-first with explicit commands (task 2.0.1)

**Context:** the hub surface grew to 60 operations across identity, hosts,
connections, projects, triggers, runs, activity, chatrooms, relay and
webhooks, but the OpenAPI document only covered 45 of them and described every
body as an opaque object. The SDK tasks (2.0.2–2.0.6) need one contract they
can generate from, and a contract that changes silently is worse than no
contract.

**Options considered:**
- `#[utoipa::path]` + `ToSchema` derives on every handler and every DTO. The
  most idiomatic route, but it touches ~20 module files (outside this task's
  owned paths), splits the contract across the codebase, and still needs a
  separate inventory check to catch an undocumented route.
- Hand-written prose reference only. Cheap, but not machine-readable and not
  enforceable; SDK drift becomes a review problem.
- One `ROUTES` table plus one `SCHEMAS` table in `src/api/openapi.rs`, with
  generated artifacts checked byte for byte (chosen). One place to update, one
  place to review, and the route-registration parser in the existing drift
  test already had the mechanism.

**Decision:**
- **Document generation is table-driven.** `ROUTES` now carries `surface`,
  `access`, `required_scope`, `required_role`, request and response component
  names, success status, alternative success statuses (idempotent replay) and
  per-route error statuses. `SCHEMAS` describes every hub request/response
  body field by field, including nullability. Hub operations carry
  `x-surface`, `x-auth`, `x-required-scope`, `x-required-role` and
  `x-contract-version`.
- **Three drift gates.** The hub module registration parser must equal the
  table (`every_registered_hub_route_is_in_the_table`); the served document
  and generated reference must equal the committed copies
  (`published_contract_matches_generated_document`,
  `api_reference_matches_generated_document`); CI boots the degraded server,
  diffs the served document against the artifact and validates it against the
  OpenAPI 3.1 meta-schema. Adding a route without documenting it, or
  documenting it without regenerating, fails.
- **Stable, pinnable paths.** `/openapi.json` and `/openapi/v1.json` serve the
  same bytes; `docs/api/openapi-v1.json` is the committed release copy, and
  `docs/API_REFERENCE.md` is generated from the same tables. The 3.1
  meta-schema validation caught a real defect during development (path
  parameters lacked `required: true`), so CI runs it rather than only the
  in-repo structural checks.
- **Resource-first, commands by exception.** Resources (hosts, connections,
  projects, revisions, keys, members, invitations, rooms, relay state) keep
  CRUD verbs and ids. Operations with no addressable resource or that are
  transitions (validate, install, dispatch, mint/exchange enrollment, password
  change, publish) are explicit `POST` commands under a verb path; retry-safe
  commands carry `deliveryKey` or `execution_id`. The reasoning and the full
  command table are in `docs/API_CONTRACT.md`.
- **Stability policy is written down.** `docs/API_CONTRACT.md` lists what is
  stable in `v1`, what may change additively, what is breaking, and the
  announce-and-overlap process (major bump, both documents served,
  `deprecated`/`Deprecation`/`Sunset`, `⚠️ BREAKING` changelog, at least one
  minor release or 90 days).

**Consequences:**
- `cargo test openapi` is now a contract gate, not just a route inventory.
  Regeneration is explicit: two ignored tests write the artifact and the
  reference.
- The game surface is unchanged on the wire; its documentation stays loose
  (no per-route schemas) because existing players are already shipping against
  it and the contract's job is the hub.
- The `x-auth` vocabulary gained `member_or_api_key` and `signature`; the old
  CI check inferred `api_key` from the presence of a scope, which was wrong
  for routes that accept either a member or a key.
- Adding a hub route is a two-file change (table + regeneration) and CI
  rejects anything less.

---

## 2026-10-29 — TypeScript SDK: generated from the committed contract, fetch-only, bounded retries (task 2.0.2)

**Context:** the contract (2.0.1) describes 83 operations, but every client
until now hand-wrote its own requests. A hand-written SDK drifts the moment a
route or field changes, and a generated SDK with a runtime it cannot type is
not much better: the browser build must not pull in Node, and rate-limit
handling must not be a `while (true)`.

**Options considered:**
- **A generic `request(path, method)` wrapper.** Small, but it does not give a
  typed method per route, so the contract cannot be enforced. Rejected.
- **OpenAPI codegen as a dependency (`openapi-typescript`, `orval`).** It would
  generate types but no runtime, still needs a hand-written client, and adds a
  build-time dependency the repo would have to install and pin. The renderer is
  ~400 lines of plain Node instead. Rejected.
- **Ship compiled JS from a TypeScript source tree.** Every consumer install
  would need a build step, and the committed output would be unreviewable
  machine code. Rejected: the package ships hand-written JS with generated
  `.d.ts` declarations, so editors and `tsc` see the contract's types and the
  runtime is reviewable.
- **Chosen: generated declarations plus a generated route table and resource
  namespace.** `docs/api/openapi-v1.json` is the only input; `npm run check`
  regenerates in memory and compares byte for byte. The one explicit
  `operationId → namespace.method` map is the review point, and an unmapped
  route fails generation rather than shipping untyped.

**Consequences:**
- `fetch` is the whole transport (`Response`, `Headers`, `AbortController`),
  so the browser entry reaches no `node:` module. `src/node.js` is the only
  file that imports `node:fs`, and a test walks the import graph from
  `src/browser.js` to prove it.
- Retries are 429-only and bounded by `maxRetries` (default 2): a 429 means the
  request was not processed, so replaying it cannot duplicate a write even for
  a non-idempotent command. A `Retry-After` longer than `maxRetryAfterMs`
  (default 60 s) is surfaced instead of slept through, because sleeping an hour
  inside a request is worse than telling the caller.
- Token refresh is at most once per request (proactive on known expiry,
  reactive on 401). The refresh hook returning `null` clears the retry, so a
  stale credential cannot loop.
- The document publishes no query parameters; the SDK types the handler names
  it can verify and records the gap in `docs/API_CONTRACT.md` §5.1 rather than
  pretending the contract covers paging.

---

## 2026-10-29 — Rust SDK: one contract, one typed request per route, blocking by default (task 2.0.3)

**Context:** the ratatui console, the reference host agent and the Zoid client
all need the hub contract from Rust. The TypeScript SDK already defines the
resource shape, but Rust consumers live in a terminal and on an agent that must
not care which async executor is fashionable. The crate therefore has to be the
same library again, not a second opinion, and it must be usable with no runtime.

**Options considered:**
- **Async-only, Tokio required.** One transport, but every consumer pays for a
  runtime even when it only lists hosts once, and the CLI would have to create
  one. Rejected: "no unavoidably async runtime choice" is a stated goal.
- **A hand-written method per route.** Readable, but 83 routes and 90 schemas
  drift the day the contract changes, and the drift is silent. Rejected.
- **OpenAPI codegen as a dependency.** The TS SDK already proved a ~600-line
  local renderer is enough and keeps the review point explicit. Rejected.
- **Chosen: a feature-split crate with generated artifacts.** No default
  feature is the contract-only core; `blocking` (default) is `ureq` with no
  runtime; `async` is `reqwest` and therefore Tokio. `docs/api/openapi-v1.json`
  is the only input, and every route becomes a type implementing
  `route::Endpoint` with a typed `Response`, path, query and body. The
  `operationId → namespace.method` map is explicit and fails generation if a
  route is unmapped, exactly as in `sdk/ts`.

**Consequences:**
- The test suite parses the committed contract and asserts that the route
  table is the same set of operations with the same methods, paths, path
  parameters and body presence. Node is not needed to run `cargo test`; the
  generator's `--check` is the byte-for-byte gate in CI.
- Generated files are rustfmt-formatted by the generator itself, so `cargo fmt
  --check` and `generate.mjs --check` agree. Without that, the two gates would
  fight.
- The blocking facade is the richer one: cursor iterators and a real socket
  (`tungstenite`). The async facade returns the same typed stream handle and
  leaves the socket to the caller, avoiding a second async-stack dependency
  (`tokio-tungstenite`) for a feature only one client uses.
- Retry semantics are copied deliberately from the TypeScript SDK so the
  conformance suite in 2.0.5 can compare them: 429 only, bounded, `Retry-After`
  honoured up to 60 s, refresh at most once per request. Only the
  delta-seconds `Retry-After` form is parsed; the pool sends nothing else.
- `Problem` is parsed leniently (only `code` is required) so a failure from a
  proxy or a game route still yields a typed error with the status and the
  `x-request-id` header.

---

### 2026-10-29 — The Python SDK is stdlib-only, with one generated surface

**Context:** task 2.0.4 adds the Python binding for the existing agent tooling
(TideTable, TKRun). Those tools are dependency-light and often run in a bare
interpreter; the SDK must be pip-installable from a wheel with no optional
extra, and it must not become a third opinion about retries or errors.

**Options considered:**
- **`httpx` for both facades.** One transport and real async, but it becomes a
  hard dependency and its retry/timeout policy is another thing to align with
  the other SDKs. Rejected for the "no optional extra" requirement.
- **`requests` + `aiohttp`.** Two dependencies, two APIs and a larger install
  for a client that mostly lists hosts and dispatches runs. Rejected.
- **A thread-pool wrapper as the async client.** No dependencies, but it is not
  asynchronous under the hood and would mislead a caller that expects
  concurrency. Rejected.
- **Chosen: `http.client` for sync and `asyncio` streams for async.** Both
  transports are in the standard library; only the socket implementation
  differs. Request assembly, retry policy, refresh-once, problem parsing and
  decoding are shared, so the two facades cannot drift. The generated modules
  hold the route table, one dataclass per schema and one resource method per
  route per facade; `sdk/python/scripts/generate.py --check` is the drift gate.

**Consequences:**
- The wheel has no `Requires-Dist` and no extras. The packaging test builds it
  and imports it through `zipimport`, so the claim is checked, not asserted.
- The async transport owns a small HTTP/1.1 reader (content-length and chunked
  bodies). It deliberately does not implement HTTP/2 or compression: the pool
  serves JSON over HTTP/1.1 and the other SDKs send no `Accept-Encoding` either.
- Models are tolerant on the way in and exact on the way out: unknown response
  members survive `to_dict`, absent optional members stay absent, and open-enum
  values stay opaque strings. The model round-trip test runs over every schema.
- The WebSocket is a typed handle plus frame parsing, not a socket driver, for
  the same reason as the Rust async facade: a caller can use the library it
  already has, and the client adds no second async stack.
- Route coverage is a test, not a checklist: the recorded HTTP server answers
  every contract route and fails on an unexpected method or path, on both
  facades.

---

## 2026-10-29 — SDK conformance: one scenario list, three adapter processes, double comparison (task 2.0.5)

**Context.** Three SDKs were written against the same contract by three
different generators, so they could agree with the document and still disagree
with each other on shapes the document does not pin: the exact problem code for
a refusal, the cursor a page returns, how a socket frame is classified.

**Decision:**
- **A binding is a process, not a test file.** Every SDK ships one adapter that
  answers six generic commands (`contract`, `describe`, `call`, `page`,
  `stream`, `frames`) over newline-delimited JSON. The harness never imports an
  SDK and holds no per-language code, so a scenario is data: adding one adds a
  step to a JSON file and all three bindings must answer it. That is what makes
  "same library three times" enforceable rather than aspirational.
- **Compare twice.** Each answer is checked against the step's `expect`
  pattern (the contract's promise) *and* against the other bindings for the
  paths the step lists in `agree` (the promise the contract does not spell out).
  The second check is what catches a divergence neither the document nor a
  single binding's test would fail on.
- **An incomplete run is never a pass.** `--bindings` exists for debugging and
  exits 3; a missing adapter, a crashed adapter and a skipped step all fail.
- **The suite runs against a real pool.** `provision.py` creates the state over
  the public API and starts the reference host agent for the connected host; CI
  supplies MySQL, Redis and the server. A mock would agree with itself and
  prove nothing about the pool's problem codes or cursors.
- **The harness is gated locally.** `tests/conformance_suite.rs` runs the
  harness selftest and the scenario listing under `cargo test`, so a broken
  scenario or a matcher that stopped failing divergences breaks the normal
  gate without needing a database.

**Consequences:**
- Captures are scoped per binding. A scenario that registers a host and then
  mints its enrollment token must not read the sibling binding's id: the token
  is single use, so a shared scope would fail for the wrong reason.
- `expect` accepts dotted keys as paths as well as nested objects, so a
  scenario can pin `body.run.id` without nesting the whole response.
- The suite is the reason `ChatFrame::cursor()` in the Rust SDK now matches the
  envelope: `cursor` is the `chat.current` resume point, and a live message
  exposes its sequence on the message. Python and TypeScript already did this;
  the three-way comparison is what surfaced it.
- A new *command* is still an adapter change by design. That is the boundary:
  the suite owns behaviour, the adapter owns translation.

---

## 2026-10-29 — The CLI is a client with a store of its own (task 2.0.6)

**Context.** The hub surface is now a typed contract with three SDKs. The
operator still needs a shell client for the things a script does at 3am: sign
in, enrol a machine, install a trigger, dispatch and follow a run, read a room,
follow a session's relay. The console (2.2.x) will need the same credential
store, and both must be usable over SSH with no browser.

**Decision:**
- **One command, one SDK call, printed as the SDK's type.** `--json` is
  `serde_json` of the value the SDK decoded, not a CLI-shaped view, so the
  output is the contract shape and a script cannot drift from the SDK. The
  human mode is a table over the same value.
- **No argument-parsing dependency.** The grammar is a flag table in
  `cli/src/args.rs`: unknown flags, inapplicable flags and duplicate flags are
  refused. It keeps the CLI's dependency set to the SDK, `serde` and
  `serde_json`, which is the same discipline the SDK applies to its own
  dependencies.
- **The store is the CLI's, outside any repository.** The file is
  `$TIDEPOOL_HOME/credentials.json` or the XDG config directory, written
  atomically with `0600`, and `save` refuses a path whose ancestors contain
  `.git`. The refusal is a test, not a convention: "a credential is never
  written into a repository" is enforced mechanically.
- **Redaction is a function, not a habit.** `credentials::redact` keeps four
  leading characters and the length; `Debug` for the store and the file applies
  it too. `login --json` still deserialises into `PoolLoginResponse`, with the
  `token` field redacted, so the shape stays the SDK's type while the value
  cannot be replayed from a log.
- **The two loops are named as loops.** v1 has no run stream and no
  trigger-list route, so `run watch` polls `GET /v1/activity/runs/{run}` and
  `trigger list` reads the project's active configuration revision. Relay
  following uses the SDK's cursor iterator; the CLI never walks a cursor.
- **The password comes from stdin only.** No `--password` flag exists, so it
  cannot land in `argv`, shell history or a process listing.

**Consequences:**
- The console (2.2.5) reads the same file and the same shape; a sign-in from
  either is visible to the other.
- A host enrolled by the CLI keeps its relationship credential in the store,
  ready for the host agent that task 2.3.2 adds.
- The CLI cannot list triggers as resources because the pool does not: it shows
  the authored revision, which is where the pool keeps them. If a trigger-list
  route lands, `trigger list` becomes one different SDK call and nothing else
  changes.

---

### 2026-10-29 — The control plane consumes clawffice-design as a pinned, drift-checked snapshot

**Context:** task 2.1.1 scaffolds `tidepool-control-plane`, which must render the
five Clawffice palettes with Biolume default and Tidepool's `tide` hue driving
focus and selection. `clawffice-design` is a separate repository, the root
README says its tokens are "referenced, never vendored", and the plan allows
either GitHub Packages or a cloned submodule. Neither is available to an
offline checkout or to CI without a second clone, and a web app that cannot
build is worse than one with a stale-but-labelled copy of the tokens.

**Options considered:**
- Depend on `clawffice-design` from a registry — needs a release and a token,
  and the package has no published entry point today.
- Copy the palette values into CSS by hand — no provenance, and nothing fails
  when the design system changes.
- Generate a pinned JSON snapshot at build time and test it against a sibling
  checkout (chosen) — an offline clone builds, the snapshot records the commit
  it came from, and drift is a test failure rather than a silent divergence.

**Decision:**
- `src/lib/design/clawffice-design.snapshot.json` holds `tokens/core.json`,
  `tokens/apps/tidepool.json`, `tokens/cells.json` and the five palette files,
  generated by `src/lib/design/snapshot.mjs` from `$CLAWFFICE_DESIGN_DIR` or a
  sibling checkout. `npm run design:check` fails when it is stale.
- `design.drift.test.ts` compares the snapshot to the checkout file by file and
  skips when no checkout is present, so CI stays green without the design repo
  while a developer machine catches drift.
- Tokens become `--tp-*` custom properties in `tokens.ts`; no component holds a
  colour literal. Focus and selection read `--tp-focus` (the `tide` hue for the
  palette's scheme), never the palette primary, so identity survives a palette
  switch.
- Status is a typed table (`src/lib/status.ts`) of word + glyph + colour role.
  A status cannot be added without a word and an icon, which is design rule 8
  enforced by a test rather than by review.

**Consequences:**
- Adding a palette or changing a token is a two-step change: update the design
  repository, then `npm run design:snapshot`; forgetting the second step fails
  `design.drift.test.ts` on any machine that clones both.
- Component tests render through `svelte/server` in the `node` environment, so
  the suite needs no jsdom, no browser and no pool; the interactive parts
  (hash navigation, store subscription) are exercised through their pure
  modules. A browser pass is still needed for how it looks.
- The status table is the one place future views (hosts 2.1.3, runs 2.1.7) take
  their state vocabulary from; a view that adds a status adds a word and an
  icon in the same change.

---

## 2026-10-29 — Control plane sessions and role gating (task 2.1.2)

**Decision: the browser session is the pool JWT, remembered in web storage with
the lifetime the pool reported.** `AuthSession` stores the token, its
`expiresAt` instant, the operator and (once `/v1/pool/me` answers) the
organization. A reload reads the record, rejects it locally when the lifetime
has passed, and otherwise verifies it with `GET /v1/pool/me`; a 401/403 clears
the record and returns to sign-in, while a network failure keeps the remembered
session rather than throwing a person out over a blip. The controller is
DOM-free and takes the gateway, storage and clock by injection, which is why
the lifecycle is tested headlessly.

**Decision: controls are gated from the contract, not from a hand-written role
table.** Every `ControlSpec` names the hub operation it will call and records
whether that operation mutates and whether it is owner-only. The unit test
cross-checks every entry, for every role, against the generated SDK's
`allows()`, so a contract change that narrows a route fails the mapping. A
prohibited control is not rendered as a button; it appears in a closed
disclosure with the reason (`owner-only` or `viewer is read-only`).

**Decision: sign-out must be a server request, and the local token is dropped
even when the request fails.** The controller asks the pool to end the session
first, then clears storage. If the pool cannot confirm, the person is signed
out locally and the failure is shown — never silently swallowed.

**Open: the hub contract has no logout route.** The control plane calls
`POST /v1/pool/logout` with the bearer token, the natural companion of
`/v1/pool/login`. Against a pool without that route the call fails with
`session_revocation_unavailable`, the local token is still cleared, and the UI
says the pool could not confirm. Until a pool task adds the route (and a
server-side denylist or session record), "sign-out invalidates the session
server-side" cannot be guaranteed against a live pool; the end-to-end test
proves the client half against a local pool that implements the route.

**Consequences:**
- Any future mutating view adds its control to `src/auth/roles.ts`; the
  contract cross-check test then proves the gate matches the pool.
- Storing the JWT in `localStorage` means an XSS can read it. The app has no
  cookie-based session option yet; a pool-set HttpOnly cookie would be the
  stronger shape and is a contract change, not a client-only one.
- The session record is versioned (`v: 1`); changing its shape must bump the
  version so an old record is discarded rather than half-read.

---

## 2026-10-29 — Hosts view: connection and execution are separate (task 2.1.3)

**Decision: the hosts page renders two independent facts per host — lifecycle
status and the `host.execute` grant — and never folds one into the other.** The
status badge answers "what is the pool's relationship with this machine";
the execution chip answers "may the pool dispatch runs to it". A connected host
without the grant is labelled *connected only*, in the protocol's own words, and
the grant action sits beside it. A revoked host shows no execution action at
all. This is why the page has two chips rather than one combined state: an
operator debugging "why did nothing run" must be able to see that the machine is
reachable and still not permitted.

**Decision: granting and revoking execution is a labelled action with its
consequence printed before the confirm step, not a toggle.** `executionAction`
produces the consequence sentence, the restated confirm sentence naming the
host's slug, and the confirm button label; the component prints the consequence
beside the button and opens the confirm step on the first click. Revoking keeps
the host enrolled and connected, which the copy says explicitly; granting says
the pool may start agents from then on. A toggle with a hover tooltip would let
a person change dispatch behaviour without reading what changed.

**Decision: the one-time token lives in the controller's state and is dropped
the moment it is dismissed or the page reloads.** The pool stores only a hash,
so the token cannot be fetched again; showing it in a permanent panel would
imply it could be. The enrolment panel builds the command from the token and the
pool URL at that moment and replaces the form, so it cannot be minted twice by
accident.

**Decision: the enrolment command is the reference agent's real invocation.**
It derives the WebSocket URL from the pool URL (`http`→`ws`, `https`→`wss`,
`/ws/host`), shell-quotes the token, generates the relationship credential on
the host and keeps it in `~/.tidepool/host-credential`, then runs
`cargo run -p tidepool-host-agent`. The host protocol requires the host to hold
its own credential, so a command that passed a fresh credential inline each run
would be wrong; this one keeps it.

**Open: the contract has no last-seen timestamp.** `HostView` carries
`created_at` and `revoked_at` only. The page still renders Last seen for every
host, reads `last_seen_at` if a pool starts sending it, and otherwise says the
pool did not report it. It does not substitute `created_at` or `updated_at`,
which would be a different fact. Adding `last_seen_at` is a pool/contract task;
until then the hosts view is honest about the gap rather than approximating it.

**Consequences:**
- Any future status word for hosts belongs in `HOST_STATUS_DESCRIPTORS`, not in
  a component, so the icon-plus-word rule and the distinct-word test keep
  covering it.
- The hosts route now needs the session token and role from the shell; the
  route wrapper and `App.svelte` pass them through. A future page-level data
  layer should replace that prop threading rather than adding another ad hoc
  controller in a component.
- The repository's `npm test` suite requires `npm install` in
  `tidepool-control-plane` (no `node_modules` is checked in); CI has no frontend
  job yet, so a control-plane change is verified locally.

---

## 2026-10-29 — Connections view: write-only credentials and named checks (task 2.1.4)

**Decision: the credential field has no read path at any layer.** The pool
contract projects `credential_configured` as a boolean and never the reference
or value. The control-plane model mirrors that: `secretFieldView` takes only
the boolean, its `value` field is typed `null`, and the controller's view state
contains no credential input. The only credential action is *replace*, and the
replacement input is cleared on submit. This is deliberately stronger than
"don't render the secret": there is nowhere in the model to put one.

**Decision: verification is a named list of checks, not a boolean.** The
contract has no `/verify` operation, so the page reads the connection and runs
five checks: the pool answered, the connection is active, the credential is
configured, the resource scope is registered, and the transport is ready. A
failure names the check and uses the provider's own words (for example Slack's
`bot_token, signing_secret, app_token` requirement, or the pool's exact
problem-detail sentence when the read itself failed). This keeps
"verification failed" actionable, which a bare false cannot be.

**Open: the contract carries neither transport nor credential rotation.**
`ConnectionView` has no transport field, so a Slack webhook and a Slack socket
connection are indistinguishable after a reload; the page remembers the
transport chosen here per connection id as a browser preference and defaults
to webhook, and says so. `PATCH /v1/connections/{id}` accepts no
`credential_ref`, so the replace flow reports the gap and tells the operator to
rotate the secret in the pool environment, then verify. A future contract
revision that adds `transport` and a rotation operation should remove the
preference and the explanatory copy, not work around them.

**Decision: the origin-change list is derived from the origin the page last
showed.** The page remembers `https://…` in `localStorage` and compares it with
the configured pool origin on the next visit. For every webhook connection it
prints the old URL beside the new one; socket mode and the gateway are absent
because there is nothing at the provider to change. The rendering and the
derivation are tested; confirming it after editing the pool URL in Settings in
the running app needs a human.

**Consequences:**
- Any new provider belongs in `CONNECTION_PROVIDERS_CATALOG` (transports,
  credential keys, resource kinds, callback path), not in a component.
- Verification words and requirements live beside the catalogue, so a provider
  transport change cannot drift from what the page tells an operator.
- The page needs the session token and role from the shell, as hosts does; the
  route wrapper and `App.svelte` pass them through.

---

## 2026-10-29 — Projects and the configuration editor (task 2.1.5)

**Decision: the editor is a text editor over authored files, not a model that
renders a document.** A `Bundle` is an ordered list of `{path, content}` pairs;
`editBundleFile` replaces the one path's content and returns every untouched
file object unchanged, and `requestFromBundle` sends those bytes verbatim. No
parse/stringify cycle sits between the textarea and the validate/install body,
so an unedited file cannot be reindented, reordered or have its trailing
whitespace normalised. Tests assert untouched content byte-for-byte (including
CRLF, tabs and a missing final newline) and unchanged file identity.

**Decision: field errors are located by parsing the authored manifest, not by
searching strings.** The pool names a field path; the editor parses the
manifest with byte offsets, indexes both name and `[index]` spellings (plus the
value of string-array items, which is how `hosts.studio-mac` resolves), and
shortens the path to the nearest authored ancestor when a property is missing.
Unresolvable paths are reported at line 1 with `located: false` rather than
pointed at a guessed line. This is why a field error can be shown beside the
line instead of beside a name.

**Decision: a save validates first and installs only a valid bundle.** The
validate call is the pool's own endpoint. A refusal never reaches install, so
the active revision cannot move; the draft and its broken text stay in the
editor with the issues attached. If install returns `activated: false` the
controller treats it as a refusal too, reloads the history and states that the
previously active revision is unchanged. A transport failure is `failed`, also
leaving the active revision alone.

**Decision: the active revision is resolved from the history and never
inferred.** `resolveActiveRevision` accepts exactly one `active` flag. Zero
active revisions with a non-empty history, more than one active, or a served
active revision the history does not mark active all produce an explicit
problem; the page disables saving rather than editing on top of an ambiguous
configuration.

**Decision: diffs are over authored files.** `diffBundles` walks the bundles in
authored order (base order, then files new to the target), reports added,
removed, changed or unchanged, and aligns changed files line by line. A file
above the alignment cap summarizes by line counts instead of freezing the page.
`compare` takes an optional base revision; with none it compares against the
active revision, which is what the history's compare control asks for.

**Consequences:**
- Any new configuration field the pool validates needs no editor change for
  display: the issue list renders whatever path the pool names, located by the
  same parser. A field naming a *different* authored file would need the locator
  extended, and the contract currently names only manifest fields.
- The route wiring lives in `src/lib/router.ts` and `src/App.svelte`, outside
  the task's owned paths; the page cannot be reached without those edits.
- The project-create flow is not part of this page; projects are created
  through the pool's existing endpoint and appear in the list.

---

## 2026-10-29 — Trigger editor: one document, host-checked choices (task 2.1.6)

**Decision: the document is canonical and YAML is a projection of it.** The
editor keeps one `TriggerDocument`; a form edit rewrites it and regenerates the
YAML, and a YAML edit is parsed and adopted only when it parses. The YAML
reader is deliberately small and strict: it accepts the trigger's mapping,
sequence and scalar shapes, refuses tabs, block scalars, inline mappings,
duplicate keys and unknown keys, and reports the source line. A refusal leaves
the last parsed document in place and the page refuses to switch to the form
until the text is fixed, so the form never claims text it does not describe.
The alternative — a form model that regenerates YAML on every keystroke and a
YAML model that regenerates the form — lets the two drift, which the success
criterion forbids.

**Decision: availability is a comparison, never a rewrite.** A host's report is
narrowed to the pool's `offered_providers`; the cascade offers those providers,
their models, their modes and their thinking options. `selectionAvailability`
compares the authored provider/model/mode/thinking to those lists and returns
`available`, `unavailable`, `unset` or `unknown`. An unavailable value is kept
in the document and in its select, labelled `(unavailable)`, and `preflightIssues`
refuses the save at the contract field (`run.agent.model`) with the pool's own
wording (`host 'swaygent' cannot run 'claude-2'`). No code path chooses a
fallback: only the author or a host that reports the value again changes it.
With no report (loading, offline, unknown host) the state is `unknown`, not
`unavailable`; the editor cannot claim what it has not heard.

**Decision: save is the pool's validation twice.** `preflightIssues` mirrors
the pool's structural rules, the `from_users` allowlist, the execution grant
and the agent check, so a host that cannot run the authored agent is refused
before a request leaves the browser. A preflight pass then calls
`POST /v1/triggers/validate`; only `valid: true` reaches
`POST /v1/triggers/install`. A refusal writes nothing, keeps the draft and its
issues, and never moves a previously installed revision. A pool error is a
failure state, not a silent success.

**Decision: host and directory changes reload the choices, and stale answers
lose.** Both `setHost` and `setDirectory` call `GET /v1/hosts/{id}/capabilities`;
a monotonically increasing token means an earlier answer cannot overwrite a
newer one when the author changes twice quickly. An offline host is explained
in words and offered a retry; the rest of the trigger stays editable. A host
the pool already lists as offline is not asked at all.

**Consequence: the working directory is editor context, not document state.**
`TriggerRun` is `{ host, agent }`; the contract has no directory field and
denies unknown fields. The editor therefore keeps the directory in its own
state (it is what a host is asked about, and changing it must reload), and
does not write it into either view or the install body. Adding it to the
contract would be a server task, not one this page can invent.

**Consequence: route wiring is outside the owned paths.** The page is at
`#/triggers` through edits to `src/lib/router.ts` and `src/App.svelte` (and the
router test), plus the thin `src/routes/Triggers.svelte` wrapper. Without those
the route cannot be reached.

---

## 2026-10-29 — Runs and activity: filter at the page, follow with a timer (task 2.1.7)

**Decision: the page re-applies its filters to every returned page.** The hub
already applies `project`, `provider`, `trigger`, `status` and `kind` to the
whole union (`src/services/activity.rs`), but the success criterion is about
what a person sees, so `runs.ts` also filters every page before it reaches the
view and again when the page renders. A missing `project` or `provider` is a
non-match, not a wildcard: an unattributable union row can never appear under a
chosen project. The cost is a second, cheap pass over at most 25 rows; the
benefit is that a stale response or a pool bug cannot leak another project's
runs into the page.

**Decision: "live" is a bounded poll, not a new stream.** The v1 contract has
`GET /v1/activity/runs/{run}` and no activity push channel; the session relay is
for chat, not run state. `RunsController` therefore reads the detail again on a
`RunsScheduler` timer while `runIsLive` is true, which includes a run whose word
is `succeeded` but whose last step is still `running`. A terminal detail stops
the timer; a 404 stops it; a network blip keeps it while the last known detail
was live. The timer is injected so tests fire the exact callback a browser
would, with no real delay. If the hub later adds a run stream, only the
controller's scheduling changes.

**Decision: failure codes are a table, and the code is never dropped.** The
pool stores stable codes with step failures (`daemon_not_connected: host 'x' is
offline`, `execution_failed`, `invalid_output`, `step_deadline`). The page does
not paraphrase the raw string and does not replace it: `FAILURE_SENTENCES`
maps known codes to a sentence, the remaining detail stays underneath, and the
code is rendered in a `<code>` element for a bug report. An unknown code still
shows the pool's text and the code. `daemon_not_connected` is deliberately
worded as "The host went offline…" rather than as a generic error, which is the
criterion the runner flagged for human review.

**Decision: output is rendered and copied verbatim.** `stepOutputText` returns
the stored string unchanged; the component puts it in a `<pre>` (escaped by
Svelte, never parsed as markdown or JSON) and the copy control writes the same
string with no trim. The clipboard writer is injectable, so the copy path is
tested exactly and a refused clipboard is reported beside the text instead of
failing the page.

**Consequence: routing and auth wiring are outside the owned paths.**
`src/routes/Runs.svelte` became a thin wrapper and `src/App.svelte` now passes
`pool`, `token` and `role`; without those the page could not load or gate the
dispatch control. Everything else is inside `src/lib/runs.ts` and
`src/routes/runs/`.

---

## 2026-10-29 — Members and API keys: refuse first, explain in words, keep the record (task 2.1.8)

**Decision: the last-owner rule is enforced in the page before the request,
and the pool's own refusal is rendered in the same place.** The hub refuses to
remove or demote the sole owner (`409`), but the success criterion is that a
person reads the reason, not that a button fails. `removalRule` and
`roleChangeRule` therefore block the call locally and attach
`LAST_OWNER_REASON` to the row, which the component prints beside the disabled
role select and remove button. The controller repeats the check, so a call
cannot slip through from a non-component caller. When the page's owner count is
stale — another session removed an owner — the pool's 409 arrives through
`presentProblem` and shows the pool's detail in the same problem panel. The
local rule shortens no request that should be made; it only stops one the
contract already forbids.

**Decision: a one-time value lives in controller state, not in the row.** The
API key's secret and the invitation's acceptance token are returned exactly
once. Both are held as `createdKey` / `createdInvitation` and are cleared by
dismissing the panel or by any reload (`load` drops them), so a later render
cannot imply they can be read again. The secret is not copied into the key row,
the list markup or the URL. Copying uses `copySecret`, which takes the
clipboard by injection: success is a boolean, a refused clipboard prints a
"select it and copy by hand" note beside the text, and the test writes the
exact secret string.

**Decision: a revoked key stays in the list.** Revocation returns `204` with no
body, so `revokeKey` marks the row locally with the status descriptor, the
revocation instant and a note that it is kept for the audit trail;
`sortApiKeyRows` moves it after the active keys without removing it. The e2e
then reloads from the pool and sees the same revoked state, which proves the
list is not just an optimistic echo.

**Decision: owner-only reads are not requested by non-owners.** `GET
/v1/pool/members` allows any member, but invitations and API keys are
owner-only. `load({ includeOwnerData })` is false for every other role, so
opening Settings as a viewer does not raise a 403 for data the page would not
show. The viewer still sees the member list read-only with one sentence about
the owner-only controls.

**Consequence: routing and auth wiring are outside the owned paths.**
`src/routes/Settings.svelte` became the thin wrapper over
`./settings/SettingsPage.svelte`, and `src/App.svelte` now passes `pool`,
`token` and `role`; without those the page could not load or gate the controls.
Everything else is inside `src/lib/keys.ts` and `src/routes/settings/`.

---

## 2026-10-29 — Chatrooms in the browser: merge by id, step the cursor back, never imply current (task 2.1.9)

**Decision: the page merges by the pool's message id and sorts by the pool's
sequence, never by time.** A message can arrive from a history page, a live
`chat.message` frame, a reconnect replay and a `chat.history` frame, in any
order. `mergeMessages` keeps one row per id, lets the newer copy win and orders
by `sequence` with the id as a tie-break. A live frame therefore keeps the id
the pool minted, and a duplicate is invisible rather than a second row. The e2e
proves this across the real SDK: the sender's response and the receiver's live
frame name the same `server-121` id.

**Decision: "older" is a backward cursor walk, because the contract only pages
forward.** `GET /v1/chatrooms/{room}/messages` takes `after`, so there is no
cheap "before" read. `olderMessagesPage` steps `after` back one page at a time,
keeps only sequences below the oldest held message, and steps again when a
short page could mean sequence gaps rather than the room's start. The result is
the newest `pageSize` messages below the window, so merging cannot overlap what
the reader already has. This is the one place where the page pays a few extra
reads for a capability the server does not expose; changing the route table was
out of this task's scope and would be a contract change.

**Decision: currentness is a fetched fact, not a mood.** `current` is set only
after a history read reaches the head and, when a stream is open, after the
server's `chat.current` frame. Cached history, an in-flight fetch, a
`chat.catchUp` frame and a closed socket all render the catch-up or reconnecting
banner, and `isCurrent` is false whenever `catchingUp` is true. A dropped socket
puts the page straight back into catch-up; reconnecting reuses the last cursor
and stays in catch-up until `chat.current`, so a reconnect never claims a
currentness it has not confirmed.

**Decision: author kind is carried by word, glyph and label, not colour.** The
pool tells a person from an agent only through `Principal.kind`. The page turns
that into "Person"/"Agent session", a filled circle/diamond, an accessible
label and `data-author-kind`. The member list uses the same descriptor, so the
distinction is consistent between history and presence and survives a
monochrome palette.

**Decision: the controller notifies on every state change.** Live frames arrive
outside a click handler, so a page that only adopted the result of an `await`
would render history once and then freeze. `RoomsController` writes through a
private setter that calls `onChange`, and the page adopts every snapshot. That
also makes the cached history render under the catch-up banner while the first
fetch is still in flight, which is the behaviour the task calls for.

**Consequence: routing and auth wiring are outside the owned paths.**
`src/lib/router.ts`, `src/App.svelte` and the route-table test gained the
`/rooms` entry; `App.svelte` passes the pool, token and signed-in member id.
Everything else is inside `src/lib/rooms.ts` and `src/routes/rooms/`.

---

## 2026-10-29 — Settings, palettes and pool-served hosting: the browser owns the preferences, the pool marks its own origin (task 2.1.10)

**Decision: user preferences are browser storage, not build-time configuration.**
The pool URL, palette, clock and density are read from `localStorage` and
written on change, so pointing the app elsewhere or repainting it needs no
rebuild and no redeploy. `preferences.ts` is the only place that knows the keys
and the fallbacks: Biolume for the palette, 24-hour local comfortable for the
rest, and an empty pool URL means "use what the deployment gave you".

**Decision: the palette has one owner.** `Root` owns the theme store; the
settings page receives the current palette and a handler rather than writing
storage behind the shell's back. The choice is remembered under the same key the
theme store already used, so reload and live switch cannot diverge. Density is
applied by appending `--tp-space-*` overrides to the shell's inline token style,
which is why compact spacing reaches every page without touching `app.css`.

**Decision: the pool advertises itself to the app with a served marker, not a
baked URL.** The static handler in `src/main.rs` injects
`<meta name="tidepool-pool-origin" content="same-origin">` into the built
`index.html` and serves every other byte unchanged. The app resolves its pool in
this order: `VITE_POOL_URL`, then the saved URL, then — only with the marker —
the origin it was opened from. Consequence: one build runs on the pool's origin
with no CORS, the same build on a static host stays unconfigured rather than
calling the static host's API, and an operator can still repoint a pool-served
copy at another pool from Settings.

**Decision: the pool's static route is a fallback, never a shadow.** Only
requests no API route matched reach it, and it refuses the reserved first
segments outright. Pages (`/`, extensionless paths) fall back to `index.html`
because the router is hash-based; a missing file with an extension is a real
404. Every resolved path is canonicalised and must stay inside the bundle, so a
traversal or an escaping symlink cannot read the host. The directory must have
an `index.html` or startup fails: an empty bundle is a misconfiguration, not a
degraded mode.

**Consequence: shell wiring is outside the owned paths.**
`tidepool-control-plane/src/App.svelte`, `src/Root.svelte`, `src/main.ts` and
`src/routes/Settings.svelte` gained the preference props, the density style and
the pool-rebuild path; `docs/CHANGES.md` records the same. Everything decidable
lives in `src/routes/settings/preferences.ts` with tests.

---

## 2026-10-29 — The console borrows the CLI's login and has one action list (task 2.2.1)

**Decision: the console does not own a credential format.** It loads
`tidepool_cli::credentials::CredentialStore`, the same file and the same path
rules as the CLI, rather than copying the store or inventing a second login.
Signing in once therefore serves both binaries, and a store that the CLI refuses
to write is refused by the console for the same reason. The store's `Debug`
already redacts secrets, so the console never had to learn how.

**Decision: keys and the palette are the same actions.** `Action` is the only
list; `Action::ALL` is what the palette renders, and every key binding dispatches
one of those values. A test asserts the palette's entries equal `Action::ALL`, so
an action reachable from a key but not the palette (or the reverse) fails the
gate rather than drifting.

**Decision: startup probes at most once, and only with a pool and a token.**
`App::from_credentials` drops the client when the store names no pool, and
`App::start` only probes from `Connecting`; that is the mechanical form of "no
network call on startup unless a pool is configured". Any failure — transport,
refused URL, 401 — becomes `Connection::Unreachable` plus one notice, never a
panic and never a second notice: the notice is a single `Option`, so reconnect
replaces it instead of stacking. The console keeps taking keys in that state.

**Consequence: the workspace gained a member.** `tidepool-console` is in
`members` and `default-members` so `cargo fmt/clippy/test` cover it; that edit to
the root `Cargo.toml` is outside the task's `touches` but the crate cannot be
built by the existing gate without it.

---

## 2026-10-29 — Presence is polled, last seen is local, capabilities are verbatim (task 2.2.2)

**Decision: the hosts tab polls `GET /v1/hosts` instead of waiting for a push.**
The hub contract has no host-presence stream (the chatrooms have one; hosts do
not), and the console may not add an endpoint. So the shell's event loop calls
`App::tick`, the tab re-reads the list every two seconds, and a host that
disappears from `connected` is drawn offline on the next frame with no key
press. The read is throttled in the tab, so the 250 ms redraw does not become a
4 Hz API client, and a failed read keeps the previous rows on screen with one
notice rather than blanking the table.

**Decision: "last seen" is what this console saw, and says so.** `HostView` has
no `last_seen_at`; inventing one from `created_at` would be indistinguishable
from a real observation. The tab records the instant a poll last saw the host
`connected` and keeps it when a later poll reports the host offline; a host
never seen connected reads `never`. The pane's label is the observation, not a
pool timestamp, which is the honest shape until the contract grows one.

**Decision: capabilities are rendered from the report, unchanged.** The detail
pane prints `reported_at`, `offered_providers` and one line per reported
`AgentCapability` (provider/model/mode, then thinking and provider-native
options only when the host sent them). It adds no default mode, no provider
list and no inferred entry; an empty report renders "no capabilities reported".
The report is cached per host, dropped on grant/revoke (the offered set may
change), and `c` re-reads it after a failure.

**Decision: enrollment is one push action, and the command is the reference
agent's.** The tab registers the host and mints the ten-minute single-use token
in the same request pair, then shows the token once with the
`tidepool-host-agent` invocation that exchanges it (`HOST_ID`, `ENROLLMENT_TOKEN`,
`POOL_HTTP_URL`). The token is never written to the credential store or the
log; dismissing the offer discards it.

**Consequence: shell wiring is outside the owned paths.**
`tidepool-console/src/app.rs`, `src/ui/mod.rs`, `src/main.rs` and the new
`src/tabs/mod.rs` gained the state field, the key routing, the tick and the SDK
implementations; `docs/CHANGES.md` records the same. Everything decidable lives
in `tabs/hosts.rs` and `ui/hosts.rs` with tests.

---

## 2026-10-29 — The runs tab follows the clock, and trigger names come from the revision (task 2.2.3)

**Decision: "live" is a bounded poll of `GET /v1/activity/runs/{run}`.** The
activity API has no push channel for a run detail, so the tab reads it again
every second while the run or any step is pending or running. A run that says
`succeeded` but still has a running step keeps following, because the run word
is written when the last step finishes; once every step is terminal the timer
stops and a further tick makes no request. The poll interval is injected through
`drive(client, now)`, so the tests advance the clock instead of sleeping.

**Decision: filters are sent and applied again.** The tab builds
`GetV1ActivityQuery` from only the non-empty filters, which is what the pool
expects (an absent filter means "all"), and then applies the same predicates to
each page it receives. A pool that ignored a filter, or a stale page that
arrived after the filter changed, therefore cannot show another project's or
provider's records. A missing field never satisfies an active filter.

**Decision: the trigger list reads the active configuration revision.** The
pool has no trigger-list route (the CLI already documents the same gap), so the
console reads `GET /v1/projects/{id}/configuration` and parses the authored
`triggers/` files. The control plane's canonical writer emits block YAML, so the
tab has a small reader for exactly that shape and accepts JSON too; an
unreadable file is still listed with the parse problem beside it rather than
hidden. A trigger is shown active only when the revision itself is active and
valid.

**Decision: dispatch refuses locally what the pool would refuse.** The actor
must be in the selected trigger's `from_users` allowlist (case-insensitive), and
an inactive or unknown trigger is refused before a request leaves the console.
The delivery key is generated once when the form opens and stays stable if the
person edits and retries, so a double press returns the existing run; a fresh
form gets a fresh key. The actor defaults to the signed-in member.

**Decision: a failure code is a sentence with the code beside it.** The table
mirrors the control plane's mapping. A known code renders as words; an unknown
code keeps the pool's own text and still shows the code; a bare code in
`reason` is treated the same way. Nothing is hidden, so an operator can quote
the code in a bug report.

**Consequence: shell wiring and one dependency are outside the owned paths.**
`app.rs`, `ui/mod.rs`, `tabs/mod.rs`, `main.rs` gained the state field, key
routing, tick and SDK calls, and `tidepool-console/Cargo.toml` added
`serde_json` (already in the workspace lock) for the authored documents.

---

## 2026-10-29 — The rooms page walks one forward cursor in both directions (task 2.2.4)

**Decision: older history is the same forward cursor, stepped backwards.** The
chatroom contract only reads `?after=<sequence>` forward. Opening a room pages
forward from the beginning to the head and keeps the newest 200 messages; the
discarded prefix is what `has_older` means. Scrolling to the top asks for the
page immediately below the oldest held sequence by stepping the cursor back a
page at a time and discarding everything at or above the cursor, which is the
control plane's `olderMessagesPage` algorithm ported to Rust. A short read is not
proof of the start (sequences can have gaps), so the walk continues until a full
page is collected or `after=0` is read. Merges are keyed by the server's message
id and ordered by sequence then id, never by timestamp, so an overlapping page
cannot duplicate or reorder anything.

**Decision: live messages are a bounded poll of the same history route.** The
console's blocking client has no WebSocket dependency and the task must not add
one, so an open room reads forward from its highest sequence once a second. The
pool's stored `MessageView` is appended as returned, which is what keeps the
server id and sequence. A send's returned view is merged the same way, so a
message can be on screen before the next poll and still appear once.

**Decision: the relay's joining point is an explicit origin on every event.**
Following a session reads the publication state, then pages events from the
first sequence. Every event read while a page still reports `next` is
`Origin::Backfill`; the first read after the head is reached marks the phase
live, and every later event is `Origin::Live`. Rows of both origins stay in one
sequence-ordered list, `live_from` names the first live row, and the pane prints
`backfill`/`live` labels plus grey/green colours and a divider. That is the
distinction the criterion asks for, and it is asserted against the rendered
buffer, not just the state.

**Decision: Enter composes, Ctrl-S sends.** A chat message is usually more than
one line, so Enter inserts a newline and the deliberate key is `Ctrl-S`. A blank
composer sends nothing; a failed send leaves the text in place with the error in
the notice. The composer and the session prompt own every key while open, so `q`
types rather than quitting.

**Decision: presence is the room's member list, polled.** The contract exposes
membership but no watcher channel through this client; the pane says so and the
tab re-reads the member list every three seconds. Room-list refresh is fifteen
seconds, live messages one second, and the relay one second after the backfill
ends. Every interval is driven by `drive(client, now)`, so tests advance an
`Instant` instead of sleeping.

**Consequence: shell wiring is outside the owned paths.** `app.rs`,
`ui/mod.rs`, `tabs/mod.rs` and `main.rs` gained the state field, member
propagation, key routing, tick, renderer delegation and the six SDK calls;
`docs/CHANGES.md` carries the task entry.

---

## 2026-10-29 — Console session, CLI parity and headless mode (task 2.2.5)

**Decision: one credential store, the CLI's.** The console does not own a copy,
a second format or a fallback location. `auth::Session` is a thin view over
`tidepool_cli::credentials::CredentialStore`, so `tidepool login`, the console's
own startup and `tidepool-console --headless login` all read and write one file.
That is what makes either skin's sign-in visible to the other without a sync
step, and it keeps the redaction and "never inside a repository" rules in one
implementation.

**Decision: the role is the pool's, asked each time it is needed.** The pool
reads the role from the member row on every request, so the console asks
`GET /v1/pool/me` rather than caching a role in the store: headless mode asks
before every mutating command, and the terminal UI asks on the connection probe
and on reconnect. The mapping restates the pool's `require_mutating` line and
nothing more: `viewer` is read-only; `member`, `admin` and `owner` may mutate.
Unknown wire roles are read as `viewer`, so a newer pool cannot make an older
console offer an action it does not understand. Owner-only organization actions
are not in the console's surface, so they are not modelled; there is no
console-only permission. A missing role (no pool, signed out, unanswered probe)
is not a denial: the pool remains the authority and refuses what the console
cannot know.

**Decision: headless mode delegates instead of reimplementing.** It parses with
the CLI's own grammar, then calls `tidepool_cli::run_with_path` with the same
tokens, store and printer. One parser, one printer, one SDK call path, so the
`--json` bytes are equal by construction rather than by a maintained copy; the
role gate is the only thing this module adds. If headless grew its own command
handling, the parity criterion would become a test that can drift.

**Consequence: shell wiring is outside the owned paths.** `app.rs` gained the
role on `Identity`, the key gate, the notice and the status-line role;
`main.rs` gained the module declarations and the headless branch. `tabs/hosts.rs`
was read for its prompt semantics and left byte-identical. `docs/CHANGES.md`
carries the task entry.

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
