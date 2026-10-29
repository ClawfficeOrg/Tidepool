// Generated from docs/api/openapi-v1.json. Do not edit by hand.
// Regenerate: node scripts/generate.mjs (from sdk/rust).

//! Generated request types: one per contract route.
//!
//! Each type implements [`crate::route::Endpoint`], which binds it to the
//! route table and gives it a typed response body and request body.

#![allow(clippy::redundant_field_names)]
#![allow(clippy::needless_update)]

use crate::route::Endpoint;

/// Success body of `post_v1_webhooks_slack_slug`, whose contract declares more than one.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum PostV1WebhooksSlackSlugResponse {
    /// A `ChallengeResponse` body.
    ChallengeResponse(crate::types::ChallengeResponse),
    /// A `WebhookResponse` body.
    WebhookResponse(crate::types::WebhookResponse),
}

/// Revoke an API key (owner only; effective immediately)
/// `DELETE /v1/api-keys/{id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeleteV1ApiKeysId {
    /// Path parameter `id`.
    pub id: String,
}

impl DeleteV1ApiKeysId {
    /// Build the request.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for DeleteV1ApiKeysId {
    type Response = ();
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_DELETE_V1_API_KEYS_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
}

/// Delete a database connection
/// `DELETE /v1/connections/{id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeleteV1ConnectionsId {
    /// Path parameter `id`.
    pub id: String,
}

impl DeleteV1ConnectionsId {
    /// Build the request.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for DeleteV1ConnectionsId {
    type Response = ();
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_DELETE_V1_CONNECTIONS_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
}

/// Revoke the pool relationship for a host
/// `DELETE /v1/hosts/{id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeleteV1HostsId {
    /// Path parameter `id`.
    pub id: String,
}

impl DeleteV1HostsId {
    /// Build the request.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for DeleteV1HostsId {
    type Response = ();
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_DELETE_V1_HOSTS_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
}

/// Revoke host.execute; the host becomes connected-only
/// `DELETE /v1/hosts/{id}/execution`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeleteV1HostsIdExecution {
    /// Path parameter `id`.
    pub id: String,
}

impl DeleteV1HostsIdExecution {
    /// Build the request.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for DeleteV1HostsIdExecution {
    type Response = crate::types::HostView;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_DELETE_V1_HOSTS_ID_EXECUTION;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
}

/// Soft-delete the authenticated player's own account
/// `DELETE /v1/players/{id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeleteV1PlayersId {
    /// Path parameter `id`.
    pub id: String,
}

impl DeleteV1PlayersId {
    /// Build the request.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for DeleteV1PlayersId {
    type Response = ();
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_DELETE_V1_PLAYERS_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
}

/// Revoke a pending invitation (owner only)
/// `DELETE /v1/pool/invitations/{id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeleteV1PoolInvitationsId {
    /// Path parameter `id`.
    pub id: String,
}

impl DeleteV1PoolInvitationsId {
    /// Build the request.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for DeleteV1PoolInvitationsId {
    type Response = ();
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_DELETE_V1_POOL_INVITATIONS_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
}

/// Remove a member (owner only; last owner protected)
/// `DELETE /v1/pool/members/{id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeleteV1PoolMembersId {
    /// Path parameter `id`.
    pub id: String,
}

impl DeleteV1PoolMembersId {
    /// Build the request.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for DeleteV1PoolMembersId {
    type Response = ();
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_DELETE_V1_POOL_MEMBERS_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
}

/// Delete a project and its revisions
/// `DELETE /v1/projects/{id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeleteV1ProjectsId {
    /// Path parameter `id`.
    pub id: String,
}

impl DeleteV1ProjectsId {
    /// Build the request.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for DeleteV1ProjectsId {
    type Response = ();
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_DELETE_V1_PROJECTS_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
}

/// Delete a single game save
/// `DELETE /v1/saves/{player_id}/{save_id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeleteV1SavesPlayerIdSaveId {
    /// Path parameter `player_id`.
    pub player_id: String,
    /// Path parameter `save_id`.
    pub save_id: String,
}

impl DeleteV1SavesPlayerIdSaveId {
    /// Build the request.
    pub fn new(player_id: impl Into<String>, save_id: impl Into<String>) -> Self {
        Self {
            player_id: player_id.into(),
            save_id: save_id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for DeleteV1SavesPlayerIdSaveId {
    type Response = ();
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_DELETE_V1_SAVES_PLAYER_ID_SAVE_ID;
    fn path(&self) -> String {
        crate::route::path_with(
            Self::ROUTE.path,
            &[
                ("player_id", self.player_id.as_str()),
                ("save_id", self.save_id.as_str()),
            ],
        )
    }
}

/// Stop publication immediately and permanently
/// `DELETE /v1/sessions/{session}/relay`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeleteV1SessionsSessionRelay {
    /// Path parameter `session`.
    pub session: String,
}

impl DeleteV1SessionsSessionRelay {
    /// Build the request.
    pub fn new(session: impl Into<String>) -> Self {
        Self {
            session: session.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for DeleteV1SessionsSessionRelay {
    type Response = crate::types::RelayState;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_DELETE_V1_SESSIONS_SESSION_RELAY;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("session", self.session.as_str())])
    }
}

/// Liveness probe with DB status
/// `GET /healthz`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetHealthz {}

impl GetHealthz {
    /// Build the request.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Endpoint for GetHealthz {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_HEALTHZ;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
}

/// Prometheus text-format metrics
/// `GET /metrics`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetMetrics {}

impl GetMetrics {
    /// Build the request.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Endpoint for GetMetrics {
    type Response = String;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_METRICS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn decode_response(body: &[u8]) -> crate::Result<Self::Response> {
        Ok(String::from_utf8_lossy(body).into_owned())
    }
}

/// This OpenAPI document
/// `GET /openapi.json`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetOpenapiJson {}

impl GetOpenapiJson {
    /// Build the request.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Endpoint for GetOpenapiJson {
    type Response = crate::types::OpenApiDocument;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_OPENAPI_JSON;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
}

/// This OpenAPI document at its pinned major-version path
/// `GET /openapi/v1.json`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetOpenapiV1Json {}

impl GetOpenapiV1Json {
    /// Build the request.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Endpoint for GetOpenapiV1Json {
    type Response = crate::types::OpenApiDocument;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_OPENAPI_V1_JSON;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
}

/// Query parameters for `get_v1_activity`.
///
/// The contract does not publish these yet (`docs/API_CONTRACT.md` §5.1);
/// the names mirror the handler structs and the TypeScript SDK.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1ActivityQuery {
    /// `project`
    pub project: Option<String>,
    /// `provider`
    pub provider: Option<String>,
    /// `trigger`
    pub trigger: Option<String>,
    /// `status`
    pub status: Option<String>,
    /// `kind`
    pub kind: Option<String>,
    /// `cursor`
    pub cursor: Option<String>,
    /// `limit`
    pub limit: Option<u64>,
}

impl GetV1ActivityQuery {
    fn to_pairs(&self) -> Vec<(&'static str, String)> {
        let mut pairs = Vec::new();
        if let Some(value) = &self.project {
            pairs.push(("project", value.clone()));
        }
        if let Some(value) = &self.provider {
            pairs.push(("provider", value.clone()));
        }
        if let Some(value) = &self.trigger {
            pairs.push(("trigger", value.clone()));
        }
        if let Some(value) = &self.status {
            pairs.push(("status", value.clone()));
        }
        if let Some(value) = &self.kind {
            pairs.push(("kind", value.clone()));
        }
        if let Some(value) = &self.cursor {
            pairs.push(("cursor", value.clone()));
        }
        if let Some(value) = self.limit {
            pairs.push(("limit", value.to_string()));
        }
        pairs
    }
}

/// List one page of receipts, runs, steps and executions
/// `GET /v1/activity`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1Activity {
    /// Query parameters.
    pub query: GetV1ActivityQuery,
}

impl GetV1Activity {
    /// Build the request.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the query parameters.
    pub fn with_query(mut self, query: GetV1ActivityQuery) -> Self {
        self.query = query;
        self
    }
}

impl Endpoint for GetV1Activity {
    type Response = crate::types::ActivityPage;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_V1_ACTIVITY;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn query(&self) -> Vec<(&'static str, String)> {
        self.query.to_pairs()
    }
}

/// Fetch one run with the arrival record that started it
/// `GET /v1/activity/runs/{run}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1ActivityRunsRun {
    /// Path parameter `run`.
    pub run: String,
}

impl GetV1ActivityRunsRun {
    /// Build the request.
    pub fn new(run: impl Into<String>) -> Self {
        Self {
            run: run.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for GetV1ActivityRunsRun {
    type Response = crate::types::ActivityRunDetail;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_V1_ACTIVITY_RUNS_RUN;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("run", self.run.as_str())])
    }
}

/// List organization API keys (owner only; prefix only)
/// `GET /v1/api-keys`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1ApiKeys {}

impl GetV1ApiKeys {
    /// Build the request.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Endpoint for GetV1ApiKeys {
    type Response = crate::types::ApiKeysResponse;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_V1_API_KEYS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
}

/// Introspect an API key (requires projects:read)
/// `GET /v1/api-keys/whoami`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1ApiKeysWhoami {}

impl GetV1ApiKeysWhoami {
    /// Build the request.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Endpoint for GetV1ApiKeysWhoami {
    type Response = crate::types::ApiKeyView;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_V1_API_KEYS_WHOAMI;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
}

/// List rooms visible to the operator
/// `GET /v1/chatrooms`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1Chatrooms {}

impl GetV1Chatrooms {
    /// Build the request.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Endpoint for GetV1Chatrooms {
    type Response = crate::types::RoomViewList;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_V1_CHATROOMS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
}

/// List a room's members
/// `GET /v1/chatrooms/{room}/members`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1ChatroomsRoomMembers {
    /// Path parameter `room`.
    pub room: String,
}

impl GetV1ChatroomsRoomMembers {
    /// Build the request.
    pub fn new(room: impl Into<String>) -> Self {
        Self {
            room: room.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for GetV1ChatroomsRoomMembers {
    type Response = crate::types::RoomMemberViewList;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_GET_V1_CHATROOMS_ROOM_MEMBERS;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("room", self.room.as_str())])
    }
}

/// Query parameters for `get_v1_chatrooms_room_messages`.
///
/// The contract does not publish these yet (`docs/API_CONTRACT.md` §5.1);
/// the names mirror the handler structs and the TypeScript SDK.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1ChatroomsRoomMessagesQuery {
    /// `after`
    pub after: Option<u64>,
    /// `limit`
    pub limit: Option<u64>,
}

impl GetV1ChatroomsRoomMessagesQuery {
    fn to_pairs(&self) -> Vec<(&'static str, String)> {
        let mut pairs = Vec::new();
        if let Some(value) = self.after {
            pairs.push(("after", value.to_string()));
        }
        if let Some(value) = self.limit {
            pairs.push(("limit", value.to_string()));
        }
        pairs
    }
}

/// Read room history after a sequence cursor
/// `GET /v1/chatrooms/{room}/messages`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1ChatroomsRoomMessages {
    /// Path parameter `room`.
    pub room: String,
    /// Query parameters.
    pub query: GetV1ChatroomsRoomMessagesQuery,
}

impl GetV1ChatroomsRoomMessages {
    /// Build the request.
    pub fn new(room: impl Into<String>) -> Self {
        Self {
            room: room.into(),
            ..Self::default()
        }
    }

    /// Replace the query parameters.
    pub fn with_query(mut self, query: GetV1ChatroomsRoomMessagesQuery) -> Self {
        self.query = query;
        self
    }
}

impl Endpoint for GetV1ChatroomsRoomMessages {
    type Response = crate::types::HistoryResponse;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_GET_V1_CHATROOMS_ROOM_MESSAGES;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("room", self.room.as_str())])
    }
    fn query(&self) -> Vec<(&'static str, String)> {
        self.query.to_pairs()
    }
}

/// Query parameters for `get_v1_chatrooms_room_stream`.
///
/// The contract does not publish these yet (`docs/API_CONTRACT.md` §5.1);
/// the names mirror the handler structs and the TypeScript SDK.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1ChatroomsRoomStreamQuery {
    /// `token`
    pub token: Option<String>,
    /// `after`
    pub after: Option<u64>,
}

impl GetV1ChatroomsRoomStreamQuery {
    fn to_pairs(&self) -> Vec<(&'static str, String)> {
        let mut pairs = Vec::new();
        if let Some(value) = &self.token {
            pairs.push(("token", value.clone()));
        }
        if let Some(value) = self.after {
            pairs.push(("after", value.to_string()));
        }
        pairs
    }
}

/// Upgrade to the room's live message stream
/// `GET /v1/chatrooms/{room}/stream`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1ChatroomsRoomStream {
    /// Path parameter `room`.
    pub room: String,
    /// Query parameters.
    pub query: GetV1ChatroomsRoomStreamQuery,
}

impl GetV1ChatroomsRoomStream {
    /// Build the request.
    pub fn new(room: impl Into<String>) -> Self {
        Self {
            room: room.into(),
            ..Self::default()
        }
    }

    /// Replace the query parameters.
    pub fn with_query(mut self, query: GetV1ChatroomsRoomStreamQuery) -> Self {
        self.query = query;
        self
    }
}

impl Endpoint for GetV1ChatroomsRoomStream {
    type Response = crate::stream::ChatStream;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_GET_V1_CHATROOMS_ROOM_STREAM;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("room", self.room.as_str())])
    }
    fn query(&self) -> Vec<(&'static str, String)> {
        self.query.to_pairs()
    }
}

/// List effective provider connections (environment overlay applied)
/// `GET /v1/connections`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1Connections {}

impl GetV1Connections {
    /// Build the request.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Endpoint for GetV1Connections {
    type Response = crate::types::ConnectionsResponse;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_V1_CONNECTIONS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
}

/// Fetch a connection by id or env:<slug>
/// `GET /v1/connections/{id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1ConnectionsId {
    /// Path parameter `id`.
    pub id: String,
}

impl GetV1ConnectionsId {
    /// Build the request.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for GetV1ConnectionsId {
    type Response = crate::types::ConnectionView;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_V1_CONNECTIONS_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
}

/// Fetch a game's settings
/// `GET /v1/games/{game_id}/settings`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1GamesGameIdSettings {
    /// Path parameter `game_id`.
    pub game_id: String,
}

impl GetV1GamesGameIdSettings {
    /// Build the request.
    pub fn new(game_id: impl Into<String>) -> Self {
        Self {
            game_id: game_id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for GetV1GamesGameIdSettings {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_GET_V1_GAMES_GAME_ID_SETTINGS;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("game_id", self.game_id.as_str())])
    }
}

/// List live hosts in the pool
/// `GET /v1/hosts`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1Hosts {}

impl GetV1Hosts {
    /// Build the request.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Endpoint for GetV1Hosts {
    type Response = crate::types::HostsResponse;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_V1_HOSTS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
}

/// Fetch a live host by immutable id
/// `GET /v1/hosts/{id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1HostsId {
    /// Path parameter `id`.
    pub id: String,
}

impl GetV1HostsId {
    /// Build the request.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for GetV1HostsId {
    type Response = crate::types::HostView;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_V1_HOSTS_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
}

/// Read the host's last capability report and offered providers
/// `GET /v1/hosts/{id}/capabilities`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1HostsIdCapabilities {
    /// Path parameter `id`.
    pub id: String,
}

impl GetV1HostsIdCapabilities {
    /// Build the request.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for GetV1HostsIdCapabilities {
    type Response = crate::types::HostCapabilityReport;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_GET_V1_HOSTS_ID_CAPABILITIES;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
}

/// Query parameters for `get_v1_leaderboards_game_id`.
///
/// The contract does not publish these yet (`docs/API_CONTRACT.md` §5.1);
/// the names mirror the handler structs and the TypeScript SDK.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1LeaderboardsGameIdQuery {
    /// `page`
    pub page: Option<u64>,
    /// `per_page`
    pub per_page: Option<u64>,
}

impl GetV1LeaderboardsGameIdQuery {
    fn to_pairs(&self) -> Vec<(&'static str, String)> {
        let mut pairs = Vec::new();
        if let Some(value) = self.page {
            pairs.push(("page", value.to_string()));
        }
        if let Some(value) = self.per_page {
            pairs.push(("per_page", value.to_string()));
        }
        pairs
    }
}

/// Fetch one ranked page of a leaderboard
/// `GET /v1/leaderboards/{game_id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1LeaderboardsGameId {
    /// Path parameter `game_id`.
    pub game_id: String,
    /// Query parameters.
    pub query: GetV1LeaderboardsGameIdQuery,
}

impl GetV1LeaderboardsGameId {
    /// Build the request.
    pub fn new(game_id: impl Into<String>) -> Self {
        Self {
            game_id: game_id.into(),
            ..Self::default()
        }
    }

    /// Replace the query parameters.
    pub fn with_query(mut self, query: GetV1LeaderboardsGameIdQuery) -> Self {
        self.query = query;
        self
    }
}

impl Endpoint for GetV1LeaderboardsGameId {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_GET_V1_LEADERBOARDS_GAME_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("game_id", self.game_id.as_str())])
    }
    fn query(&self) -> Vec<(&'static str, String)> {
        self.query.to_pairs()
    }
}

/// Fetch a player's public profile
/// `GET /v1/players/{id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1PlayersId {
    /// Path parameter `id`.
    pub id: String,
}

impl GetV1PlayersId {
    /// Build the request.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for GetV1PlayersId {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_V1_PLAYERS_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
}

/// List the authenticated player's subaccounts
/// `GET /v1/players/{id}/subaccounts`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1PlayersIdSubaccounts {
    /// Path parameter `id`.
    pub id: String,
}

impl GetV1PlayersIdSubaccounts {
    /// Build the request.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for GetV1PlayersIdSubaccounts {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_GET_V1_PLAYERS_ID_SUBACCOUNTS;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
}

/// List pending team invitations (owner only)
/// `GET /v1/pool/invitations`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1PoolInvitations {}

impl GetV1PoolInvitations {
    /// Build the request.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Endpoint for GetV1PoolInvitations {
    type Response = crate::types::InvitationsResponse;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_V1_POOL_INVITATIONS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
}

/// Fetch the signed-in operator and organization
/// `GET /v1/pool/me`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1PoolMe {}

impl GetV1PoolMe {
    /// Build the request.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Endpoint for GetV1PoolMe {
    type Response = crate::types::PoolIdentityView;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_V1_POOL_ME;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
}

/// List the organization's operators
/// `GET /v1/pool/members`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1PoolMembers {}

impl GetV1PoolMembers {
    /// Build the request.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Endpoint for GetV1PoolMembers {
    type Response = crate::types::MembersResponse;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_V1_POOL_MEMBERS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
}

/// List projects in the organization
/// `GET /v1/projects`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1Projects {}

impl GetV1Projects {
    /// Build the request.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Endpoint for GetV1Projects {
    type Response = crate::types::ProjectsResponse;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_V1_PROJECTS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
}

/// Fetch a project by id
/// `GET /v1/projects/{id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1ProjectsId {
    /// Path parameter `id`.
    pub id: String,
}

impl GetV1ProjectsId {
    /// Build the request.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for GetV1ProjectsId {
    type Response = crate::types::ProjectView;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_V1_PROJECTS_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
}

/// Fetch a project's active configuration revision
/// `GET /v1/projects/{id}/configuration`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1ProjectsIdConfiguration {
    /// Path parameter `id`.
    pub id: String,
}

impl GetV1ProjectsIdConfiguration {
    /// Build the request.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for GetV1ProjectsIdConfiguration {
    type Response = crate::types::RevisionView;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_GET_V1_PROJECTS_ID_CONFIGURATION;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
}

/// List a project's configuration revisions
/// `GET /v1/projects/{id}/revisions`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1ProjectsIdRevisions {
    /// Path parameter `id`.
    pub id: String,
}

impl GetV1ProjectsIdRevisions {
    /// Build the request.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for GetV1ProjectsIdRevisions {
    type Response = crate::types::RevisionsResponse;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_GET_V1_PROJECTS_ID_REVISIONS;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
}

/// Fetch one configuration revision with its files
/// `GET /v1/projects/{id}/revisions/{revision_id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1ProjectsIdRevisionsRevisionId {
    /// Path parameter `id`.
    pub id: String,
    /// Path parameter `revision_id`.
    pub revision_id: String,
}

impl GetV1ProjectsIdRevisionsRevisionId {
    /// Build the request.
    pub fn new(id: impl Into<String>, revision_id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            revision_id: revision_id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for GetV1ProjectsIdRevisionsRevisionId {
    type Response = crate::types::RevisionView;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_GET_V1_PROJECTS_ID_REVISIONS_REVISION_ID;
    fn path(&self) -> String {
        crate::route::path_with(
            Self::ROUTE.path,
            &[
                ("id", self.id.as_str()),
                ("revision_id", self.revision_id.as_str()),
            ],
        )
    }
}

/// List the authenticated player's saves, newest first
/// `GET /v1/saves/{player_id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1SavesPlayerId {
    /// Path parameter `player_id`.
    pub player_id: String,
}

impl GetV1SavesPlayerId {
    /// Build the request.
    pub fn new(player_id: impl Into<String>) -> Self {
        Self {
            player_id: player_id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for GetV1SavesPlayerId {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_V1_SAVES_PLAYER_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("player_id", self.player_id.as_str())])
    }
}

/// Fetch a single game save
/// `GET /v1/saves/{player_id}/{save_id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1SavesPlayerIdSaveId {
    /// Path parameter `player_id`.
    pub player_id: String,
    /// Path parameter `save_id`.
    pub save_id: String,
}

impl GetV1SavesPlayerIdSaveId {
    /// Build the request.
    pub fn new(player_id: impl Into<String>, save_id: impl Into<String>) -> Self {
        Self {
            player_id: player_id.into(),
            save_id: save_id.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for GetV1SavesPlayerIdSaveId {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_GET_V1_SAVES_PLAYER_ID_SAVE_ID;
    fn path(&self) -> String {
        crate::route::path_with(
            Self::ROUTE.path,
            &[
                ("player_id", self.player_id.as_str()),
                ("save_id", self.save_id.as_str()),
            ],
        )
    }
}

/// Read a session's relay publication state
/// `GET /v1/sessions/{session}/relay`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1SessionsSessionRelay {
    /// Path parameter `session`.
    pub session: String,
}

impl GetV1SessionsSessionRelay {
    /// Build the request.
    pub fn new(session: impl Into<String>) -> Self {
        Self {
            session: session.into(),
            ..Self::default()
        }
    }
}

impl Endpoint for GetV1SessionsSessionRelay {
    type Response = crate::types::RelayState;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_GET_V1_SESSIONS_SESSION_RELAY;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("session", self.session.as_str())])
    }
}

/// Query parameters for `get_v1_sessions_session_relay_events`.
///
/// The contract does not publish these yet (`docs/API_CONTRACT.md` §5.1);
/// the names mirror the handler structs and the TypeScript SDK.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1SessionsSessionRelayEventsQuery {
    /// `after`
    pub after: Option<u64>,
    /// `limit`
    pub limit: Option<u64>,
}

impl GetV1SessionsSessionRelayEventsQuery {
    fn to_pairs(&self) -> Vec<(&'static str, String)> {
        let mut pairs = Vec::new();
        if let Some(value) = self.after {
            pairs.push(("after", value.to_string()));
        }
        if let Some(value) = self.limit {
            pairs.push(("limit", value.to_string()));
        }
        pairs
    }
}

/// Backfill relayed events after a sequence cursor
/// `GET /v1/sessions/{session}/relay/events`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1SessionsSessionRelayEvents {
    /// Path parameter `session`.
    pub session: String,
    /// Query parameters.
    pub query: GetV1SessionsSessionRelayEventsQuery,
}

impl GetV1SessionsSessionRelayEvents {
    /// Build the request.
    pub fn new(session: impl Into<String>) -> Self {
        Self {
            session: session.into(),
            ..Self::default()
        }
    }

    /// Replace the query parameters.
    pub fn with_query(mut self, query: GetV1SessionsSessionRelayEventsQuery) -> Self {
        self.query = query;
        self
    }
}

impl Endpoint for GetV1SessionsSessionRelayEvents {
    type Response = crate::types::RelayPage;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_GET_V1_SESSIONS_SESSION_RELAY_EVENTS;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("session", self.session.as_str())])
    }
    fn query(&self) -> Vec<(&'static str, String)> {
        self.query.to_pairs()
    }
}

/// Report whether the pool still needs first-run setup
/// `GET /v1/setup/status`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetV1SetupStatus {}

impl GetV1SetupStatus {
    /// Build the request.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Endpoint for GetV1SetupStatus {
    type Response = crate::types::SetupStatusResponse;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_GET_V1_SETUP_STATUS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
}

/// Rename, enable or disable a database connection
/// `PATCH /v1/connections/{id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PatchV1ConnectionsId {
    /// Path parameter `id`.
    pub id: String,
    /// Request body.
    pub body: crate::types::UpdateConnectionRequest,
}

impl PatchV1ConnectionsId {
    /// Build the request.
    pub fn new(id: impl Into<String>, body: crate::types::UpdateConnectionRequest) -> Self {
        Self {
            id: id.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PatchV1ConnectionsId {
    type Response = crate::types::ConnectionView;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_PATCH_V1_CONNECTIONS_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Rename a host slug or display name; the id is unchanged
/// `PATCH /v1/hosts/{id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PatchV1HostsId {
    /// Path parameter `id`.
    pub id: String,
    /// Request body.
    pub body: crate::types::RenameHostRequest,
}

impl PatchV1HostsId {
    /// Build the request.
    pub fn new(id: impl Into<String>, body: crate::types::RenameHostRequest) -> Self {
        Self {
            id: id.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PatchV1HostsId {
    type Response = crate::types::HostView;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_PATCH_V1_HOSTS_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Change a member's role (owner only; last owner protected)
/// `PATCH /v1/pool/members/{id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PatchV1PoolMembersId {
    /// Path parameter `id`.
    pub id: String,
    /// Request body.
    pub body: crate::types::UpdateRoleRequest,
}

impl PatchV1PoolMembersId {
    /// Build the request.
    pub fn new(id: impl Into<String>, body: crate::types::UpdateRoleRequest) -> Self {
        Self {
            id: id.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PatchV1PoolMembersId {
    type Response = crate::types::PoolUserView;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_PATCH_V1_POOL_MEMBERS_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Rename a project slug or name; the id is unchanged
/// `PATCH /v1/projects/{id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PatchV1ProjectsId {
    /// Path parameter `id`.
    pub id: String,
    /// Request body.
    pub body: crate::types::RenameProjectRequest,
}

impl PatchV1ProjectsId {
    /// Build the request.
    pub fn new(id: impl Into<String>, body: crate::types::RenameProjectRequest) -> Self {
        Self {
            id: id.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PatchV1ProjectsId {
    type Response = crate::types::ProjectView;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_PATCH_V1_PROJECTS_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Create an API key; the full secret is shown once
/// `POST /v1/api-keys`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1ApiKeys {
    /// Request body.
    pub body: crate::types::CreateApiKeyRequest,
}

impl PostV1ApiKeys {
    /// Build the request.
    pub fn new(body: crate::types::CreateApiKeyRequest) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1ApiKeys {
    type Response = crate::types::CreateApiKeyResponse;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_API_KEYS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Verify credentials and issue a signed JWT
/// `POST /v1/auth/login`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1AuthLogin {
    /// Request body.
    pub body: crate::types::JsonValue,
}

impl PostV1AuthLogin {
    /// Build the request.
    pub fn new(body: crate::types::JsonValue) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1AuthLogin {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_AUTH_LOGIN;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Register a new player and return the public profile
/// `POST /v1/auth/register`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1AuthRegister {
    /// Request body.
    pub body: crate::types::JsonValue,
}

impl PostV1AuthRegister {
    /// Build the request.
    pub fn new(body: crate::types::JsonValue) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1AuthRegister {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_AUTH_REGISTER;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Post a message to a room
/// `POST /v1/chatrooms/{room}/messages`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1ChatroomsRoomMessages {
    /// Path parameter `room`.
    pub room: String,
    /// Request body.
    pub body: crate::types::SendMessageBody,
}

impl PostV1ChatroomsRoomMessages {
    /// Build the request.
    pub fn new(room: impl Into<String>, body: crate::types::SendMessageBody) -> Self {
        Self {
            room: room.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1ChatroomsRoomMessages {
    type Response = crate::types::MessageView;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_POST_V1_CHATROOMS_ROOM_MESSAGES;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("room", self.room.as_str())])
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Create a provider connection from a credential reference
/// `POST /v1/connections`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1Connections {
    /// Request body.
    pub body: crate::types::CreateConnectionRequest,
}

impl PostV1Connections {
    /// Build the request.
    pub fn new(body: crate::types::CreateConnectionRequest) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1Connections {
    type Response = crate::types::ConnectionView;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_CONNECTIONS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Ingest a batch of analytics events (fire-and-forget)
/// `POST /v1/events`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1Events {
    /// Request body.
    pub body: crate::types::JsonValue,
}

impl PostV1Events {
    /// Build the request.
    pub fn new(body: crate::types::JsonValue) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1Events {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_EVENTS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Submit player feedback
/// `POST /v1/feedback`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1Feedback {
    /// Request body.
    pub body: crate::types::JsonValue,
}

impl PostV1Feedback {
    /// Build the request.
    pub fn new(body: crate::types::JsonValue) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1Feedback {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_FEEDBACK;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Register a host; the slug is derived from the hostname
/// `POST /v1/hosts`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1Hosts {
    /// Request body.
    pub body: crate::types::CreateHostRequest,
}

impl PostV1Hosts {
    /// Build the request.
    pub fn new(body: crate::types::CreateHostRequest) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1Hosts {
    type Response = crate::types::HostView;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_HOSTS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Exchange an enrollment token for a host relationship credential
/// `POST /v1/hosts/enroll`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1HostsEnroll {
    /// Request body.
    pub body: crate::types::EnrollHostRequest,
}

impl PostV1HostsEnroll {
    /// Build the request.
    pub fn new(body: crate::types::EnrollHostRequest) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1HostsEnroll {
    type Response = crate::types::HostView;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_HOSTS_ENROLL;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Mint a ten-minute single-use host enrollment token
/// `POST /v1/hosts/enrollment-tokens`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1HostsEnrollmentTokens {
    /// Request body.
    pub body: crate::types::MintEnrollmentTokenRequest,
}

impl PostV1HostsEnrollmentTokens {
    /// Build the request.
    pub fn new(body: crate::types::MintEnrollmentTokenRequest) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1HostsEnrollmentTokens {
    type Response = crate::types::MintEnrollmentTokenResponse;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_POST_V1_HOSTS_ENROLLMENT_TOKENS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Host reports providers, models, modes and thinking options
/// `POST /v1/hosts/{id}/capabilities`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1HostsIdCapabilities {
    /// Path parameter `id`.
    pub id: String,
    /// Request body.
    pub body: crate::types::CapabilityReportRequest,
}

impl PostV1HostsIdCapabilities {
    /// Build the request.
    pub fn new(id: impl Into<String>, body: crate::types::CapabilityReportRequest) -> Self {
        Self {
            id: id.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1HostsIdCapabilities {
    type Response = crate::types::HostCapabilityReport;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_POST_V1_HOSTS_ID_CAPABILITIES;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Grant host.execute so the host may run agents
/// `POST /v1/hosts/{id}/execution`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1HostsIdExecution {
    /// Path parameter `id`.
    pub id: String,
    /// Request body.
    pub body: crate::types::JsonValue,
}

impl PostV1HostsIdExecution {
    /// Build the request.
    pub fn new(id: impl Into<String>, body: crate::types::JsonValue) -> Self {
        Self {
            id: id.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1HostsIdExecution {
    type Response = crate::types::HostView;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_POST_V1_HOSTS_ID_EXECUTION;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Create or replay a create-agent intent with a stable execution id
/// `POST /v1/hosts/{id}/executions`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1HostsIdExecutions {
    /// Path parameter `id`.
    pub id: String,
    /// Request body.
    pub body: crate::types::CreateExecutionRequest,
}

impl PostV1HostsIdExecutions {
    /// Build the request.
    pub fn new(id: impl Into<String>, body: crate::types::CreateExecutionRequest) -> Self {
        Self {
            id: id.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1HostsIdExecutions {
    type Response = crate::types::CreateExecutionResponse;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_POST_V1_HOSTS_ID_EXECUTIONS;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Submit a leaderboard entry for the authenticated player
/// `POST /v1/leaderboards/{game_id}/entries`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1LeaderboardsGameIdEntries {
    /// Path parameter `game_id`.
    pub game_id: String,
    /// Request body.
    pub body: crate::types::JsonValue,
}

impl PostV1LeaderboardsGameIdEntries {
    /// Build the request.
    pub fn new(game_id: impl Into<String>, body: crate::types::JsonValue) -> Self {
        Self {
            game_id: game_id.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1LeaderboardsGameIdEntries {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_POST_V1_LEADERBOARDS_GAME_ID_ENTRIES;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("game_id", self.game_id.as_str())])
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Create a subaccount under the authenticated player
/// `POST /v1/players/subaccount`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1PlayersSubaccount {
    /// Request body.
    pub body: crate::types::JsonValue,
}

impl PostV1PlayersSubaccount {
    /// Build the request.
    pub fn new(body: crate::types::JsonValue) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1PlayersSubaccount {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_POST_V1_PLAYERS_SUBACCOUNT;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Invite an operator by email; the token is shown once
/// `POST /v1/pool/invitations`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1PoolInvitations {
    /// Request body.
    pub body: crate::types::InviteRequest,
}

impl PostV1PoolInvitations {
    /// Build the request.
    pub fn new(body: crate::types::InviteRequest) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1PoolInvitations {
    type Response = crate::types::InviteResponse;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_POOL_INVITATIONS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Accept an invitation and create the member account
/// `POST /v1/pool/invitations/accept`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1PoolInvitationsAccept {
    /// Request body.
    pub body: crate::types::AcceptInvitationRequest,
}

impl PostV1PoolInvitationsAccept {
    /// Build the request.
    pub fn new(body: crate::types::AcceptInvitationRequest) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1PoolInvitationsAccept {
    type Response = crate::types::PoolLoginResponse;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_POST_V1_POOL_INVITATIONS_ACCEPT;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Sign an operator in and issue a JWT
/// `POST /v1/pool/login`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1PoolLogin {
    /// Request body.
    pub body: crate::types::PoolLoginRequest,
}

impl PostV1PoolLogin {
    /// Build the request.
    pub fn new(body: crate::types::PoolLoginRequest) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1PoolLogin {
    type Response = crate::types::PoolLoginResponse;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_POOL_LOGIN;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Sign out: revoke the presented operator token
/// `POST /v1/pool/logout`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1PoolLogout {
    /// Request body.
    pub body: crate::types::JsonValue,
}

impl PostV1PoolLogout {
    /// Build the request.
    pub fn new(body: crate::types::JsonValue) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1PoolLogout {
    type Response = ();
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_POOL_LOGOUT;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Replace the operator's password (clears first-sign-in requirement)
/// `POST /v1/pool/password`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1PoolPassword {
    /// Request body.
    pub body: crate::types::ChangePasswordRequest,
}

impl PostV1PoolPassword {
    /// Build the request.
    pub fn new(body: crate::types::ChangePasswordRequest) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1PoolPassword {
    type Response = ();
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_POOL_PASSWORD;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Create a project
/// `POST /v1/projects`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1Projects {
    /// Request body.
    pub body: crate::types::CreateProjectRequest,
}

impl PostV1Projects {
    /// Build the request.
    pub fn new(body: crate::types::CreateProjectRequest) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1Projects {
    type Response = crate::types::ProjectView;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_PROJECTS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Install a configuration bundle as a new immutable revision
/// `POST /v1/projects/{id}/configuration/install`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1ProjectsIdConfigurationInstall {
    /// Path parameter `id`.
    pub id: String,
    /// Request body.
    pub body: crate::types::ConfigurationRequest,
}

impl PostV1ProjectsIdConfigurationInstall {
    /// Build the request.
    pub fn new(id: impl Into<String>, body: crate::types::ConfigurationRequest) -> Self {
        Self {
            id: id.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1ProjectsIdConfigurationInstall {
    type Response = crate::types::InstallResponse;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_POST_V1_PROJECTS_ID_CONFIGURATION_INSTALL;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Validate a configuration bundle without writing pool state
/// `POST /v1/projects/{id}/configuration/validate`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1ProjectsIdConfigurationValidate {
    /// Path parameter `id`.
    pub id: String,
    /// Request body.
    pub body: crate::types::ConfigurationRequest,
}

impl PostV1ProjectsIdConfigurationValidate {
    /// Build the request.
    pub fn new(id: impl Into<String>, body: crate::types::ConfigurationRequest) -> Self {
        Self {
            id: id.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1ProjectsIdConfigurationValidate {
    type Response = crate::types::ValidateResponse;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_POST_V1_PROJECTS_ID_CONFIGURATION_VALIDATE;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Dispatch a workflow run from a manual request
/// `POST /v1/runs`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1Runs {
    /// Request body.
    pub body: crate::types::ManualDispatchRequest,
}

impl PostV1Runs {
    /// Build the request.
    pub fn new(body: crate::types::ManualDispatchRequest) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1Runs {
    type Response = crate::types::ManualDispatchResponse;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_RUNS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Create a game save
/// `POST /v1/saves`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1Saves {
    /// Request body.
    pub body: crate::types::JsonValue,
}

impl PostV1Saves {
    /// Build the request.
    pub fn new(body: crate::types::JsonValue) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1Saves {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_SAVES;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Publish one allow-listed session event
/// `POST /v1/sessions/{session}/relay/events`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1SessionsSessionRelayEvents {
    /// Path parameter `session`.
    pub session: String,
    /// Request body.
    pub body: crate::types::PublishBody,
}

impl PostV1SessionsSessionRelayEvents {
    /// Build the request.
    pub fn new(session: impl Into<String>, body: crate::types::PublishBody) -> Self {
        Self {
            session: session.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1SessionsSessionRelayEvents {
    type Response = crate::types::RelayEvent;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_POST_V1_SESSIONS_SESSION_RELAY_EVENTS;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("session", self.session.as_str())])
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Create the organization and first owner from the browser
/// `POST /v1/setup`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1Setup {
    /// Request body.
    pub body: crate::types::SetupRequest,
}

impl PostV1Setup {
    /// Build the request.
    pub fn new(body: crate::types::SetupRequest) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1Setup {
    type Response = crate::types::PoolIdentityView;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_SETUP;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Issue a one-shot WebSocket connection ticket
/// `POST /v1/socket-tickets`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1SocketTickets {
    /// Request body.
    pub body: crate::types::JsonValue,
}

impl PostV1SocketTickets {
    /// Build the request.
    pub fn new(body: crate::types::JsonValue) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1SocketTickets {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_SOCKET_TICKETS;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Install or replace a trigger; keeps its id and bumps the revision
/// `POST /v1/triggers/install`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1TriggersInstall {
    /// Request body.
    pub body: crate::types::TriggerInstallRequest,
}

impl PostV1TriggersInstall {
    /// Build the request.
    pub fn new(body: crate::types::TriggerInstallRequest) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1TriggersInstall {
    type Response = crate::types::TriggerInstallResponse;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_TRIGGERS_INSTALL;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Validate one trigger document without writing pool state
/// `POST /v1/triggers/validate`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1TriggersValidate {
    /// Request body.
    pub body: crate::types::TriggerDocument,
}

impl PostV1TriggersValidate {
    /// Build the request.
    pub fn new(body: crate::types::TriggerDocument) -> Self {
        Self {
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1TriggersValidate {
    type Response = crate::types::TriggerValidateResponse;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_POST_V1_TRIGGERS_VALIDATE;
    fn path(&self) -> String {
        Self::ROUTE.path.to_owned()
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Receive a GitHub App webhook; signature verified and delivery deduplicated
/// `POST /v1/webhooks/github/{slug}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1WebhooksGithubSlug {
    /// Path parameter `slug`.
    pub slug: String,
    /// Request body.
    pub body: crate::types::RawJson,
}

impl PostV1WebhooksGithubSlug {
    /// Build the request.
    pub fn new(slug: impl Into<String>, body: crate::types::RawJson) -> Self {
        Self {
            slug: slug.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1WebhooksGithubSlug {
    type Response = crate::types::WebhookResponse;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_POST_V1_WEBHOOKS_GITHUB_SLUG;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("slug", self.slug.as_str())])
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Receive a Slack Events API callback; signature verified and event deduplicated
/// `POST /v1/webhooks/slack/{slug}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostV1WebhooksSlackSlug {
    /// Path parameter `slug`.
    pub slug: String,
    /// Request body.
    pub body: crate::types::RawJson,
}

impl PostV1WebhooksSlackSlug {
    /// Build the request.
    pub fn new(slug: impl Into<String>, body: crate::types::RawJson) -> Self {
        Self {
            slug: slug.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PostV1WebhooksSlackSlug {
    type Response = PostV1WebhooksSlackSlugResponse;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_POST_V1_WEBHOOKS_SLACK_SLUG;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("slug", self.slug.as_str())])
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Set a game's settings (upsert)
/// `PUT /v1/games/{game_id}/settings`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PutV1GamesGameIdSettings {
    /// Path parameter `game_id`.
    pub game_id: String,
    /// Request body.
    pub body: crate::types::JsonValue,
}

impl PutV1GamesGameIdSettings {
    /// Build the request.
    pub fn new(game_id: impl Into<String>, body: crate::types::JsonValue) -> Self {
        Self {
            game_id: game_id.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PutV1GamesGameIdSettings {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_PUT_V1_GAMES_GAME_ID_SETTINGS;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("game_id", self.game_id.as_str())])
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Update the authenticated player's own leaderboard entry
/// `PUT /v1/leaderboards/{game_id}/entries/{player_id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PutV1LeaderboardsGameIdEntriesPlayerId {
    /// Path parameter `game_id`.
    pub game_id: String,
    /// Path parameter `player_id`.
    pub player_id: String,
    /// Request body.
    pub body: crate::types::JsonValue,
}

impl PutV1LeaderboardsGameIdEntriesPlayerId {
    /// Build the request.
    pub fn new(
        game_id: impl Into<String>,
        player_id: impl Into<String>,
        body: crate::types::JsonValue,
    ) -> Self {
        Self {
            game_id: game_id.into(),
            player_id: player_id.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PutV1LeaderboardsGameIdEntriesPlayerId {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_PUT_V1_LEADERBOARDS_GAME_ID_ENTRIES_PLAYER_ID;
    fn path(&self) -> String {
        crate::route::path_with(
            Self::ROUTE.path,
            &[
                ("game_id", self.game_id.as_str()),
                ("player_id", self.player_id.as_str()),
            ],
        )
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Update the authenticated player's own profile
/// `PUT /v1/players/{id}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PutV1PlayersId {
    /// Path parameter `id`.
    pub id: String,
    /// Request body.
    pub body: crate::types::JsonValue,
}

impl PutV1PlayersId {
    /// Build the request.
    pub fn new(id: impl Into<String>, body: crate::types::JsonValue) -> Self {
        Self {
            id: id.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PutV1PlayersId {
    type Response = crate::types::JsonObject;
    const ROUTE: &'static crate::route::RouteInfo = &crate::routes::ROUTE_PUT_V1_PLAYERS_ID;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("id", self.id.as_str())])
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

/// Opt a session into publication
/// `PUT /v1/sessions/{session}/relay`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PutV1SessionsSessionRelay {
    /// Path parameter `session`.
    pub session: String,
    /// Request body.
    pub body: crate::types::JsonValue,
}

impl PutV1SessionsSessionRelay {
    /// Build the request.
    pub fn new(session: impl Into<String>, body: crate::types::JsonValue) -> Self {
        Self {
            session: session.into(),
            body: body,
            ..Self::default()
        }
    }
}

impl Endpoint for PutV1SessionsSessionRelay {
    type Response = crate::types::RelayState;
    const ROUTE: &'static crate::route::RouteInfo =
        &crate::routes::ROUTE_PUT_V1_SESSIONS_SESSION_RELAY;
    fn path(&self) -> String {
        crate::route::path_with(Self::ROUTE.path, &[("session", self.session.as_str())])
    }
    fn body(&self) -> crate::Result<Option<Vec<u8>>> {
        Ok(Some(serde_json::to_vec(&self.body)?))
    }
}

#[cfg(test)]
mod typed {
    use super::*;

    fn assert_endpoint<E: Endpoint>() {}

    #[test]
    fn every_route_has_a_typed_endpoint() {
        assert_endpoint::<DeleteV1ApiKeysId>();
        assert_endpoint::<DeleteV1ConnectionsId>();
        assert_endpoint::<DeleteV1HostsId>();
        assert_endpoint::<DeleteV1HostsIdExecution>();
        assert_endpoint::<DeleteV1PlayersId>();
        assert_endpoint::<DeleteV1PoolInvitationsId>();
        assert_endpoint::<DeleteV1PoolMembersId>();
        assert_endpoint::<DeleteV1ProjectsId>();
        assert_endpoint::<DeleteV1SavesPlayerIdSaveId>();
        assert_endpoint::<DeleteV1SessionsSessionRelay>();
        assert_endpoint::<GetHealthz>();
        assert_endpoint::<GetMetrics>();
        assert_endpoint::<GetOpenapiJson>();
        assert_endpoint::<GetOpenapiV1Json>();
        assert_endpoint::<GetV1Activity>();
        assert_endpoint::<GetV1ActivityRunsRun>();
        assert_endpoint::<GetV1ApiKeys>();
        assert_endpoint::<GetV1ApiKeysWhoami>();
        assert_endpoint::<GetV1Chatrooms>();
        assert_endpoint::<GetV1ChatroomsRoomMembers>();
        assert_endpoint::<GetV1ChatroomsRoomMessages>();
        assert_endpoint::<GetV1ChatroomsRoomStream>();
        assert_endpoint::<GetV1Connections>();
        assert_endpoint::<GetV1ConnectionsId>();
        assert_endpoint::<GetV1GamesGameIdSettings>();
        assert_endpoint::<GetV1Hosts>();
        assert_endpoint::<GetV1HostsId>();
        assert_endpoint::<GetV1HostsIdCapabilities>();
        assert_endpoint::<GetV1LeaderboardsGameId>();
        assert_endpoint::<GetV1PlayersId>();
        assert_endpoint::<GetV1PlayersIdSubaccounts>();
        assert_endpoint::<GetV1PoolInvitations>();
        assert_endpoint::<GetV1PoolMe>();
        assert_endpoint::<GetV1PoolMembers>();
        assert_endpoint::<GetV1Projects>();
        assert_endpoint::<GetV1ProjectsId>();
        assert_endpoint::<GetV1ProjectsIdConfiguration>();
        assert_endpoint::<GetV1ProjectsIdRevisions>();
        assert_endpoint::<GetV1ProjectsIdRevisionsRevisionId>();
        assert_endpoint::<GetV1SavesPlayerId>();
        assert_endpoint::<GetV1SavesPlayerIdSaveId>();
        assert_endpoint::<GetV1SessionsSessionRelay>();
        assert_endpoint::<GetV1SessionsSessionRelayEvents>();
        assert_endpoint::<GetV1SetupStatus>();
        assert_endpoint::<PatchV1ConnectionsId>();
        assert_endpoint::<PatchV1HostsId>();
        assert_endpoint::<PatchV1PoolMembersId>();
        assert_endpoint::<PatchV1ProjectsId>();
        assert_endpoint::<PostV1ApiKeys>();
        assert_endpoint::<PostV1AuthLogin>();
        assert_endpoint::<PostV1AuthRegister>();
        assert_endpoint::<PostV1ChatroomsRoomMessages>();
        assert_endpoint::<PostV1Connections>();
        assert_endpoint::<PostV1Events>();
        assert_endpoint::<PostV1Feedback>();
        assert_endpoint::<PostV1Hosts>();
        assert_endpoint::<PostV1HostsEnroll>();
        assert_endpoint::<PostV1HostsEnrollmentTokens>();
        assert_endpoint::<PostV1HostsIdCapabilities>();
        assert_endpoint::<PostV1HostsIdExecution>();
        assert_endpoint::<PostV1HostsIdExecutions>();
        assert_endpoint::<PostV1LeaderboardsGameIdEntries>();
        assert_endpoint::<PostV1PlayersSubaccount>();
        assert_endpoint::<PostV1PoolInvitations>();
        assert_endpoint::<PostV1PoolInvitationsAccept>();
        assert_endpoint::<PostV1PoolLogin>();
        assert_endpoint::<PostV1PoolLogout>();
        assert_endpoint::<PostV1PoolPassword>();
        assert_endpoint::<PostV1Projects>();
        assert_endpoint::<PostV1ProjectsIdConfigurationInstall>();
        assert_endpoint::<PostV1ProjectsIdConfigurationValidate>();
        assert_endpoint::<PostV1Runs>();
        assert_endpoint::<PostV1Saves>();
        assert_endpoint::<PostV1SessionsSessionRelayEvents>();
        assert_endpoint::<PostV1Setup>();
        assert_endpoint::<PostV1SocketTickets>();
        assert_endpoint::<PostV1TriggersInstall>();
        assert_endpoint::<PostV1TriggersValidate>();
        assert_endpoint::<PostV1WebhooksGithubSlug>();
        assert_endpoint::<PostV1WebhooksSlackSlug>();
        assert_endpoint::<PutV1GamesGameIdSettings>();
        assert_endpoint::<PutV1LeaderboardsGameIdEntriesPlayerId>();
        assert_endpoint::<PutV1PlayersId>();
        assert_endpoint::<PutV1SessionsSessionRelay>();
    }
}
