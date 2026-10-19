# Tidepool Todo — index

Master index of the plan. **TideTable JSON is canonical** from v1 on; the
markdown files are the historical record and take no new tasks.

## Current

- [`todo-v1.json`](todo-v1.json) — **v1.x**, ships `0.5.0`: the rename from
  its former name, pool identity, hosts, connections, triggers and workflows,
  dispatch and the run record, chatrooms and the session relay, self-hosting.
  ⭐ active line.
- [`todo-v2.json`](todo-v2.json) — **v2.x**, ships `0.6.0`: the clients. The
  hub API contract, TypeScript / Rust / Python SDKs with one conformance suite,
  the `tidepool` CLI, the Svelte control plane, the ratatui console, and the
  reference human client as a Zoid tray app. Rhumb federation is deferred to v3.

Reference material the tasks point at:

- [`HUB_CAPABILITIES.md`](HUB_CAPABILITIES.md) — Paseo Hub read on 2026-10-19,
  mapped capability by capability to Tidepool and to the task that builds it.

## History (markdown, read-only)

- [`todo-v0.md`](todo-v0.md) — releases 0.1–0.3: foundation, auth and player
  management, leaderboards, game saves, WebSocket channels.
- [`todo-v1.md`](todo-v1.md) — release 0.4: analytics, game config, feedback,
  rate limiting, Prometheus metrics, OpenAPI.
- [`RECOVERY_PLAN.md`](RECOVERY_PLAN.md) — the recovery sequencer (R-1…R-10)
  that closed the gaps left by the OpenClaw memory blow-up.

These three are kept because they are the record of how the game API got here,
including the `.env` incident and the governance repair. Do not add to them.

## How to run the plan (ralph)

TideTable is installed (`tidetable` and `tt`, v0.12.0) and works from any
directory. `--local` is required when the first argument is a plan file:
without it the CLI runs in hub/server mode and treats the path as a slug.

If `tidetable` is not on PATH, stop: `scripts/check-tidetable.sh` prints the
one-sentence install instruction and exits 1. Install with
`uv tool install --editable ~/git_repos/clawffice/TideTable` (or `pipx`), or
fall back to `(cd ~/git_repos/clawffice/TideTable && python3 -m tidetable --local ...)`.

```sh
TODO=~/git_repos/Clawffice/Playzoid-Server/docs/todo-v1.json

tidetable --local validate "$TODO"
tidetable --local ready "$TODO" --json       # what is runnable now
tidetable --local ready "$TODO" --disjoint   # what is safe to fan out
tidetable --local next "$TODO" --json
tidetable --local start "$TODO" 1.0.2 --owner ralph
tidetable --local done "$TODO" 1.0.2
```

- One task per loop; `start` records the claim and the lease, so two agents
  cannot take the same task and a crashed session's task returns to the pool.
- `touches` is the file-ownership field: two tasks whose paths overlap are
  never handed out together by `ready --disjoint`.
- Branch per task: `task/vX.Y.Z`, off the release line; PRs against `main`.
- Green gate per task is unchanged: `cargo fmt --check`,
  `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`.

## Conventions

- Task ids are semver `X.Y.Z`: **Y = release line, Z = task number.**
- Release lines merge to `main` and tag `vX.Y.0`; a line whose `Y = 0` needs an
  RC tag and human sign-off first.
- Never push directly to `main`. Never self-approve or self-merge.
- Cross-file prerequisites do not validate, so `todo-v2.json` records what a
  task depends on in v1 as `requires_v1` (documentation, not scheduling). Do
  not start a v2 task while its `requires_v1` entries are pending.

## Related docs

- `AGENTS.md`, `docs/memory.md`, `docs/learnings.md` — required reading per task
- `docs/GUIDELINES.md` — code standards, CI, branching rules
- `docs/TALO_API.md` — the verified Talo surface the game API keeps
- `docs/CHANGES.md` — per-release changelog
