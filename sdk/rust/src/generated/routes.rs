// Generated from docs/api/openapi-v1.json. Do not edit by hand.
// Regenerate: node scripts/generate.mjs (from sdk/rust).

//! Generated route table: the contract's access model and wire shape.

pub use crate::route::{Auth, Method, RouteInfo, Surface};

/// SHA-256 of the contract this module was generated from.
pub const CONTRACT_SHA256: &str =
    "7d1637dad19eb999ffee34094a5fac2d50c58734bccc61c876883c0781c6e9f4";

/// Contract version reported by the committed document.
pub const CONTRACT_VERSION: &str = "1.0.0";

/// API-key scopes a key may hold (`docs/API_CONTRACT.md` §4).
pub const SCOPES: &[&str] = &[
    "projects:read",
    "configuration:validate",
    "configuration:install",
    "runs:dispatch",
    "daemons:enroll",
];

/// Member roles (`docs/API_CONTRACT.md` §4).
pub const ROLES: &[&str] = &["owner", "admin", "member", "viewer"];

/// `DELETE /v1/api-keys/{id}` — Revoke an API key (owner only; effective immediately)
pub static ROUTE_DELETE_V1_API_KEYS_ID: RouteInfo = RouteInfo {
    operation_id: "delete_v1_api_keys_id",
    method: Method::Delete,
    path: "/v1/api-keys/{id}",
    path_params: &["id"],
    query_params: &[],
    has_body: false,
    response: "()",
    success: &[204],
    errors: &[401, 403, 404],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: Some("owner"),
    surface: Surface::Hub,
    deprecated: false,
    summary: "Revoke an API key (owner only; effective immediately)",
};

/// `DELETE /v1/connections/{id}` — Delete a database connection
pub static ROUTE_DELETE_V1_CONNECTIONS_ID: RouteInfo = RouteInfo {
    operation_id: "delete_v1_connections_id",
    method: Method::Delete,
    path: "/v1/connections/{id}",
    path_params: &["id"],
    query_params: &[],
    has_body: false,
    response: "()",
    success: &[204],
    errors: &[401, 403, 404, 409],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Delete a database connection",
};

/// `DELETE /v1/hosts/{id}` — Revoke the pool relationship for a host
pub static ROUTE_DELETE_V1_HOSTS_ID: RouteInfo = RouteInfo {
    operation_id: "delete_v1_hosts_id",
    method: Method::Delete,
    path: "/v1/hosts/{id}",
    path_params: &["id"],
    query_params: &[],
    has_body: false,
    response: "()",
    success: &[204],
    errors: &[401, 403, 404],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Revoke the pool relationship for a host",
};

/// `DELETE /v1/hosts/{id}/execution` — Revoke host.execute; the host becomes connected-only
pub static ROUTE_DELETE_V1_HOSTS_ID_EXECUTION: RouteInfo = RouteInfo {
    operation_id: "delete_v1_hosts_id_execution",
    method: Method::Delete,
    path: "/v1/hosts/{id}/execution",
    path_params: &["id"],
    query_params: &[],
    has_body: false,
    response: "HostView",
    success: &[200],
    errors: &[401, 403, 404],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Revoke host.execute; the host becomes connected-only",
};

/// `DELETE /v1/players/{id}` — Soft-delete the authenticated player's own account
pub static ROUTE_DELETE_V1_PLAYERS_ID: RouteInfo = RouteInfo {
    operation_id: "delete_v1_players_id",
    method: Method::Delete,
    path: "/v1/players/{id}",
    path_params: &["id"],
    query_params: &[],
    has_body: false,
    response: "()",
    success: &[204],
    errors: &[400, 401],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "Soft-delete the authenticated player's own account",
};

/// `DELETE /v1/pool/invitations/{id}` — Revoke a pending invitation (owner only)
pub static ROUTE_DELETE_V1_POOL_INVITATIONS_ID: RouteInfo = RouteInfo {
    operation_id: "delete_v1_pool_invitations_id",
    method: Method::Delete,
    path: "/v1/pool/invitations/{id}",
    path_params: &["id"],
    query_params: &[],
    has_body: false,
    response: "()",
    success: &[204],
    errors: &[401, 403, 404, 410],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: Some("owner"),
    surface: Surface::Hub,
    deprecated: false,
    summary: "Revoke a pending invitation (owner only)",
};

/// `DELETE /v1/pool/members/{id}` — Remove a member (owner only; last owner protected)
pub static ROUTE_DELETE_V1_POOL_MEMBERS_ID: RouteInfo = RouteInfo {
    operation_id: "delete_v1_pool_members_id",
    method: Method::Delete,
    path: "/v1/pool/members/{id}",
    path_params: &["id"],
    query_params: &[],
    has_body: false,
    response: "()",
    success: &[204],
    errors: &[401, 403, 404, 409],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: Some("owner"),
    surface: Surface::Hub,
    deprecated: false,
    summary: "Remove a member (owner only; last owner protected)",
};

/// `DELETE /v1/projects/{id}` — Delete a project and its revisions
pub static ROUTE_DELETE_V1_PROJECTS_ID: RouteInfo = RouteInfo {
    operation_id: "delete_v1_projects_id",
    method: Method::Delete,
    path: "/v1/projects/{id}",
    path_params: &["id"],
    query_params: &[],
    has_body: false,
    response: "()",
    success: &[204],
    errors: &[401, 403, 404],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Delete a project and its revisions",
};

/// `DELETE /v1/saves/{player_id}/{save_id}` — Delete a single game save
pub static ROUTE_DELETE_V1_SAVES_PLAYER_ID_SAVE_ID: RouteInfo = RouteInfo {
    operation_id: "delete_v1_saves_player_id_save_id",
    method: Method::Delete,
    path: "/v1/saves/{player_id}/{save_id}",
    path_params: &["player_id", "save_id"],
    query_params: &[],
    has_body: false,
    response: "()",
    success: &[204],
    errors: &[400, 401],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "Delete a single game save",
};

/// `DELETE /v1/sessions/{session}/relay` — Stop publication immediately and permanently
pub static ROUTE_DELETE_V1_SESSIONS_SESSION_RELAY: RouteInfo = RouteInfo {
    operation_id: "delete_v1_sessions_session_relay",
    method: Method::Delete,
    path: "/v1/sessions/{session}/relay",
    path_params: &["session"],
    query_params: &[],
    has_body: false,
    response: "RelayState",
    success: &[200],
    errors: &[401, 403, 404],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Stop publication immediately and permanently",
};

/// `GET /healthz` — Liveness probe with DB status
pub static ROUTE_GET_HEALTHZ: RouteInfo = RouteInfo {
    operation_id: "get_healthz",
    method: Method::Get,
    path: "/healthz",
    path_params: &[],
    query_params: &[],
    has_body: false,
    response: "JsonObject",
    success: &[200],
    errors: &[],
    rate_limited: false,
    auth: Auth::Public,
    scope: None,
    role: None,
    surface: Surface::System,
    deprecated: false,
    summary: "Liveness probe with DB status",
};

/// `GET /metrics` — Prometheus text-format metrics
pub static ROUTE_GET_METRICS: RouteInfo = RouteInfo {
    operation_id: "get_metrics",
    method: Method::Get,
    path: "/metrics",
    path_params: &[],
    query_params: &[],
    has_body: false,
    response: "String",
    success: &[200],
    errors: &[],
    rate_limited: false,
    auth: Auth::Public,
    scope: None,
    role: None,
    surface: Surface::System,
    deprecated: false,
    summary: "Prometheus text-format metrics",
};

/// `GET /openapi.json` — This OpenAPI document
pub static ROUTE_GET_OPENAPI_JSON: RouteInfo = RouteInfo {
    operation_id: "get_openapi_json",
    method: Method::Get,
    path: "/openapi.json",
    path_params: &[],
    query_params: &[],
    has_body: false,
    response: "OpenApiDocument",
    success: &[200],
    errors: &[],
    rate_limited: false,
    auth: Auth::Public,
    scope: None,
    role: None,
    surface: Surface::System,
    deprecated: false,
    summary: "This OpenAPI document",
};

/// `GET /openapi/v1.json` — This OpenAPI document at its pinned major-version path
pub static ROUTE_GET_OPENAPI_V1_JSON: RouteInfo = RouteInfo {
    operation_id: "get_openapi_v1_json",
    method: Method::Get,
    path: "/openapi/v1.json",
    path_params: &[],
    query_params: &[],
    has_body: false,
    response: "OpenApiDocument",
    success: &[200],
    errors: &[],
    rate_limited: false,
    auth: Auth::Public,
    scope: None,
    role: None,
    surface: Surface::System,
    deprecated: false,
    summary: "This OpenAPI document at its pinned major-version path",
};

/// `GET /v1/activity` — List one page of receipts, runs, steps and executions
pub static ROUTE_GET_V1_ACTIVITY: RouteInfo = RouteInfo {
    operation_id: "get_v1_activity",
    method: Method::Get,
    path: "/v1/activity",
    path_params: &[],
    query_params: &[
        "project", "provider", "trigger", "status", "kind", "cursor", "limit",
    ],
    has_body: false,
    response: "ActivityPage",
    success: &[200],
    errors: &[401, 403, 422],
    rate_limited: true,
    auth: Auth::MemberOrApiKey,
    scope: Some("projects:read"),
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "List one page of receipts, runs, steps and executions",
};

/// `GET /v1/activity/runs/{run}` — Fetch one run with the arrival record that started it
pub static ROUTE_GET_V1_ACTIVITY_RUNS_RUN: RouteInfo = RouteInfo {
    operation_id: "get_v1_activity_runs_run",
    method: Method::Get,
    path: "/v1/activity/runs/{run}",
    path_params: &["run"],
    query_params: &[],
    has_body: false,
    response: "ActivityRunDetail",
    success: &[200],
    errors: &[401, 403, 404],
    rate_limited: true,
    auth: Auth::MemberOrApiKey,
    scope: Some("projects:read"),
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Fetch one run with the arrival record that started it",
};

/// `GET /v1/api-keys` — List organization API keys (owner only; prefix only)
pub static ROUTE_GET_V1_API_KEYS: RouteInfo = RouteInfo {
    operation_id: "get_v1_api_keys",
    method: Method::Get,
    path: "/v1/api-keys",
    path_params: &[],
    query_params: &[],
    has_body: false,
    response: "ApiKeysResponse",
    success: &[200],
    errors: &[401, 403],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: Some("owner"),
    surface: Surface::Hub,
    deprecated: false,
    summary: "List organization API keys (owner only; prefix only)",
};

/// `GET /v1/api-keys/whoami` — Introspect an API key (requires projects:read)
pub static ROUTE_GET_V1_API_KEYS_WHOAMI: RouteInfo = RouteInfo {
    operation_id: "get_v1_api_keys_whoami",
    method: Method::Get,
    path: "/v1/api-keys/whoami",
    path_params: &[],
    query_params: &[],
    has_body: false,
    response: "ApiKeyView",
    success: &[200],
    errors: &[401, 403],
    rate_limited: true,
    auth: Auth::ApiKey,
    scope: Some("projects:read"),
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Introspect an API key (requires projects:read)",
};

/// `GET /v1/chatrooms` — List rooms visible to the operator
pub static ROUTE_GET_V1_CHATROOMS: RouteInfo = RouteInfo {
    operation_id: "get_v1_chatrooms",
    method: Method::Get,
    path: "/v1/chatrooms",
    path_params: &[],
    query_params: &[],
    has_body: false,
    response: "RoomViewList",
    success: &[200],
    errors: &[401, 403],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "List rooms visible to the operator",
};

/// `GET /v1/chatrooms/{room}/members` — List a room's members
pub static ROUTE_GET_V1_CHATROOMS_ROOM_MEMBERS: RouteInfo = RouteInfo {
    operation_id: "get_v1_chatrooms_room_members",
    method: Method::Get,
    path: "/v1/chatrooms/{room}/members",
    path_params: &["room"],
    query_params: &[],
    has_body: false,
    response: "RoomMemberViewList",
    success: &[200],
    errors: &[401, 403, 404],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "List a room's members",
};

/// `GET /v1/chatrooms/{room}/messages` — Read room history after a sequence cursor
pub static ROUTE_GET_V1_CHATROOMS_ROOM_MESSAGES: RouteInfo = RouteInfo {
    operation_id: "get_v1_chatrooms_room_messages",
    method: Method::Get,
    path: "/v1/chatrooms/{room}/messages",
    path_params: &["room"],
    query_params: &["after", "limit"],
    has_body: false,
    response: "HistoryResponse",
    success: &[200],
    errors: &[401, 403, 404, 422],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Read room history after a sequence cursor",
};

/// `GET /v1/chatrooms/{room}/stream` — Upgrade to the room's live message stream
pub static ROUTE_GET_V1_CHATROOMS_ROOM_STREAM: RouteInfo = RouteInfo {
    operation_id: "get_v1_chatrooms_room_stream",
    method: Method::Get,
    path: "/v1/chatrooms/{room}/stream",
    path_params: &["room"],
    query_params: &["token", "after"],
    has_body: false,
    response: "ChatStream",
    success: &[],
    errors: &[401, 403, 404],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Upgrade to the room's live message stream",
};

/// `GET /v1/connections` — List effective provider connections (environment overlay applied)
pub static ROUTE_GET_V1_CONNECTIONS: RouteInfo = RouteInfo {
    operation_id: "get_v1_connections",
    method: Method::Get,
    path: "/v1/connections",
    path_params: &[],
    query_params: &[],
    has_body: false,
    response: "ConnectionsResponse",
    success: &[200],
    errors: &[401, 403],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "List effective provider connections (environment overlay applied)",
};

/// `GET /v1/connections/{id}` — Fetch a connection by id or env:<slug>
pub static ROUTE_GET_V1_CONNECTIONS_ID: RouteInfo = RouteInfo {
    operation_id: "get_v1_connections_id",
    method: Method::Get,
    path: "/v1/connections/{id}",
    path_params: &["id"],
    query_params: &[],
    has_body: false,
    response: "ConnectionView",
    success: &[200],
    errors: &[401, 403, 404],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Fetch a connection by id or env:<slug>",
};

/// `GET /v1/games/{game_id}/settings` — Fetch a game's settings
pub static ROUTE_GET_V1_GAMES_GAME_ID_SETTINGS: RouteInfo = RouteInfo {
    operation_id: "get_v1_games_game_id_settings",
    method: Method::Get,
    path: "/v1/games/{game_id}/settings",
    path_params: &["game_id"],
    query_params: &[],
    has_body: false,
    response: "JsonObject",
    success: &[200],
    errors: &[400, 401],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "Fetch a game's settings",
};

/// `GET /v1/hosts` — List live hosts in the pool
pub static ROUTE_GET_V1_HOSTS: RouteInfo = RouteInfo {
    operation_id: "get_v1_hosts",
    method: Method::Get,
    path: "/v1/hosts",
    path_params: &[],
    query_params: &[],
    has_body: false,
    response: "HostsResponse",
    success: &[200],
    errors: &[401, 403],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "List live hosts in the pool",
};

/// `GET /v1/hosts/{id}` — Fetch a live host by immutable id
pub static ROUTE_GET_V1_HOSTS_ID: RouteInfo = RouteInfo {
    operation_id: "get_v1_hosts_id",
    method: Method::Get,
    path: "/v1/hosts/{id}",
    path_params: &["id"],
    query_params: &[],
    has_body: false,
    response: "HostView",
    success: &[200],
    errors: &[401, 403, 404],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Fetch a live host by immutable id",
};

/// `GET /v1/hosts/{id}/capabilities` — Read the host's last capability report and offered providers
pub static ROUTE_GET_V1_HOSTS_ID_CAPABILITIES: RouteInfo = RouteInfo {
    operation_id: "get_v1_hosts_id_capabilities",
    method: Method::Get,
    path: "/v1/hosts/{id}/capabilities",
    path_params: &["id"],
    query_params: &[],
    has_body: false,
    response: "HostCapabilityReport",
    success: &[200],
    errors: &[401, 403, 404],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Read the host's last capability report and offered providers",
};

/// `GET /v1/leaderboards/{game_id}` — Fetch one ranked page of a leaderboard
pub static ROUTE_GET_V1_LEADERBOARDS_GAME_ID: RouteInfo = RouteInfo {
    operation_id: "get_v1_leaderboards_game_id",
    method: Method::Get,
    path: "/v1/leaderboards/{game_id}",
    path_params: &["game_id"],
    query_params: &["page", "per_page"],
    has_body: false,
    response: "JsonObject",
    success: &[200],
    errors: &[400, 401],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "Fetch one ranked page of a leaderboard",
};

/// `GET /v1/players/{id}` — Fetch a player's public profile
pub static ROUTE_GET_V1_PLAYERS_ID: RouteInfo = RouteInfo {
    operation_id: "get_v1_players_id",
    method: Method::Get,
    path: "/v1/players/{id}",
    path_params: &["id"],
    query_params: &[],
    has_body: false,
    response: "JsonObject",
    success: &[200],
    errors: &[400, 401],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "Fetch a player's public profile",
};

/// `GET /v1/players/{id}/subaccounts` — List the authenticated player's subaccounts
pub static ROUTE_GET_V1_PLAYERS_ID_SUBACCOUNTS: RouteInfo = RouteInfo {
    operation_id: "get_v1_players_id_subaccounts",
    method: Method::Get,
    path: "/v1/players/{id}/subaccounts",
    path_params: &["id"],
    query_params: &[],
    has_body: false,
    response: "JsonObject",
    success: &[200],
    errors: &[400, 401],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "List the authenticated player's subaccounts",
};

/// `GET /v1/pool/invitations` — List pending team invitations (owner only)
pub static ROUTE_GET_V1_POOL_INVITATIONS: RouteInfo = RouteInfo {
    operation_id: "get_v1_pool_invitations",
    method: Method::Get,
    path: "/v1/pool/invitations",
    path_params: &[],
    query_params: &[],
    has_body: false,
    response: "InvitationsResponse",
    success: &[200],
    errors: &[401, 403],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: Some("owner"),
    surface: Surface::Hub,
    deprecated: false,
    summary: "List pending team invitations (owner only)",
};

/// `GET /v1/pool/me` — Fetch the signed-in operator and organization
pub static ROUTE_GET_V1_POOL_ME: RouteInfo = RouteInfo {
    operation_id: "get_v1_pool_me",
    method: Method::Get,
    path: "/v1/pool/me",
    path_params: &[],
    query_params: &[],
    has_body: false,
    response: "PoolIdentityView",
    success: &[200],
    errors: &[401, 403],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Fetch the signed-in operator and organization",
};

/// `GET /v1/pool/members` — List the organization's operators
pub static ROUTE_GET_V1_POOL_MEMBERS: RouteInfo = RouteInfo {
    operation_id: "get_v1_pool_members",
    method: Method::Get,
    path: "/v1/pool/members",
    path_params: &[],
    query_params: &[],
    has_body: false,
    response: "MembersResponse",
    success: &[200],
    errors: &[401, 403],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "List the organization's operators",
};

/// `GET /v1/projects` — List projects in the organization
pub static ROUTE_GET_V1_PROJECTS: RouteInfo = RouteInfo {
    operation_id: "get_v1_projects",
    method: Method::Get,
    path: "/v1/projects",
    path_params: &[],
    query_params: &[],
    has_body: false,
    response: "ProjectsResponse",
    success: &[200],
    errors: &[401, 403],
    rate_limited: true,
    auth: Auth::MemberOrApiKey,
    scope: Some("projects:read"),
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "List projects in the organization",
};

/// `GET /v1/projects/{id}` — Fetch a project by id
pub static ROUTE_GET_V1_PROJECTS_ID: RouteInfo = RouteInfo {
    operation_id: "get_v1_projects_id",
    method: Method::Get,
    path: "/v1/projects/{id}",
    path_params: &["id"],
    query_params: &[],
    has_body: false,
    response: "ProjectView",
    success: &[200],
    errors: &[401, 403, 404],
    rate_limited: true,
    auth: Auth::MemberOrApiKey,
    scope: Some("projects:read"),
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Fetch a project by id",
};

/// `GET /v1/projects/{id}/configuration` — Fetch a project's active configuration revision
pub static ROUTE_GET_V1_PROJECTS_ID_CONFIGURATION: RouteInfo = RouteInfo {
    operation_id: "get_v1_projects_id_configuration",
    method: Method::Get,
    path: "/v1/projects/{id}/configuration",
    path_params: &["id"],
    query_params: &[],
    has_body: false,
    response: "RevisionView",
    success: &[200],
    errors: &[401, 403, 404],
    rate_limited: true,
    auth: Auth::MemberOrApiKey,
    scope: Some("projects:read"),
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Fetch a project's active configuration revision",
};

/// `GET /v1/projects/{id}/revisions` — List a project's configuration revisions
pub static ROUTE_GET_V1_PROJECTS_ID_REVISIONS: RouteInfo = RouteInfo {
    operation_id: "get_v1_projects_id_revisions",
    method: Method::Get,
    path: "/v1/projects/{id}/revisions",
    path_params: &["id"],
    query_params: &[],
    has_body: false,
    response: "RevisionsResponse",
    success: &[200],
    errors: &[401, 403, 404],
    rate_limited: true,
    auth: Auth::MemberOrApiKey,
    scope: Some("projects:read"),
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "List a project's configuration revisions",
};

/// `GET /v1/projects/{id}/revisions/{revision_id}` — Fetch one configuration revision with its files
pub static ROUTE_GET_V1_PROJECTS_ID_REVISIONS_REVISION_ID: RouteInfo = RouteInfo {
    operation_id: "get_v1_projects_id_revisions_revision_id",
    method: Method::Get,
    path: "/v1/projects/{id}/revisions/{revision_id}",
    path_params: &["id", "revision_id"],
    query_params: &[],
    has_body: false,
    response: "RevisionView",
    success: &[200],
    errors: &[401, 403, 404],
    rate_limited: true,
    auth: Auth::MemberOrApiKey,
    scope: Some("projects:read"),
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Fetch one configuration revision with its files",
};

/// `GET /v1/saves/{player_id}` — List the authenticated player's saves, newest first
pub static ROUTE_GET_V1_SAVES_PLAYER_ID: RouteInfo = RouteInfo {
    operation_id: "get_v1_saves_player_id",
    method: Method::Get,
    path: "/v1/saves/{player_id}",
    path_params: &["player_id"],
    query_params: &[],
    has_body: false,
    response: "JsonObject",
    success: &[200],
    errors: &[400, 401],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "List the authenticated player's saves, newest first",
};

/// `GET /v1/saves/{player_id}/{save_id}` — Fetch a single game save
pub static ROUTE_GET_V1_SAVES_PLAYER_ID_SAVE_ID: RouteInfo = RouteInfo {
    operation_id: "get_v1_saves_player_id_save_id",
    method: Method::Get,
    path: "/v1/saves/{player_id}/{save_id}",
    path_params: &["player_id", "save_id"],
    query_params: &[],
    has_body: false,
    response: "JsonObject",
    success: &[200],
    errors: &[400, 401],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "Fetch a single game save",
};

/// `GET /v1/sessions/{session}/relay` — Read a session's relay publication state
pub static ROUTE_GET_V1_SESSIONS_SESSION_RELAY: RouteInfo = RouteInfo {
    operation_id: "get_v1_sessions_session_relay",
    method: Method::Get,
    path: "/v1/sessions/{session}/relay",
    path_params: &["session"],
    query_params: &[],
    has_body: false,
    response: "RelayState",
    success: &[200],
    errors: &[401, 403, 404],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Read a session's relay publication state",
};

/// `GET /v1/sessions/{session}/relay/events` — Backfill relayed events after a sequence cursor
pub static ROUTE_GET_V1_SESSIONS_SESSION_RELAY_EVENTS: RouteInfo = RouteInfo {
    operation_id: "get_v1_sessions_session_relay_events",
    method: Method::Get,
    path: "/v1/sessions/{session}/relay/events",
    path_params: &["session"],
    query_params: &["after", "limit"],
    has_body: false,
    response: "RelayPage",
    success: &[200],
    errors: &[401, 403, 404, 422],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Backfill relayed events after a sequence cursor",
};

/// `GET /v1/setup/status` — Report whether the pool still needs first-run setup
pub static ROUTE_GET_V1_SETUP_STATUS: RouteInfo = RouteInfo {
    operation_id: "get_v1_setup_status",
    method: Method::Get,
    path: "/v1/setup/status",
    path_params: &[],
    query_params: &[],
    has_body: false,
    response: "SetupStatusResponse",
    success: &[200],
    errors: &[],
    rate_limited: true,
    auth: Auth::Public,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Report whether the pool still needs first-run setup",
};

/// `PATCH /v1/connections/{id}` — Rename, enable or disable a database connection
pub static ROUTE_PATCH_V1_CONNECTIONS_ID: RouteInfo = RouteInfo {
    operation_id: "patch_v1_connections_id",
    method: Method::Patch,
    path: "/v1/connections/{id}",
    path_params: &["id"],
    query_params: &[],
    has_body: true,
    response: "ConnectionView",
    success: &[200],
    errors: &[400, 401, 403, 404, 409, 422],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Rename, enable or disable a database connection",
};

/// `PATCH /v1/hosts/{id}` — Rename a host slug or display name; the id is unchanged
pub static ROUTE_PATCH_V1_HOSTS_ID: RouteInfo = RouteInfo {
    operation_id: "patch_v1_hosts_id",
    method: Method::Patch,
    path: "/v1/hosts/{id}",
    path_params: &["id"],
    query_params: &[],
    has_body: true,
    response: "HostView",
    success: &[200],
    errors: &[400, 401, 403, 404, 409, 422],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Rename a host slug or display name; the id is unchanged",
};

/// `PATCH /v1/pool/members/{id}` — Change a member's role (owner only; last owner protected)
pub static ROUTE_PATCH_V1_POOL_MEMBERS_ID: RouteInfo = RouteInfo {
    operation_id: "patch_v1_pool_members_id",
    method: Method::Patch,
    path: "/v1/pool/members/{id}",
    path_params: &["id"],
    query_params: &[],
    has_body: true,
    response: "PoolUserView",
    success: &[200],
    errors: &[400, 401, 403, 404, 409, 422],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: Some("owner"),
    surface: Surface::Hub,
    deprecated: false,
    summary: "Change a member's role (owner only; last owner protected)",
};

/// `PATCH /v1/projects/{id}` — Rename a project slug or name; the id is unchanged
pub static ROUTE_PATCH_V1_PROJECTS_ID: RouteInfo = RouteInfo {
    operation_id: "patch_v1_projects_id",
    method: Method::Patch,
    path: "/v1/projects/{id}",
    path_params: &["id"],
    query_params: &[],
    has_body: true,
    response: "ProjectView",
    success: &[200],
    errors: &[400, 401, 403, 404, 409, 422],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Rename a project slug or name; the id is unchanged",
};

/// `POST /v1/api-keys` — Create an API key; the full secret is shown once
pub static ROUTE_POST_V1_API_KEYS: RouteInfo = RouteInfo {
    operation_id: "post_v1_api_keys",
    method: Method::Post,
    path: "/v1/api-keys",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "CreateApiKeyResponse",
    success: &[201],
    errors: &[400, 401, 403, 422],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: Some("owner"),
    surface: Surface::Hub,
    deprecated: false,
    summary: "Create an API key; the full secret is shown once",
};

/// `POST /v1/auth/login` — Verify credentials and issue a signed JWT
pub static ROUTE_POST_V1_AUTH_LOGIN: RouteInfo = RouteInfo {
    operation_id: "post_v1_auth_login",
    method: Method::Post,
    path: "/v1/auth/login",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "JsonObject",
    success: &[200],
    errors: &[],
    rate_limited: false,
    auth: Auth::Public,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "Verify credentials and issue a signed JWT",
};

/// `POST /v1/auth/register` — Register a new player and return the public profile
pub static ROUTE_POST_V1_AUTH_REGISTER: RouteInfo = RouteInfo {
    operation_id: "post_v1_auth_register",
    method: Method::Post,
    path: "/v1/auth/register",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "JsonObject",
    success: &[201],
    errors: &[],
    rate_limited: false,
    auth: Auth::Public,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "Register a new player and return the public profile",
};

/// `POST /v1/chatrooms/{room}/messages` — Post a message to a room
pub static ROUTE_POST_V1_CHATROOMS_ROOM_MESSAGES: RouteInfo = RouteInfo {
    operation_id: "post_v1_chatrooms_room_messages",
    method: Method::Post,
    path: "/v1/chatrooms/{room}/messages",
    path_params: &["room"],
    query_params: &[],
    has_body: true,
    response: "MessageView",
    success: &[201],
    errors: &[400, 401, 403, 404, 422],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Post a message to a room",
};

/// `POST /v1/connections` — Create a provider connection from a credential reference
pub static ROUTE_POST_V1_CONNECTIONS: RouteInfo = RouteInfo {
    operation_id: "post_v1_connections",
    method: Method::Post,
    path: "/v1/connections",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "ConnectionView",
    success: &[201],
    errors: &[400, 401, 403, 409, 422],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Create a provider connection from a credential reference",
};

/// `POST /v1/events` — Ingest a batch of analytics events (fire-and-forget)
pub static ROUTE_POST_V1_EVENTS: RouteInfo = RouteInfo {
    operation_id: "post_v1_events",
    method: Method::Post,
    path: "/v1/events",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "JsonObject",
    success: &[202],
    errors: &[400, 401],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "Ingest a batch of analytics events (fire-and-forget)",
};

/// `POST /v1/feedback` — Submit player feedback
pub static ROUTE_POST_V1_FEEDBACK: RouteInfo = RouteInfo {
    operation_id: "post_v1_feedback",
    method: Method::Post,
    path: "/v1/feedback",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "JsonObject",
    success: &[201],
    errors: &[400, 401],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "Submit player feedback",
};

/// `POST /v1/hosts` — Register a host; the slug is derived from the hostname
pub static ROUTE_POST_V1_HOSTS: RouteInfo = RouteInfo {
    operation_id: "post_v1_hosts",
    method: Method::Post,
    path: "/v1/hosts",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "HostView",
    success: &[201],
    errors: &[400, 401, 403, 409, 422],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Register a host; the slug is derived from the hostname",
};

/// `POST /v1/hosts/enroll` — Exchange an enrollment token for a host relationship credential
pub static ROUTE_POST_V1_HOSTS_ENROLL: RouteInfo = RouteInfo {
    operation_id: "post_v1_hosts_enroll",
    method: Method::Post,
    path: "/v1/hosts/enroll",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "HostView",
    success: &[201],
    errors: &[400, 401, 404, 409, 422],
    rate_limited: true,
    auth: Auth::Public,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Exchange an enrollment token for a host relationship credential",
};

/// `POST /v1/hosts/enrollment-tokens` — Mint a ten-minute single-use host enrollment token
pub static ROUTE_POST_V1_HOSTS_ENROLLMENT_TOKENS: RouteInfo = RouteInfo {
    operation_id: "post_v1_hosts_enrollment_tokens",
    method: Method::Post,
    path: "/v1/hosts/enrollment-tokens",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "MintEnrollmentTokenResponse",
    success: &[201],
    errors: &[400, 401, 403, 404, 422],
    rate_limited: true,
    auth: Auth::MemberOrApiKey,
    scope: Some("daemons:enroll"),
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Mint a ten-minute single-use host enrollment token",
};

/// `POST /v1/hosts/{id}/capabilities` — Host reports providers, models, modes and thinking options
pub static ROUTE_POST_V1_HOSTS_ID_CAPABILITIES: RouteInfo = RouteInfo {
    operation_id: "post_v1_hosts_id_capabilities",
    method: Method::Post,
    path: "/v1/hosts/{id}/capabilities",
    path_params: &["id"],
    query_params: &[],
    has_body: true,
    response: "HostCapabilityReport",
    success: &[200],
    errors: &[400, 401, 403, 404, 422],
    rate_limited: true,
    auth: Auth::Host,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Host reports providers, models, modes and thinking options",
};

/// `POST /v1/hosts/{id}/execution` — Grant host.execute so the host may run agents
pub static ROUTE_POST_V1_HOSTS_ID_EXECUTION: RouteInfo = RouteInfo {
    operation_id: "post_v1_hosts_id_execution",
    method: Method::Post,
    path: "/v1/hosts/{id}/execution",
    path_params: &["id"],
    query_params: &[],
    has_body: true,
    response: "HostView",
    success: &[200],
    errors: &[401, 403, 404, 409],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Grant host.execute so the host may run agents",
};

/// `POST /v1/hosts/{id}/executions` — Create or replay a create-agent intent with a stable execution id
pub static ROUTE_POST_V1_HOSTS_ID_EXECUTIONS: RouteInfo = RouteInfo {
    operation_id: "post_v1_hosts_id_executions",
    method: Method::Post,
    path: "/v1/hosts/{id}/executions",
    path_params: &["id"],
    query_params: &[],
    has_body: true,
    response: "CreateExecutionResponse",
    success: &[200, 201],
    errors: &[400, 401, 403, 404, 409, 422],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Create or replay a create-agent intent with a stable execution id",
};

/// `POST /v1/leaderboards/{game_id}/entries` — Submit a leaderboard entry for the authenticated player
pub static ROUTE_POST_V1_LEADERBOARDS_GAME_ID_ENTRIES: RouteInfo = RouteInfo {
    operation_id: "post_v1_leaderboards_game_id_entries",
    method: Method::Post,
    path: "/v1/leaderboards/{game_id}/entries",
    path_params: &["game_id"],
    query_params: &[],
    has_body: true,
    response: "JsonObject",
    success: &[201],
    errors: &[400, 401],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "Submit a leaderboard entry for the authenticated player",
};

/// `POST /v1/players/subaccount` — Create a subaccount under the authenticated player
pub static ROUTE_POST_V1_PLAYERS_SUBACCOUNT: RouteInfo = RouteInfo {
    operation_id: "post_v1_players_subaccount",
    method: Method::Post,
    path: "/v1/players/subaccount",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "JsonObject",
    success: &[201],
    errors: &[400, 401],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "Create a subaccount under the authenticated player",
};

/// `POST /v1/pool/invitations` — Invite an operator by email; the token is shown once
pub static ROUTE_POST_V1_POOL_INVITATIONS: RouteInfo = RouteInfo {
    operation_id: "post_v1_pool_invitations",
    method: Method::Post,
    path: "/v1/pool/invitations",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "InviteResponse",
    success: &[201],
    errors: &[400, 401, 403, 409, 422],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: Some("owner"),
    surface: Surface::Hub,
    deprecated: false,
    summary: "Invite an operator by email; the token is shown once",
};

/// `POST /v1/pool/invitations/accept` — Accept an invitation and create the member account
pub static ROUTE_POST_V1_POOL_INVITATIONS_ACCEPT: RouteInfo = RouteInfo {
    operation_id: "post_v1_pool_invitations_accept",
    method: Method::Post,
    path: "/v1/pool/invitations/accept",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "PoolLoginResponse",
    success: &[201],
    errors: &[400, 404, 410, 422],
    rate_limited: true,
    auth: Auth::Public,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Accept an invitation and create the member account",
};

/// `POST /v1/pool/login` — Sign an operator in and issue a JWT
pub static ROUTE_POST_V1_POOL_LOGIN: RouteInfo = RouteInfo {
    operation_id: "post_v1_pool_login",
    method: Method::Post,
    path: "/v1/pool/login",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "PoolLoginResponse",
    success: &[200],
    errors: &[400, 401, 422],
    rate_limited: true,
    auth: Auth::Public,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Sign an operator in and issue a JWT",
};

/// `POST /v1/pool/password` — Replace the operator's password (clears first-sign-in requirement)
pub static ROUTE_POST_V1_POOL_PASSWORD: RouteInfo = RouteInfo {
    operation_id: "post_v1_pool_password",
    method: Method::Post,
    path: "/v1/pool/password",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "()",
    success: &[204],
    errors: &[400, 401, 403, 422],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Replace the operator's password (clears first-sign-in requirement)",
};

/// `POST /v1/projects` — Create a project
pub static ROUTE_POST_V1_PROJECTS: RouteInfo = RouteInfo {
    operation_id: "post_v1_projects",
    method: Method::Post,
    path: "/v1/projects",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "ProjectView",
    success: &[201],
    errors: &[400, 401, 403, 409, 422],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Create a project",
};

/// `POST /v1/projects/{id}/configuration/install` — Install a configuration bundle as a new immutable revision
pub static ROUTE_POST_V1_PROJECTS_ID_CONFIGURATION_INSTALL: RouteInfo = RouteInfo {
    operation_id: "post_v1_projects_id_configuration_install",
    method: Method::Post,
    path: "/v1/projects/{id}/configuration/install",
    path_params: &["id"],
    query_params: &[],
    has_body: true,
    response: "InstallResponse",
    success: &[201],
    errors: &[400, 401, 403, 404, 422],
    rate_limited: true,
    auth: Auth::MemberOrApiKey,
    scope: Some("configuration:install"),
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Install a configuration bundle as a new immutable revision",
};

/// `POST /v1/projects/{id}/configuration/validate` — Validate a configuration bundle without writing pool state
pub static ROUTE_POST_V1_PROJECTS_ID_CONFIGURATION_VALIDATE: RouteInfo = RouteInfo {
    operation_id: "post_v1_projects_id_configuration_validate",
    method: Method::Post,
    path: "/v1/projects/{id}/configuration/validate",
    path_params: &["id"],
    query_params: &[],
    has_body: true,
    response: "ValidateResponse",
    success: &[200],
    errors: &[400, 401, 403, 404, 422],
    rate_limited: true,
    auth: Auth::MemberOrApiKey,
    scope: Some("configuration:validate"),
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Validate a configuration bundle without writing pool state",
};

/// `POST /v1/runs` — Dispatch a workflow run from a manual request
pub static ROUTE_POST_V1_RUNS: RouteInfo = RouteInfo {
    operation_id: "post_v1_runs",
    method: Method::Post,
    path: "/v1/runs",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "ManualDispatchResponse",
    success: &[200, 201],
    errors: &[400, 401, 403, 404, 409, 422],
    rate_limited: true,
    auth: Auth::MemberOrApiKey,
    scope: Some("runs:dispatch"),
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Dispatch a workflow run from a manual request",
};

/// `POST /v1/saves` — Create a game save
pub static ROUTE_POST_V1_SAVES: RouteInfo = RouteInfo {
    operation_id: "post_v1_saves",
    method: Method::Post,
    path: "/v1/saves",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "JsonObject",
    success: &[201],
    errors: &[400, 401],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "Create a game save",
};

/// `POST /v1/sessions/{session}/relay/events` — Publish one allow-listed session event
pub static ROUTE_POST_V1_SESSIONS_SESSION_RELAY_EVENTS: RouteInfo = RouteInfo {
    operation_id: "post_v1_sessions_session_relay_events",
    method: Method::Post,
    path: "/v1/sessions/{session}/relay/events",
    path_params: &["session"],
    query_params: &[],
    has_body: true,
    response: "RelayEvent",
    success: &[201],
    errors: &[400, 401, 403, 404, 409, 422],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Publish one allow-listed session event",
};

/// `POST /v1/setup` — Create the organization and first owner from the browser
pub static ROUTE_POST_V1_SETUP: RouteInfo = RouteInfo {
    operation_id: "post_v1_setup",
    method: Method::Post,
    path: "/v1/setup",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "PoolIdentityView",
    success: &[201],
    errors: &[400, 409, 422],
    rate_limited: true,
    auth: Auth::Public,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Create the organization and first owner from the browser",
};

/// `POST /v1/socket-tickets` — Issue a one-shot WebSocket connection ticket
pub static ROUTE_POST_V1_SOCKET_TICKETS: RouteInfo = RouteInfo {
    operation_id: "post_v1_socket_tickets",
    method: Method::Post,
    path: "/v1/socket-tickets",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "JsonObject",
    success: &[200],
    errors: &[],
    rate_limited: false,
    auth: Auth::Public,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "Issue a one-shot WebSocket connection ticket",
};

/// `POST /v1/triggers/install` — Install or replace a trigger; keeps its id and bumps the revision
pub static ROUTE_POST_V1_TRIGGERS_INSTALL: RouteInfo = RouteInfo {
    operation_id: "post_v1_triggers_install",
    method: Method::Post,
    path: "/v1/triggers/install",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "TriggerInstallResponse",
    success: &[201],
    errors: &[400, 401, 403, 404, 422],
    rate_limited: true,
    auth: Auth::MemberOrApiKey,
    scope: Some("configuration:install"),
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Install or replace a trigger; keeps its id and bumps the revision",
};

/// `POST /v1/triggers/validate` — Validate one trigger document without writing pool state
pub static ROUTE_POST_V1_TRIGGERS_VALIDATE: RouteInfo = RouteInfo {
    operation_id: "post_v1_triggers_validate",
    method: Method::Post,
    path: "/v1/triggers/validate",
    path_params: &[],
    query_params: &[],
    has_body: true,
    response: "TriggerValidateResponse",
    success: &[200],
    errors: &[400, 401, 403, 404, 422],
    rate_limited: true,
    auth: Auth::MemberOrApiKey,
    scope: Some("configuration:validate"),
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Validate one trigger document without writing pool state",
};

/// `POST /v1/webhooks/github/{slug}` — Receive a GitHub App webhook; signature verified and delivery deduplicated
pub static ROUTE_POST_V1_WEBHOOKS_GITHUB_SLUG: RouteInfo = RouteInfo {
    operation_id: "post_v1_webhooks_github_slug",
    method: Method::Post,
    path: "/v1/webhooks/github/{slug}",
    path_params: &["slug"],
    query_params: &[],
    has_body: true,
    response: "WebhookResponse",
    success: &[202],
    errors: &[400, 401, 404, 422],
    rate_limited: true,
    auth: Auth::Signature,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Receive a GitHub App webhook; signature verified and delivery deduplicated",
};

/// `POST /v1/webhooks/slack/{slug}` — Receive a Slack Events API callback; signature verified and event deduplicated
pub static ROUTE_POST_V1_WEBHOOKS_SLACK_SLUG: RouteInfo = RouteInfo {
    operation_id: "post_v1_webhooks_slack_slug",
    method: Method::Post,
    path: "/v1/webhooks/slack/{slug}",
    path_params: &["slug"],
    query_params: &[],
    has_body: true,
    response: "PostV1WebhooksSlackSlugResponse",
    success: &[200, 202],
    errors: &[400, 401, 404, 422],
    rate_limited: true,
    auth: Auth::Signature,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Receive a Slack Events API callback; signature verified and event deduplicated",
};

/// `PUT /v1/games/{game_id}/settings` — Set a game's settings (upsert)
pub static ROUTE_PUT_V1_GAMES_GAME_ID_SETTINGS: RouteInfo = RouteInfo {
    operation_id: "put_v1_games_game_id_settings",
    method: Method::Put,
    path: "/v1/games/{game_id}/settings",
    path_params: &["game_id"],
    query_params: &[],
    has_body: true,
    response: "JsonObject",
    success: &[200],
    errors: &[400, 401],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "Set a game's settings (upsert)",
};

/// `PUT /v1/leaderboards/{game_id}/entries/{player_id}` — Update the authenticated player's own leaderboard entry
pub static ROUTE_PUT_V1_LEADERBOARDS_GAME_ID_ENTRIES_PLAYER_ID: RouteInfo = RouteInfo {
    operation_id: "put_v1_leaderboards_game_id_entries_player_id",
    method: Method::Put,
    path: "/v1/leaderboards/{game_id}/entries/{player_id}",
    path_params: &["game_id", "player_id"],
    query_params: &[],
    has_body: true,
    response: "JsonObject",
    success: &[200],
    errors: &[400, 401],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "Update the authenticated player's own leaderboard entry",
};

/// `PUT /v1/players/{id}` — Update the authenticated player's own profile
pub static ROUTE_PUT_V1_PLAYERS_ID: RouteInfo = RouteInfo {
    operation_id: "put_v1_players_id",
    method: Method::Put,
    path: "/v1/players/{id}",
    path_params: &["id"],
    query_params: &[],
    has_body: true,
    response: "JsonObject",
    success: &[200],
    errors: &[400, 401],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Game,
    deprecated: false,
    summary: "Update the authenticated player's own profile",
};

/// `PUT /v1/sessions/{session}/relay` — Opt a session into publication
pub static ROUTE_PUT_V1_SESSIONS_SESSION_RELAY: RouteInfo = RouteInfo {
    operation_id: "put_v1_sessions_session_relay",
    method: Method::Put,
    path: "/v1/sessions/{session}/relay",
    path_params: &["session"],
    query_params: &[],
    has_body: true,
    response: "RelayState",
    success: &[200],
    errors: &[401, 403, 404, 409],
    rate_limited: true,
    auth: Auth::Member,
    scope: None,
    role: None,
    surface: Surface::Hub,
    deprecated: false,
    summary: "Opt a session into publication",
};

/// Every route in the committed contract, in operation-id order.
pub static ROUTES: &[&RouteInfo] = &[
    &ROUTE_DELETE_V1_API_KEYS_ID,
    &ROUTE_DELETE_V1_CONNECTIONS_ID,
    &ROUTE_DELETE_V1_HOSTS_ID,
    &ROUTE_DELETE_V1_HOSTS_ID_EXECUTION,
    &ROUTE_DELETE_V1_PLAYERS_ID,
    &ROUTE_DELETE_V1_POOL_INVITATIONS_ID,
    &ROUTE_DELETE_V1_POOL_MEMBERS_ID,
    &ROUTE_DELETE_V1_PROJECTS_ID,
    &ROUTE_DELETE_V1_SAVES_PLAYER_ID_SAVE_ID,
    &ROUTE_DELETE_V1_SESSIONS_SESSION_RELAY,
    &ROUTE_GET_HEALTHZ,
    &ROUTE_GET_METRICS,
    &ROUTE_GET_OPENAPI_JSON,
    &ROUTE_GET_OPENAPI_V1_JSON,
    &ROUTE_GET_V1_ACTIVITY,
    &ROUTE_GET_V1_ACTIVITY_RUNS_RUN,
    &ROUTE_GET_V1_API_KEYS,
    &ROUTE_GET_V1_API_KEYS_WHOAMI,
    &ROUTE_GET_V1_CHATROOMS,
    &ROUTE_GET_V1_CHATROOMS_ROOM_MEMBERS,
    &ROUTE_GET_V1_CHATROOMS_ROOM_MESSAGES,
    &ROUTE_GET_V1_CHATROOMS_ROOM_STREAM,
    &ROUTE_GET_V1_CONNECTIONS,
    &ROUTE_GET_V1_CONNECTIONS_ID,
    &ROUTE_GET_V1_GAMES_GAME_ID_SETTINGS,
    &ROUTE_GET_V1_HOSTS,
    &ROUTE_GET_V1_HOSTS_ID,
    &ROUTE_GET_V1_HOSTS_ID_CAPABILITIES,
    &ROUTE_GET_V1_LEADERBOARDS_GAME_ID,
    &ROUTE_GET_V1_PLAYERS_ID,
    &ROUTE_GET_V1_PLAYERS_ID_SUBACCOUNTS,
    &ROUTE_GET_V1_POOL_INVITATIONS,
    &ROUTE_GET_V1_POOL_ME,
    &ROUTE_GET_V1_POOL_MEMBERS,
    &ROUTE_GET_V1_PROJECTS,
    &ROUTE_GET_V1_PROJECTS_ID,
    &ROUTE_GET_V1_PROJECTS_ID_CONFIGURATION,
    &ROUTE_GET_V1_PROJECTS_ID_REVISIONS,
    &ROUTE_GET_V1_PROJECTS_ID_REVISIONS_REVISION_ID,
    &ROUTE_GET_V1_SAVES_PLAYER_ID,
    &ROUTE_GET_V1_SAVES_PLAYER_ID_SAVE_ID,
    &ROUTE_GET_V1_SESSIONS_SESSION_RELAY,
    &ROUTE_GET_V1_SESSIONS_SESSION_RELAY_EVENTS,
    &ROUTE_GET_V1_SETUP_STATUS,
    &ROUTE_PATCH_V1_CONNECTIONS_ID,
    &ROUTE_PATCH_V1_HOSTS_ID,
    &ROUTE_PATCH_V1_POOL_MEMBERS_ID,
    &ROUTE_PATCH_V1_PROJECTS_ID,
    &ROUTE_POST_V1_API_KEYS,
    &ROUTE_POST_V1_AUTH_LOGIN,
    &ROUTE_POST_V1_AUTH_REGISTER,
    &ROUTE_POST_V1_CHATROOMS_ROOM_MESSAGES,
    &ROUTE_POST_V1_CONNECTIONS,
    &ROUTE_POST_V1_EVENTS,
    &ROUTE_POST_V1_FEEDBACK,
    &ROUTE_POST_V1_HOSTS,
    &ROUTE_POST_V1_HOSTS_ENROLL,
    &ROUTE_POST_V1_HOSTS_ENROLLMENT_TOKENS,
    &ROUTE_POST_V1_HOSTS_ID_CAPABILITIES,
    &ROUTE_POST_V1_HOSTS_ID_EXECUTION,
    &ROUTE_POST_V1_HOSTS_ID_EXECUTIONS,
    &ROUTE_POST_V1_LEADERBOARDS_GAME_ID_ENTRIES,
    &ROUTE_POST_V1_PLAYERS_SUBACCOUNT,
    &ROUTE_POST_V1_POOL_INVITATIONS,
    &ROUTE_POST_V1_POOL_INVITATIONS_ACCEPT,
    &ROUTE_POST_V1_POOL_LOGIN,
    &ROUTE_POST_V1_POOL_PASSWORD,
    &ROUTE_POST_V1_PROJECTS,
    &ROUTE_POST_V1_PROJECTS_ID_CONFIGURATION_INSTALL,
    &ROUTE_POST_V1_PROJECTS_ID_CONFIGURATION_VALIDATE,
    &ROUTE_POST_V1_RUNS,
    &ROUTE_POST_V1_SAVES,
    &ROUTE_POST_V1_SESSIONS_SESSION_RELAY_EVENTS,
    &ROUTE_POST_V1_SETUP,
    &ROUTE_POST_V1_SOCKET_TICKETS,
    &ROUTE_POST_V1_TRIGGERS_INSTALL,
    &ROUTE_POST_V1_TRIGGERS_VALIDATE,
    &ROUTE_POST_V1_WEBHOOKS_GITHUB_SLUG,
    &ROUTE_POST_V1_WEBHOOKS_SLACK_SLUG,
    &ROUTE_PUT_V1_GAMES_GAME_ID_SETTINGS,
    &ROUTE_PUT_V1_LEADERBOARDS_GAME_ID_ENTRIES_PLAYER_ID,
    &ROUTE_PUT_V1_PLAYERS_ID,
    &ROUTE_PUT_V1_SESSIONS_SESSION_RELAY,
];

/// Route metadata by operation id, or `None` when the id is not in the contract.
pub fn route(operation_id: &str) -> Option<&'static RouteInfo> {
    ROUTES
        .iter()
        .copied()
        .find(|route| route.operation_id == operation_id)
}
