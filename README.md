# Tidepool

Tidepool is the Clawffice suite's **hub and game server**: a Talo-compatible
backend for players, leaderboards, saves, channels, analytics and feedback,
with the layer above it — a pool that machines enrol with, that receives events
from GitHub, Slack and Discord, and that asks enrolled hosts to run agents.
Pools federate with other pools; that link is **Rhumb**. Tidepool is
self-hostable and open.

The name is the suite's: a tidepool is a rock pool left by the current, holding
its own small world until the water returns. The product hue is `tide`
(`#4A9FE0` dark, `#1864AB` light), the current is water, and the creature is the
rock pool itself.

Design tokens, marks and palettes live in
[`clawffice-design`](../clawffice-design) (`tokens/apps/tidepool.json`). They are
**referenced, never vendored**: nothing in this repository imports them at run
time.

## Status

The game API is implemented and green: auth, players and subaccounts,
leaderboards, saves, WebSocket channels, analytics events, game settings and
feedback, with `/metrics`, `/healthz` and a generated `/openapi.json`. The game
routes are mounted at their canonical `/v1/...` paths, with legacy unprefixed
aliases during the transition.

The hub is the next line of work. The plan of record is
[`docs/todo-v1.json`](docs/todo-v1.json); clients (SDKs, control plane, console,
reference client) are [`docs/todo-v2.json`](docs/todo-v2.json). See
[`docs/HUB_CAPABILITIES.md`](docs/HUB_CAPABILITIES.md) for the product shape,
capability by capability.

## Quick start

```sh
cp .env.example .env          # then set a fresh JWT_SECRET (openssl rand -hex 32)
docker compose -f config/docker-compose.dev.yml up -d   # MySQL + Redis
cargo run                                               # tidepool on :8080
```

First run needs an organization. Open the pool's setup page and `POST
/v1/setup`, or set `TIDEBOOT_ORGANIZATION_NAME`, `TIDEBOOT_OWNER_EMAIL` and
`TIDEBOOT_OWNER_PASSWORD` (at least 12 characters) for an unattended install;
the temporary password must be changed on first sign-in.

| Endpoint | Purpose |
|---|---|
| `GET /healthz` | Liveness; reports database reachability |
| `GET /metrics` | Prometheus metrics, all series prefixed `tidepool_` |
| `GET /openapi.json` | Generated OpenAPI 3.1 document |
| `/v1/setup` | First-run organization and owner setup (browser installs) |
| `/v1/pool/...` | Operator sign-in, profile and password change |
| `/v1/api-keys` | Organization API keys and scoped automation |

Hub failures use RFC 9457 problem details; see
[`docs/HUB_API_ERRORS.md`](docs/HUB_API_ERRORS.md).
| `/v1/...` | Game API (see `docs/TALO_API.md`) |

The Prometheus prefix is `tidepool_`, not the old name: dashboards and alerts
matching the previous series must be updated. The HTTP game API surface is
unchanged by the rename.

## Tests

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo test --test '*' -- --ignored    # needs the dev stack up
```

Live-stack integration tests are `#[ignore]` by default and use
`DATABASE_URL` / `REDIS_URL` when set. The minimal host agent lives in
`crates/tidepool-host-agent`; see `docs/HOST_PROTOCOL.md`.

## Layout

| Path | What lives there |
|---|---|
| `src/api/` | HTTP routes, OpenAPI table, health and metrics |
| `src/services/` | Domain logic and SQL |
| `src/sockets/` | WebSocket tickets and the channel hub |
| `src/middleware/` | Auth, rate limiting, metrics |
| `migrations/` | Forward-only MySQL migrations |
| `config/` | The dev compose stack |
| `docs/` | Plan, API reference, decisions, changelog |
| `tests/` | Unit-adjacent and live-stack integration tests |

## Working in the repository

Read [`AGENTS.md`](AGENTS.md) first, then
[`docs/GUIDELINES.md`](docs/GUIDELINES.md), [`docs/memory.md`](docs/memory.md)
and the TideTable plan. Work is claimed through TideTable, one task per branch
(`task/vX.Y.Z`), and merged by pull request with a green gate.
