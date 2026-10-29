//! Machine-readable API contract (tasks 0.4.10 and 2.0.1).
//!
//! The document is **generated from two tables in this file**:
//!
//! - [`ROUTES`] — every route, its tag, its access model, its required API-key
//!   scope and role, and the request/response schemas it uses.
//! - [`SCHEMAS`] — every hub request and response body, field by field.
//!
//! Because the tables are the only input, the document cannot silently drift:
//! `cargo test openapi` compares the generated document and the generated
//! human-readable reference against the committed copies, and compares the
//! route table against the `web::scope(...)` registrations in the hub modules.
//! Adding a route without documenting it — or documenting it without
//! regenerating the published artifact — fails the build.
//!
//! The document is served at `/openapi.json` and at the stable,
//! major-version-pinned path `/openapi/v1.json`. The release copy lives at
//! `docs/api/openapi-v1.json`; the policy that says what may change within
//! `v1` is [`docs/API_CONTRACT.md`](../../docs/API_CONTRACT.md).
//!
//! ## Resource versus command
//!
//! The hub surface is resource-oriented wherever a thing has an identity and a
//! lifecycle (`hosts`, `projects`, `connections`, `api-keys`, `members`,
//! `invitations`, `chatrooms`, `sessions/{id}/relay`). Operations that create
//! no addressable resource, or that are transitions rather than state, are
//! commands under a verb path: `triggers/validate`, `triggers/install`,
//! `runs`, `configuration/validate`, `configuration/install`,
//! `enrollment-tokens`, `enroll`, `password`, `executions`. Commands that a
//! client may retry carry an explicit idempotency key
//! (`deliveryKey`, `executionId`) in the body. The reasoning is recorded in
//! [`docs/API_CONTRACT.md`](../../docs/API_CONTRACT.md#resource-versus-command).

use actix_web::{HttpResponse, web};
use utoipa::openapi::{
    ArrayBuilder, ComponentsBuilder, Content, InfoBuilder, KnownFormat, ObjectBuilder, OpenApi,
    PathItem, PathsBuilder, Ref, RefOr, Required, Response, ResponseBuilder, Responses,
    ResponsesBuilder, Schema, SchemaFormat, SecurityRequirement, Tag, Type,
    extensions::Extensions,
    path::{HttpMethod, Operation, OperationBuilder, Parameter, ParameterBuilder, ParameterIn},
    request_body::{RequestBody, RequestBodyBuilder},
    schema::SchemaType,
    security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme},
};

/// Major version of the published contract. The pinned served path is
/// `/openapi/v{CONTRACT_MAJOR}.json`.
pub const CONTRACT_MAJOR: &str = "1";

/// Full contract version. Bump the minor for additive changes and the major
/// (plus a new published artifact) for a breaking change.
pub const CONTRACT_VERSION: &str = "1.0.0";

/// Path of the committed, human-readable reference.
#[cfg(test)]
const REFERENCE_PATH: &str = "docs/API_REFERENCE.md";

/// Path of the committed, machine-readable release copy.
#[cfg(test)]
fn artifact_path() -> String {
    format!("docs/api/openapi-v{CONTRACT_MAJOR}.json")
}

/// Which product surface a route belongs to. Game routes keep their existing
/// loose documentation; hub routes are the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Surface {
    /// Talo-compatible player/game API under `/v1/auth`, `/v1/players`, ...
    Game,
    /// Pool/hub operator API.
    Hub,
    /// Infrastructure routes outside `/v1` (`/healthz`, `/metrics`, ...).
    System,
}

impl Surface {
    /// Wire spelling used by the `x-surface` extension.
    fn label(self) -> &'static str {
        match self {
            Surface::Game => "game",
            Surface::Hub => "hub",
            Surface::System => "system",
        }
    }
}

/// How a caller authenticates to a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Access {
    /// No credentials required.
    Public,
    /// Operator JWT (`Authorization: Bearer <jwt>`).
    Member,
    /// Operator JWT, or an API key carrying the required scope.
    MemberOrApiKey,
    /// API key only (`x-api-key`, or a Bearer key).
    ApiKey,
    /// Host relationship credential (`x-host-credential`).
    Host,
    /// A provider signature (GitHub `X-Hub-Signature-256`, Slack
    /// `X-Slack-Signature`). Documented, not expressible as an OpenAPI
    /// security scheme because the header and algorithm are provider-specific.
    Signature,
}

impl Access {
    /// Wire spelling used by the `x-auth` extension.
    fn label(self) -> &'static str {
        match self {
            Access::Public => "public",
            Access::Member => "member",
            Access::MemberOrApiKey => "member_or_api_key",
            Access::ApiKey => "api_key",
            Access::Host => "host",
            Access::Signature => "signature",
        }
    }

    /// Whether the route needs an `Authorization`-style credential at all.
    fn authenticated(self) -> bool {
        !matches!(self, Access::Public | Access::Signature)
    }
}

/// One documented HTTP route. The route table is the single source of truth.
struct RouteSpec {
    method: HttpMethod,
    path: &'static str,
    tag: &'static str,
    summary: &'static str,
    surface: Surface,
    access: Access,
    /// API-key scope required when the caller authenticates with a key.
    required_scope: Option<&'static str>,
    /// Minimum operator role, when the handler enforces one (`owner`).
    required_role: Option<&'static str>,
    /// Component schema of the JSON request body.
    request: Option<&'static str>,
    /// Component schema of the success response body.
    response: Option<&'static str>,
    /// Success status code.
    success: &'static str,
    /// Additional success statuses (idempotent replay) and their schemas.
    alternates: &'static [(&'static str, &'static str)],
    /// Additional documented error statuses for this route.
    errors: &'static [&'static str],
}

/// Defaults shared by hub routes; every row overrides what it needs.
const HUB: RouteSpec = RouteSpec {
    method: HttpMethod::Get,
    path: "",
    tag: "hub",
    summary: "",
    surface: Surface::Hub,
    access: Access::Member,
    required_scope: None,
    required_role: None,
    request: None,
    response: None,
    success: "200",
    alternates: &[],
    errors: &[],
};

/// Defaults shared by game routes.
const GAME: RouteSpec = RouteSpec {
    method: HttpMethod::Get,
    path: "",
    tag: "game",
    summary: "",
    surface: Surface::Game,
    access: Access::Public,
    required_scope: None,
    required_role: None,
    request: None,
    response: None,
    success: "200",
    alternates: &[],
    errors: &[],
};

/// Defaults shared by system routes.
const SYSTEM: RouteSpec = RouteSpec {
    method: HttpMethod::Get,
    path: "",
    tag: "system",
    summary: "",
    surface: Surface::System,
    access: Access::Public,
    required_scope: None,
    required_role: None,
    request: None,
    response: None,
    success: "200",
    alternates: &[],
    errors: &[],
};

/// The route table. Canonical `/v1` game routes plus the un-prefixed system
/// endpoints; legacy `/auth`, `/players`, ... aliases are the same handlers and
/// are omitted for brevity (the canonical mount is authoritative).
const ROUTES: &[RouteSpec] = &[
    // ---------------------------------------------------------------- game
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/auth/register",
        tag: "auth",
        summary: "Register a new player and return the public profile",
        success: "201",
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/auth/login",
        tag: "auth",
        summary: "Verify credentials and issue a signed JWT",
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/players/{id}",
        tag: "players",
        summary: "Fetch a player's public profile",
        access: Access::Member,
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Put,
        path: "/v1/players/{id}",
        tag: "players",
        summary: "Update the authenticated player's own profile",
        access: Access::Member,
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/players/{id}",
        tag: "players",
        summary: "Soft-delete the authenticated player's own account",
        access: Access::Member,
        success: "204",
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/players/subaccount",
        tag: "players",
        summary: "Create a subaccount under the authenticated player",
        access: Access::Member,
        success: "201",
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/players/{id}/subaccounts",
        tag: "players",
        summary: "List the authenticated player's subaccounts",
        access: Access::Member,
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/leaderboards/{game_id}",
        tag: "leaderboards",
        summary: "Fetch one ranked page of a leaderboard",
        access: Access::Member,
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/leaderboards/{game_id}/entries",
        tag: "leaderboards",
        summary: "Submit a leaderboard entry for the authenticated player",
        access: Access::Member,
        success: "201",
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Put,
        path: "/v1/leaderboards/{game_id}/entries/{player_id}",
        tag: "leaderboards",
        summary: "Update the authenticated player's own leaderboard entry",
        access: Access::Member,
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/saves",
        tag: "saves",
        summary: "Create a game save",
        access: Access::Member,
        success: "201",
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/saves/{player_id}",
        tag: "saves",
        summary: "List the authenticated player's saves, newest first",
        access: Access::Member,
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/saves/{player_id}/{save_id}",
        tag: "saves",
        summary: "Fetch a single game save",
        access: Access::Member,
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/saves/{player_id}/{save_id}",
        tag: "saves",
        summary: "Delete a single game save",
        access: Access::Member,
        success: "204",
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/games/{game_id}/settings",
        tag: "game settings",
        summary: "Fetch a game's settings",
        access: Access::Member,
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Put,
        path: "/v1/games/{game_id}/settings",
        tag: "game settings",
        summary: "Set a game's settings (upsert)",
        access: Access::Member,
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/events",
        tag: "analytics",
        summary: "Ingest a batch of analytics events (fire-and-forget)",
        access: Access::Member,
        success: "202",
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/feedback",
        tag: "feedback",
        summary: "Submit player feedback",
        access: Access::Member,
        success: "201",
        ..GAME
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/socket-tickets",
        tag: "websocket",
        summary: "Issue a one-shot WebSocket connection ticket",
        success: "200",
        ..GAME
    },
    // ----------------------------------------------------------- pool setup
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/setup/status",
        tag: "pool",
        summary: "Report whether the pool still needs first-run setup",
        access: Access::Public,
        response: Some("SetupStatusResponse"),
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/setup",
        tag: "pool",
        summary: "Create the organization and first owner from the browser",
        access: Access::Public,
        request: Some("SetupRequest"),
        response: Some("PoolIdentityView"),
        success: "201",
        errors: &["409"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/pool/login",
        tag: "pool",
        summary: "Sign an operator in and issue a JWT",
        access: Access::Public,
        request: Some("PoolLoginRequest"),
        response: Some("PoolLoginResponse"),
        errors: &["401"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/pool/me",
        tag: "pool",
        summary: "Fetch the signed-in operator and organization",
        response: Some("PoolIdentityView"),
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/pool/password",
        tag: "pool",
        summary: "Replace the operator's password (clears first-sign-in requirement)",
        request: Some("ChangePasswordRequest"),
        success: "204",
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/pool/members",
        tag: "pool",
        summary: "List the organization's operators",
        response: Some("MembersResponse"),
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Patch,
        path: "/v1/pool/members/{id}",
        tag: "pool",
        summary: "Change a member's role (owner only; last owner protected)",
        required_role: Some("owner"),
        request: Some("UpdateRoleRequest"),
        response: Some("PoolUserView"),
        errors: &["404", "409"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/pool/members/{id}",
        tag: "pool",
        summary: "Remove a member (owner only; last owner protected)",
        required_role: Some("owner"),
        success: "204",
        errors: &["404", "409"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/pool/invitations",
        tag: "pool",
        summary: "List pending team invitations (owner only)",
        required_role: Some("owner"),
        response: Some("InvitationsResponse"),
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/pool/invitations",
        tag: "pool",
        summary: "Invite an operator by email; the token is shown once",
        required_role: Some("owner"),
        request: Some("InviteRequest"),
        response: Some("InviteResponse"),
        success: "201",
        errors: &["409"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/pool/invitations/accept",
        tag: "pool",
        summary: "Accept an invitation and create the member account",
        access: Access::Public,
        request: Some("AcceptInvitationRequest"),
        response: Some("PoolLoginResponse"),
        success: "201",
        errors: &["404", "410"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/pool/invitations/{id}",
        tag: "pool",
        summary: "Revoke a pending invitation (owner only)",
        required_role: Some("owner"),
        success: "204",
        errors: &["404", "410"],
        ..HUB
    },
    // ------------------------------------------------------------ api keys
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/api-keys",
        tag: "api keys",
        summary: "List organization API keys (owner only; prefix only)",
        required_role: Some("owner"),
        response: Some("ApiKeysResponse"),
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/api-keys",
        tag: "api keys",
        summary: "Create an API key; the full secret is shown once",
        required_role: Some("owner"),
        request: Some("CreateApiKeyRequest"),
        response: Some("CreateApiKeyResponse"),
        success: "201",
        errors: &["422"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/api-keys/whoami",
        tag: "api keys",
        summary: "Introspect an API key (requires projects:read)",
        access: Access::ApiKey,
        required_scope: Some("projects:read"),
        response: Some("ApiKeyView"),
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/api-keys/{id}",
        tag: "api keys",
        summary: "Revoke an API key (owner only; effective immediately)",
        required_role: Some("owner"),
        success: "204",
        errors: &["404"],
        ..HUB
    },
    // --------------------------------------------------------------- hosts
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/hosts",
        tag: "hosts",
        summary: "List live hosts in the pool",
        response: Some("HostsResponse"),
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/hosts",
        tag: "hosts",
        summary: "Register a host; the slug is derived from the hostname",
        request: Some("CreateHostRequest"),
        response: Some("HostView"),
        success: "201",
        errors: &["409"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/hosts/{id}",
        tag: "hosts",
        summary: "Fetch a live host by immutable id",
        response: Some("HostView"),
        errors: &["404"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Patch,
        path: "/v1/hosts/{id}",
        tag: "hosts",
        summary: "Rename a host slug or display name; the id is unchanged",
        request: Some("RenameHostRequest"),
        response: Some("HostView"),
        errors: &["404", "409"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/hosts/{id}",
        tag: "hosts",
        summary: "Revoke the pool relationship for a host",
        success: "204",
        errors: &["404"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/hosts/{id}/execution",
        tag: "hosts",
        summary: "Grant host.execute so the host may run agents",
        response: Some("HostView"),
        errors: &["404", "409"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/hosts/{id}/execution",
        tag: "hosts",
        summary: "Revoke host.execute; the host becomes connected-only",
        response: Some("HostView"),
        errors: &["404"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/hosts/{id}/capabilities",
        tag: "hosts",
        summary: "Read the host's last capability report and offered providers",
        response: Some("HostCapabilityReport"),
        errors: &["404"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/hosts/{id}/capabilities",
        tag: "hosts",
        summary: "Host reports providers, models, modes and thinking options",
        access: Access::Host,
        request: Some("CapabilityReportRequest"),
        response: Some("HostCapabilityReport"),
        errors: &["401", "404", "422"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/hosts/{id}/executions",
        tag: "hosts",
        summary: "Create or replay a create-agent intent with a stable execution id",
        request: Some("CreateExecutionRequest"),
        response: Some("CreateExecutionResponse"),
        success: "201",
        alternates: &[("200", "CreateExecutionResponse")],
        errors: &["404", "409", "422"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/hosts/enrollment-tokens",
        tag: "hosts",
        summary: "Mint a ten-minute single-use host enrollment token",
        access: Access::MemberOrApiKey,
        required_scope: Some("daemons:enroll"),
        request: Some("MintEnrollmentTokenRequest"),
        response: Some("MintEnrollmentTokenResponse"),
        success: "201",
        errors: &["404"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/hosts/enroll",
        tag: "hosts",
        summary: "Exchange an enrollment token for a host relationship credential",
        access: Access::Public,
        request: Some("EnrollHostRequest"),
        response: Some("HostView"),
        success: "201",
        // 410: the token exists but expired, or the key that minted it was
        // revoked (services::enrollment::EnrollmentError::{TokenExpired,
        // TokenRevoked}). The conformance suite pins it as `gone`, not 404/409.
        errors: &["401", "404", "409", "410"],
        ..HUB
    },
    // --------------------------------------------------------- connections
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/connections",
        tag: "connections",
        summary: "List effective provider connections (environment overlay applied)",
        response: Some("ConnectionsResponse"),
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/connections",
        tag: "connections",
        summary: "Create a provider connection from a credential reference",
        request: Some("CreateConnectionRequest"),
        response: Some("ConnectionView"),
        success: "201",
        errors: &["409", "422"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/connections/{id}",
        tag: "connections",
        summary: "Fetch a connection by id or env:<slug>",
        response: Some("ConnectionView"),
        errors: &["404"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Patch,
        path: "/v1/connections/{id}",
        tag: "connections",
        summary: "Rename, enable or disable a database connection",
        request: Some("UpdateConnectionRequest"),
        response: Some("ConnectionView"),
        errors: &["404", "409"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/connections/{id}",
        tag: "connections",
        summary: "Delete a database connection",
        success: "204",
        errors: &["404", "409"],
        ..HUB
    },
    // ------------------------------------------------------------ projects
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/projects",
        tag: "projects",
        summary: "List projects in the organization",
        access: Access::MemberOrApiKey,
        required_scope: Some("projects:read"),
        response: Some("ProjectsResponse"),
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/projects",
        tag: "projects",
        summary: "Create a project",
        request: Some("CreateProjectRequest"),
        response: Some("ProjectView"),
        success: "201",
        errors: &["409", "422"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/projects/{id}",
        tag: "projects",
        summary: "Fetch a project by id",
        access: Access::MemberOrApiKey,
        required_scope: Some("projects:read"),
        response: Some("ProjectView"),
        errors: &["404"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Patch,
        path: "/v1/projects/{id}",
        tag: "projects",
        summary: "Rename a project slug or name; the id is unchanged",
        request: Some("RenameProjectRequest"),
        response: Some("ProjectView"),
        errors: &["404", "409"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/projects/{id}",
        tag: "projects",
        summary: "Delete a project and its revisions",
        success: "204",
        errors: &["404"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/projects/{id}/configuration/validate",
        tag: "projects",
        summary: "Validate a configuration bundle without writing pool state",
        access: Access::MemberOrApiKey,
        required_scope: Some("configuration:validate"),
        request: Some("ConfigurationRequest"),
        response: Some("ValidateResponse"),
        errors: &["404"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/projects/{id}/configuration/install",
        tag: "projects",
        summary: "Install a configuration bundle as a new immutable revision",
        access: Access::MemberOrApiKey,
        required_scope: Some("configuration:install"),
        request: Some("ConfigurationRequest"),
        response: Some("InstallResponse"),
        success: "201",
        errors: &["404", "422"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/projects/{id}/configuration",
        tag: "projects",
        summary: "Fetch a project's active configuration revision",
        access: Access::MemberOrApiKey,
        required_scope: Some("projects:read"),
        response: Some("RevisionView"),
        errors: &["404"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/projects/{id}/revisions",
        tag: "projects",
        summary: "List a project's configuration revisions",
        access: Access::MemberOrApiKey,
        required_scope: Some("projects:read"),
        response: Some("RevisionsResponse"),
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/projects/{id}/revisions/{revision_id}",
        tag: "projects",
        summary: "Fetch one configuration revision with its files",
        access: Access::MemberOrApiKey,
        required_scope: Some("projects:read"),
        response: Some("RevisionView"),
        errors: &["404"],
        ..HUB
    },
    // ------------------------------------------------------------ triggers
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/triggers/validate",
        tag: "triggers",
        summary: "Validate one trigger document without writing pool state",
        access: Access::MemberOrApiKey,
        required_scope: Some("configuration:validate"),
        request: Some("TriggerDocument"),
        response: Some("TriggerValidateResponse"),
        errors: &["403", "404"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/triggers/install",
        tag: "triggers",
        summary: "Install or replace a trigger; keeps its id and bumps the revision",
        access: Access::MemberOrApiKey,
        required_scope: Some("configuration:install"),
        request: Some("TriggerInstallRequest"),
        response: Some("TriggerInstallResponse"),
        success: "201",
        errors: &["403", "404", "422"],
        ..HUB
    },
    // ---------------------------------------------------------------- runs
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/runs",
        tag: "runs",
        summary: "Dispatch a workflow run from a manual request",
        access: Access::MemberOrApiKey,
        required_scope: Some("runs:dispatch"),
        request: Some("ManualDispatchRequest"),
        response: Some("ManualDispatchResponse"),
        success: "201",
        alternates: &[("200", "ManualDispatchResponse")],
        errors: &["403", "404", "409", "422"],
        ..HUB
    },
    // ------------------------------------------------------------ activity
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/activity",
        tag: "activity",
        summary: "List one page of receipts, runs, steps and executions",
        access: Access::MemberOrApiKey,
        required_scope: Some("projects:read"),
        response: Some("ActivityPage"),
        errors: &["422"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/activity/runs/{run}",
        tag: "activity",
        summary: "Fetch one run with the arrival record that started it",
        access: Access::MemberOrApiKey,
        required_scope: Some("projects:read"),
        response: Some("ActivityRunDetail"),
        errors: &["404"],
        ..HUB
    },
    // ----------------------------------------------------------- chatrooms
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/chatrooms",
        tag: "chatrooms",
        summary: "List rooms visible to the operator",
        response: Some("RoomViewList"),
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/chatrooms/{room}/members",
        tag: "chatrooms",
        summary: "List a room's members",
        response: Some("RoomMemberViewList"),
        errors: &["404"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/chatrooms/{room}/messages",
        tag: "chatrooms",
        summary: "Read room history after a sequence cursor",
        response: Some("HistoryResponse"),
        errors: &["404", "422"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/chatrooms/{room}/messages",
        tag: "chatrooms",
        summary: "Post a message to a room",
        request: Some("SendMessageBody"),
        response: Some("MessageView"),
        success: "201",
        errors: &["403", "404", "422"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/chatrooms/{room}/stream",
        tag: "chatrooms",
        summary: "Upgrade to the room's live message stream",
        success: "101",
        errors: &["403", "404"],
        ..HUB
    },
    // --------------------------------------------------------------- relay
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/sessions/{session}/relay",
        tag: "relay",
        summary: "Read a session's relay publication state",
        response: Some("RelayState"),
        errors: &["404"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Put,
        path: "/v1/sessions/{session}/relay",
        tag: "relay",
        summary: "Opt a session into publication",
        response: Some("RelayState"),
        errors: &["404", "409"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Delete,
        path: "/v1/sessions/{session}/relay",
        tag: "relay",
        summary: "Stop publication immediately and permanently",
        response: Some("RelayState"),
        errors: &["404"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/sessions/{session}/relay/events",
        tag: "relay",
        summary: "Publish one allow-listed session event",
        request: Some("PublishBody"),
        response: Some("RelayEvent"),
        success: "201",
        errors: &["404", "409", "422"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/v1/sessions/{session}/relay/events",
        tag: "relay",
        summary: "Backfill relayed events after a sequence cursor",
        response: Some("RelayPage"),
        errors: &["404", "422"],
        ..HUB
    },
    // ------------------------------------------------------------ webhooks
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/webhooks/github/{slug}",
        tag: "webhooks",
        summary: "Receive a GitHub App webhook; signature verified and delivery deduplicated",
        access: Access::Signature,
        request: Some("RawJson"),
        response: Some("WebhookResponse"),
        success: "202",
        errors: &["401", "404"],
        ..HUB
    },
    RouteSpec {
        method: HttpMethod::Post,
        path: "/v1/webhooks/slack/{slug}",
        tag: "webhooks",
        summary: "Receive a Slack Events API callback; signature verified and event deduplicated",
        access: Access::Signature,
        request: Some("RawJson"),
        response: Some("WebhookResponse"),
        success: "202",
        alternates: &[("200", "ChallengeResponse")],
        errors: &["401", "404"],
        ..HUB
    },
    // -------------------------------------------------------------- system
    RouteSpec {
        method: HttpMethod::Get,
        path: "/healthz",
        tag: "system",
        summary: "Liveness probe with DB status",
        ..SYSTEM
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/metrics",
        tag: "system",
        summary: "Prometheus text-format metrics",
        ..SYSTEM
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/openapi.json",
        tag: "system",
        summary: "This OpenAPI document",
        response: Some("OpenApiDocument"),
        ..SYSTEM
    },
    RouteSpec {
        method: HttpMethod::Get,
        path: "/openapi/v1.json",
        tag: "system",
        summary: "This OpenAPI document at its pinned major-version path",
        response: Some("OpenApiDocument"),
        ..SYSTEM
    },
];

/// Type of one schema property.
#[derive(Debug, Clone, Copy)]
enum Kind {
    /// JSON string.
    Str,
    /// Nullable JSON string.
    NullableStr,
    /// JSON integer.
    Int,
    /// Nullable JSON integer.
    NullableInt,
    /// JSON boolean.
    Bool,
    /// Nullable JSON boolean.
    NullableBool,
    /// RFC 3339 date-time string.
    DateTime,
    /// Nullable RFC 3339 date-time string.
    NullableDateTime,
    /// Free-form JSON value.
    Json,
    /// Array of strings.
    StrArray,
    /// String-to-string map.
    StrMap,
    /// A string with a fixed set of values.
    Enum(&'static [&'static str]),
    /// A `$ref` to another component schema.
    Ref(&'static str),
    /// An array of `$ref`s.
    RefArray(&'static str),
}

/// One field of a component schema.
struct Field {
    name: &'static str,
    kind: Kind,
    required: bool,
    description: Option<&'static str>,
}

/// Required field without a description.
const fn f(name: &'static str, kind: Kind) -> Field {
    Field {
        name,
        kind,
        required: true,
        description: None,
    }
}

/// Optional field without a description.
const fn o(name: &'static str, kind: Kind) -> Field {
    Field {
        name,
        kind,
        required: false,
        description: None,
    }
}

/// Required field with a description.
const fn fd(name: &'static str, kind: Kind, description: &'static str) -> Field {
    Field {
        name,
        kind,
        required: true,
        description: Some(description),
    }
}

/// Optional field with a description.
const fn od(name: &'static str, kind: Kind, description: &'static str) -> Field {
    Field {
        name,
        kind,
        required: false,
        description: Some(description),
    }
}

/// One component schema. Either an object with [`Field`]s, an array of one
/// referenced schema, or an unconstrained JSON value.
struct SchemaSpec {
    name: &'static str,
    description: &'static str,
    fields: &'static [Field],
    /// When set, the schema is a JSON array whose items `$ref` this schema.
    array_of: Option<&'static str>,
    /// When true the schema accepts any JSON value (no fields).
    any_value: bool,
}

/// Object schema with fields.
const fn s(name: &'static str, description: &'static str, fields: &'static [Field]) -> SchemaSpec {
    SchemaSpec {
        name,
        description,
        fields,
        array_of: None,
        any_value: false,
    }
}

/// Array schema of one referenced component.
const fn sa(name: &'static str, description: &'static str, item: &'static str) -> SchemaSpec {
    SchemaSpec {
        name,
        description,
        fields: &[],
        array_of: Some(item),
        any_value: false,
    }
}

/// Unconstrained JSON schema.
const fn sj(name: &'static str, description: &'static str) -> SchemaSpec {
    SchemaSpec {
        name,
        description,
        fields: &[],
        array_of: None,
        any_value: true,
    }
}

/// Every component schema the contract publishes. Hub request and response
/// bodies all resolve here; the game surface keeps the loose object bodies it
/// had before the contract existed.
const SCHEMAS: &[SchemaSpec] = &[
    // ------------------------------------------------------------- errors
    s(
        "Problem",
        "RFC 9457 problem detail returned by every hub route on failure",
        &[
            fd("type", Kind::Str, "Problem type URI"),
            fd("title", Kind::Str, "Short human-readable summary"),
            f("status", Kind::Int),
            od("detail", Kind::Str, "Human-readable explanation"),
            fd(
                "code",
                Kind::Enum(&[
                    "unauthorized",
                    "forbidden",
                    "missing_scope",
                    "invalid_request",
                    "validation_failed",
                    "not_found",
                    "conflict",
                    "gone",
                    "method_not_allowed",
                    "rate_limited",
                    "daemon_not_connected",
                    "configuration_invalid",
                    "service_unavailable",
                    "internal_error",
                ]),
                "Stable machine-readable code (docs/HUB_API_ERRORS.md)",
            ),
            fd(
                "requestId",
                Kind::Str,
                "Correlation id, also echoed in x-request-id",
            ),
        ],
    ),
    s(
        "GameError",
        "Error body used by the game surface (`{\"error\": \"...\"}`)",
        &[f("error", Kind::Str)],
    ),
    sj(
        "RawJson",
        "Any JSON value; provider-specific webhook payload delivered verbatim",
    ),
    s(
        "OpenApiDocument",
        "This OpenAPI 3.1 documents",
        &[
            fd("openapi", Kind::Str, "OpenAPI version"),
            f("info", Kind::Json),
            f("paths", Kind::Json),
        ],
    ),
    // -------------------------------------------------------------- views
    s(
        "OrganizationView",
        "One organization (one pool in v1)",
        &[
            f("id", Kind::Str),
            f("name", Kind::Str),
            f("created_at", Kind::DateTime),
        ],
    ),
    s(
        "PoolUserView",
        "Operator account, never carrying credential material",
        &[
            f("id", Kind::Str),
            f("email", Kind::Str),
            f("display_name", Kind::Str),
            f("role", Kind::Enum(&["owner", "admin", "member", "viewer"])),
            f("password_change_required", Kind::Bool),
            o("last_login_at", Kind::NullableDateTime),
            f("created_at", Kind::DateTime),
        ],
    ),
    s(
        "PoolInvitationView",
        "Pending or historical invitation metadata (never the token)",
        &[
            f("id", Kind::Str),
            f("email", Kind::Str),
            f("role", Kind::Enum(&["owner", "admin", "member", "viewer"])),
            f("expires_at", Kind::DateTime),
            o("accepted_at", Kind::NullableDateTime),
            o("revoked_at", Kind::NullableDateTime),
            f("created_at", Kind::DateTime),
        ],
    ),
    s(
        "PoolIdentityView",
        "The signed-in operator and its organization",
        &[
            f("organization", Kind::Ref("OrganizationView")),
            f("user", Kind::Ref("PoolUserView")),
        ],
    ),
    s(
        "PoolLoginResponse",
        "A freshly issued operator JWT",
        &[
            fd(
                "token",
                Kind::Str,
                "Signed JWT; subject is the operator public id",
            ),
            fd("expires_in", Kind::Int, "Token lifetime in seconds"),
            fd(
                "must_change_password",
                Kind::Bool,
                "True while the bootstrap password must be replaced",
            ),
            f("user", Kind::Ref("PoolUserView")),
        ],
    ),
    s(
        "SetupStatusResponse",
        "First-run setup state",
        &[
            f("needs_setup", Kind::Bool),
            f("bootstrap_configured", Kind::Bool),
        ],
    ),
    s(
        "SetupRequest",
        "First-run browser setup payload",
        &[
            f("organization_name", Kind::Str),
            f("email", Kind::Str),
            f("display_name", Kind::Str),
            fd("password", Kind::Str, "At least 12 characters"),
        ],
    ),
    s(
        "PoolLoginRequest",
        "Operator sign-in payload",
        &[f("email", Kind::Str), f("password", Kind::Str)],
    ),
    s(
        "ChangePasswordRequest",
        "Replace the operator's password",
        &[
            f("current_password", Kind::Str),
            fd("new_password", Kind::Str, "At least 12 characters"),
        ],
    ),
    s(
        "MembersResponse",
        "Operators in the organization",
        &[f("members", Kind::RefArray("PoolUserView"))],
    ),
    s(
        "InvitationsResponse",
        "Invitations still awaiting acceptance",
        &[f("invitations", Kind::RefArray("PoolInvitationView"))],
    ),
    s(
        "InviteRequest",
        "Invite an operator by email",
        &[
            f("email", Kind::Str),
            fd(
                "role",
                Kind::Enum(&["owner", "admin", "member", "viewer"]),
                "Defaults to member",
            ),
        ],
    ),
    s(
        "InviteResponse",
        "Invitation metadata plus the one-time acceptance token",
        &[
            f("invitation", Kind::Ref("PoolInvitationView")),
            fd("token", Kind::Str, "Shown once; not stored in plaintext"),
        ],
    ),
    s(
        "AcceptInvitationRequest",
        "Accept an invitation and create the member account",
        &[
            f("token", Kind::Str),
            f("display_name", Kind::Str),
            fd("password", Kind::Str, "At least 12 characters"),
        ],
    ),
    s(
        "UpdateRoleRequest",
        "Change a member's role",
        &[f(
            "role",
            Kind::Enum(&["owner", "admin", "member", "viewer"]),
        )],
    ),
    // ------------------------------------------------------------ api keys
    s(
        "ApiKeyView",
        "API key metadata; the secret is never returned after creation",
        &[
            f("id", Kind::Str),
            f("name", Kind::Str),
            f("prefix", Kind::Str),
            f("scopes", Kind::StrArray),
            f("status", Kind::Enum(&["active", "revoked"])),
            o("last_used_at", Kind::NullableDateTime),
            o("revoked_at", Kind::NullableDateTime),
            f("created_at", Kind::DateTime),
        ],
    ),
    s(
        "ApiKeysResponse",
        "Organization API keys, newest first",
        &[f("api_keys", Kind::RefArray("ApiKeyView"))],
    ),
    s(
        "CreateApiKeyRequest",
        "Create an API key",
        &[
            f("name", Kind::Str),
            fd(
                "scopes",
                Kind::StrArray,
                "Subset of projects:read, configuration:validate, \
                 configuration:install, runs:dispatch, daemons:enroll",
            ),
        ],
    ),
    s(
        "CreateApiKeyResponse",
        "Key metadata plus the one-time secret",
        &[
            f("api_key", Kind::Ref("ApiKeyView")),
            fd("secret", Kind::Str, "`<prefix>.<secret>`; shown once"),
        ],
    ),
    // --------------------------------------------------------------- hosts
    s(
        "HostView",
        "One enrolled machine",
        &[
            f("id", Kind::Str),
            f("slug", Kind::Str),
            f("display_name", Kind::Str),
            f("hostname", Kind::Str),
            f(
                "status",
                Kind::Enum(&["pending_approval", "connected", "offline", "revoked"]),
            ),
            f("execution_enabled", Kind::Bool),
            o("revoked_at", Kind::NullableDateTime),
            f("created_at", Kind::DateTime),
        ],
    ),
    s(
        "HostsResponse",
        "Live hosts, ordered by slug",
        &[f("hosts", Kind::RefArray("HostView"))],
    ),
    s(
        "CreateHostRequest",
        "Register a host",
        &[
            f("hostname", Kind::Str),
            o("display_name", Kind::NullableStr),
        ],
    ),
    s(
        "RenameHostRequest",
        "Rename a host; at least one field is required",
        &[
            o("slug", Kind::NullableStr),
            o("display_name", Kind::NullableStr),
        ],
    ),
    s(
        "AgentCapability",
        "One provider/model/mode/thinking combination a host can run",
        &[
            f("provider", Kind::Str),
            f("model", Kind::Str),
            f("execution_mode", Kind::Str),
            o("thinking_option", Kind::NullableStr),
            o("options", Kind::Json),
        ],
    ),
    s(
        "CapabilityReportRequest",
        "A host's capability report",
        &[f("capabilities", Kind::RefArray("AgentCapability"))],
    ),
    s(
        "HostCapabilityReport",
        "The last stored capability report, plus what the pool may offer",
        &[
            f("host_id", Kind::Str),
            o("reported_at", Kind::NullableDateTime),
            f("capabilities", Kind::RefArray("AgentCapability")),
            f("offered_providers", Kind::StrArray),
        ],
    ),
    s(
        "HostExecutionView",
        "One agent execution started on a host",
        &[
            f("id", Kind::Str),
            f(
                "status",
                Kind::Enum(&["queued", "running", "succeeded", "failed", "interrupted"]),
            ),
            o("output", Kind::NullableStr),
            o("completed_at", Kind::NullableDateTime),
        ],
    ),
    s(
        "CreateExecutionRequest",
        "Create or replay a create-agent intent",
        &[
            fd("execution_id", Kind::Str, "Stable id; the replay key"),
            f("prompt", Kind::Str),
        ],
    ),
    s(
        "CreateExecutionResponse",
        "The stored execution and whether the call replayed an existing one",
        &[
            f("execution", Kind::Ref("HostExecutionView")),
            f("replayed", Kind::Bool),
        ],
    ),
    s(
        "MintEnrollmentTokenRequest",
        "Mint a single-use enrollment token for one host",
        &[f("host_id", Kind::Str)],
    ),
    s(
        "EnrollmentTokenView",
        "Enrollment token metadata (never the token)",
        &[
            f("id", Kind::Str),
            f("expires_at", Kind::DateTime),
            o("consumed_at", Kind::NullableDateTime),
            o("revoked_at", Kind::NullableDateTime),
            f("created_at", Kind::DateTime),
        ],
    ),
    s(
        "MintEnrollmentTokenResponse",
        "The one-time enrollment token",
        &[
            f("token", Kind::Str),
            f("enrollment_token", Kind::Ref("EnrollmentTokenView")),
        ],
    ),
    s(
        "EnrollHostRequest",
        "Exchange an enrollment token for a host credential",
        &[
            f("token", Kind::Str),
            fd(
                "credential",
                Kind::Str,
                "Host-generated relationship secret",
            ),
        ],
    ),
    // --------------------------------------------------------- connections
    s(
        "ConnectionResourceView",
        "One resource a connection may address",
        &[
            f("resource_kind", Kind::Str),
            f("external_id", Kind::Str),
            o("display_name", Kind::NullableStr),
        ],
    ),
    s(
        "ConnectionView",
        "One provider connection",
        &[
            f("id", Kind::Str),
            f("provider", Kind::Enum(&["github", "slack", "discord"])),
            f("slug", Kind::Str),
            f("display_name", Kind::Str),
            f("status", Kind::Enum(&["active", "disabled"])),
            f("managed_by", Kind::Enum(&["database", "environment"])),
            f("credential_configured", Kind::Bool),
            f("resources", Kind::RefArray("ConnectionResourceView")),
            f("created_at", Kind::DateTime),
        ],
    ),
    s(
        "ConnectionsResponse",
        "Effective connections after the environment overlay",
        &[f("connections", Kind::RefArray("ConnectionView"))],
    ),
    s(
        "ResourceRequest",
        "A resource a new connection may address",
        &[
            f("resource_kind", Kind::Str),
            f("external_id", Kind::Str),
            o("display_name", Kind::NullableStr),
        ],
    ),
    s(
        "CreateConnectionRequest",
        "Create a connection from a credential reference",
        &[
            f("provider", Kind::Enum(&["github", "slack", "discord"])),
            f("slug", Kind::Str),
            f("display_name", Kind::Str),
            fd(
                "credential_ref",
                Kind::Str,
                "Environment variable name; the secret itself is never sent",
            ),
            o("resources", Kind::RefArray("ResourceRequest")),
        ],
    ),
    s(
        "UpdateConnectionRequest",
        "Rename, enable or disable a database connection",
        &[
            o("slug", Kind::NullableStr),
            o("display_name", Kind::NullableStr),
            o("status", Kind::NullableStr),
        ],
    ),
    // ------------------------------------------------------------ projects
    s(
        "ProjectView",
        "One project",
        &[
            f("id", Kind::Str),
            f("slug", Kind::Str),
            f("name", Kind::Str),
            f("created_at", Kind::DateTime),
            f("updated_at", Kind::DateTime),
        ],
    ),
    s(
        "ProjectsResponse",
        "Projects in the organization",
        &[f("projects", Kind::RefArray("ProjectView"))],
    ),
    s(
        "CreateProjectRequest",
        "Create a project",
        &[f("name", Kind::Str), o("slug", Kind::NullableStr)],
    ),
    s(
        "RenameProjectRequest",
        "Rename a project; the id is unchanged",
        &[o("slug", Kind::NullableStr), o("name", Kind::NullableStr)],
    ),
    s(
        "ConfigFileRequest",
        "One configuration file as authored",
        &[f("path", Kind::Str), f("content", Kind::Str)],
    ),
    s(
        "ConfigurationRequest",
        "A configuration bundle (at most 100 files)",
        &[f("files", Kind::RefArray("ConfigFileRequest"))],
    ),
    s(
        "FieldIssue",
        "One named validation issue",
        &[
            f("field", Kind::Str),
            f("revision", Kind::Int),
            f("message", Kind::Str),
        ],
    ),
    s(
        "ValidateResponse",
        "Validation result; nothing was written",
        &[
            f("valid", Kind::Bool),
            f("version", Kind::Int),
            f("errors", Kind::RefArray("FieldIssue")),
        ],
    ),
    s(
        "RevisionFileView",
        "One file of a stored revision",
        &[f("path", Kind::Str), f("content", Kind::Str)],
    ),
    s(
        "RevisionMetaView",
        "Revision metadata without file bodies",
        &[
            f("id", Kind::Str),
            f("version", Kind::Int),
            f("active", Kind::Bool),
            f("status", Kind::Enum(&["valid", "invalid"])),
            f("error_count", Kind::Int),
            f("created_at", Kind::DateTime),
        ],
    ),
    s(
        "RevisionView",
        "One immutable configuration revision",
        &[
            f("id", Kind::Str),
            f("version", Kind::Int),
            f("active", Kind::Bool),
            f("status", Kind::Enum(&["valid", "invalid"])),
            f("errors", Kind::RefArray("FieldIssue")),
            f("files", Kind::RefArray("RevisionFileView")),
            f("created_at", Kind::DateTime),
        ],
    ),
    s(
        "InstallResponse",
        "The installed revision and whether it became active",
        &[
            f("revision", Kind::Ref("RevisionView")),
            f("activated", Kind::Bool),
        ],
    ),
    s(
        "RevisionsResponse",
        "A project's revisions, newest first",
        &[f("revisions", Kind::RefArray("RevisionMetaView"))],
    ),
    // ------------------------------------------------------------ triggers
    s(
        "TriggerDocument",
        "One authored trigger document",
        &[
            f("name", Kind::Str),
            f("event", Kind::Ref("TriggerEvent")),
            f("filters", Kind::Ref("TriggerFilters")),
            f("run", Kind::Ref("TriggerRun")),
        ],
    ),
    s(
        "TriggerEvent",
        "The provider event that may start a trigger",
        &[
            f("provider", Kind::Enum(&["github", "slack", "discord"])),
            fd("type", Kind::Str, "Provider event type"),
        ],
    ),
    s(
        "TriggerFilters",
        "Which deliveries of that event are allowed through",
        &[
            f("connection", Kind::Str),
            f("resource", Kind::Ref("TriggerResourceFilter")),
            fd(
                "from_users",
                Kind::StrArray,
                "Required allowlist; an empty list is rejected",
            ),
            o("channel", Kind::NullableStr),
            o("label", Kind::NullableStr),
            o("contains", Kind::NullableStr),
            o("pattern", Kind::NullableStr),
        ],
    ),
    s(
        "TriggerResourceFilter",
        "The provider resource a trigger is scoped to",
        &[f("kind", Kind::Str), f("id", Kind::Str)],
    ),
    s(
        "TriggerRun",
        "Where and with which agent a matched trigger runs",
        &[
            f("host", Kind::Str),
            f("agent", Kind::Ref("AgentSelection")),
        ],
    ),
    s(
        "AgentSelection",
        "A named provider/model/mode choice",
        &[
            f("provider", Kind::Str),
            f("model", Kind::Str),
            f("execution_mode", Kind::Str),
            o("thinking_option", Kind::NullableStr),
        ],
    ),
    s(
        "TriggerIssue",
        "One named trigger validation issue",
        &[f("field", Kind::Str), f("message", Kind::Str)],
    ),
    s(
        "TriggerValidateResponse",
        "Trigger validation result; nothing was written",
        &[
            f("name", Kind::Str),
            f("valid", Kind::Bool),
            f("errors", Kind::RefArray("TriggerIssue")),
        ],
    ),
    s(
        "TriggerInstallRequest",
        "A trigger document plus the project it belongs to",
        &[
            fd("project", Kind::Str, "Public id of the owning project"),
            f("name", Kind::Str),
            f("event", Kind::Ref("TriggerEvent")),
            f("filters", Kind::Ref("TriggerFilters")),
            f("run", Kind::Ref("TriggerRun")),
        ],
    ),
    s(
        "TriggerInstallResponse",
        "The installed trigger and the revision the install produced",
        &[
            fd(
                "id",
                Kind::Str,
                "Immutable trigger id; a replacement keeps it",
            ),
            f("name", Kind::Str),
            f("revision", Kind::Str),
            f("version", Kind::Int),
            f("active", Kind::Bool),
            f("replaced", Kind::Bool),
        ],
    ),
    // ---------------------------------------------------------------- runs
    s(
        "RunStepView",
        "One step of a workflow run",
        &[
            f("id", Kind::Str),
            f("step", Kind::Str),
            f("position", Kind::Int),
            f(
                "status",
                Kind::Enum(&[
                    "pending",
                    "running",
                    "succeeded",
                    "failed",
                    "skipped",
                    "interrupted",
                ]),
            ),
            o("condition", Kind::NullableBool),
            o("executionId", Kind::NullableStr),
            o("environment", Kind::NullableStr),
            o("host", Kind::NullableStr),
            o("directory", Kind::NullableStr),
            o("branch", Kind::NullableStr),
            o("output", Kind::NullableStr),
            o("error", Kind::NullableStr),
            o("reason", Kind::NullableStr),
            o("startedAt", Kind::NullableDateTime),
            o("completedAt", Kind::NullableDateTime),
        ],
    ),
    s(
        "RunView",
        "One workflow run with its steps",
        &[
            f("id", Kind::Str),
            f("project", Kind::Str),
            f("trigger", Kind::Str),
            f("triggerName", Kind::Str),
            f("workflow", Kind::Str),
            f("origin", Kind::Enum(&["event", "manual"])),
            o("actor", Kind::NullableStr),
            fd("deliveryKey", Kind::Str, "Idempotency key for the arrival"),
            f(
                "status",
                Kind::Enum(&["pending", "running", "succeeded", "failed", "interrupted"]),
            ),
            f("requestText", Kind::Str),
            f("inputs", Kind::StrMap),
            o("deadlineAt", Kind::NullableDateTime),
            o("startedAt", Kind::NullableDateTime),
            o("completedAt", Kind::NullableDateTime),
            f("createdAt", Kind::DateTime),
            f("steps", Kind::RefArray("RunStepView")),
        ],
    ),
    s(
        "ManualDispatchRequest",
        "A manual run request",
        &[
            f("project", Kind::Str),
            f("trigger", Kind::Str),
            f("actor", Kind::Str),
            o("revision", Kind::NullableStr),
            fd(
                "deliveryKey",
                Kind::Str,
                "Stable per request; a repeat returns the existing run",
            ),
            o("input", Kind::Str),
        ],
    ),
    s(
        "ManualDispatchResponse",
        "The run a manual request created or matched",
        &[
            f("run", Kind::Ref("RunView")),
            fd("deduplicated", Kind::Bool, "True on an idempotent replay"),
        ],
    ),
    // ------------------------------------------------------------ activity
    s(
        "ActivityEntry",
        "One receipt, run, step or execution in the activity feed",
        &[
            f("kind", Kind::Enum(&["receipt", "run", "step", "execution"])),
            f("id", Kind::Str),
            o("project", Kind::NullableStr),
            o("provider", Kind::NullableStr),
            o("trigger", Kind::NullableStr),
            o("triggerName", Kind::NullableStr),
            o("run", Kind::NullableStr),
            o("step", Kind::NullableStr),
            o("execution", Kind::NullableStr),
            f("status", Kind::Str),
            o("eventType", Kind::NullableStr),
            o("actor", Kind::NullableStr),
            o("matched", Kind::NullableBool),
            o("duplicates", Kind::NullableInt),
            o("output", Kind::NullableStr),
            o("error", Kind::NullableStr),
            o("continuation", Kind::NullableStr),
            f("at", Kind::DateTime),
        ],
    ),
    s(
        "ActivityPage",
        "One cursor-paged activity window",
        &[
            f("items", Kind::RefArray("ActivityEntry")),
            o("nextCursor", Kind::NullableStr),
            f("hasMore", Kind::Bool),
        ],
    ),
    s(
        "OutputLimit",
        "One declared output capability limit",
        &[
            f("capability", Kind::Str),
            f("maximum", Kind::Int),
            f("required", Kind::Bool),
        ],
    ),
    s(
        "StepOutputLimits",
        "Output limits declared by one step",
        &[
            f("step", Kind::Str),
            f("limits", Kind::RefArray("OutputLimit")),
        ],
    ),
    s(
        "ArrivalDetail",
        "How an arrival created, continued or restored an agent",
        &[
            f("mode", Kind::Enum(&["created", "continued", "restored"])),
            o("provider", Kind::NullableStr),
            o("conversationKey", Kind::NullableStr),
            f("steered", Kind::Bool),
            o("receipt", Kind::NullableStr),
            f("outputLimits", Kind::RefArray("StepOutputLimits")),
        ],
    ),
    s(
        "ActivityRunDetail",
        "One run plus the arrival that started it",
        &[
            f("run", Kind::Ref("RunView")),
            f("arrival", Kind::Ref("ArrivalDetail")),
        ],
    ),
    // ----------------------------------------------------------- chatrooms
    s(
        "Principal",
        "The single principal behind a room member or message author",
        &[
            f("kind", Kind::Enum(&["user", "session"])),
            f("id", Kind::Str),
        ],
    ),
    s(
        "RoomView",
        "One chatroom",
        &[
            f("id", Kind::Str),
            f("name", Kind::Str),
            f("visibility", Kind::Enum(&["private", "unlisted", "public"])),
            o("origin_pool", Kind::NullableStr),
            f("created_at", Kind::DateTime),
            f("updated_at", Kind::DateTime),
        ],
    ),
    sa("RoomViewList", "Rooms visible to the caller", "RoomView"),
    s(
        "RoomMemberView",
        "One room member",
        &[
            f("id", Kind::Str),
            f("principal", Kind::Ref("Principal")),
            f("role", Kind::Enum(&["owner", "member", "viewer"])),
            f("joined_at", Kind::DateTime),
        ],
    ),
    sa("RoomMemberViewList", "A room's members", "RoomMemberView"),
    s(
        "MessageView",
        "One chat message",
        &[
            f("id", Kind::Str),
            o("author", Kind::Ref("Principal")),
            f("body", Kind::Str),
            f("project", Kind::Str),
            o("reply_to", Kind::NullableStr),
            f("created_at", Kind::DateTime),
            f("sequence", Kind::Int),
        ],
    ),
    s(
        "HistoryResponse",
        "One page of chat history",
        &[
            f("messages", Kind::RefArray("MessageView")),
            o("next", Kind::NullableInt),
        ],
    ),
    s(
        "SendMessageBody",
        "Post a message to a room",
        &[f("body", Kind::Str), o("replyTo", Kind::NullableStr)],
    ),
    // --------------------------------------------------------------- relay
    s(
        "RelayState",
        "A session's relay publication state (off by default)",
        &[
            f("session", Kind::Str),
            f("enabled", Kind::Bool),
            o("startedAt", Kind::NullableDateTime),
            o("stoppedAt", Kind::NullableDateTime),
        ],
    ),
    s(
        "RelayEvent",
        "One relayed session event; never carries file contents",
        &[
            f("id", Kind::Str),
            f("session", Kind::Str),
            f("sequence", Kind::Int),
            f(
                "kind",
                Kind::Enum(&["started", "status", "output", "tool", "message", "stopped"]),
            ),
            f("payload", Kind::Json),
            f("createdAt", Kind::DateTime),
        ],
    ),
    s(
        "RelayPage",
        "One page of relayed events",
        &[
            f("events", Kind::RefArray("RelayEvent")),
            o("next", Kind::NullableInt),
        ],
    ),
    s(
        "PublishBody",
        "Publish one relayed event",
        &[
            f(
                "kind",
                Kind::Enum(&["started", "status", "output", "tool", "message", "stopped"]),
            ),
            fd(
                "payload",
                Kind::Json,
                "At most 8 KiB; credential-like keys are refused",
            ),
        ],
    ),
    // ------------------------------------------------------------ webhooks
    s(
        "WebhookResponse",
        "A verified provider delivery",
        &[
            f("received", Kind::Bool),
            f("duplicate", Kind::Bool),
            f("event", Kind::Str),
            o("action", Kind::NullableStr),
            f("accepted", Kind::Bool),
        ],
    ),
    s(
        "ChallengeResponse",
        "Slack URL-verification echo",
        &[f("challenge", Kind::Str)],
    ),
];

/// Register the OpenAPI routes.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/openapi.json").route(web::get().to(openapi_handler)))
        .service(web::resource("/openapi/v1.json").route(web::get().to(openapi_handler)));
}

/// Serve the generated OpenAPI document as JSON.
async fn openapi_handler() -> HttpResponse {
    match document_json() {
        Ok(json) => HttpResponse::Ok()
            .content_type("application/json")
            .body(json),
        Err(e) => {
            tracing::error!(error = %e, "openapi: serialization failed");
            HttpResponse::InternalServerError()
                .json(serde_json::json!({ "error": "internal error" }))
        }
    }
}

/// Canonical serialization of the served document: key-sorted pretty JSON with
/// a trailing newline, so the served bytes and the committed artifact are
/// identical run to run.
fn document_json() -> Result<String, serde_json::Error> {
    // `Extensions` is a hash map; round-tripping through `serde_json::Value`
    // gives a deterministic (BTreeMap) key order for the published bytes.
    let value = serde_json::to_value(document())?;
    let mut json = serde_json::to_string_pretty(&value)?;
    json.push('\n');
    Ok(json)
}

/// Build the complete OpenAPI document from the route and schema tables.
fn document() -> OpenApi {
    let mut paths = PathsBuilder::new();
    for route in ROUTES {
        paths = paths.path(
            route.path,
            PathItem::new(route.method.clone(), build_operation(route)),
        );
    }

    let mut components = ComponentsBuilder::new()
        .security_scheme(
            "bearerAuth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some(
                        "JWT issued by POST /v1/auth/login (player) or \
                         POST /v1/pool/login (operator).",
                    ))
                    .build(),
            ),
        )
        .security_scheme(
            "apiKeyAuth",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::with_description(
                "x-api-key",
                "Organization API key. Scopes: projects:read, \
                 configuration:validate, configuration:install, runs:dispatch, \
                 daemons:enroll.",
            ))),
        )
        .security_scheme(
            "hostCredential",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::with_description(
                "X-Host-Credential",
                "Relationship credential returned by POST /v1/hosts/enroll.",
            ))),
        );
    for schema in SCHEMAS {
        components = components.schema(schema.name, build_schema(schema));
    }
    let components = components.build();

    let tags: Vec<Tag> = ordered_tags().into_iter().map(Tag::new).collect();

    let info = InfoBuilder::new()
        .title("Tidepool Server API")
        .version(CONTRACT_VERSION)
        .description(Some(
            "Tidepool is one binary with two surfaces: the Talo-compatible game \
             API and the pool/hub operator API. Game routes under /v1/auth, \
             /v1/players, /v1/leaderboards, /v1/saves, /v1/games, /v1/events, \
             /v1/feedback and /v1/socket-tickets are unchanged by the contract. \
             Hub routes (/v1/setup, /v1/pool, /v1/api-keys, /v1/hosts, \
             /v1/connections, /v1/projects, /v1/triggers, /v1/runs, \
             /v1/activity, /v1/chatrooms, /v1/sessions/{session}/relay and \
             /v1/webhooks) return RFC 9457 application/problem+json with a \
             stable code and requestId. See docs/API_CONTRACT.md for what is \
             stable within v1, what may change, and how a breaking change is \
             announced; docs/HUB_API_ERRORS.md lists the error codes.",
        ))
        .build();

    let mut doc = OpenApi::builder()
        .info(info)
        .paths(paths.build())
        .components(Some(components))
        .tags(Some(tags))
        .build();
    doc.servers = None;
    doc
}

/// Tags in first-appearance order, so the document is deterministic.
fn ordered_tags() -> Vec<&'static str> {
    let mut tags: Vec<&'static str> = Vec::new();
    for route in ROUTES {
        if !tags.contains(&route.tag) {
            tags.push(route.tag);
        }
    }
    tags
}

/// Build one operation for a route.
fn build_operation(route: &RouteSpec) -> Operation {
    let mut builder = OperationBuilder::new()
        .operation_id(Some(operation_id(route)))
        .summary(Some(route.summary))
        .tag(route.tag)
        .responses(build_responses(route));

    match route.access {
        Access::Public | Access::Signature => {}
        Access::Member => {
            builder =
                builder.security(SecurityRequirement::new("bearerAuth", Vec::<String>::new()));
        }
        Access::MemberOrApiKey => {
            builder = builder
                .security(SecurityRequirement::new("bearerAuth", Vec::<String>::new()))
                .security(SecurityRequirement::new("apiKeyAuth", Vec::<String>::new()));
        }
        Access::ApiKey => {
            builder =
                builder.security(SecurityRequirement::new("apiKeyAuth", Vec::<String>::new()));
        }
        Access::Host => {
            builder = builder.security(SecurityRequirement::new(
                "hostCredential",
                Vec::<String>::new(),
            ));
        }
    }

    let mut extensions = Extensions::from_iter([
        ("x-surface", serde_json::json!(route.surface.label())),
        ("x-auth", serde_json::json!(route.access.label())),
        ("x-contract-version", serde_json::json!(CONTRACT_VERSION)),
    ]);
    if let Some(scope) = route.required_scope {
        extensions.insert("x-required-scope".to_string(), serde_json::json!(scope));
    }
    if let Some(role) = route.required_role {
        extensions.insert("x-required-role".to_string(), serde_json::json!(role));
    }
    builder = builder.extensions(Some(extensions));

    let params: Vec<Parameter> = path_params(route.path)
        .iter()
        .map(|n| path_parameter(n))
        .collect();
    builder = builder.parameters(Some(params));

    if let Some(schema) = route.request {
        builder = builder.request_body(Some(request_body(schema)));
    }

    builder.build()
}

/// Operation id derived from method and path, safe for generated clients.
fn operation_id(route: &RouteSpec) -> String {
    let sanitized: String = route
        .path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let collapsed = sanitized
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    format!("{}_{}", method_str(route.method.clone()), collapsed)
}

/// Standard responses for a route: the success status(es), the JSON body
/// schemas, and the error statuses the contract promises.
fn build_responses(route: &RouteSpec) -> Responses {
    let mut builder = ResponsesBuilder::new().response(
        route.success,
        success_response(route, route.success, route.response),
    );
    for (status, schema) in route.alternates {
        builder = builder.response(*status, success_response(route, status, Some(schema)));
    }
    for status in error_statuses(route) {
        builder = builder.response(
            status,
            err_response(status, error_description(status), route.surface),
        );
    }
    builder.build()
}

/// JSON response for a success status.
fn success_response(route: &RouteSpec, code: &str, schema: Option<&str>) -> Response {
    let description = success_description(code);
    let builder = ResponseBuilder::new().description(description);
    if code == "204" {
        return builder.build();
    }
    if route.path == "/metrics" {
        return builder
            .content(
                "text/plain",
                Content::new(Some(Schema::Object(
                    ObjectBuilder::new()
                        .schema_type(SchemaType::Type(Type::String))
                        .build(),
                ))),
            )
            .build();
    }
    if code == "101" {
        return builder.description("Switching Protocols").build();
    }
    let content = match schema {
        Some(name) => Content::new(Some(Ref::from_schema_name(name))),
        None => json_content(),
    };
    builder.content("application/json", content).build()
}

/// Error response body: RFC 9457 for hub routes, `{"error": ...}` for game.
fn err_response(status: &str, description: &str, surface: Surface) -> Response {
    let (content_type, schema) = match surface {
        Surface::Hub => ("application/problem+json", Ref::from_schema_name("Problem")),
        Surface::Game | Surface::System => ("application/json", Ref::from_schema_name("GameError")),
    };
    let description = format!("{status} — {description}");
    ResponseBuilder::new()
        .description(description)
        .content(content_type, Content::new(Some(schema)))
        .build()
}

/// Loose JSON body content for surfaces that are not part of the contract.
fn json_content() -> Content {
    Content::new(Some(Schema::Object(ObjectBuilder::new().build())))
}

/// Description text for a success status code.
fn success_description(code: &str) -> &'static str {
    match code {
        "200" => "OK",
        "201" => "Created",
        "202" => "Accepted",
        "204" => "No Content",
        "101" => "Switching Protocols",
        _ => "Success",
    }
}

/// Error statuses documented for a route: the per-route extras plus the
/// defaults implied by its access model, body and path parameters.
fn error_statuses(route: &RouteSpec) -> Vec<&'static str> {
    let mut statuses: Vec<&'static str> = Vec::new();
    let push = |statuses: &mut Vec<&'static str>, status: &'static str| {
        if !statuses.contains(&status) {
            statuses.push(status);
        }
    };
    if route.surface == Surface::Hub {
        if route.access.authenticated() {
            push(&mut statuses, "401");
            push(&mut statuses, "403");
        }
        if route.request.is_some() {
            push(&mut statuses, "400");
            push(&mut statuses, "422");
        }
        if route.path.contains('{') {
            push(&mut statuses, "404");
        }
        push(&mut statuses, "429");
        push(&mut statuses, "500");
        push(&mut statuses, "503");
    } else if route.access.authenticated() {
        push(&mut statuses, "400");
        push(&mut statuses, "401");
        push(&mut statuses, "429");
        push(&mut statuses, "503");
    }
    for status in route.errors {
        push(&mut statuses, status);
    }
    statuses.sort_unstable();
    statuses
}

/// Human-readable default description for an error status.
fn error_description(status: &str) -> &'static str {
    match status {
        "400" => "Bad request — malformed input",
        "401" => "Unauthorized — missing or invalid credentials",
        "403" => "Forbidden — role or API-key scope does not allow this operation",
        "404" => "Not found — no such resource in this organization",
        "409" => "Conflict — already initialized, already used, or the last owner",
        "410" => "Gone — invitation expired or revoked",
        "422" => "Unprocessable entity — field validation failed",
        "429" => "Too many requests — rate limit exceeded",
        "500" => "Internal error",
        "503" => "Service unavailable — database not configured",
        _ => "Error",
    }
}

/// Lower-case HTTP method name for `operationId`.
fn method_str(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "get",
        HttpMethod::Post => "post",
        HttpMethod::Put => "put",
        HttpMethod::Patch => "patch",
        HttpMethod::Delete => "delete",
        HttpMethod::Head => "head",
        HttpMethod::Options => "options",
        HttpMethod::Trace => "trace",
    }
}

/// `{...}` path template placeholders in `path`.
fn path_params(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|seg| seg.starts_with('{') && seg.ends_with('}'))
        .map(|seg| seg.trim_matches(['{', '}']).to_string())
        .collect()
}

/// Required path parameter of string type.
fn path_parameter(name: &str) -> Parameter {
    ParameterBuilder::new()
        .name(name)
        .parameter_in(ParameterIn::Path)
        .required(Required::True)
        .schema(Some(Schema::Object(
            ObjectBuilder::new()
                .schema_type(SchemaType::Type(Type::String))
                .build(),
        )))
        .build()
}

/// OpenAPI request body referencing a component schema.
fn request_body(schema: &str) -> RequestBody {
    RequestBodyBuilder::new()
        .content(
            "application/json",
            Content::new(Some(Ref::from_schema_name(schema))),
        )
        .required(Some(Required::True))
        .build()
}

/// Build one component schema from the schema table.
fn build_schema(spec: &SchemaSpec) -> Schema {
    if spec.any_value {
        return Schema::Object(
            ObjectBuilder::new()
                .schema_type(SchemaType::AnyValue)
                .description(Some(spec.description))
                .build(),
        );
    }
    if let Some(item) = spec.array_of {
        return Schema::Array(
            ArrayBuilder::new()
                .items(Ref::from_schema_name(item))
                .description(Some(spec.description))
                .build(),
        );
    }
    let mut object = ObjectBuilder::new().description(Some(spec.description));
    for field in spec.fields {
        object = object.property(field.name, property_schema(field.kind, field.description));
        if field.required {
            object = object.required(field.name);
        }
    }
    Schema::Object(object.build())
}

/// Schema of one property, discriminated by [`Kind`].
fn property_schema(kind: Kind, description: Option<&'static str>) -> RefOr<Schema> {
    match kind {
        Kind::Ref(name) => {
            let mut reference = Ref::from_schema_name(name);
            if let Some(description) = description {
                reference.description = description.to_string();
            }
            RefOr::Ref(reference)
        }
        Kind::RefArray(name) => Schema::Array(
            ArrayBuilder::new()
                .items(Ref::from_schema_name(name))
                .description(description)
                .build(),
        )
        .into(),
        Kind::StrMap => Schema::Object(
            ObjectBuilder::new()
                .additional_properties(Some(Schema::Object(
                    ObjectBuilder::new()
                        .schema_type(SchemaType::Type(Type::String))
                        .build(),
                )))
                .description(description)
                .build(),
        )
        .into(),
        Kind::StrArray => Schema::Array(
            ArrayBuilder::new()
                .items(
                    ObjectBuilder::new()
                        .schema_type(SchemaType::Type(Type::String))
                        .build(),
                )
                .description(description)
                .build(),
        )
        .into(),
        Kind::Enum(values) => Schema::Object(
            ObjectBuilder::new()
                .schema_type(SchemaType::Type(Type::String))
                .enum_values(Some(values.iter().copied()))
                .description(description)
                .build(),
        )
        .into(),
        other => Schema::Object(
            ObjectBuilder::new()
                .schema_type(schema_type(other))
                .format(format_of(other))
                .description(description)
                .build(),
        )
        .into(),
    }
}

/// Scalar/union type for a [`Kind`].
fn schema_type(kind: Kind) -> SchemaType {
    match kind {
        Kind::Str | Kind::DateTime | Kind::StrMap => SchemaType::Type(Type::String),
        Kind::NullableStr | Kind::NullableDateTime => {
            SchemaType::Array(vec![Type::String, Type::Null])
        }
        Kind::Int => SchemaType::Type(Type::Integer),
        Kind::NullableInt => SchemaType::Array(vec![Type::Integer, Type::Null]),
        Kind::Bool => SchemaType::Type(Type::Boolean),
        Kind::NullableBool => SchemaType::Array(vec![Type::Boolean, Type::Null]),
        Kind::Json => SchemaType::AnyValue,
        Kind::StrArray => SchemaType::Type(Type::Array),
        Kind::Enum(_) => SchemaType::Type(Type::String),
        Kind::Ref(_) => SchemaType::AnyValue,
        Kind::RefArray(_) => SchemaType::Type(Type::Array),
    }
}

/// `date-time` format for timestamp kinds.
fn format_of(kind: Kind) -> Option<SchemaFormat> {
    match kind {
        Kind::DateTime | Kind::NullableDateTime => {
            Some(SchemaFormat::KnownFormat(KnownFormat::DateTime))
        }
        _ => None,
    }
}

/// Render the human-readable reference from the same tables.
#[cfg(test)]
fn render_reference() -> String {
    let mut out = String::new();
    out.push_str("# Tidepool API reference\n\n");
    out.push_str(
        "> Generated from `src/api/openapi.rs` by `cargo test regenerate_api_reference -- --ignored`.\n",
    );
    out.push_str("> Do not edit this file by hand: the test suite fails when it drifts.\n");
    out.push_str(&format!(
        "> Contract version: `{CONTRACT_VERSION}` — see [API_CONTRACT.md](API_CONTRACT.md).\n\n"
    ));
    out.push_str(&format!(
        "The machine-readable document is served at `/openapi.json` and at the pinned \
         major-version path `/openapi/v{CONTRACT_MAJOR}.json`. The release copy is \
         `docs/api/openapi-v{CONTRACT_MAJOR}.json`.\n\n"
    ));

    for (surface, title) in [
        (Surface::Game, "Game API"),
        (Surface::Hub, "Hub API"),
        (Surface::System, "System"),
    ] {
        out.push_str(&format!("## {title}\n\n"));
        let mut tag = "";
        for route in ROUTES.iter().filter(|route| route.surface == surface) {
            if route.tag != tag {
                tag = route.tag;
                if !out.ends_with("\n\n") {
                    out.push('\n');
                }
                out.push_str(&format!("### {tag}\n\n"));
                out.push_str("| Method | Path | Auth | Scope | Request | Success | Response |\n");
                out.push_str("|---|---|---|---|---|---|---|\n");
            }
            let scope = route.required_scope.unwrap_or("—");
            let request = route.request.unwrap_or("—");
            let response = match (route.success, route.response) {
                ("204", _) | ("101", _) => "—".to_string(),
                (_, Some(name)) => name.to_string(),
                (_, None) => "object".to_string(),
            };
            out.push_str(&format!(
                "| {} | `{}` | {} | {} | {} | {} | {} |\n",
                method_str(route.method.clone()).to_uppercase(),
                route.path,
                route.access.label(),
                scope,
                request,
                route.success,
                response
            ));
        }
        out.push('\n');
    }

    out.push_str("## Schemas\n\n");
    for spec in SCHEMAS {
        out.push_str(&format!("### {}\n\n{}\n\n", spec.name, spec.description));
        if spec.any_value {
            out.push_str("Any JSON value.\n\n");
            continue;
        }
        if let Some(item) = spec.array_of {
            out.push_str(&format!(
                "Array of [`{item}`](#{}).\n\n",
                item.to_lowercase()
            ));
            continue;
        }
        out.push_str("| Field | Type | Required | Description |\n");
        out.push_str("|---|---|---|---|\n");
        for field in spec.fields {
            out.push_str(&format!(
                "| `{}` | {} | {} | {} |\n",
                field.name,
                kind_label(field.kind),
                if field.required { "yes" } else { "no" },
                field.description.unwrap_or("")
            ));
        }
        out.push('\n');
    }
    out
}

/// Markdown label for a [`Kind`].
#[cfg(test)]
fn kind_label(kind: Kind) -> String {
    match kind {
        Kind::Str => "string".to_string(),
        Kind::NullableStr => "string or null".to_string(),
        Kind::Int => "integer".to_string(),
        Kind::NullableInt => "integer or null".to_string(),
        Kind::Bool => "boolean".to_string(),
        Kind::NullableBool => "boolean or null".to_string(),
        Kind::DateTime => "string (date-time)".to_string(),
        Kind::NullableDateTime => "string (date-time) or null".to_string(),
        Kind::Json => "object".to_string(),
        Kind::StrArray => "string[]".to_string(),
        Kind::StrMap => "object (string map)".to_string(),
        Kind::Enum(values) => values
            .iter()
            .map(|value| format!("`{value}`"))
            .collect::<Vec<_>>()
            .join(" \\| "),
        Kind::Ref(name) => format!("[{name}](#{})", name.to_lowercase()),
        Kind::RefArray(name) => format!("[{name}](#{})[]", name.to_lowercase()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn json_doc() -> serde_json::Value {
        serde_json::from_str(&document_json().expect("serializes")).expect("valid JSON")
    }

    fn has_operation(item: &PathItem, method: HttpMethod) -> bool {
        match method {
            HttpMethod::Get => item.get.is_some(),
            HttpMethod::Post => item.post.is_some(),
            HttpMethod::Put => item.put.is_some(),
            HttpMethod::Patch => item.patch.is_some(),
            HttpMethod::Delete => item.delete.is_some(),
            _ => false,
        }
    }

    fn operation<'a>(doc: &'a OpenApi, route: &RouteSpec) -> &'a Operation {
        let item = doc
            .paths
            .paths
            .get(route.path)
            .unwrap_or_else(|| panic!("missing path {}", route.path));
        match route.method {
            HttpMethod::Get => item.get.as_ref(),
            HttpMethod::Post => item.post.as_ref(),
            HttpMethod::Put => item.put.as_ref(),
            HttpMethod::Patch => item.patch.as_ref(),
            HttpMethod::Delete => item.delete.as_ref(),
            _ => None,
        }
        .unwrap_or_else(|| {
            panic!(
                "missing operation {} {}",
                method_str(route.method.clone()),
                route.path
            )
        })
    }

    #[test]
    fn document_lists_every_route() {
        let doc = document();
        for route in ROUTES {
            let item = doc
                .paths
                .paths
                .get(route.path)
                .unwrap_or_else(|| panic!("missing path {}", route.path));
            assert!(
                has_operation(item, route.method.clone()),
                "{} missing {}",
                route.path,
                method_str(route.method.clone())
            );
        }
    }

    /// Pin the exact inventory so a changed prefix, a dropped route or an added
    /// system route fails here before it reaches a client.
    #[test]
    fn game_routes_are_unmoved_under_v1() {
        const SYSTEM_PATHS: &[&str] =
            &["/healthz", "/metrics", "/openapi.json", "/openapi/v1.json"];

        assert_eq!(ROUTES.len(), 83, "ROUTES must describe all 83 routes");
        assert_eq!(
            ROUTES
                .iter()
                .filter(|route| route.surface == Surface::Hub)
                .count(),
            60,
            "the hub contract documents 60 operations"
        );
        assert_eq!(
            ROUTES
                .iter()
                .filter(|route| route.path.starts_with("/v1/"))
                .count(),
            79,
            "19 game and 60 hub operations must stay under /v1/"
        );

        let system: BTreeSet<&str> = ROUTES
            .iter()
            .map(|route| route.path)
            .filter(|path| !path.starts_with("/v1/"))
            .collect();
        let expected: BTreeSet<&str> = SYSTEM_PATHS.iter().copied().collect();
        assert_eq!(
            system, expected,
            "only the four system routes may sit outside /v1/"
        );
    }

    #[test]
    fn document_has_expected_meta() {
        let doc = document();
        assert_eq!(doc.info.title, "Tidepool Server API");
        assert_eq!(doc.info.version, CONTRACT_VERSION);
        assert!(doc.paths.paths.contains_key("/v1/auth/register"));
        assert!(doc.paths.paths.contains_key("/metrics"));
        assert!(doc.paths.paths.contains_key("/openapi.json"));
        assert!(doc.paths.paths.contains_key("/openapi/v1.json"));
        let components = doc.components.expect("components present");
        assert!(components.security_schemes.contains_key("bearerAuth"));
        assert!(components.security_schemes.contains_key("apiKeyAuth"));
        assert!(components.security_schemes.contains_key("hostCredential"));
        assert!(components.schemas.contains_key("Problem"));
    }

    #[test]
    fn document_is_valid_json_with_expected_paths() {
        let value = json_doc();
        assert_eq!(value["openapi"], "3.1.0");
        assert_eq!(value["info"]["version"], CONTRACT_VERSION);
        for route in ROUTES {
            assert!(
                value["paths"].get(route.path).is_some(),
                "missing {} in JSON",
                route.path
            );
        }
    }

    /// A small, offline stand-in for the OpenAPI 3.1 meta-schema: the parts a
    /// structure change here could break. CI additionally validates the served
    /// document against the real meta-schema with openapi-spec-validator.
    #[test]
    fn document_is_structurally_openapi_31() {
        let value = json_doc();
        let version = value["openapi"].as_str().expect("openapi string");
        assert!(
            version.split('.').next().is_some_and(|major| major == "3"),
            "unexpected OpenAPI version {version}"
        );
        assert!(value["info"]["title"].is_string());
        assert!(value["info"]["version"].is_string());
        let paths = value["paths"].as_object().expect("paths object");
        assert!(!paths.is_empty());
        let methods = [
            "get", "put", "post", "delete", "options", "head", "patch", "trace",
        ];
        for (path, item) in paths {
            assert!(path.starts_with('/'), "path must start with /: {path}");
            let item = item.as_object().expect("path item object");
            let mut operations = 0;
            for (method, operation) in item {
                if !methods.contains(&method.as_str()) {
                    continue;
                }
                operations += 1;
                let operation = operation.as_object().expect("operation object");
                assert!(
                    operation.get("responses").is_some(),
                    "{method} {path} has no responses"
                );
                let responses = operation["responses"]
                    .as_object()
                    .expect("responses object");
                assert!(!responses.is_empty());
                for (status, response) in responses {
                    assert!(
                        status.len() == 3 && status.chars().all(|c| c.is_ascii_digit()),
                        "{method} {path} has a bad status {status}"
                    );
                    assert!(
                        response.get("description").is_some(),
                        "{method} {path} {status} has no description"
                    );
                }
                if let Some(body) = operation.get("requestBody") {
                    assert!(body.get("content").is_some(), "{method} {path} empty body");
                }
                for parameter in operation
                    .get("parameters")
                    .and_then(|value| value.as_array())
                    .into_iter()
                    .flatten()
                {
                    if parameter.get("in").and_then(|value| value.as_str()) == Some("path") {
                        assert_eq!(
                            parameter.get("required"),
                            Some(&serde_json::Value::Bool(true)),
                            "{method} {path} path parameter must be required"
                        );
                    }
                }
            }
            assert!(operations > 0, "{path} has no operations");
        }
    }

    #[test]
    fn operations_have_unique_ids() {
        let mut ids = BTreeSet::new();
        for route in ROUTES {
            let id = operation_id(route);
            assert!(ids.insert(id.clone()), "duplicate operationId {id}");
            assert!(
                id.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "operationId must be a safe identifier: {id}"
            );
        }
    }

    /// Every hub operation documents how it is authorized, the role and scope
    /// it needs, and the security schemes it accepts.
    #[test]
    fn hub_routes_document_access_and_required_scope() {
        let doc = document();
        for route in ROUTES.iter().filter(|route| route.surface == Surface::Hub) {
            let operation = operation(&doc, route);
            let extensions = operation
                .extensions
                .as_ref()
                .unwrap_or_else(|| panic!("{} has no extensions", route.path));
            assert_eq!(
                extensions.get("x-surface").and_then(|value| value.as_str()),
                Some("hub"),
                "{} surface",
                route.path
            );
            assert_eq!(
                extensions.get("x-auth").and_then(|value| value.as_str()),
                Some(route.access.label()),
                "{} access",
                route.path
            );
            assert_eq!(
                extensions
                    .get("x-required-scope")
                    .and_then(|value| value.as_str()),
                route.required_scope,
                "{} scope",
                route.path
            );
            assert_eq!(
                extensions
                    .get("x-required-role")
                    .and_then(|value| value.as_str()),
                route.required_role,
                "{} role",
                route.path
            );

            let declared: BTreeSet<String> = operation
                .security
                .as_ref()
                .map(|requirements| {
                    requirements
                        .iter()
                        .filter_map(|requirement| serde_json::to_value(requirement).ok())
                        .filter_map(|value| value.as_object().cloned())
                        .flat_map(|object| object.keys().cloned().collect::<Vec<_>>().into_iter())
                        .collect()
                })
                .unwrap_or_default();
            let expected: BTreeSet<String> = match route.access {
                Access::Public | Access::Signature => BTreeSet::new(),
                Access::Member => BTreeSet::from(["bearerAuth".to_string()]),
                Access::MemberOrApiKey => {
                    BTreeSet::from(["bearerAuth".to_string(), "apiKeyAuth".to_string()])
                }
                Access::ApiKey => BTreeSet::from(["apiKeyAuth".to_string()]),
                Access::Host => BTreeSet::from(["hostCredential".to_string()]),
            };
            assert_eq!(declared, expected, "{} security schemes", route.path);
        }

        let scoped = ROUTES
            .iter()
            .filter(|route| route.required_scope.is_some())
            .count();
        assert_eq!(scoped, 14, "14 hub operations require an API-key scope");
    }

    /// Every hub operation documents its request body and every JSON success
    /// body; 204 and 101 responses have none.
    #[test]
    fn every_hub_operation_documents_its_bodies() {
        let doc = document();
        for route in ROUTES.iter().filter(|route| route.surface == Surface::Hub) {
            let operation = operation(&doc, route);
            match route.request {
                Some(name) => {
                    let body = operation
                        .request_body
                        .as_ref()
                        .unwrap_or_else(|| panic!("{} has no request body", route.path));
                    let content = body
                        .content
                        .get("application/json")
                        .unwrap_or_else(|| panic!("{} body is not JSON", route.path));
                    assert_eq!(
                        content.schema.as_ref().and_then(schema_ref),
                        Some(name),
                        "{} request schema",
                        route.path
                    );
                }
                None => assert!(
                    operation.request_body.is_none(),
                    "{} documents a body it does not accept",
                    route.path
                ),
            }

            if matches!(route.success, "204" | "101") {
                assert!(
                    route.response.is_none(),
                    "{} must not declare a success body",
                    route.path
                );
            } else {
                let name = route
                    .response
                    .unwrap_or_else(|| panic!("{} has no response schema", route.path));
                let response = operation
                    .responses
                    .responses
                    .get(route.success)
                    .unwrap_or_else(|| panic!("{} missing {}", route.path, route.success));
                let response = match response {
                    RefOr::T(response) => response,
                    RefOr::Ref(_) => panic!("{} success is a $ref", route.path),
                };
                let content = response
                    .content
                    .get("application/json")
                    .unwrap_or_else(|| panic!("{} success is not JSON", route.path));
                assert_eq!(
                    content.schema.as_ref().and_then(schema_ref),
                    Some(name),
                    "{} response schema",
                    route.path
                );
            }
        }
    }

    fn schema_ref(schema: &RefOr<Schema>) -> Option<&str> {
        match schema {
            RefOr::Ref(reference) => reference.ref_location.strip_prefix("#/components/schemas/"),
            RefOr::T(_) => None,
        }
    }

    /// Every `$ref` in the document resolves to a published component.
    #[test]
    fn every_schema_reference_resolves() {
        let value = json_doc();
        let mut stack = vec![&value];
        let mut found = 0usize;
        while let Some(node) = stack.pop() {
            match node {
                serde_json::Value::Object(map) => {
                    for (key, child) in map {
                        if key == "$ref" {
                            let reference = child.as_str().expect("$ref string");
                            let name = reference
                                .strip_prefix("#/components/schemas/")
                                .unwrap_or_else(|| panic!("unsupported $ref {reference}"));
                            assert!(
                                value["components"]["schemas"].get(name).is_some(),
                                "dangling $ref {reference}"
                            );
                            found += 1;
                        }
                        stack.push(child);
                    }
                }
                serde_json::Value::Array(items) => stack.extend(items.iter()),
                _ => {}
            }
        }
        assert!(found > 60, "expected many schema references, found {found}");
    }

    #[test]
    fn schema_table_is_unique_and_complete() {
        let mut names = BTreeSet::new();
        for spec in SCHEMAS {
            assert!(names.insert(spec.name), "duplicate schema {}", spec.name);
            if spec.array_of.is_some() {
                assert!(spec.fields.is_empty(), "{} has both forms", spec.name);
            }
            if spec.any_value {
                assert!(spec.fields.is_empty(), "{} has both forms", spec.name);
            }
        }
        for name in [
            "Problem",
            "TriggerDocument",
            "RunView",
            "ActivityPage",
            "RelayPage",
        ] {
            assert!(names.contains(name), "missing schema {name}");
        }
        // Every schema the route table names exists.
        for route in ROUTES {
            for name in [route.request, route.response].into_iter().flatten() {
                assert!(
                    names.contains(name),
                    "{} refers to unknown {name}",
                    route.path
                );
            }
            for (_, name) in route.alternates {
                assert!(
                    names.contains(name),
                    "{} refers to unknown {name}",
                    route.path
                );
            }
        }
    }

    /// Parse the hub modules' `web::scope(...).route(...)` registrations and
    /// require the OpenAPI table to describe exactly the same inventory. This
    /// fails when a hub route is added to code without a table entry, and when
    /// a table entry is removed while the route remains registered.
    #[test]
    fn every_registered_hub_route_is_in_the_table() {
        fn parse_scope(source: &str) -> BTreeSet<(String, String)> {
            let mut routes = BTreeSet::new();
            let mut scope: Option<String> = None;
            let mut pending_route: Option<String> = None;
            let mut pending_path_line = false;
            for line in source.lines() {
                let line = line.trim();
                if let Some(rest) = line.split_once("web::scope(\"")
                    && let Some((path, _)) = rest.1.split_once('"')
                {
                    scope = Some(path.to_string());
                }
                let Some(scope_path) = scope.as_ref() else {
                    continue;
                };
                if pending_route.is_none() {
                    if let Some(rest) = line.split_once(".route(\"")
                        && let Some((route_path, _)) = rest.1.split_once('"')
                    {
                        pending_route = Some(route_path.to_string());
                        pending_path_line = false;
                    } else if line.contains(".route(") {
                        pending_path_line = true;
                    } else if pending_path_line
                        && let Some((route_path, _)) = line.trim_start_matches('"').split_once('"')
                    {
                        pending_route = Some(route_path.to_string());
                        pending_path_line = false;
                    }
                }
                let Some(route_path) = pending_route.clone() else {
                    continue;
                };
                // The route and its method may be on the same line or split
                // across lines by rustfmt.
                let Some(method) = ["get", "post", "put", "patch", "delete"]
                    .into_iter()
                    .find(|method| line.contains(&format!("web::{method}()")))
                else {
                    continue;
                };
                let full = if route_path.is_empty() {
                    scope_path.clone()
                } else {
                    format!("{scope_path}{route_path}")
                };
                routes.insert((method.to_string(), full));
                pending_route = None;
            }
            routes
        }

        let mut registered = parse_scope(include_str!("pool_identity.rs"));
        registered.extend(parse_scope(include_str!("api_keys.rs")));
        registered.extend(parse_scope(include_str!("hosts.rs")));
        registered.extend(parse_scope(include_str!("connections.rs")));
        registered.extend(parse_scope(include_str!("projects.rs")));
        registered.extend(parse_scope(include_str!("triggers.rs")));
        registered.extend(parse_scope(include_str!("manual_runs.rs")));
        registered.extend(parse_scope(include_str!("activity.rs")));
        registered.extend(parse_scope(include_str!("chatrooms.rs")));
        registered.extend(parse_scope(include_str!("relay.rs")));
        registered.extend(parse_scope(include_str!("webhooks/github.rs")));
        registered.extend(parse_scope(include_str!("webhooks/slack.rs")));

        let documented: BTreeSet<(String, String)> = ROUTES
            .iter()
            .filter(|route| route.surface == Surface::Hub)
            .map(|route| {
                (
                    method_str(route.method.clone()).to_string(),
                    route.path.to_string(),
                )
            })
            .collect();

        assert_eq!(
            registered, documented,
            "hub routes registered in code and documented in ROUTES must match"
        );
        assert_eq!(registered.len(), 60, "expected 60 hub operations");
    }

    /// The published, commit-tracked artifact must be exactly what the tables
    /// generate. Adding a route to `ROUTES` without regenerating this file
    /// fails here — that is the drift gate.
    #[test]
    fn published_contract_matches_generated_document() {
        let committed = include_str!("../../docs/api/openapi-v1.json");
        let generated = document_json().expect("serializes");
        assert_eq!(
            committed, generated,
            "docs/api/openapi-v1.json is stale; run `cargo test regenerate_published_contract -- --ignored`"
        );
    }

    /// The human-readable reference is generated from the same tables and is
    /// checked the same way.
    #[test]
    fn api_reference_matches_generated_document() {
        let committed = include_str!("../../docs/API_REFERENCE.md");
        assert_eq!(
            committed,
            render_reference(),
            "{REFERENCE_PATH} is stale; run `cargo test regenerate_api_reference -- --ignored`"
        );
    }

    /// The pinned path is major-versioned and stable.
    #[test]
    fn versioned_paths_are_stable() {
        assert_eq!(CONTRACT_MAJOR, "1");
        assert_eq!(artifact_path(), "docs/api/openapi-v1.json");
        assert!(ROUTES.iter().any(|route| route.path == "/openapi/v1.json"));
        assert!(ROUTES.iter().any(|route| route.path == "/openapi.json"));
        let value = json_doc();
        assert_eq!(value["info"]["version"], CONTRACT_VERSION);
        assert_eq!(
            value["paths"]["/openapi/v1.json"]["get"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/OpenApiDocument"
        );
    }

    /// The policy document is part of the deliverable: it must record the
    /// resource-versus-command decision and the stability/announcement rules
    /// that the generated schemas rely on.
    #[test]
    fn contract_document_records_decisions_and_policy() {
        let contract = include_str!("../../docs/API_CONTRACT.md");
        for needed in [
            "## 2. Resource versus command",
            "**Decision: the hub is resource-oriented",
            "## 3. Stability policy for the `v1` major",
            "### 3.1 Stable within `v1`",
            "### 3.2 May change within `v1`",
            "### 3.3 Breaking within `v1`",
            "### 3.5 How a breaking change is announced",
            "/openapi/v1.json",
            "docs/api/openapi-v1.json",
            "deliveryKey",
            "execution_id",
            "x-required-scope",
        ] {
            assert!(
                contract.contains(needed),
                "docs/API_CONTRACT.md must contain {needed:?}"
            );
        }
    }

    /// Regenerate the committed machine-readable artifact.
    #[test]
    #[ignore = "writes docs/api/openapi-v1.json; run explicitly to regenerate"]
    fn regenerate_published_contract() {
        let path = artifact_path();
        if let Some(parent) = std::path::Path::new(&path).parent() {
            std::fs::create_dir_all(parent).expect("create docs/api");
        }
        std::fs::write(&path, document_json().expect("serializes")).expect("write artifact");
    }

    /// Regenerate the committed human-readable reference.
    #[test]
    #[ignore = "writes docs/API_REFERENCE.md; run explicitly to regenerate"]
    fn regenerate_api_reference() {
        std::fs::write(REFERENCE_PATH, render_reference()).expect("write reference");
    }
}
