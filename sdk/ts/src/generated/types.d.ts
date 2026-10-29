// Generated from docs/api/openapi-v1.json. Do not edit by hand.
// Regenerate: npm run generate (from sdk/ts).

/** Any JSON value. */
export type JsonValue = string | number | boolean | null | JsonValue[] | JsonObject;

/** A JSON object with opaque members. */
export interface JsonObject {
  [key: string]: JsonValue;
}

/** A query-string value. */
export type QueryValue = string | number | boolean | undefined;

/** Query-string values keyed by parameter name. */
export type QueryValues = Record<string, QueryValue>;

/** Stable hub problem codes (docs/HUB_API_ERRORS.md), plus unknown codes verbatim. */
export type ProblemCode = "unauthorized" | "forbidden" | "missing_scope" | "invalid_request" | "validation_failed" | "not_found" | "conflict" | "gone" | "method_not_allowed" | "rate_limited" | "daemon_not_connected" | "configuration_invalid" | "service_unavailable" | "internal_error" | (string & {});

/**
 * Accept an invitation and create the member account
 */
export interface AcceptInvitationRequest {
  display_name: string;
  /**
   * At least 12 characters
   */
  password: string;
  token: string;
}

/**
 * One receipt, run, step or execution in the activity feed
 */
export interface ActivityEntry {
  actor?: string | null;
  at: string;
  continuation?: string | null;
  duplicates?: number | null;
  error?: string | null;
  eventType?: string | null;
  execution?: string | null;
  id: string;
  kind: "receipt" | "run" | "step" | "execution" | (string & {});
  matched?: boolean | null;
  output?: string | null;
  project?: string | null;
  provider?: string | null;
  run?: string | null;
  status: string;
  step?: string | null;
  trigger?: string | null;
  triggerName?: string | null;
}

/**
 * One cursor-paged activity window
 */
export interface ActivityPage {
  hasMore: boolean;
  items: ActivityEntry[];
  nextCursor?: string | null;
}

/**
 * One run plus the arrival that started it
 */
export interface ActivityRunDetail {
  arrival: ArrivalDetail;
  run: RunView;
}

/**
 * One provider/model/mode/thinking combination a host can run
 */
export interface AgentCapability {
  execution_mode: string;
  model: string;
  options?: unknown;
  provider: string;
  thinking_option?: string | null;
}

/**
 * A named provider/model/mode choice
 */
export interface AgentSelection {
  execution_mode: string;
  model: string;
  provider: string;
  thinking_option?: string | null;
}

/**
 * API key metadata; the secret is never returned after creation
 */
export interface ApiKeyView {
  created_at: string;
  id: string;
  last_used_at?: string | null;
  name: string;
  prefix: string;
  revoked_at?: string | null;
  scopes: string[];
  status: "active" | "revoked" | (string & {});
}

/**
 * Organization API keys, newest first
 */
export interface ApiKeysResponse {
  api_keys: ApiKeyView[];
}

/**
 * How an arrival created, continued or restored an agent
 */
export interface ArrivalDetail {
  conversationKey?: string | null;
  mode: "created" | "continued" | "restored";
  outputLimits: StepOutputLimits[];
  provider?: string | null;
  receipt?: string | null;
  steered: boolean;
}

/**
 * A host's capability report
 */
export interface CapabilityReportRequest {
  capabilities: AgentCapability[];
}

/**
 * Slack URL-verification echo
 */
export interface ChallengeResponse {
  challenge: string;
}

/**
 * Replace the operator's password
 */
export interface ChangePasswordRequest {
  current_password: string;
  /**
   * At least 12 characters
   */
  new_password: string;
}

/**
 * One configuration file as authored
 */
export interface ConfigFileRequest {
  content: string;
  path: string;
}

/**
 * A configuration bundle (at most 100 files)
 */
export interface ConfigurationRequest {
  files: ConfigFileRequest[];
}

/**
 * One resource a connection may address
 */
export interface ConnectionResourceView {
  display_name?: string | null;
  external_id: string;
  resource_kind: string;
}

/**
 * One provider connection
 */
export interface ConnectionView {
  created_at: string;
  credential_configured: boolean;
  display_name: string;
  id: string;
  managed_by: "database" | "environment";
  provider: "github" | "slack" | "discord" | (string & {});
  resources: ConnectionResourceView[];
  slug: string;
  status: "active" | "disabled" | (string & {});
}

/**
 * Effective connections after the environment overlay
 */
export interface ConnectionsResponse {
  connections: ConnectionView[];
}

/**
 * Create an API key
 */
export interface CreateApiKeyRequest {
  name: string;
  /**
   * Subset of projects:read, configuration:validate, configuration:install, runs:dispatch, daemons:enroll
   */
  scopes: string[];
}

/**
 * Key metadata plus the one-time secret
 */
export interface CreateApiKeyResponse {
  api_key: ApiKeyView;
  /**
   * `<prefix>.<secret>`; shown once
   */
  secret: string;
}

/**
 * Create a connection from a credential reference
 */
export interface CreateConnectionRequest {
  /**
   * Environment variable name; the secret itself is never sent
   */
  credential_ref: string;
  display_name: string;
  provider: "github" | "slack" | "discord" | (string & {});
  resources?: ResourceRequest[];
  slug: string;
}

/**
 * Create or replay a create-agent intent
 */
export interface CreateExecutionRequest {
  /**
   * Stable id; the replay key
   */
  execution_id: string;
  prompt: string;
}

/**
 * The stored execution and whether the call replayed an existing one
 */
export interface CreateExecutionResponse {
  execution: HostExecutionView;
  replayed: boolean;
}

/**
 * Register a host
 */
export interface CreateHostRequest {
  display_name?: string | null;
  hostname: string;
}

/**
 * Create a project
 */
export interface CreateProjectRequest {
  name: string;
  slug?: string | null;
}

/**
 * Exchange an enrollment token for a host credential
 */
export interface EnrollHostRequest {
  /**
   * Host-generated relationship secret
   */
  credential: string;
  token: string;
}

/**
 * Enrollment token metadata (never the token)
 */
export interface EnrollmentTokenView {
  consumed_at?: string | null;
  created_at: string;
  expires_at: string;
  id: string;
  revoked_at?: string | null;
}

/**
 * One named validation issue
 */
export interface FieldIssue {
  field: string;
  message: string;
  revision: number;
}

/**
 * Error body used by the game surface (`{"error": "..."}`)
 */
export interface GameError {
  error: string;
}

/**
 * One page of chat history
 */
export interface HistoryResponse {
  messages: MessageView[];
  next?: number | null;
}

/**
 * The last stored capability report, plus what the pool may offer
 */
export interface HostCapabilityReport {
  capabilities: AgentCapability[];
  host_id: string;
  offered_providers: string[];
  reported_at?: string | null;
}

/**
 * One agent execution started on a host
 */
export interface HostExecutionView {
  completed_at?: string | null;
  id: string;
  output?: string | null;
  status: "queued" | "running" | "succeeded" | "failed" | "interrupted" | (string & {});
}

/**
 * One enrolled machine
 */
export interface HostView {
  created_at: string;
  display_name: string;
  execution_enabled: boolean;
  hostname: string;
  id: string;
  revoked_at?: string | null;
  slug: string;
  status: "pending_approval" | "connected" | "offline" | "revoked" | (string & {});
}

/**
 * Live hosts, ordered by slug
 */
export interface HostsResponse {
  hosts: HostView[];
}

/**
 * The installed revision and whether it became active
 */
export interface InstallResponse {
  activated: boolean;
  revision: RevisionView;
}

/**
 * Invitations still awaiting acceptance
 */
export interface InvitationsResponse {
  invitations: PoolInvitationView[];
}

/**
 * Invite an operator by email
 */
export interface InviteRequest {
  email: string;
  /**
   * Defaults to member
   */
  role: "owner" | "admin" | "member" | "viewer";
}

/**
 * Invitation metadata plus the one-time acceptance token
 */
export interface InviteResponse {
  invitation: PoolInvitationView;
  /**
   * Shown once; not stored in plaintext
   */
  token: string;
}

/**
 * A manual run request
 */
export interface ManualDispatchRequest {
  actor: string;
  /**
   * Stable per request; a repeat returns the existing run
   */
  deliveryKey: string;
  input?: string;
  project: string;
  revision?: string | null;
  trigger: string;
}

/**
 * The run a manual request created or matched
 */
export interface ManualDispatchResponse {
  /**
   * True on an idempotent replay
   */
  deduplicated: boolean;
  run: RunView;
}

/**
 * Operators in the organization
 */
export interface MembersResponse {
  members: PoolUserView[];
}

/**
 * One chat message
 */
export interface MessageView {
  author?: Principal;
  body: string;
  created_at: string;
  id: string;
  project: string;
  reply_to?: string | null;
  sequence: number;
}

/**
 * Mint a single-use enrollment token for one host
 */
export interface MintEnrollmentTokenRequest {
  host_id: string;
}

/**
 * The one-time enrollment token
 */
export interface MintEnrollmentTokenResponse {
  enrollment_token: EnrollmentTokenView;
  token: string;
}

/**
 * This OpenAPI 3.1 documents
 */
export interface OpenApiDocument {
  info: unknown;
  /**
   * OpenAPI version
   */
  openapi: string;
  paths: unknown;
}

/**
 * One organization (one pool in v1)
 */
export interface OrganizationView {
  created_at: string;
  id: string;
  name: string;
}

/**
 * One declared output capability limit
 */
export interface OutputLimit {
  capability: string;
  maximum: number;
  required: boolean;
}

/**
 * The signed-in operator and its organization
 */
export interface PoolIdentityView {
  organization: OrganizationView;
  user: PoolUserView;
}

/**
 * Pending or historical invitation metadata (never the token)
 */
export interface PoolInvitationView {
  accepted_at?: string | null;
  created_at: string;
  email: string;
  expires_at: string;
  id: string;
  revoked_at?: string | null;
  role: "owner" | "admin" | "member" | "viewer";
}

/**
 * Operator sign-in payload
 */
export interface PoolLoginRequest {
  email: string;
  password: string;
}

/**
 * A freshly issued operator JWT
 */
export interface PoolLoginResponse {
  /**
   * Token lifetime in seconds
   */
  expires_in: number;
  /**
   * True while the bootstrap password must be replaced
   */
  must_change_password: boolean;
  /**
   * Signed JWT; subject is the operator public id
   */
  token: string;
  user: PoolUserView;
}

/**
 * Operator account, never carrying credential material
 */
export interface PoolUserView {
  created_at: string;
  display_name: string;
  email: string;
  id: string;
  last_login_at?: string | null;
  password_change_required: boolean;
  role: "owner" | "admin" | "member" | "viewer";
}

/**
 * The single principal behind a room member or message author
 */
export interface Principal {
  id: string;
  kind: "user" | "session" | (string & {});
}

/**
 * RFC 9457 problem detail returned by every hub route on failure
 */
export interface Problem {
  /**
   * Stable machine-readable code (docs/HUB_API_ERRORS.md)
   */
  code: ProblemCode;
  /**
   * Human-readable explanation
   */
  detail?: string;
  /**
   * Correlation id, also echoed in x-request-id
   */
  requestId: string;
  status: number;
  /**
   * Short human-readable summary
   */
  title: string;
  /**
   * Problem type URI
   */
  type: string;
}

/**
 * One project
 */
export interface ProjectView {
  created_at: string;
  id: string;
  name: string;
  slug: string;
  updated_at: string;
}

/**
 * Projects in the organization
 */
export interface ProjectsResponse {
  projects: ProjectView[];
}

/**
 * Publish one relayed event
 */
export interface PublishBody {
  kind: "started" | "status" | "output" | "tool" | "message" | "stopped" | (string & {});
  /**
   * At most 8 KiB; credential-like keys are refused
   */
  payload: unknown;
}

/**
 * One relayed session event; never carries file contents
 */
export interface RelayEvent {
  createdAt: string;
  id: string;
  kind: "started" | "status" | "output" | "tool" | "message" | "stopped" | (string & {});
  payload: unknown;
  sequence: number;
  session: string;
}

/**
 * One page of relayed events
 */
export interface RelayPage {
  events: RelayEvent[];
  next?: number | null;
}

/**
 * A session's relay publication state (off by default)
 */
export interface RelayState {
  enabled: boolean;
  session: string;
  startedAt?: string | null;
  stoppedAt?: string | null;
}

/**
 * Rename a host; at least one field is required
 */
export interface RenameHostRequest {
  display_name?: string | null;
  slug?: string | null;
}

/**
 * Rename a project; the id is unchanged
 */
export interface RenameProjectRequest {
  name?: string | null;
  slug?: string | null;
}

/**
 * A resource a new connection may address
 */
export interface ResourceRequest {
  display_name?: string | null;
  external_id: string;
  resource_kind: string;
}

/**
 * One file of a stored revision
 */
export interface RevisionFileView {
  content: string;
  path: string;
}

/**
 * Revision metadata without file bodies
 */
export interface RevisionMetaView {
  active: boolean;
  created_at: string;
  error_count: number;
  id: string;
  status: "valid" | "invalid" | (string & {});
  version: number;
}

/**
 * One immutable configuration revision
 */
export interface RevisionView {
  active: boolean;
  created_at: string;
  errors: FieldIssue[];
  files: RevisionFileView[];
  id: string;
  status: "valid" | "invalid" | (string & {});
  version: number;
}

/**
 * A project's revisions, newest first
 */
export interface RevisionsResponse {
  revisions: RevisionMetaView[];
}

/**
 * One room member
 */
export interface RoomMemberView {
  id: string;
  joined_at: string;
  principal: Principal;
  role: "owner" | "member" | "viewer";
}

/**
 * A room's members
 */
export interface RoomMemberViewList {
}

/**
 * One chatroom
 */
export interface RoomView {
  created_at: string;
  id: string;
  name: string;
  origin_pool?: string | null;
  updated_at: string;
  visibility: "private" | "unlisted" | "public";
}

/**
 * Rooms visible to the caller
 */
export interface RoomViewList {
}

/**
 * One step of a workflow run
 */
export interface RunStepView {
  branch?: string | null;
  completedAt?: string | null;
  condition?: boolean | null;
  directory?: string | null;
  environment?: string | null;
  error?: string | null;
  executionId?: string | null;
  host?: string | null;
  id: string;
  output?: string | null;
  position: number;
  reason?: string | null;
  startedAt?: string | null;
  status: "pending" | "running" | "succeeded" | "failed" | "skipped" | "interrupted" | (string & {});
  step: string;
}

/**
 * One workflow run with its steps
 */
export interface RunView {
  actor?: string | null;
  completedAt?: string | null;
  createdAt: string;
  deadlineAt?: string | null;
  /**
   * Idempotency key for the arrival
   */
  deliveryKey: string;
  id: string;
  inputs: Record<string, string>;
  origin: "event" | "manual";
  project: string;
  requestText: string;
  startedAt?: string | null;
  status: "pending" | "running" | "succeeded" | "failed" | "interrupted" | (string & {});
  steps: RunStepView[];
  trigger: string;
  triggerName: string;
  workflow: string;
}

/**
 * Post a message to a room
 */
export interface SendMessageBody {
  body: string;
  replyTo?: string | null;
}

/**
 * First-run browser setup payload
 */
export interface SetupRequest {
  display_name: string;
  email: string;
  organization_name: string;
  /**
   * At least 12 characters
   */
  password: string;
}

/**
 * First-run setup state
 */
export interface SetupStatusResponse {
  bootstrap_configured: boolean;
  needs_setup: boolean;
}

/**
 * Output limits declared by one step
 */
export interface StepOutputLimits {
  limits: OutputLimit[];
  step: string;
}

/**
 * One authored trigger document
 */
export interface TriggerDocument {
  event: TriggerEvent;
  filters: TriggerFilters;
  name: string;
  run: TriggerRun;
}

/**
 * The provider event that may start a trigger
 */
export interface TriggerEvent {
  provider: "github" | "slack" | "discord" | (string & {});
  /**
   * Provider event type
   */
  type: string;
}

/**
 * Which deliveries of that event are allowed through
 */
export interface TriggerFilters {
  channel?: string | null;
  connection: string;
  contains?: string | null;
  /**
   * Required allowlist; an empty list is rejected
   */
  from_users: string[];
  label?: string | null;
  pattern?: string | null;
  resource: TriggerResourceFilter;
}

/**
 * A trigger document plus the project it belongs to
 */
export interface TriggerInstallRequest {
  event: TriggerEvent;
  filters: TriggerFilters;
  name: string;
  /**
   * Public id of the owning project
   */
  project: string;
  run: TriggerRun;
}

/**
 * The installed trigger and the revision the install produced
 */
export interface TriggerInstallResponse {
  active: boolean;
  /**
   * Immutable trigger id; a replacement keeps it
   */
  id: string;
  name: string;
  replaced: boolean;
  revision: string;
  version: number;
}

/**
 * One named trigger validation issue
 */
export interface TriggerIssue {
  field: string;
  message: string;
}

/**
 * The provider resource a trigger is scoped to
 */
export interface TriggerResourceFilter {
  id: string;
  kind: string;
}

/**
 * Where and with which agent a matched trigger runs
 */
export interface TriggerRun {
  agent: AgentSelection;
  host: string;
}

/**
 * Trigger validation result; nothing was written
 */
export interface TriggerValidateResponse {
  errors: TriggerIssue[];
  name: string;
  valid: boolean;
}

/**
 * Rename, enable or disable a database connection
 */
export interface UpdateConnectionRequest {
  display_name?: string | null;
  slug?: string | null;
  status?: string | null;
}

/**
 * Change a member's role
 */
export interface UpdateRoleRequest {
  role: "owner" | "admin" | "member" | "viewer";
}

/**
 * Validation result; nothing was written
 */
export interface ValidateResponse {
  errors: FieldIssue[];
  valid: boolean;
  version: number;
}

/**
 * A verified provider delivery
 */
export interface WebhookResponse {
  accepted: boolean;
  action?: string | null;
  duplicate: boolean;
  event: string;
  received: boolean;
}

/**
 * Any JSON value; provider-specific webhook payload delivered verbatim
 */
export type RawJson = JsonValue;
