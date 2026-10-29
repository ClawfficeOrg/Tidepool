# tidepool CLI (task 2.0.6)

`cli/` is the `tidepool` binary: a thin command line over the Rust SDK
(`sdk/rust`, crate `tidepool-client`). It is a client and holds no rules of its
own. Every command builds the request the SDK already types, sends that one
call, and prints what the pool returned.

```sh
cargo run -p tidepool-cli --bin tidepool -- host list
```

## Commands

| Command | SDK call | What it prints |
|---|---|---|
| `login` | `pool.login` (`POST /v1/pool/login`) | the signed-in member; the token is stored, never printed |
| `pool status` | `setup.status` (`GET /v1/setup/status`) | whether the pool still needs first-run setup |
| `host list` | `hosts.list` (`GET /v1/hosts`) | every enrolled host |
| `host enroll` | `hosts.create` (unless `--host`), `hosts.mint_enrollment_token`, `hosts.enroll` | the enrolled host; the credential is stored, never printed |
| `host grant` | `hosts.start_execution` / `hosts.stop_execution` with `--revoke` | the updated host |
| `trigger list` | `projects.configuration` (`GET /v1/projects/{id}/configuration`) | the project's active authored revision |
| `trigger validate` | `triggers.validate` (`POST /v1/triggers/validate`) | the pool's verdict and named issues |
| `trigger install` | `triggers.install` (`POST /v1/triggers/install`) | the installed trigger, its revision and version |
| `run dispatch` | `runs.dispatch` (`POST /v1/runs`) | the created (or deduplicated) run |
| `run watch` | `activity.run` (`GET /v1/activity/runs/{run}`) | the run detail once it reaches a terminal status |
| `room list` | `chatrooms.list` (`GET /v1/chatrooms`) | the rooms this member can see |
| `room send` | `chatrooms.send` (`POST /v1/chatrooms/{room}/messages`) | the server's copy of the message |
| `relay follow` | `relay.events` (`GET /v1/sessions/{session}/relay/events`) | the session's relayed events, in sequence order |

Two commands are loops rather than one call, both named for what they do:

- `run watch` polls the run detail until the status is `succeeded`, `failed` or
  `interrupted`. The v1 contract has no run stream, so polling is the contract's
  own surface, not a second implementation of anything.
- `relay follow` iterates the SDK's `paging::relay_events`, so the cursor is the
  SDK's and the pool's, never the CLI's.

`trigger list` reads the active configuration revision because v1 has no
trigger-list route: a trigger is installed into a project and the authored
files, including the ones under `triggers/`, are what the pool returns.

## Flags

Every command accepts:

- `--json` — print the SDK's own type as JSON (`serde` of the value the SDK
  decoded), so a script reads exactly the contract shape. `relay follow`
  prints one JSON object per line, each a `RelayEvent`.
- `--pool <url>` — the pool to talk to, overriding the remembered one.
- `--help` — the usage text.

Command-specific flags are listed by `tidepool help`; an unknown or
inapplicable flag is refused rather than ignored.

`login` reads the password from stdin and only from stdin:

```sh
read -rs PASSWORD
printf '%s\n' "$PASSWORD" | tidepool login --pool https://pool.example.com \
  --email ops@example.com --password-stdin
```

The password never appears in `argv`, in shell history or in the store.

`run dispatch` defaults `--actor` to the signed-in member's email and generates
a `deliveryKey` when `--delivery-key` is absent; a supplied key is sent verbatim,
so a repeated dispatch is the pool's idempotent replay.

## Credential store

The CLI owns one JSON file, and nothing else is written to disk:

1. `$TIDEPOOL_HOME/credentials.json`
2. `$XDG_CONFIG_HOME/tidepool/credentials.json`
3. `$HOME/.config/tidepool/credentials.json`

The file is written atomically (temporary file then rename) with `0600`
permissions where the platform has them, and it is never written inside a git
working tree: `save` refuses any path whose ancestors contain `.git` and tells
the person to point `TIDEPOOL_HOME` elsewhere. That is the mechanical form of
"a credential is never written into a repository".

The shape, which the console (task 2.2.5) shares:

```json
{
  "pool": "https://pool.example.com",
  "email": "ops@example.com",
  "token": "<operator JWT>",
  "active_host": "host_1",
  "host_credentials": {
    "host_1": "<host relationship credential>"
  }
}
```

A credential is never printed in full. `login` prints the member and a redacted
token; `host enroll` prints the host and a redacted credential with the store
path. Redaction keeps four leading characters and the length, and refuses to
show anything for a secret of eight characters or fewer. `Debug` for the store
and the file redacts every secret too, so a log or a panic cannot leak one.

## Errors

A pool failure is printed as the problem details the SDK kept, for example:

```
tidepool: get_v1_hosts -> 403 forbidden: role or scope does not allow this (requestId req_1)
```

No error message carries a credential.
