// Generated from docs/api/openapi-v1.json. Do not edit by hand.
// Regenerate: npm run generate (from sdk/ts).

import type { ChatStream, RequestOptions } from "../stream.js";
import type {
  AcceptInvitationRequest,
  ActivityPage,
  ActivityRunDetail,
  ApiKeyView,
  ApiKeysResponse,
  CapabilityReportRequest,
  ChallengeResponse,
  ChangePasswordRequest,
  ConfigurationRequest,
  ConnectionView,
  ConnectionsResponse,
  CreateApiKeyRequest,
  CreateApiKeyResponse,
  CreateConnectionRequest,
  CreateExecutionRequest,
  CreateExecutionResponse,
  CreateHostRequest,
  CreateProjectRequest,
  EnrollHostRequest,
  HistoryResponse,
  HostCapabilityReport,
  HostView,
  HostsResponse,
  InstallResponse,
  InterruptExecutionResponse,
  InvitationsResponse,
  InviteRequest,
  InviteResponse,
  JsonObject,
  ManualDispatchRequest,
  ManualDispatchResponse,
  MembersResponse,
  MessageView,
  MintEnrollmentTokenRequest,
  MintEnrollmentTokenResponse,
  OpenApiDocument,
  PoolIdentityView,
  PoolLoginRequest,
  PoolLoginResponse,
  PoolUserView,
  ProjectView,
  ProjectsResponse,
  PublishBody,
  RawJson,
  RelayEvent,
  RelayPage,
  RelayState,
  RenameHostRequest,
  RenameProjectRequest,
  RevisionView,
  RevisionsResponse,
  RoomMemberViewList,
  RoomViewList,
  SendMessageBody,
  SetupRequest,
  SetupStatusResponse,
  TriggerDocument,
  TriggerInstallRequest,
  TriggerInstallResponse,
  TriggerValidateResponse,
  UpdateConnectionRequest,
  UpdateRoleRequest,
  ValidateResponse,
  WebhookResponse,
} from "./types.js";

/** One typed method per resource and command in the contract. */
export interface Resources {
  activity: {
    /**
     * List one page of receipts, runs, steps and executions
     */
    list(options?: RequestOptions<{ project?: string; provider?: string; trigger?: string; status?: string; kind?: string; cursor?: string; limit?: number; }>): Promise<ActivityPage>;
    /**
     * Fetch one run with the arrival record that started it
     */
    run(run: string, options?: RequestOptions): Promise<ActivityRunDetail>;
  };
  apiKeys: {
    /**
     * Create an API key; the full secret is shown once
     */
    create(body: CreateApiKeyRequest, options?: RequestOptions): Promise<CreateApiKeyResponse>;
    /**
     * List organization API keys (owner only; prefix only)
     */
    list(options?: RequestOptions): Promise<ApiKeysResponse>;
    /**
     * Revoke an API key (owner only; effective immediately)
     */
    remove(id: string, options?: RequestOptions): Promise<void>;
    /**
     * Introspect an API key (requires projects:read)
     */
    whoami(options?: RequestOptions): Promise<ApiKeyView>;
  };
  chatrooms: {
    /**
     * List rooms visible to the operator
     */
    list(options?: RequestOptions): Promise<RoomViewList>;
    /**
     * List a room's members
     */
    members(room: string, options?: RequestOptions): Promise<RoomMemberViewList>;
    /**
     * Read room history after a sequence cursor
     */
    messages(room: string, options?: RequestOptions<{ after?: number; limit?: number; }>): Promise<HistoryResponse>;
    /**
     * Post a message to a room
     */
    send(room: string, body: SendMessageBody, options?: RequestOptions): Promise<MessageView>;
    /**
     * Upgrade to the room's live message stream
     */
    stream(room: string, options?: RequestOptions<{ token?: string; after?: number; }>): ChatStream;
  };
  connections: {
    /**
     * Create a provider connection from a credential reference
     */
    create(body: CreateConnectionRequest, options?: RequestOptions): Promise<ConnectionView>;
    /**
     * Fetch a connection by id or env:<slug>
     */
    get(id: string, options?: RequestOptions): Promise<ConnectionView>;
    /**
     * List effective provider connections (environment overlay applied)
     */
    list(options?: RequestOptions): Promise<ConnectionsResponse>;
    /**
     * Delete a database connection
     */
    remove(id: string, options?: RequestOptions): Promise<void>;
    /**
     * Rename, enable or disable a database connection
     */
    update(id: string, body: UpdateConnectionRequest, options?: RequestOptions): Promise<ConnectionView>;
  };
  game: {
    auth: {
      /**
       * Verify credentials and issue a signed JWT
       */
      login(body: JsonObject, options?: RequestOptions): Promise<JsonObject>;
      /**
       * Register a new player and return the public profile
       */
      register(body: JsonObject, options?: RequestOptions): Promise<JsonObject>;
    };
    events: {
      /**
       * Ingest a batch of analytics events (fire-and-forget)
       */
      create(body: JsonObject, options?: RequestOptions): Promise<JsonObject>;
    };
    feedback: {
      /**
       * Submit player feedback
       */
      create(body: JsonObject, options?: RequestOptions): Promise<JsonObject>;
    };
    games: {
      /**
       * Fetch a game's settings
       */
      settings(game_id: string, options?: RequestOptions): Promise<JsonObject>;
      /**
       * Set a game's settings (upsert)
       */
      updateSettings(game_id: string, body: JsonObject, options?: RequestOptions): Promise<JsonObject>;
    };
    leaderboards: {
      /**
       * Fetch one ranked page of a leaderboard
       */
      get(game_id: string, options?: RequestOptions<{ page?: number; per_page?: number; }>): Promise<JsonObject>;
      /**
       * Update the authenticated player's own leaderboard entry
       */
      putEntry(params: { game_id: string; player_id: string; }, body: JsonObject, options?: RequestOptions): Promise<JsonObject>;
      /**
       * Submit a leaderboard entry for the authenticated player
       */
      submitEntry(game_id: string, body: JsonObject, options?: RequestOptions): Promise<JsonObject>;
    };
    players: {
      /**
       * Create a subaccount under the authenticated player
       */
      createSubaccount(body: JsonObject, options?: RequestOptions): Promise<JsonObject>;
      /**
       * Fetch a player's public profile
       */
      get(id: string, options?: RequestOptions): Promise<JsonObject>;
      /**
       * Soft-delete the authenticated player's own account
       */
      remove(id: string, options?: RequestOptions): Promise<void>;
      /**
       * List the authenticated player's subaccounts
       */
      subaccounts(id: string, options?: RequestOptions): Promise<JsonObject>;
      /**
       * Update the authenticated player's own profile
       */
      update(id: string, body: JsonObject, options?: RequestOptions): Promise<JsonObject>;
    };
    saves: {
      /**
       * Create a game save
       */
      create(body: JsonObject, options?: RequestOptions): Promise<JsonObject>;
      /**
       * Fetch a single game save
       */
      get(params: { player_id: string; save_id: string; }, options?: RequestOptions): Promise<JsonObject>;
      /**
       * List the authenticated player's saves, newest first
       */
      list(player_id: string, options?: RequestOptions): Promise<JsonObject>;
      /**
       * Delete a single game save
       */
      remove(params: { player_id: string; save_id: string; }, options?: RequestOptions): Promise<void>;
    };
    socketTickets: {
      /**
       * Issue a one-shot WebSocket connection ticket
       */
      create(body: JsonObject, options?: RequestOptions): Promise<JsonObject>;
    };
  };
  hosts: {
    /**
     * Read the host's last capability report and offered providers
     */
    capabilities(id: string, options?: RequestOptions): Promise<HostCapabilityReport>;
    /**
     * Register a host; the slug is derived from the hostname
     */
    create(body: CreateHostRequest, options?: RequestOptions): Promise<HostView>;
    /**
     * Create or replay a create-agent intent with a stable execution id
     */
    createExecution(id: string, body: CreateExecutionRequest, options?: RequestOptions): Promise<CreateExecutionResponse>;
    /**
     * Exchange an enrollment token for a host relationship credential
     */
    enroll(body: EnrollHostRequest, options?: RequestOptions): Promise<HostView>;
    /**
     * Fetch a live host by immutable id
     */
    get(id: string, options?: RequestOptions): Promise<HostView>;
    /**
     * Send interrupt_execution to the host running an execution
     */
    interruptExecution(params: { id: string; execution_id: string; }, body: JsonObject, options?: RequestOptions): Promise<InterruptExecutionResponse>;
    /**
     * List live hosts in the pool
     */
    list(options?: RequestOptions): Promise<HostsResponse>;
    /**
     * Mint a ten-minute single-use host enrollment token
     */
    mintEnrollmentToken(body: MintEnrollmentTokenRequest, options?: RequestOptions): Promise<MintEnrollmentTokenResponse>;
    /**
     * Revoke the pool relationship for a host
     */
    remove(id: string, options?: RequestOptions): Promise<void>;
    /**
     * Rename a host slug or display name; the id is unchanged
     */
    rename(id: string, body: RenameHostRequest, options?: RequestOptions): Promise<HostView>;
    /**
     * Host reports providers, models, modes and thinking options
     */
    reportCapabilities(id: string, body: CapabilityReportRequest, options?: RequestOptions): Promise<HostCapabilityReport>;
    /**
     * Grant host.execute so the host may run agents
     */
    startExecution(id: string, body: JsonObject, options?: RequestOptions): Promise<HostView>;
    /**
     * Revoke host.execute; the host becomes connected-only
     */
    stopExecution(id: string, options?: RequestOptions): Promise<HostView>;
  };
  pool: {
    /**
     * Accept an invitation and create the member account
     */
    acceptInvitation(body: AcceptInvitationRequest, options?: RequestOptions): Promise<PoolLoginResponse>;
    /**
     * Replace the operator's password (clears first-sign-in requirement)
     */
    changePassword(body: ChangePasswordRequest, options?: RequestOptions): Promise<void>;
    /**
     * List pending team invitations (owner only)
     */
    invitations(options?: RequestOptions): Promise<InvitationsResponse>;
    /**
     * Invite an operator by email; the token is shown once
     */
    invite(body: InviteRequest, options?: RequestOptions): Promise<InviteResponse>;
    /**
     * Sign an operator in and issue a JWT
     */
    login(body: PoolLoginRequest, options?: RequestOptions): Promise<PoolLoginResponse>;
    /**
     * Sign out: revoke the presented operator token
     */
    logout(body: JsonObject, options?: RequestOptions): Promise<void>;
    /**
     * Fetch the signed-in operator and organization
     */
    me(options?: RequestOptions): Promise<PoolIdentityView>;
    /**
     * List the organization's operators
     */
    members(options?: RequestOptions): Promise<MembersResponse>;
    /**
     * Remove a member (owner only; last owner protected)
     */
    removeMember(id: string, options?: RequestOptions): Promise<void>;
    /**
     * Revoke a pending invitation (owner only)
     */
    revokeInvitation(id: string, options?: RequestOptions): Promise<void>;
    /**
     * Change a member's role (owner only; last owner protected)
     */
    updateMember(id: string, body: UpdateRoleRequest, options?: RequestOptions): Promise<PoolUserView>;
  };
  projects: {
    /**
     * Fetch a project's active configuration revision
     */
    configuration(id: string, options?: RequestOptions): Promise<RevisionView>;
    /**
     * Create a project
     */
    create(body: CreateProjectRequest, options?: RequestOptions): Promise<ProjectView>;
    /**
     * Fetch a project by id
     */
    get(id: string, options?: RequestOptions): Promise<ProjectView>;
    /**
     * Install a configuration bundle as a new immutable revision
     */
    installConfiguration(id: string, body: ConfigurationRequest, options?: RequestOptions): Promise<InstallResponse>;
    /**
     * List projects in the organization
     */
    list(options?: RequestOptions): Promise<ProjectsResponse>;
    /**
     * Delete a project and its revisions
     */
    remove(id: string, options?: RequestOptions): Promise<void>;
    /**
     * Rename a project slug or name; the id is unchanged
     */
    rename(id: string, body: RenameProjectRequest, options?: RequestOptions): Promise<ProjectView>;
    /**
     * Fetch one configuration revision with its files
     */
    revision(params: { id: string; revision_id: string; }, options?: RequestOptions): Promise<RevisionView>;
    /**
     * List a project's configuration revisions
     */
    revisions(id: string, options?: RequestOptions): Promise<RevisionsResponse>;
    /**
     * Validate a configuration bundle without writing pool state
     */
    validateConfiguration(id: string, body: ConfigurationRequest, options?: RequestOptions): Promise<ValidateResponse>;
  };
  relay: {
    /**
     * Opt a session into publication
     */
    enable(session: string, body: JsonObject, options?: RequestOptions): Promise<RelayState>;
    /**
     * Backfill relayed events after a sequence cursor
     */
    events(session: string, options?: RequestOptions<{ after?: number; limit?: number; }>): Promise<RelayPage>;
    /**
     * Read a session's relay publication state
     */
    get(session: string, options?: RequestOptions): Promise<RelayState>;
    /**
     * Publish one allow-listed session event
     */
    publish(session: string, body: PublishBody, options?: RequestOptions): Promise<RelayEvent>;
    /**
     * Stop publication immediately and permanently
     */
    stop(session: string, options?: RequestOptions): Promise<RelayState>;
  };
  runs: {
    /**
     * Dispatch a workflow run from a manual request
     */
    dispatch(body: ManualDispatchRequest, options?: RequestOptions): Promise<ManualDispatchResponse>;
  };
  setup: {
    /**
     * Create the organization and first owner from the browser
     */
    create(body: SetupRequest, options?: RequestOptions): Promise<PoolIdentityView>;
    /**
     * Report whether the pool still needs first-run setup
     */
    status(options?: RequestOptions): Promise<SetupStatusResponse>;
  };
  system: {
    /**
     * Liveness probe with DB status
     */
    health(options?: RequestOptions): Promise<JsonObject>;
    /**
     * Prometheus text-format metrics
     */
    metrics(options?: RequestOptions): Promise<void>;
    /**
     * This OpenAPI document
     */
    openapi(options?: RequestOptions): Promise<OpenApiDocument>;
    /**
     * This OpenAPI document at its pinned major-version path
     */
    openapiV1(options?: RequestOptions): Promise<OpenApiDocument>;
  };
  triggers: {
    /**
     * Install or replace a trigger; keeps its id and bumps the revision
     */
    install(body: TriggerInstallRequest, options?: RequestOptions): Promise<TriggerInstallResponse>;
    /**
     * Validate one trigger document without writing pool state
     */
    validate(body: TriggerDocument, options?: RequestOptions): Promise<TriggerValidateResponse>;
  };
  webhooks: {
    /**
     * Receive a GitHub App webhook; signature verified and delivery deduplicated
     */
    github(slug: string, body: RawJson, options?: RequestOptions): Promise<WebhookResponse>;
    /**
     * Receive a Slack Events API callback; signature verified and event deduplicated
     */
    slack(slug: string, body: RawJson, options?: RequestOptions): Promise<ChallengeResponse | WebhookResponse>;
  };
}

/** Bind every typed method to one client instance. */
export declare function buildResources(client: import("../client.js").TidepoolClient): Resources;
