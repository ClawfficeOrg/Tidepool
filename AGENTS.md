# Agent Instructions

These instructions apply to every agent working in this repository.

---

## Required Reading

Before starting any task, read:

- [`docs/GUIDELINES.md`](docs/GUIDELINES.md) — code standards, CI, branching and PR rules
- [`docs/todo.md`](docs/todo.md) — the plan index; it points at the TideTable files below
- [`docs/todo-v1.json`](docs/todo-v1.json) / [`docs/todo-v2.json`](docs/todo-v2.json) —
  the plan of record (TideTable JSON). `v1` is the active line: the rename from
  Playzoid, and the hub. `v2` is the clients.
- [`docs/HUB_CAPABILITIES.md`](docs/HUB_CAPABILITIES.md) — Paseo Hub read and
  mapped to Tidepool, capability by capability
- [`docs/memory.md`](docs/memory.md) — architectural decision log and the
  hard rules
- [`docs/RECOVERY_PLAN.md`](docs/RECOVERY_PLAN.md) — historical recovery context

Also read as relevant to your specific task:

- [`docs/TALO_API.md`](docs/TALO_API.md) — verified Talo API surface shapes
- [`docs/TALO_API_STRUCTS.md`](docs/TALO_API_STRUCTS.md) — raw Rust struct extractions (note the `RAW EXTRACTION ARTIFACT` header; rename structs before lifting)
- [`docs/TaloRustServerPlan.md`](docs/TaloRustServerPlan.md) — full server architecture plan

---

## Branching Model

All work goes through PRs against `main`. Never push directly to `main`.

| Branch type     | Pattern                                          |
|-----------------|--------------------------------------------------|
| Feature         | `feature/<scope>/<short-description>`            |
| Fix             | `fix/<scope>/<short-description>`                |
| Refactor        | `refactor/<scope>/<description>`                 |
| Docs            | `docs/<description>`                             |
| Chore / tooling | `chore/<description>`                            |
| Ralph tasks     | `task/vX.Y.Z` (e.g. `task/v1.1.1`)               |

**Never self-approve or auto-merge PRs you authored.** All PRs require CI green
plus at least one human reviewer before merge.

## The plan is TideTable JSON

`docs/todo-v1.json` and `docs/todo-v2.json` are the plan of record; the
markdown `todo-v*.md` files are the historical record and take no new tasks.
Claim work through TideTable rather than by editing the plan by hand:

```sh
# TideTable is installed (tidetable/tt, v0.12.0); it lives at
# ~/git_repos/clawffice/TideTable. --local is required for plan files.
TODO=docs/todo-v1.json

tidetable --local validate "$TODO"
tidetable --local next "$TODO" --json
tidetable --local start "$TODO" 1.1.1 --owner <you>
tidetable --local done "$TODO" 1.1.1
```

`start` claims the task under a lease, so two agents cannot take the same one
and a crashed session's task returns to the pool. `touches` is the file
ownership field: two tasks whose paths overlap are never handed out together by
`tidetable ready --disjoint`. Cross-file prerequisites do not validate, so
`todo-v2.json` records its dependencies on v1 as `requires_v1` — do not start a
v2 task while those are pending. See `docs/todo.md` for the full command list.

---

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <short description>

[optional body]
[optional footer(s): e.g. "Closes task 0.3.5"]
```

Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `ci`, `perf`

---

## Code Quality

Rules:

- Use `Result<T, E>` consistently; no `unwrap()` or `expect()` outside of tests.
- Use `thiserror` for defining error types; `anyhow` is acceptable in tests and scripts.
- Use `tracing::instrument` on handler functions for automatic span creation.
- All public structs and functions must have doc comments (`///`).
- Database query results must use `sqlx::FromRow` — avoid raw tuple mapping.
- Validate all request bodies before processing.
- Use parameterized `sqlx` queries — never string interpolation in SQL.

Security requirements:

- **Never hardcode secrets.** All credentials go in `.env` (local) or environment variables (CI/prod).
- Passwords must be hashed with **Argon2id** (`argon2` crate) — never stored plain.
- JWT secret sourced from env, validated ≥ 32 bytes at startup.
- Validate and sanitise all user-supplied input (paths, IDs, identifiers).
- Run `cargo audit` periodically; flag new advisories in `docs/ralph-log.md`.

---

## Testing

Baseline checks (run these before committing — there is no pre-commit hook
that runs them automatically):

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

All three must pass before opening a PR.

Every implementation task must also:

- Include unit tests in the same file (`#[cfg(test)] mod tests {}`).
- Include integration tests in `tests/` for any new HTTP endpoint.

---

## PR Requirements

Every PR description (whether opened by ralph or by a human) must include:

- `Closes task <id>` (e.g. `Closes task 0.3.5`)
- The acceptance criteria from the task block in `docs/todo-v<N>.md` with each item checked
- `cargo test` output (last 20 lines)
- For WS or load-sensitive changes: load test results

---

## Documentation and Memory

After completing any task:

- Update `docs/CHANGES.md` with a brief changelog entry.
- Append a decision entry to `docs/memory.md` if an architectural choice was
  made, and a learning to `docs/learnings.md` if there is a durable lesson.
- Mark the task `- [x]` in its `docs/todo-v<N>.md` file.

### Todo conventions

Tasks use semver ids `X.Y.Z` (`Y` = release line, `Z` = task number):

```
- [ ] `0.3.5` Task title
  Complexity: Small
  Agent: basic_dev_agent
```

- `[ ]` open — ralph will pick it up; `[x]` done and merged.
- One release line per `release/vX.Y` branch; one `task/vX.Y.Z` branch per task.
- On line completion: review → merge → tag `vX.Y.0` → bump project version.
- Lines where Y = 0 need an RC tag + human sign-off before merge.

---

## Skills

If a relevant skill exists in `skills/`, use it. Skills keep work consistent.

- `skills/ralph.md` — autonomous task loop agent skill
