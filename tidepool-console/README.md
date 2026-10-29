# `tidepool-console`

The operator's terminal: a ratatui console over the Tidepool Rust SDK.

It is the CLI's second skin, not a second client. One credential store, one
printer, one set of SDK calls. The terminal UI is for a person at a desk; the
`--headless` mode is for a script, and it prints exactly what `tidepool` prints.

## Two skins, one session

The console reads and writes the CLI's credential file, in the CLI's format, at
the CLI's location:

1. `$TIDEPOOL_HOME/credentials.json` when `TIDEPOOL_HOME` is set;
2. otherwise `$XDG_CONFIG_HOME/tidepool/credentials.json`;
3. otherwise `$HOME/.config/tidepool/credentials.json`.

So a sign-in made once serves both:

```sh
# From the CLI, then open the console.
tidepool login --pool https://pool.example.com --email you@example.com --password-stdin

# Or from the console's headless skin; `tidepool host list` then uses it too.
tidepool-console --headless login \
  --pool https://pool.example.com --email you@example.com --password-stdin
```

The token is never printed in full. `login` redacts it before printing the
response, and the store is written before anything is reported, so a failed
save cannot leave you believing you are signed in. The console makes no network
call at startup unless the store names both a pool and a token; a pool that does
not answer is a notice on the status line, not a crash.

The terminal UI itself has no sign-in form: it reuses the store. Run
`tidepool login` (or the headless `login` above) and reopen the console.

## Roles

The role comes from the pool (`GET /v1/pool/me`), never from a console-side
guess and never from a cache. It mirrors the pool's own line exactly:

- `viewer` is read-only. Every mutating request would be refused by the pool,
  so the console does not offer one: the action key is refused with a notice
  naming the role and who can change it, and headless mode stops before the
  mutating request is sent. Reads, room history, the activity feed and relay
  following still work.
- `member`, `admin` and `owner` may operate the pool: enrol a host, grant or
  revoke `host.execute`, dispatch a run, install a trigger, send a message.
- Owner-only organization actions (members, API keys) are outside this
  console's surface, so they are not modelled at all.

An unknown role is read as `viewer`, the least privilege: a newer pool must not
make an older console offer a mutation it does not understand. Headless mode
asks the pool before every mutating command, so a role change applies to the
next scripted action. The terminal UI asks on the connection probe and again on
reconnect (`r`), so reopen or reconnect after an owner changes your role.

## Commands both skins share

Every CLI command exists in the console's headless mode, with the same flags and
the same output. `tidepool-console --headless <command>` takes exactly the CLI
grammar; `--json` prints the SDK's type, and without it the same short table is
printed.

| Command | The CLI | The console, headless |
| --- | --- | --- |
| `login` | `tidepool login --email <you> --password-stdin` | `tidepool-console --headless login --email <you> --password-stdin` |
| `pool status` | `tidepool pool status` | `tidepool-console --headless pool status` |
| `host list` | `tidepool host list --json` | `tidepool-console --headless host list --json` |
| `host enroll` | `tidepool host enroll --hostname <name>` | `tidepool-console --headless host enroll --hostname <name>` |
| `host grant` | `tidepool host grant --host <id> [--revoke]` | `tidepool-console --headless host grant --host <id> [--revoke]` |
| `trigger list` | `tidepool trigger list --project <id>` | `tidepool-console --headless trigger list --project <id>` |
| `trigger validate` | `tidepool trigger validate --file <path>` | `tidepool-console --headless trigger validate --file <path>` |
| `trigger install` | `tidepool trigger install --project <id> --file <path>` | `tidepool-console --headless trigger install --project <id> --file <path>` |
| `run dispatch` | `tidepool run dispatch --project <id> --trigger <name>` | `tidepool-console --headless run dispatch --project <id> --trigger <name>` |
| `run watch` | `tidepool run watch --run <id>` | `tidepool-console --headless run watch --run <id>` |
| `room list` | `tidepool room list --json` | `tidepool-console --headless room list --json` |
| `room send` | `tidepool room send --room <id> --body <text>` | `tidepool-console --headless room send --room <id> --body <text>` |
| `relay follow` | `tidepool relay follow --session <id>` | `tidepool-console --headless relay follow --session <id>` |

Common flags are the CLI's too: `--json`, `--pool <url>`, `--help`.

The terminal UI covers the same work with keys, on four tabs:

- **Hosts** — `↑↓` select, `s` sort, `S` direction, `f` filter, `/` search,
  `e` enrol, `g` grant execution, `x` revoke, `c` read capabilities.
- **Runs** — `↑↓` select, `Enter` open, `d` dispatch, `p` project, `v` provider,
  `t` trigger, `f` status, `x` clear, `r` refresh.
- **Rooms** — `Enter` open a room, `c` compose, `Ctrl-S` send, `f` follow a
  session relay, `x` stop following, `r` refresh.
- **Settings** — the connection, the pool, the member and the role.

`Tab`/`Shift-Tab` move between tabs, `1`–`4` jump to one, `p` or `:` opens the
command palette, `r` reconnects, `a` opens the about screen and `q` quits.

## Identity

The first frame is the Tidepool banner from `clawffice-design`, rendered from
the design repository's own ANSI file rather than a copy pasted into Rust. The
banner, the 12×12 pool cell sprite and the colours live in
[`assets/design/`](assets/design/), with the design commit and file hashes in
`SOURCE.json`; a sibling checkout of the design repository is compared by the
tests when it is present.

The palette is the Clawffice palette, not the terminal's defaults. `biolume` is
the default, and `TIDEPOOL_PALETTE` selects `biolume`, `boiled`, `deep-tide`,
`phosphor` or `sea-glass`. The Tidepool product hue (`tide`) drives focus,
selection and borders; status words also carry a success, warning or danger
colour from the palette. A terminal that cannot show colour (`NO_COLOR`, or
`TERM=dumb`) gets the same screens with every style stripped, and a terminal
narrower than the banner gets a cropped banner or, below 40 columns, the
one-line wordmark instead of a panic. `a` opens the about screen, which reports
the console version, the contract version, the palette and the design commit.

## Neither skin holds logic the pool does not

The console and the CLI both hold only transport and presentation: they parse a
command, call one typed SDK endpoint, and print what came back. Validation,
scheduling, deduplication, paging and every permission decision live in the
pool and in the SDK. The role line above is the pool's `require_mutating` line
restated so a forbidden action is hidden rather than attempted; it is not an
extra rule, and a console that disagrees with the pool still loses, because the
pool is what actually refuses the request.

Headless mode proves this mechanically: it delegates to the CLI's own
`run_with_path`, so the command grammar, the credential store and the printer
are the same code, and `--json` is byte-for-byte the CLI's output.

## Building and testing

```sh
cargo build -p tidepool-console
cargo test -p tidepool-console
```

The tests are hermetic: a scripted HTTP server stands in for the pool, and no
database, Redis, live pool or terminal is required.
