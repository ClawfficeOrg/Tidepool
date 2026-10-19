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

## API

`GET/POST /v1/connections`, `GET/PATCH/DELETE /v1/connections/{id}`. PATCH can
rename, enable or disable a database connection. Reads omit the credential
reference and report `credential_configured` instead.
