# Connections

A connection is a provider account the pool may receive events from. The pool
stores a **reference** to credentials, never the credential values:

- `connections` — provider kind, slug, display name, `credential_ref`, status.
- `connection_resources` — the provider resource identities it may deliver for
  (`repository`, `workspace`, `guild`, `channel`, ...).

## Credential references

`credential_ref` names an environment variable (or secret-store key) whose
value is a JSON object of provider fields:

```json
// GITHUB_ACME_CREDS
{"app_id": "123", "private_key": "-----BEGIN...", "webhook_secret": "..."}
```

Slack needs `bot_token` and `signing_secret` (plus `app_token` for socket mode
and an optional `bot_user_id`); Discord needs `bot_token`. Credentials are
validated at save time and are never returned by any read endpoint.

## Environment precedence

`TIDEPOOL_CONNECTIONS` accepts a complete JSON array:

```json
[
  {
    "provider": "github",
    "slug": "acme",
    "display_name": "Acme",
    "credential_ref": "GITHUB_ACME_CREDS",
    "resources": [{"resource_kind": "repository", "external_id": "acme/app"}]
  }
]
```

An environment entry wins over a database row with the same slug, appears in
reads as `managed_by: "environment"` with id `env:<slug>`, and cannot be
mutated through the API. The shadowed database row is preserved but is not
effective.

## GitHub App

Credentials (`GITHUB_ACME_CREDS`) are a JSON object with `app_id`,
`private_key` and `webhook_secret`. Point the App webhook at:

```
POST /v1/webhooks/github/{connection-slug}
```

The route is public; security is the GitHub signature. It verifies
`X-Hub-Signature-256` over the raw body before any matching, deduplicates
`X-GitHub-Delivery` (the unique key records each delivery once), and resolves
`repository.full_name` against the connection's `repository` resources
(case-insensitive). A delivery from a repository the connection does not own is
acknowledged but marked `accepted: false`.

Typed events:

| GitHub event | action | internal event |
|---|---|---|
| `issues` | `opened` | `issue.created` |
| `issues` | `labeled` | `issue.label_added` |
| `pull_request` | `opened` | `pull_request.created` |
| `pull_request` | `labeled` | `pull_request.label_added` |
| `issue_comment` | `created` | `issue.comment_created`, or `pull_request.comment_created` when the issue is a PR |
| `pull_request_review_comment` | `created` | `pull_request.review_comment_created` |
| `pull_request_review` | `submitted` | `pull_request.review_submitted` |
| `ping` | — | `ping` |
| `installation` | any | `installation` |
| `installation_repositories` | any | `installation_repositories` |

Other events are accepted and typed as `unsupported`. Trigger/configuration
validation calls `validate_repository`, which fails with a named `repository`
field when no active connection owns the repository.

## Slack app

Credentials (`SLACK_ACME_CREDS`) are a JSON object:

```json
// SLACK_ACME_CREDS
{
  "bot_token": "xoxb-...",
  "signing_secret": "...",
  "app_token": "xapp-...",     // socket mode only
  "bot_user_id": "U0BOT"       // optional; sharpens mention handling
}
```

Resources are the workspaces the connection delivers for
(`resource_kind: "workspace"`, the Slack team id) and, optionally, channel
filters (`resource_kind: "channel"`). With no channel resource every channel in
an owned workspace is delivered; with one or more, only those channels are.
Channel ids and names are matched case-insensitively and a leading `#` is
ignored.

### Two transports, one event

| Transport | Needs a public address | Verification |
|---|---|---|
| Webhook | yes | `X-Slack-Signature` over `v0:<timestamp>:<body>` |
| Socket mode | no | the `app_token` used to dial the socket |

**Webhook.** Point the Slack app's Event Subscriptions at:

```
POST /v1/webhooks/slack/{connection-slug}
```

The route is public; security is the Slack signature. It verifies
`X-Slack-Signature` over the raw body before parsing, and requires
`X-Slack-Request-Timestamp` to be within five minutes, so a captured body
cannot be replayed. Missing headers, a bad signature and a stale timestamp are
all 401 and record nothing. Slack's `url_verification` handshake is answered
with the challenge it sent.

**Socket mode.** A pool with no public address dials Slack with the
`app_token` and receives envelopes over the socket:

```json
{"envelope_id": "57d6…", "type": "events_api", "payload": { /* event_callback */ }}
```

Each envelope is acknowledged by echoing its `envelope_id`, or Slack retries
it. `SlackSocketSession` owns that protocol — hello, envelope, ack,
`disconnect` — and `run_socket_session` pumps it over a frame channel, so the
same logic runs behind a live socket and behind a scripted feed in tests.

Both transports deduplicate on Slack's `event_id` (falling back to
`channel:event_ts`), recorded in `webhook_deliveries` with `provider = slack`
and the transport in the `action` column. One event received twice — a retry,
or once per transport — is therefore recorded once.

### Typed events

| Slack event | internal event |
|---|---|
| `app_mention` | `slack.mention` |
| `message` with `channel_type: im` | `slack.direct_message` |
| `message` elsewhere | `slack.message` |
| the bot's own messages, `bot_id` posts, any subtype (edits, joins) | `slack.ignored` |
| anything else | `slack.unsupported` |

A delivery from a workspace the connection does not own, or from a filtered
channel, is acknowledged but marked `accepted: false` with a reason
(`team_not_owned`, `channel_filtered`, `self_authored`, `unsupported_event`).
`validate_workspace` lets configuration validation refuse an unknown workspace
at save time with a named `workspace` field.

**The mention never reaches a prompt.** A normalized message carries `text`
with the bot mention already removed and `raw_text` as Slack sent it; prompt
construction reads the stripped text. `<@U0BOT>` and `<@U0BOT|label>` are both
removed, other people's mentions are left alone, runs of spaces collapse and
newlines survive.

### Replies (`slack.reply`)

A step declares the outputs it may emit:

```yaml
outputs:
  - capability: slack.reply
    maximum: 2        # default 1, hard maximum 20
    required: true    # default false
```

A reply is built from the event it answers, so it can only go to the channel
the event came from, and to that event's thread (`thread_ts` when the message
was already threaded, otherwise the message's own `ts`, which starts the
thread). The capability is enforced per step execution:

- a capability the step did not declare cannot be emitted at all;
- it cannot be emitted more than `maximum` times;
- a `required` capability must be emitted at least once, or the step cannot
  finish — the run is recorded as failed instead of silently complete.

Replies are addressed and validated in the pool and performed as
`chat.postMessage` with the connection's `bot_token`; the outbound HTTP call
belongs to dispatch (task 1.4.6), as does dialling the socket-mode WebSocket.

## API

`GET/POST /v1/connections`, `GET/PATCH/DELETE /v1/connections/{id}`. PATCH can
rename, enable or disable a database connection. Reads omit the credential
reference and report `credential_configured` instead.
