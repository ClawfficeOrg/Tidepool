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

Slack needs `bot_token` and `signing_secret` (plus `app_token` for socket mode);
Discord needs `bot_token`. Credentials are validated at save time and are never
returned by any read endpoint.

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

## API

`GET/POST /v1/connections`, `GET/PATCH/DELETE /v1/connections/{id}`. PATCH can
rename, enable or disable a database connection. Reads omit the credential
reference and report `credential_configured` instead.
