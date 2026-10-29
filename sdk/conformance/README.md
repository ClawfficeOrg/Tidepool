# Cross-SDK conformance suite

One scenario list, run against all three Tidepool bindings — Python, TypeScript
and Rust — so the SDKs stay one library with three surfaces instead of three
opinions.

```
sdk/conformance/
├── suite.py            harness: scenario model, expectation matcher, runner
├── run_conformance.py  CLI: --list, --vars, --base-url
├── provision.py        creates the pool state a run needs, writes --vars
├── selftest.py         checks the harness itself; no pool, no network
├── adapters/
│   ├── manifest.json   the three bindings, each a command
│   ├── python.py
│   ├── typescript.mjs
│   └── rust/           its own workspace so the main gates are unchanged
├── scenarios/          the scenario list, one file per area
└── fixtures/           placeholder variables for --list and the selftest
```

## How a run works

```
provision.py ──► real pool ──► run_conformance.py ──► adapters (one process per binding)
                    ▲                                        │
                    └────────── real HTTP requests ──────────┘
```

`provision.py` bootstraps a pool over the public API: first operator, project,
two API keys, a GitHub connection, an offline host, an online host driven by the
reference agent (`crates/tidepool-host-agent`), a configuration revision with
one workflow per trigger, and the three triggers the scenarios use. It writes
the variable file `--vars` reads.

`run_conformance.py` then starts one adapter process per binding, replays every
scenario step against each of them, and compares two things:

* each answer with the step's `expect` pattern — the behaviour the contract
  promises;
* the answers with each other for the paths a step lists in `agree` — the
  behaviour the bindings must share even where no expectation is pinned.

A binding that returns a different problem code, a different paging cursor, or
a different frame classification fails the run. So does a run that leaves out a
binding: `--bindings` is a debugging switch and always exits 3, never 0.

## Running it

Against the CI stack (or any pool you can point at):

```sh
cargo build --manifest-path sdk/conformance/adapters/rust/Cargo.toml
cargo build -p tidepool-host-agent

python3 sdk/conformance/provision.py \
    --base-url http://127.0.0.1:8090 \
    --out /tmp/tidepool-conformance-vars.json \
    --agent-bin target/debug/tidepool-host-agent

python3 sdk/conformance/run_conformance.py \
    --base-url http://127.0.0.1:8090 \
    --vars /tmp/tidepool-conformance-vars.json
```

Exit codes: `0` clean pass with full coverage, `1` a binding failed, `2` the
suite could not run (bad manifest, scenario, variables or adapter), `3`
incomplete coverage.

Without a pool:

```sh
python3 sdk/conformance/selftest.py
python3 sdk/conformance/run_conformance.py --list \
    --vars sdk/conformance/fixtures/variables.json
```

`cargo test` runs both through `tests/conformance_suite.rs`, so a broken
scenario or a weakened matcher fails the normal gate. The real-pool run is the
`conformance` job in `.github/workflows/ci.yml`; it starts MySQL and Redis,
runs the migrations, boots the server and runs the two commands above.

## Adding a scenario

Append a step to a file under `scenarios/`. That is the whole change: every
binding is already required to answer it, and the `expect`/`agree` checks are
generic. No per-binding test code is needed.

```json
{
  "id": "capabilities.report",
  "title": "A host reports capabilities with its credential",
  "steps": [
    {
      "id": "report",
      "command": "call",
      "params": {
        "operation": "post_v1_hosts_id_capabilities",
        "pathParams": { "id": "{{offline_host_id}}" },
        "headers": { "x-host-id": "{{offline_host_id}}" },
        "body": { "capabilities": [ ... ] }
      },
      "auth": { "hostCredential": "{{offline_host_credential}}" },
      "expect": {
        "outcome": "ok",
        "body.offered_providers": { "$contains": "anthropic" }
      },
      "agree": ["body.offered_providers"],
      "capture": { "report_id": "body.host_id" }
    }
  ]
}
```

A step is:

| field     | meaning                                                        |
|-----------|----------------------------------------------------------------|
| `id`      | unique inside the scenario                                     |
| `command` | one of `contract`, `describe`, `call`, `page`, `stream`, `frames` |
| `params`  | command arguments; `{{name}}` is substituted from variables    |
| `auth`    | `token`, `apiKey` or `hostCredential` for this step            |
| `expect`  | pattern the answer must match (optional)                       |
| `agree`   | dotted paths every binding must answer identically             |
| `capture` | `name` → dotted path of a value later steps may use            |

Variables come from `--vars` (provisioned) or `capture`, are scoped per
binding, and `{{binding}}` is always available so a scenario can give each
binding its own hostname or delivery key.

### Expectation patterns

A nested object means "these fields, with these values"; a dotted key is a path
into the answer, so `"body.run.id"` works without nesting. Values are compared
exactly unless they are an operator object:

| operator    | meaning                                             |
|-------------|-----------------------------------------------------|
| `$eq` `$ne` | equality / inequality                               |
| `$gt` `$gte` `$lt` `$lte` | numeric comparison                     |
| `$type`     | JSON type (`object`, `array`, `string`, `integer`, `number`, `boolean`, `null`) |
| `$len`      | exact length, or another operator over the length   |
| `$contains` | substring, list member, or matching list member     |
| `$not`      | the pattern must not match                          |
| `$exists`   | field present (`true`) or absent (`false`)          |
| `$regex`    | `re.search` over a string                           |
| `$oneof`    | the value is one of a list                          |

An unknown operator is a suite error, not a silent pass.

## Adding a command

Only when a scenario needs behaviour the protocol cannot express. Add the
command to `COMMANDS` in `suite.py` and implement it in all three adapters.
That is the one change that is deliberately per-binding: it is the adapter's
job to translate an SDK's shape into the suite's shape, and nothing else.

## Adapter protocol

Line-oriented JSON on stdin/stdout, one reply per request:

```
→ {"id": 1, "command": "call", "params": { ... }}
← {"id": 1, "ok": true, "result": { ... }}
← {"id": 1, "ok": false, "error": {"type": "...", "message": "..."}}
```

The base URL arrives in `TIDEPOOL_CONFORMANCE_BASE_URL`. Nothing but replies may
be written to stdout; diagnostics go to stderr, which the suite captures and
redacts. Each adapter normalises:

| command    | result                                                        |
|------------|---------------------------------------------------------------|
| `contract` | `{contract_version, operation_count}`                         |
| `describe` | `{operation_id, method, path, auth, scope, role, surface, path_params, query_params}` |
| `call`     | `{outcome:"ok", body}` or `{outcome:"error", status, code, title, detail, type, request_id, attempts, retry_after_ms}` |
| `page`     | `{items: [...]}` after walking the SDK's own cursor iterator  |
| `stream`   | `{scheme, after, token_present, token_matches, after_param, path}` — no socket is opened |
| `frames`   | `{frames: [{kind, catch_up, cursor, sequences, code}]}`       |

Failure messages never print variables or credentials; response values are
redacted before they are shown.

## Scenarios

| file                | covers                                                    |
|---------------------|-----------------------------------------------------------|
| `01-auth.json`      | contract version, route metadata, anonymous/member/key credentials, scopes |
| `02-hosts.json`     | registration, enrollment, one-time token replay, revoked-token 410 |
| `03-capabilities.json` | capability report validation, host authentication, the `host.execute` grant |
| `04-triggers.json`  | trigger validation field names, install revisions, refused installs |
| `05-dispatch.json`  | offline-host preflight, actor refusal, idempotent replay, revision mismatch |
| `06-paging.json`    | relay `after`/`next` paging and the activity cursor iterator |
| `07-stream.json`    | resume handle and every socket frame shape                 |
| `08-problems.json`  | every problem-details shape and request-id echo            |
