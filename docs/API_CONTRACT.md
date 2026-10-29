# API_CONTRACT.md — the Tidepool API contract

> Established by task 2.0.1. The contract is the single input for every SDK,
> CLI and control plane, so this file says exactly what a client may rely on,
> what may change without a major version, and how a breaking change is
> announced.

---

## 1. What the contract is

Tidepool publishes one OpenAPI 3.1 document covering **both** surfaces:

| Surface | Prefixes | Stability |
|---|---|---|
| Game API (Talo-compatible) | `/v1/auth`, `/v1/players`, `/v1/leaderboards`, `/v1/saves`, `/v1/games`, `/v1/events`, `/v1/feedback`, `/v1/socket-tickets` | Stable; unchanged by this contract |
| Hub API (operator) | `/v1/setup`, `/v1/pool`, `/v1/api-keys`, `/v1/hosts`, `/v1/connections`, `/v1/projects`, `/v1/triggers`, `/v1/runs`, `/v1/activity`, `/v1/chatrooms`, `/v1/sessions/{session}/relay`, `/v1/webhooks` | Stable under this contract |
| System | `/healthz`, `/metrics`, `/openapi.json`, `/openapi/v1.json` | Operational; not a client API |

Every hub operation, its request body, its response body and the API-key scope
or role it needs is in the document. `x-surface`, `x-auth`, `x-required-scope`
and `x-required-role` carry the access model in machine-readable form; the same
facts appear in the human-readable reference.

### 1.1 Where it lives

| Artifact | Path | Purpose |
|---|---|---|
| Served document | `GET /openapi.json` | Live document for discovery and tooling |
| Pinned served document | `GET /openapi/v1.json` | Same bytes, major-version path clients can pin |
| Release copy | `docs/api/openapi-v1.json` | Committed, reviewable snapshot for the `v1` major |
| Human-readable reference | [`docs/API_REFERENCE.md`](API_REFERENCE.md) | Generated from the same tables |
| Error taxonomy | [`docs/HUB_API_ERRORS.md`](HUB_API_ERRORS.md) | Codes and problem-detail shape |

The committed snapshot is what a client pins. It is regenerated, reviewed and
released with the code, so `docs/api/openapi-v1.json` at tag `vX.Y.Z` always
describes the build at that tag. A new major adds `docs/api/openapi-v2.json`
and `/openapi/v2.json`; `v1` stays available for its deprecation window.

### 1.2 Source of truth and drift

The document and the reference are generated from two tables in
[`src/api/openapi.rs`](../src/api/openapi.rs):

- `ROUTES` — method, path, tag, surface, access model, scope, role, request
  schema, response schema, success status and per-route error statuses.
- `SCHEMAS` — every hub request and response body, field by field, with
  required/optional and nullability.

Three automated gates keep them honest:

1. `every_registered_hub_route_is_in_the_table` parses the `web::scope(...)`
   registrations in every hub module and requires an exact match with
   `ROUTES`. A route added in code without a table row fails.
2. `published_contract_matches_generated_document` and
   `api_reference_matches_generated_document` compare the committed artifacts
   byte for byte with the generated output. A table change without a
   regeneration fails, so the published contract always changes when the API
   does.
3. CI boots the server in degraded mode, fetches `/openapi/v1.json`, diffs it
   against the committed artifact, and validates it against the OpenAPI 3.1
   meta-schema.

Regenerate after an intentional change:

```sh
cargo test --lib regenerate_published_contract -- --ignored
cargo test --lib regenerate_api_reference -- --ignored
cargo test --lib openapi
```

---

## 2. Resource versus command

**Decision: the hub is resource-oriented wherever a thing has an identity and
a lifecycle, and command-oriented only where an operation is a transition
rather than a thing. Commands are explicit `POST`s under a verb path and carry
an idempotency key when a client may retry them.**

### 2.1 Why this split, and not one style

A resource style is better for everything a client stores, lists, diffs and
addresses later. A host, project, connection, key, member, invitation, room or
relay state has an immutable id, a create/read/rename/delete lifecycle and
usually a list endpoint. Modelling those as `POST /createHost` would force
every SDK to hand-write CRUD wrappers, make paging and filtering ad hoc, and
lose HTTP's idempotent `GET`/`PUT`/`DELETE` semantics (safe retries, caches,
proxies). So they stay nouns:

| Resource | Collection | Item |
|---|---|---|
| hosts | `GET/POST /v1/hosts` | `GET/PATCH/DELETE /v1/hosts/{id}` |
| connections | `GET/POST /v1/connections` | `GET/PATCH/DELETE /v1/connections/{id}` |
| projects | `GET/POST /v1/projects` | `GET/PATCH/DELETE /v1/projects/{id}` |
| configuration revisions | `GET /v1/projects/{id}/revisions` | `GET /v1/projects/{id}/revisions/{revision_id}` |
| API keys | `GET/POST /v1/api-keys` | `DELETE /v1/api-keys/{id}` |
| members | `GET /v1/pool/members` | `PATCH/DELETE /v1/pool/members/{id}` |
| invitations | `GET/POST /v1/pool/invitations` | `DELETE /v1/pool/invitations/{id}` |
| chatrooms | `GET /v1/chatrooms` | `GET /v1/chatrooms/{room}/members`, `GET/POST /v1/chatrooms/{room}/messages` |
| relay state | — | `GET/PUT/DELETE /v1/sessions/{session}/relay` |

A command style is better where there is no resource to address. Validation is
the clearest case: `POST /v1/projects/{id}/configuration/validate` and
`POST /v1/triggers/validate` compute an answer and write nothing, so there is
nothing to `GET` afterwards and no id to return. Installing a trigger or a
configuration is a transition that mints an immutable revision — the revision
is the resource, and the install is the act that creates it. Dispatching a run
is an act that may be replayed; it names its project and trigger in the body
and is deduplicated by `deliveryKey`. Enrolment is a two-step exchange
(mint a token, then exchange it), not CRUD over tokens. Password change is a
security transition, not a user resource update.

| Command | Path | Idempotency |
|---|---|---|
| Validate configuration | `POST /v1/projects/{id}/configuration/validate` | Pure; safe to retry |
| Install configuration | `POST /v1/projects/{id}/configuration/install` | Creates a new revision; retrying creates another revision |
| Validate trigger | `POST /v1/triggers/validate` | Pure; safe to retry |
| Install trigger | `POST /v1/triggers/install` | Creates or replaces by trigger name; the id is stable and `version` increments |
| Dispatch run | `POST /v1/runs` | `deliveryKey`; a repeat returns the existing run with `deduplicated: true` |
| Create/replay execution | `POST /v1/hosts/{id}/executions` | `execution_id`; a repeat returns the existing execution with `replayed: true` |
| Mint enrollment token | `POST /v1/hosts/enrollment-tokens` | Not retry-safe by design; tokens are single-use and short-lived |
| Exchange enrollment | `POST /v1/hosts/enroll` | Token is single-use; a repeat is `409`/`410` |
| Change password | `POST /v1/pool/password` | Not idempotent; each call needs the current password |
| Enable/stop relay | `PUT` / `DELETE /v1/sessions/{session}/relay` | Idempotent by verb; stopping is permanent |
| Publish relay event | `POST /v1/sessions/{session}/relay/events` | Not deduplicated; events are a stream with a monotonic sequence |

**Rejected alternatives.** A pure command API (`POST /v1/hosts/create`, …)
loses HTTP semantics and forces every SDK to invent paging, filtering and
retry rules. A pure resource API would invent fake resources for validate and
install (`POST /v1/validations` returning a validation id nobody reads) or
overload `PATCH` on a trigger to mean "compile and replace", which hides a
side effect that can fail validation and change revisions. A GraphQL-style
single endpoint was rejected because the pool's clients are HTTP tools and
SDKs, not a query language, and because the game surface is already REST.

**Consequences.** SDKs expose typed resource clients plus a small, explicitly
named command namespace (`triggers.validate`, `runs.dispatch`, …). Commands
never pretend to be `PATCH` on a resource, and every retry-safe command
documents its key in the request schema. A command that later gains a durable
resource (for example, a stored validation report) can add a resource beside
it without breaking the command.

---

## 3. Stability policy for the `v1` major

The contract version is independent of the crate version. The current
`info.version` is `1.0.0`; the pinned path is `/openapi/v1.json`.

### 3.1 Stable within `v1` — a client may rely on it

- Every documented path, HTTP method and `operationId`.
- Every request and response **field name, JSON type and nullability**,
  including camelCase vs snake_case spelling.
- Which fields are required; an optional field may be absent or `null` as the
  schema says.
- Status codes documented for an operation, and the meaning of `2xx` vs the
  documented error statuses.
- The RFC 9457 error shape, the `code` values in
  [`HUB_API_ERRORS.md`](HUB_API_ERRORS.md), `requestId`, and the
  `x-request-id` header.
- The authentication model: `Authorization: Bearer <jwt>` for operators,
  `x-api-key` for keys, `X-Host-Credential` for hosts, provider signatures for
  webhooks — and the `x-auth` / `x-required-scope` / `x-required-role`
  extensions that describe them.
- Scope names (`projects:read`, `configuration:validate`,
  `configuration:install`, `runs:dispatch`, `daemons:enroll`) and role names
  (`owner`, `admin`, `member`, `viewer`).
- Idempotency semantics: `deliveryKey` and `execution_id` replay behaviour.
- Enum string values marked closed in the schema. Enum values not listed as
  closed are still listed; clients must tolerate unknown values and treat them
  as opaque (see 3.2).

### 3.2 May change within `v1` — clients must tolerate it

- **Additive endpoints.** A new route may appear at any time.
- **Additive optional request fields.** Unknown fields are refused by the
  current handlers (`deny_unknown_fields`), so clients must send the fields the
  document lists; a new optional field is released as a minor contract bump.
- **Additive response fields.** Clients must ignore fields they do not know.
  A response may gain fields with no contract bump, and this is the main
  forward-compatibility rule.
- **New enum values** for open enums (currently `status`, `event`, `kind` and
  provider names where the provider adds one). Clients must not crash on an
  unknown value; they may display it verbatim.
- Descriptions, examples, summaries, tags, operation ordering and
  human-readable text.
- Defaults for optional request fields, as long as the documented behaviour
  stays the same.
- Rate-limit budgets and header values, as long as documented statuses and the
  `Retry-After` contract hold.
- The exact set of non-security response headers.
- Adding a new API-key scope (existing keys never gain it implicitly).

### 3.3 Breaking within `v1` — not allowed

- Removing or renaming a path, method, `operationId`, field, scope or role.
- Changing a field's type, nullability, case, or making an optional field
  required.
- Removing a documented status code or changing what a status means.
- Tightening validation so a request the document allowed starts failing.
- Changing authentication or authorization requirements for an existing
  operation.
- Removing or renaming an error `code`.
- Changing idempotency semantics (`deliveryKey`, `execution_id`, single-use
  tokens, relay stop being permanent).
- Changing a closed enum's value set.
- Changing the `Problem` shape or dropping `requestId`/`x-request-id`.

Anything in 3.3 requires a new major.

### 3.4 Additive changes and the minor version

A purely additive change (new endpoint, new optional field, new open-enum
value) bumps the contract **minor** (`1.0.0` → `1.1.0`), regenerates both
artifacts and ships in the normal release. No client change is required.

### 3.5 How a breaking change is announced

1. **Decide and record.** The change is a task in the plan, with a decision
   entry in [`docs/memory.md`](memory.md) naming the old and new shape and the
   migration.
2. **Bump the major.** `CONTRACT_VERSION` becomes `2.0.0`; `CONTRACT_MAJOR`
   becomes `2`; `docs/api/openapi-v2.json` and `/openapi/v2.json` are added.
3. **Keep `v1` serving.** Both documents are served during the overlap window;
   `v1` operations that will disappear are marked `deprecated: true` in the
   document and answered with a `Deprecation` header and a `Sunset` header
   naming the last supported date.
4. **Announce.** `docs/CHANGES.md` carries a `⚠️ BREAKING` entry with the old
   shape, the new shape and a migration recipe. The release notes and the
   tag's GitHub Release repeat it. A pinned client that has not moved still
   gets `v1` until the sunset date.
5. **Remove, later.** After the sunset date, `v1` returns the documented
   `gone` problem and the major is retired. Removing a document version is
   itself announced a release ahead.

The overlap window is at least one minor release and never shorter than 90
days from the deprecation announcement, whichever is longer.

---

## 4. Authentication, scopes and roles

| Credential | Header | Used by |
|---|---|---|
| Operator JWT | `Authorization: Bearer <jwt>` | Members of any role (reads); non-viewers (writes) |
| API key | `x-api-key: <prefix>.<secret>` (or Bearer) | Automation; each key carries a subset of the five scopes |
| Host credential | `X-Host-Credential` | Enrolled hosts reporting capabilities |
| Provider signature | `X-Hub-Signature-256` (GitHub), `X-Slack-Signature` (Slack) | Inbound webhooks |

`x-auth` values: `public`, `member`, `member_or_api_key`, `api_key`, `host`,
`signature`. A `member_or_api_key` operation accepts either an operator JWT
(member of any role for reads, non-viewer for writes) or an API key with the
`x-required-scope` scope. `x-required-role: owner` narrows an operation to the
organization owner.

| Scope | Grants |
|---|---|
| `projects:read` | Read projects, configuration revisions and activity |
| `configuration:validate` | Validate configuration and trigger documents |
| `configuration:install` | Install configuration revisions and triggers |
| `runs:dispatch` | Dispatch manual runs |
| `daemons:enroll` | Mint host enrollment tokens |

**Security boundary.** Everyone who can change the active configuration can
choose which connections, hosts and agent capabilities a project uses.
Therefore the configuration repository is itself a protected asset: writing a
configuration revision or installing a trigger is an administrative action,
and an API key with `configuration:install` is as sensitive as an owner's
password.

---

## 5. What every SDK must generate

The SDKs (tasks 2.0.2–2.0.6) take `docs/api/openapi-v1.json` as their only
input. They must:

- generate one typed client per resource and one command namespace per
  command table in section 2;
- map `Problem` to a typed error carrying `status`, `code`, `detail` and
  `requestId`, preserving unknown codes as opaque strings;
- surface `x-required-scope` and `x-required-role` so a caller can check
  permissions before sending a request;
- tolerate unknown response fields and unknown open-enum values;
- not generate a client for a route marked `deprecated: true` without a
  deprecation warning.

A conformance suite (task 2.0.6) runs the same request/response fixtures
against every SDK. The fixtures are generated from the same tables, so a route
or field change reaches every SDK through one contract change.
