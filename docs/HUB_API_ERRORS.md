# Hub API error contract

Hub routes — `/v1/setup`, `/v1/pool` and `/v1/api-keys` — return failures as
RFC 9457 `application/problem+json`. The game API under `/v1/auth`,
`/v1/players`, ... keeps its existing `{"error": "..."}` bodies.

## Shape

```json
{
  "type": "https://tidepool.dev/problems/missing_scope",
  "title": "Forbidden",
  "status": 403,
  "detail": "API key lacks the required scope projects:read",
  "code": "missing_scope",
  "requestId": "b6f0f2c1-6a64-4d5d-9f7e-2f2c1f9f9d3a"
}
```

`requestId` is also returned in the `x-request-id` response header. An incoming
`x-request-id` is reused when it is 1–128 characters of `[A-Za-z0-9._-]`;
otherwise a UUID v4 is generated.

## Codes

| code | status | meaning |
|---|---|---|
| `unauthorized` | 401 | missing or invalid credentials |
| `forbidden` | 403 | authenticated, but the role is not allowed |
| `missing_scope` | 403 | valid API key without the scope an endpoint needs |
| `invalid_request` | 400 | malformed or unusable input |
| `validation_failed` | 422 | field validation failed |
| `not_found` | 404 | no such member, invitation, key or hub route |
| `conflict` | 409 | already used / already initialized / last owner |
| `gone` | 410 | invitation expired or revoked |
| `method_not_allowed` | 405 | wrong HTTP method for a hub route |
| `rate_limited` | 429 | credential budget exhausted |
| `daemon_not_connected` | 409 | reserved for dispatch (phase 1.5) |
| `configuration_invalid` | 422 | reserved for validate/install (phase 1.3) |
| `service_unavailable` | 503 | database or Redis unavailable |
| `internal_error` | 500 | unexpected server failure |

## Authorization documentation

OpenAPI operations for hub routes carry:

- `x-auth`: `public`, `member` (operator JWT) or `api_key`.
- `x-required-scope`: the API-key scope, when `x-auth` is `api_key`.

The Rust drift test compares the hub route registrations with the single
`ROUTES` table, and the CI scrape checks the served document for `x-auth` and
`x-required-scope`. Adding a hub route without documenting it fails CI.

## Implementation

- `src/api/problem.rs` — `Problem`, `ProblemCode`, `ProblemError`.
- `src/middleware/problem.rs` — converts any non-problem hub failure (extractor
  401/403, 404, 405, 429, 5xx) into the contract; leaves game routes alone.
- `src/middleware/request_id.rs` — correlation id inserted into request
  extensions and echoed on the response.
