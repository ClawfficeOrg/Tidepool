// Generated from docs/api/openapi-v1.json. Do not edit by hand.
// Regenerate: node scripts/generate.mjs (from sdk/rust).

//! Generated contract types: one struct per schema in the document.

#![allow(clippy::struct_field_names)]

use serde::{Deserialize, Serialize};

/// Any JSON value.
pub type JsonValue = serde_json::Value;

/// A JSON object with members this SDK does not name.
pub type JsonObject = serde_json::Map<String, JsonValue>;

/// A JSON array whose members this SDK does not name.
pub type JsonArray = Vec<JsonValue>;

/// The stable problem codes the contract documents, plus any future code.
pub mod problem_code {
    /// `unauthorized`
    pub const UNAUTHORIZED: &str = "unauthorized";
    /// `forbidden`
    pub const FORBIDDEN: &str = "forbidden";
    /// `missing_scope`
    pub const MISSING_SCOPE: &str = "missing_scope";
    /// `invalid_request`
    pub const INVALID_REQUEST: &str = "invalid_request";
    /// `validation_failed`
    pub const VALIDATION_FAILED: &str = "validation_failed";
    /// `not_found`
    pub const NOT_FOUND: &str = "not_found";
    /// `conflict`
    pub const CONFLICT: &str = "conflict";
    /// `gone`
    pub const GONE: &str = "gone";
    /// `method_not_allowed`
    pub const METHOD_NOT_ALLOWED: &str = "method_not_allowed";
    /// `rate_limited`
    pub const RATE_LIMITED: &str = "rate_limited";
    /// `daemon_not_connected`
    pub const DAEMON_NOT_CONNECTED: &str = "daemon_not_connected";
    /// `configuration_invalid`
    pub const CONFIGURATION_INVALID: &str = "configuration_invalid";
    /// `service_unavailable`
    pub const SERVICE_UNAVAILABLE: &str = "service_unavailable";
    /// `internal_error`
    pub const INTERNAL_ERROR: &str = "internal_error";
}

/// Accept an invitation and create the member account
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AcceptInvitationRequest {
    /// `display_name`
    pub display_name: String,
    /// At least 12 characters
    pub password: String,
    /// `token`
    pub token: String,
}

/// One receipt, run, step or execution in the activity feed
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ActivityEntry {
    /// `actor`
    pub actor: Option<String>,
    /// `at`
    pub at: String,
    /// `continuation`
    pub continuation: Option<String>,
    /// `duplicates`
    pub duplicates: Option<i64>,
    /// `error`
    pub error: Option<String>,
    /// `eventType`
    #[serde(rename = "eventType")]
    pub event_type: Option<String>,
    /// `execution`
    pub execution: Option<String>,
    /// `id`
    pub id: String,
    /// Known values: "receipt", "run", "step", "execution". Unknown values are tolerated (contract §3.2).
    pub kind: String,
    /// `matched`
    pub matched: Option<bool>,
    /// `output`
    pub output: Option<String>,
    /// `project`
    pub project: Option<String>,
    /// `provider`
    pub provider: Option<String>,
    /// `run`
    pub run: Option<String>,
    /// `status`
    pub status: String,
    /// `step`
    pub step: Option<String>,
    /// `trigger`
    pub trigger: Option<String>,
    /// `triggerName`
    #[serde(rename = "triggerName")]
    pub trigger_name: Option<String>,
}

/// One cursor-paged activity window
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ActivityPage {
    /// `hasMore`
    #[serde(rename = "hasMore")]
    pub has_more: bool,
    /// `items`
    pub items: Vec<ActivityEntry>,
    /// `nextCursor`
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
}

/// One run plus the arrival that started it
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ActivityRunDetail {
    /// `arrival`
    pub arrival: ArrivalDetail,
    /// `run`
    pub run: RunView,
}

/// One provider/model/mode/thinking combination a host can run
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentCapability {
    /// `execution_mode`
    pub execution_mode: String,
    /// `model`
    pub model: String,
    /// `options`
    pub options: Option<JsonValue>,
    /// `provider`
    pub provider: String,
    /// `thinking_option`
    pub thinking_option: Option<String>,
}

/// A named provider/model/mode choice
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentSelection {
    /// `execution_mode`
    pub execution_mode: String,
    /// `model`
    pub model: String,
    /// `provider`
    pub provider: String,
    /// `thinking_option`
    pub thinking_option: Option<String>,
}

/// API key metadata; the secret is never returned after creation
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ApiKeyView {
    /// `created_at`
    pub created_at: String,
    /// `id`
    pub id: String,
    /// `last_used_at`
    pub last_used_at: Option<String>,
    /// `name`
    pub name: String,
    /// `prefix`
    pub prefix: String,
    /// `revoked_at`
    pub revoked_at: Option<String>,
    /// `scopes`
    pub scopes: Vec<String>,
    /// Known values: "active", "revoked". Unknown values are tolerated (contract §3.2).
    pub status: String,
}

/// Organization API keys, newest first
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ApiKeysResponse {
    /// `api_keys`
    pub api_keys: Vec<ApiKeyView>,
}

/// How an arrival created, continued or restored an agent
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ArrivalDetail {
    /// `conversationKey`
    #[serde(rename = "conversationKey")]
    pub conversation_key: Option<String>,
    /// Known values: "created", "continued", "restored". Unknown values are tolerated (contract §3.2).
    pub mode: String,
    /// `outputLimits`
    #[serde(rename = "outputLimits")]
    pub output_limits: Vec<StepOutputLimits>,
    /// `provider`
    pub provider: Option<String>,
    /// `receipt`
    pub receipt: Option<String>,
    /// `steered`
    pub steered: bool,
}

/// A host's capability report
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CapabilityReportRequest {
    /// `capabilities`
    pub capabilities: Vec<AgentCapability>,
}

/// Slack URL-verification echo
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChallengeResponse {
    /// `challenge`
    pub challenge: String,
}

/// Replace the operator's password
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChangePasswordRequest {
    /// `current_password`
    pub current_password: String,
    /// At least 12 characters
    pub new_password: String,
}

/// One configuration file as authored
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfigFileRequest {
    /// `content`
    pub content: String,
    /// `path`
    pub path: String,
}

/// A configuration bundle (at most 100 files)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfigurationRequest {
    /// `files`
    pub files: Vec<ConfigFileRequest>,
}

/// One resource a connection may address
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConnectionResourceView {
    /// `display_name`
    pub display_name: Option<String>,
    /// `external_id`
    pub external_id: String,
    /// `resource_kind`
    pub resource_kind: String,
}

/// One provider connection
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConnectionView {
    /// `created_at`
    pub created_at: String,
    /// `credential_configured`
    pub credential_configured: bool,
    /// `display_name`
    pub display_name: String,
    /// `id`
    pub id: String,
    /// Known values: "database", "environment". Unknown values are tolerated (contract §3.2).
    pub managed_by: String,
    /// Known values: "github", "slack", "discord". Unknown values are tolerated (contract §3.2).
    pub provider: String,
    /// `resources`
    pub resources: Vec<ConnectionResourceView>,
    /// `slug`
    pub slug: String,
    /// Known values: "active", "disabled". Unknown values are tolerated (contract §3.2).
    pub status: String,
}

/// Effective connections after the environment overlay
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConnectionsResponse {
    /// `connections`
    pub connections: Vec<ConnectionView>,
}

/// Create an API key
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CreateApiKeyRequest {
    /// `name`
    pub name: String,
    /// Subset of projects:read, configuration:validate, configuration:install, runs:dispatch, daemons:enroll
    pub scopes: Vec<String>,
}

/// Key metadata plus the one-time secret
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CreateApiKeyResponse {
    /// `api_key`
    pub api_key: ApiKeyView,
    /// `<prefix>.<secret>`; shown once
    pub secret: String,
}

/// Create a connection from a credential reference
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CreateConnectionRequest {
    /// Environment variable name; the secret itself is never sent
    pub credential_ref: String,
    /// `display_name`
    pub display_name: String,
    /// Known values: "github", "slack", "discord". Unknown values are tolerated (contract §3.2).
    pub provider: String,
    /// `resources`
    pub resources: Option<Vec<ResourceRequest>>,
    /// `slug`
    pub slug: String,
}

/// Create or replay a create-agent intent
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CreateExecutionRequest {
    /// Stable id; the replay key
    pub execution_id: String,
    /// `prompt`
    pub prompt: String,
}

/// The stored execution and whether the call replayed an existing one
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CreateExecutionResponse {
    /// `execution`
    pub execution: HostExecutionView,
    /// `replayed`
    pub replayed: bool,
}

/// Register a host
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CreateHostRequest {
    /// `display_name`
    pub display_name: Option<String>,
    /// `hostname`
    pub hostname: String,
}

/// Create a project
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    /// `name`
    pub name: String,
    /// `slug`
    pub slug: Option<String>,
}

/// Exchange an enrollment token for a host credential
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EnrollHostRequest {
    /// Host-generated relationship secret
    pub credential: String,
    /// `token`
    pub token: String,
}

/// Enrollment token metadata (never the token)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EnrollmentTokenView {
    /// `consumed_at`
    pub consumed_at: Option<String>,
    /// `created_at`
    pub created_at: String,
    /// `expires_at`
    pub expires_at: String,
    /// `id`
    pub id: String,
    /// `revoked_at`
    pub revoked_at: Option<String>,
}

/// One named validation issue
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FieldIssue {
    /// `field`
    pub field: String,
    /// `message`
    pub message: String,
    /// `revision`
    pub revision: i64,
}

/// Error body used by the game surface (`{"error": "..."}`)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GameError {
    /// `error`
    pub error: String,
}

/// One page of chat history
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HistoryResponse {
    /// `messages`
    pub messages: Vec<MessageView>,
    /// `next`
    pub next: Option<i64>,
}

/// The last stored capability report, plus what the pool may offer
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HostCapabilityReport {
    /// `capabilities`
    pub capabilities: Vec<AgentCapability>,
    /// `host_id`
    pub host_id: String,
    /// `offered_providers`
    pub offered_providers: Vec<String>,
    /// `reported_at`
    pub reported_at: Option<String>,
}

/// One agent execution started on a host
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HostExecutionView {
    /// `completed_at`
    pub completed_at: Option<String>,
    /// `id`
    pub id: String,
    /// `output`
    pub output: Option<String>,
    /// Known values: "queued", "running", "succeeded", "failed", "interrupted". Unknown values are tolerated (contract §3.2).
    pub status: String,
}

/// One enrolled machine
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HostView {
    /// `created_at`
    pub created_at: String,
    /// `display_name`
    pub display_name: String,
    /// `execution_enabled`
    pub execution_enabled: bool,
    /// `hostname`
    pub hostname: String,
    /// `id`
    pub id: String,
    /// `revoked_at`
    pub revoked_at: Option<String>,
    /// `slug`
    pub slug: String,
    /// Known values: "pending_approval", "connected", "offline", "revoked". Unknown values are tolerated (contract §3.2).
    pub status: String,
}

/// Live hosts, ordered by slug
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HostsResponse {
    /// `hosts`
    pub hosts: Vec<HostView>,
}

/// The installed revision and whether it became active
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InstallResponse {
    /// `activated`
    pub activated: bool,
    /// `revision`
    pub revision: RevisionView,
}

/// The execution as recorded when the interrupt was handled
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InterruptExecutionResponse {
    /// `execution`
    pub execution: HostExecutionView,
    /// True when interrupt_execution was sent; false when the execution was already terminal
    pub sent: bool,
}

/// Invitations still awaiting acceptance
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InvitationsResponse {
    /// `invitations`
    pub invitations: Vec<PoolInvitationView>,
}

/// Invite an operator by email
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InviteRequest {
    /// `email`
    pub email: String,
    /// Defaults to member Known values: "owner", "admin", "member", "viewer". Unknown values are tolerated (contract §3.2).
    pub role: String,
}

/// Invitation metadata plus the one-time acceptance token
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InviteResponse {
    /// `invitation`
    pub invitation: PoolInvitationView,
    /// Shown once; not stored in plaintext
    pub token: String,
}

/// A manual run request
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ManualDispatchRequest {
    /// `actor`
    pub actor: String,
    /// Stable per request; a repeat returns the existing run
    #[serde(rename = "deliveryKey")]
    pub delivery_key: String,
    /// `input`
    pub input: Option<String>,
    /// `project`
    pub project: String,
    /// `revision`
    pub revision: Option<String>,
    /// `trigger`
    pub trigger: String,
}

/// The run a manual request created or matched
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ManualDispatchResponse {
    /// True on an idempotent replay
    pub deduplicated: bool,
    /// `run`
    pub run: RunView,
}

/// Operators in the organization
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MembersResponse {
    /// `members`
    pub members: Vec<PoolUserView>,
}

/// One chat message
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MessageView {
    /// `author`
    pub author: Option<Principal>,
    /// `body`
    pub body: String,
    /// `created_at`
    pub created_at: String,
    /// `id`
    pub id: String,
    /// `project`
    pub project: String,
    /// `reply_to`
    pub reply_to: Option<String>,
    /// `sequence`
    pub sequence: i64,
}

/// Mint a single-use enrollment token for one host
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MintEnrollmentTokenRequest {
    /// `host_id`
    pub host_id: String,
}

/// The one-time enrollment token
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MintEnrollmentTokenResponse {
    /// `enrollment_token`
    pub enrollment_token: EnrollmentTokenView,
    /// `token`
    pub token: String,
}

/// This OpenAPI 3.1 documents
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OpenApiDocument {
    /// `info`
    pub info: JsonValue,
    /// OpenAPI version
    pub openapi: String,
    /// `paths`
    pub paths: JsonValue,
}

/// One organization (one pool in v1)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OrganizationView {
    /// `created_at`
    pub created_at: String,
    /// `id`
    pub id: String,
    /// `name`
    pub name: String,
}

/// One declared output capability limit
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OutputLimit {
    /// `capability`
    pub capability: String,
    /// `maximum`
    pub maximum: i64,
    /// `required`
    pub required: bool,
}

/// The signed-in operator and its organization
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PoolIdentityView {
    /// `organization`
    pub organization: OrganizationView,
    /// `user`
    pub user: PoolUserView,
}

/// Pending or historical invitation metadata (never the token)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PoolInvitationView {
    /// `accepted_at`
    pub accepted_at: Option<String>,
    /// `created_at`
    pub created_at: String,
    /// `email`
    pub email: String,
    /// `expires_at`
    pub expires_at: String,
    /// `id`
    pub id: String,
    /// `revoked_at`
    pub revoked_at: Option<String>,
    /// Known values: "owner", "admin", "member", "viewer". Unknown values are tolerated (contract §3.2).
    pub role: String,
}

/// Operator sign-in payload
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PoolLoginRequest {
    /// `email`
    pub email: String,
    /// `password`
    pub password: String,
}

/// A freshly issued operator JWT
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PoolLoginResponse {
    /// Token lifetime in seconds
    pub expires_in: i64,
    /// True while the bootstrap password must be replaced
    pub must_change_password: bool,
    /// Signed JWT; subject is the operator public id
    pub token: String,
    /// `user`
    pub user: PoolUserView,
}

/// Operator account, never carrying credential material
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PoolUserView {
    /// `created_at`
    pub created_at: String,
    /// `display_name`
    pub display_name: String,
    /// `email`
    pub email: String,
    /// `id`
    pub id: String,
    /// `last_login_at`
    pub last_login_at: Option<String>,
    /// `password_change_required`
    pub password_change_required: bool,
    /// Known values: "owner", "admin", "member", "viewer". Unknown values are tolerated (contract §3.2).
    pub role: String,
}

/// The single principal behind a room member or message author
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Principal {
    /// `id`
    pub id: String,
    /// Known values: "user", "session". Unknown values are tolerated (contract §3.2).
    pub kind: String,
}

/// RFC 9457 problem detail returned by every hub route on failure
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Problem {
    /// Stable machine-readable code (docs/HUB_API_ERRORS.md) Known values: "unauthorized", "forbidden", "missing_scope", "invalid_request", "validation_failed", "not_found", "conflict", "gone", "method_not_allowed", "rate_limited", "daemon_not_connected", "configuration_invalid", "service_unavailable", "internal_error". Unknown values are tolerated (contract §3.2).
    pub code: String,
    /// Human-readable explanation
    pub detail: Option<String>,
    /// Correlation id, also echoed in x-request-id
    #[serde(rename = "requestId")]
    pub request_id: Option<String>,
    /// `status`
    pub status: Option<i64>,
    /// Short human-readable summary
    pub title: Option<String>,
    /// Problem type URI
    #[serde(rename = "type")]
    pub problem_type: Option<String>,
}

/// One project
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectView {
    /// `created_at`
    pub created_at: String,
    /// `id`
    pub id: String,
    /// `name`
    pub name: String,
    /// `slug`
    pub slug: String,
    /// `updated_at`
    pub updated_at: String,
}

/// Projects in the organization
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectsResponse {
    /// `projects`
    pub projects: Vec<ProjectView>,
}

/// Publish one relayed event
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PublishBody {
    /// Known values: "started", "status", "output", "tool", "message", "stopped". Unknown values are tolerated (contract §3.2).
    pub kind: String,
    /// At most 8 KiB; credential-like keys are refused
    pub payload: JsonValue,
}

/// Any JSON value; provider-specific webhook payload delivered verbatim
pub type RawJson = JsonValue;

/// One relayed session event; never carries file contents
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RelayEvent {
    /// `createdAt`
    #[serde(rename = "createdAt")]
    pub created_at: String,
    /// `id`
    pub id: String,
    /// Known values: "started", "status", "output", "tool", "message", "stopped". Unknown values are tolerated (contract §3.2).
    pub kind: String,
    /// `payload`
    pub payload: JsonValue,
    /// `sequence`
    pub sequence: i64,
    /// `session`
    pub session: String,
}

/// One page of relayed events
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RelayPage {
    /// `events`
    pub events: Vec<RelayEvent>,
    /// `next`
    pub next: Option<i64>,
}

/// A session's relay publication state (off by default)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RelayState {
    /// `enabled`
    pub enabled: bool,
    /// `session`
    pub session: String,
    /// `startedAt`
    #[serde(rename = "startedAt")]
    pub started_at: Option<String>,
    /// `stoppedAt`
    #[serde(rename = "stoppedAt")]
    pub stopped_at: Option<String>,
}

/// Rename a host; at least one field is required
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RenameHostRequest {
    /// `display_name`
    pub display_name: Option<String>,
    /// `slug`
    pub slug: Option<String>,
}

/// Rename a project; the id is unchanged
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RenameProjectRequest {
    /// `name`
    pub name: Option<String>,
    /// `slug`
    pub slug: Option<String>,
}

/// A resource a new connection may address
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResourceRequest {
    /// `display_name`
    pub display_name: Option<String>,
    /// `external_id`
    pub external_id: String,
    /// `resource_kind`
    pub resource_kind: String,
}

/// One file of a stored revision
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RevisionFileView {
    /// `content`
    pub content: String,
    /// `path`
    pub path: String,
}

/// Revision metadata without file bodies
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RevisionMetaView {
    /// `active`
    pub active: bool,
    /// `created_at`
    pub created_at: String,
    /// `error_count`
    pub error_count: i64,
    /// `id`
    pub id: String,
    /// Known values: "valid", "invalid". Unknown values are tolerated (contract §3.2).
    pub status: String,
    /// `version`
    pub version: i64,
}

/// One immutable configuration revision
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RevisionView {
    /// `active`
    pub active: bool,
    /// `created_at`
    pub created_at: String,
    /// `errors`
    pub errors: Vec<FieldIssue>,
    /// `files`
    pub files: Vec<RevisionFileView>,
    /// `id`
    pub id: String,
    /// Known values: "valid", "invalid". Unknown values are tolerated (contract §3.2).
    pub status: String,
    /// `version`
    pub version: i64,
}

/// A project's revisions, newest first
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RevisionsResponse {
    /// `revisions`
    pub revisions: Vec<RevisionMetaView>,
}

/// One room member
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RoomMemberView {
    /// `id`
    pub id: String,
    /// `joined_at`
    pub joined_at: String,
    /// `principal`
    pub principal: Principal,
    /// Known values: "owner", "member", "viewer". Unknown values are tolerated (contract §3.2).
    pub role: String,
}

/// A room's members
pub type RoomMemberViewList = Vec<RoomMemberView>;

/// One chatroom
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RoomView {
    /// `created_at`
    pub created_at: String,
    /// `id`
    pub id: String,
    /// `name`
    pub name: String,
    /// `origin_pool`
    pub origin_pool: Option<String>,
    /// `updated_at`
    pub updated_at: String,
    /// Known values: "private", "unlisted", "public". Unknown values are tolerated (contract §3.2).
    pub visibility: String,
}

/// Rooms visible to the caller
pub type RoomViewList = Vec<RoomView>;

/// One step of a workflow run
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RunStepView {
    /// `branch`
    pub branch: Option<String>,
    /// `completedAt`
    #[serde(rename = "completedAt")]
    pub completed_at: Option<String>,
    /// `condition`
    pub condition: Option<bool>,
    /// `directory`
    pub directory: Option<String>,
    /// `environment`
    pub environment: Option<String>,
    /// `error`
    pub error: Option<String>,
    /// `executionId`
    #[serde(rename = "executionId")]
    pub execution_id: Option<String>,
    /// `host`
    pub host: Option<String>,
    /// `id`
    pub id: String,
    /// `output`
    pub output: Option<String>,
    /// `position`
    pub position: i64,
    /// `reason`
    pub reason: Option<String>,
    /// `startedAt`
    #[serde(rename = "startedAt")]
    pub started_at: Option<String>,
    /// Known values: "pending", "running", "succeeded", "failed", "skipped", "interrupted". Unknown values are tolerated (contract §3.2).
    pub status: String,
    /// `step`
    pub step: String,
}

/// One workflow run with its steps
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RunView {
    /// `actor`
    pub actor: Option<String>,
    /// `completedAt`
    #[serde(rename = "completedAt")]
    pub completed_at: Option<String>,
    /// `createdAt`
    #[serde(rename = "createdAt")]
    pub created_at: String,
    /// `deadlineAt`
    #[serde(rename = "deadlineAt")]
    pub deadline_at: Option<String>,
    /// Idempotency key for the arrival
    #[serde(rename = "deliveryKey")]
    pub delivery_key: String,
    /// `id`
    pub id: String,
    /// `inputs`
    pub inputs: std::collections::HashMap<String, String>,
    /// Known values: "event", "manual". Unknown values are tolerated (contract §3.2).
    pub origin: String,
    /// `project`
    pub project: String,
    /// `requestText`
    #[serde(rename = "requestText")]
    pub request_text: String,
    /// `startedAt`
    #[serde(rename = "startedAt")]
    pub started_at: Option<String>,
    /// Known values: "pending", "running", "succeeded", "failed", "interrupted". Unknown values are tolerated (contract §3.2).
    pub status: String,
    /// `steps`
    pub steps: Vec<RunStepView>,
    /// `trigger`
    pub trigger: String,
    /// `triggerName`
    #[serde(rename = "triggerName")]
    pub trigger_name: String,
    /// `workflow`
    pub workflow: String,
}

/// Post a message to a room
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SendMessageBody {
    /// `body`
    pub body: String,
    /// `replyTo`
    #[serde(rename = "replyTo")]
    pub reply_to: Option<String>,
}

/// First-run browser setup payload
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SetupRequest {
    /// `display_name`
    pub display_name: String,
    /// `email`
    pub email: String,
    /// `organization_name`
    pub organization_name: String,
    /// At least 12 characters
    pub password: String,
}

/// First-run setup state
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SetupStatusResponse {
    /// `bootstrap_configured`
    pub bootstrap_configured: bool,
    /// `needs_setup`
    pub needs_setup: bool,
}

/// Output limits declared by one step
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StepOutputLimits {
    /// `limits`
    pub limits: Vec<OutputLimit>,
    /// `step`
    pub step: String,
}

/// One authored trigger document
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TriggerDocument {
    /// `event`
    pub event: TriggerEvent,
    /// `filters`
    pub filters: TriggerFilters,
    /// `name`
    pub name: String,
    /// `run`
    pub run: TriggerRun,
}

/// The provider event that may start a trigger
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TriggerEvent {
    /// Known values: "github", "slack", "discord". Unknown values are tolerated (contract §3.2).
    pub provider: String,
    /// Provider event type
    #[serde(rename = "type")]
    pub type_: String,
}

/// Which deliveries of that event are allowed through
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TriggerFilters {
    /// `channel`
    pub channel: Option<String>,
    /// `connection`
    pub connection: String,
    /// `contains`
    pub contains: Option<String>,
    /// Required allowlist; an empty list is rejected
    pub from_users: Vec<String>,
    /// `label`
    pub label: Option<String>,
    /// `pattern`
    pub pattern: Option<String>,
    /// `resource`
    pub resource: TriggerResourceFilter,
}

/// A trigger document plus the project it belongs to
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TriggerInstallRequest {
    /// `event`
    pub event: TriggerEvent,
    /// `filters`
    pub filters: TriggerFilters,
    /// `name`
    pub name: String,
    /// Public id of the owning project
    pub project: String,
    /// `run`
    pub run: TriggerRun,
}

/// The installed trigger and the revision the install produced
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TriggerInstallResponse {
    /// `active`
    pub active: bool,
    /// Immutable trigger id; a replacement keeps it
    pub id: String,
    /// `name`
    pub name: String,
    /// `replaced`
    pub replaced: bool,
    /// `revision`
    pub revision: String,
    /// `version`
    pub version: i64,
}

/// One named trigger validation issue
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TriggerIssue {
    /// `field`
    pub field: String,
    /// `message`
    pub message: String,
}

/// The provider resource a trigger is scoped to
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TriggerResourceFilter {
    /// `id`
    pub id: String,
    /// `kind`
    pub kind: String,
}

/// Where and with which agent a matched trigger runs
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TriggerRun {
    /// `agent`
    pub agent: AgentSelection,
    /// `host`
    pub host: String,
}

/// Trigger validation result; nothing was written
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TriggerValidateResponse {
    /// `errors`
    pub errors: Vec<TriggerIssue>,
    /// `name`
    pub name: String,
    /// `valid`
    pub valid: bool,
}

/// Rename, enable or disable a database connection
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UpdateConnectionRequest {
    /// `display_name`
    pub display_name: Option<String>,
    /// `slug`
    pub slug: Option<String>,
    /// `status`
    pub status: Option<String>,
}

/// Change a member's role
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UpdateRoleRequest {
    /// Known values: "owner", "admin", "member", "viewer". Unknown values are tolerated (contract §3.2).
    pub role: String,
}

/// Validation result; nothing was written
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ValidateResponse {
    /// `errors`
    pub errors: Vec<FieldIssue>,
    /// `valid`
    pub valid: bool,
    /// `version`
    pub version: i64,
}

/// A verified provider delivery
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WebhookResponse {
    /// `accepted`
    pub accepted: bool,
    /// `action`
    pub action: Option<String>,
    /// `duplicate`
    pub duplicate: bool,
    /// `event`
    pub event: String,
    /// `received`
    pub received: bool,
}
