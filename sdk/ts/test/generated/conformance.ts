// Generated from docs/api/openapi-v1.json. Do not edit by hand.
// Typecheck only: npm run typecheck. Not executed.

import type { ChatStream } from "../../src/stream.js";
import type { TidepoolClient } from "../../src/index.js";
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
} from "../../src/generated/types.js";

declare const client: TidepoolClient;

const delete_v1_api_keys_id: Promise<void> = client.apiKeys.remove("id-1", );
void delete_v1_api_keys_id;

const delete_v1_connections_id: Promise<void> = client.connections.remove("id-1", );
void delete_v1_connections_id;

const delete_v1_hosts_id: Promise<void> = client.hosts.remove("id-1", );
void delete_v1_hosts_id;

const delete_v1_hosts_id_execution: Promise<HostView> = client.hosts.stopExecution("id-1", );
void delete_v1_hosts_id_execution;

const delete_v1_players_id: Promise<void> = client.game.players.remove("id-1", );
void delete_v1_players_id;

const delete_v1_pool_invitations_id: Promise<void> = client.pool.revokeInvitation("id-1", );
void delete_v1_pool_invitations_id;

const delete_v1_pool_members_id: Promise<void> = client.pool.removeMember("id-1", );
void delete_v1_pool_members_id;

const delete_v1_projects_id: Promise<void> = client.projects.remove("id-1", );
void delete_v1_projects_id;

const delete_v1_saves_player_id_save_id: Promise<void> = client.game.saves.remove({} as { player_id: string; save_id: string; }, );
void delete_v1_saves_player_id_save_id;

const delete_v1_sessions_session_relay: Promise<RelayState> = client.relay.stop("id-1", );
void delete_v1_sessions_session_relay;

const get_healthz: Promise<JsonObject> = client.system.health();
void get_healthz;

const get_metrics: Promise<void> = client.system.metrics();
void get_metrics;

const get_openapi_json: Promise<OpenApiDocument> = client.system.openapi();
void get_openapi_json;

const get_openapi_v1_json: Promise<OpenApiDocument> = client.system.openapiV1();
void get_openapi_v1_json;

const get_v1_activity: Promise<ActivityPage> = client.activity.list();
void get_v1_activity;

const get_v1_activity_runs_run: Promise<ActivityRunDetail> = client.activity.run("id-1", );
void get_v1_activity_runs_run;

const get_v1_api_keys: Promise<ApiKeysResponse> = client.apiKeys.list();
void get_v1_api_keys;

const get_v1_api_keys_whoami: Promise<ApiKeyView> = client.apiKeys.whoami();
void get_v1_api_keys_whoami;

const get_v1_chatrooms: Promise<RoomViewList> = client.chatrooms.list();
void get_v1_chatrooms;

const get_v1_chatrooms_room_members: Promise<RoomMemberViewList> = client.chatrooms.members("id-1", );
void get_v1_chatrooms_room_members;

const get_v1_chatrooms_room_messages: Promise<HistoryResponse> = client.chatrooms.messages("id-1", );
void get_v1_chatrooms_room_messages;

const get_v1_chatrooms_room_stream: ChatStream = client.chatrooms.stream("id-1", );
void get_v1_chatrooms_room_stream;

const get_v1_connections: Promise<ConnectionsResponse> = client.connections.list();
void get_v1_connections;

const get_v1_connections_id: Promise<ConnectionView> = client.connections.get("id-1", );
void get_v1_connections_id;

const get_v1_games_game_id_settings: Promise<JsonObject> = client.game.games.settings("id-1", );
void get_v1_games_game_id_settings;

const get_v1_hosts: Promise<HostsResponse> = client.hosts.list();
void get_v1_hosts;

const get_v1_hosts_id: Promise<HostView> = client.hosts.get("id-1", );
void get_v1_hosts_id;

const get_v1_hosts_id_capabilities: Promise<HostCapabilityReport> = client.hosts.capabilities("id-1", );
void get_v1_hosts_id_capabilities;

const get_v1_leaderboards_game_id: Promise<JsonObject> = client.game.leaderboards.get("id-1", );
void get_v1_leaderboards_game_id;

const get_v1_players_id: Promise<JsonObject> = client.game.players.get("id-1", );
void get_v1_players_id;

const get_v1_players_id_subaccounts: Promise<JsonObject> = client.game.players.subaccounts("id-1", );
void get_v1_players_id_subaccounts;

const get_v1_pool_invitations: Promise<InvitationsResponse> = client.pool.invitations();
void get_v1_pool_invitations;

const get_v1_pool_me: Promise<PoolIdentityView> = client.pool.me();
void get_v1_pool_me;

const get_v1_pool_members: Promise<MembersResponse> = client.pool.members();
void get_v1_pool_members;

const get_v1_projects: Promise<ProjectsResponse> = client.projects.list();
void get_v1_projects;

const get_v1_projects_id: Promise<ProjectView> = client.projects.get("id-1", );
void get_v1_projects_id;

const get_v1_projects_id_configuration: Promise<RevisionView> = client.projects.configuration("id-1", );
void get_v1_projects_id_configuration;

const get_v1_projects_id_revisions: Promise<RevisionsResponse> = client.projects.revisions("id-1", );
void get_v1_projects_id_revisions;

const get_v1_projects_id_revisions_revision_id: Promise<RevisionView> = client.projects.revision({} as { id: string; revision_id: string; }, );
void get_v1_projects_id_revisions_revision_id;

const get_v1_saves_player_id: Promise<JsonObject> = client.game.saves.list("id-1", );
void get_v1_saves_player_id;

const get_v1_saves_player_id_save_id: Promise<JsonObject> = client.game.saves.get({} as { player_id: string; save_id: string; }, );
void get_v1_saves_player_id_save_id;

const get_v1_sessions_session_relay: Promise<RelayState> = client.relay.get("id-1", );
void get_v1_sessions_session_relay;

const get_v1_sessions_session_relay_events: Promise<RelayPage> = client.relay.events("id-1", );
void get_v1_sessions_session_relay_events;

const get_v1_setup_status: Promise<SetupStatusResponse> = client.setup.status();
void get_v1_setup_status;

const patch_v1_connections_id: Promise<ConnectionView> = client.connections.update("id-1", {} as UpdateConnectionRequest, );
void patch_v1_connections_id;
// @ts-expect-error the request body is typed by the contract
void client.connections.update("id-1", 42, )

const patch_v1_hosts_id: Promise<HostView> = client.hosts.rename("id-1", {} as RenameHostRequest, );
void patch_v1_hosts_id;
// @ts-expect-error the request body is typed by the contract
void client.hosts.rename("id-1", 42, )

const patch_v1_pool_members_id: Promise<PoolUserView> = client.pool.updateMember("id-1", {} as UpdateRoleRequest, );
void patch_v1_pool_members_id;
// @ts-expect-error the request body is typed by the contract
void client.pool.updateMember("id-1", 42, )

const patch_v1_projects_id: Promise<ProjectView> = client.projects.rename("id-1", {} as RenameProjectRequest, );
void patch_v1_projects_id;
// @ts-expect-error the request body is typed by the contract
void client.projects.rename("id-1", 42, )

const post_v1_api_keys: Promise<CreateApiKeyResponse> = client.apiKeys.create({} as CreateApiKeyRequest, );
void post_v1_api_keys;
// @ts-expect-error the request body is typed by the contract
void client.apiKeys.create(42, )

const post_v1_auth_login: Promise<JsonObject> = client.game.auth.login({} as JsonObject, );
void post_v1_auth_login;
// @ts-expect-error the request body is typed by the contract
void client.game.auth.login(42, )

const post_v1_auth_register: Promise<JsonObject> = client.game.auth.register({} as JsonObject, );
void post_v1_auth_register;
// @ts-expect-error the request body is typed by the contract
void client.game.auth.register(42, )

const post_v1_chatrooms_room_messages: Promise<MessageView> = client.chatrooms.send("id-1", {} as SendMessageBody, );
void post_v1_chatrooms_room_messages;
// @ts-expect-error the request body is typed by the contract
void client.chatrooms.send("id-1", 42, )

const post_v1_connections: Promise<ConnectionView> = client.connections.create({} as CreateConnectionRequest, );
void post_v1_connections;
// @ts-expect-error the request body is typed by the contract
void client.connections.create(42, )

const post_v1_events: Promise<JsonObject> = client.game.events.create({} as JsonObject, );
void post_v1_events;
// @ts-expect-error the request body is typed by the contract
void client.game.events.create(42, )

const post_v1_feedback: Promise<JsonObject> = client.game.feedback.create({} as JsonObject, );
void post_v1_feedback;
// @ts-expect-error the request body is typed by the contract
void client.game.feedback.create(42, )

const post_v1_hosts: Promise<HostView> = client.hosts.create({} as CreateHostRequest, );
void post_v1_hosts;
// @ts-expect-error the request body is typed by the contract
void client.hosts.create(42, )

const post_v1_hosts_enroll: Promise<HostView> = client.hosts.enroll({} as EnrollHostRequest, );
void post_v1_hosts_enroll;
// @ts-expect-error the request body is typed by the contract
void client.hosts.enroll(42, )

const post_v1_hosts_enrollment_tokens: Promise<MintEnrollmentTokenResponse> = client.hosts.mintEnrollmentToken({} as MintEnrollmentTokenRequest, );
void post_v1_hosts_enrollment_tokens;
// @ts-expect-error the request body is typed by the contract
void client.hosts.mintEnrollmentToken(42, )

const post_v1_hosts_id_capabilities: Promise<HostCapabilityReport> = client.hosts.reportCapabilities("id-1", {} as CapabilityReportRequest, );
void post_v1_hosts_id_capabilities;
// @ts-expect-error the request body is typed by the contract
void client.hosts.reportCapabilities("id-1", 42, )

const post_v1_hosts_id_execution: Promise<HostView> = client.hosts.startExecution("id-1", {} as JsonObject, );
void post_v1_hosts_id_execution;
// @ts-expect-error the request body is typed by the contract
void client.hosts.startExecution("id-1", 42, )

const post_v1_hosts_id_executions: Promise<CreateExecutionResponse> = client.hosts.createExecution("id-1", {} as CreateExecutionRequest, );
void post_v1_hosts_id_executions;
// @ts-expect-error the request body is typed by the contract
void client.hosts.createExecution("id-1", 42, )

const post_v1_leaderboards_game_id_entries: Promise<JsonObject> = client.game.leaderboards.submitEntry("id-1", {} as JsonObject, );
void post_v1_leaderboards_game_id_entries;
// @ts-expect-error the request body is typed by the contract
void client.game.leaderboards.submitEntry("id-1", 42, )

const post_v1_players_subaccount: Promise<JsonObject> = client.game.players.createSubaccount({} as JsonObject, );
void post_v1_players_subaccount;
// @ts-expect-error the request body is typed by the contract
void client.game.players.createSubaccount(42, )

const post_v1_pool_invitations: Promise<InviteResponse> = client.pool.invite({} as InviteRequest, );
void post_v1_pool_invitations;
// @ts-expect-error the request body is typed by the contract
void client.pool.invite(42, )

const post_v1_pool_invitations_accept: Promise<PoolLoginResponse> = client.pool.acceptInvitation({} as AcceptInvitationRequest, );
void post_v1_pool_invitations_accept;
// @ts-expect-error the request body is typed by the contract
void client.pool.acceptInvitation(42, )

const post_v1_pool_login: Promise<PoolLoginResponse> = client.pool.login({} as PoolLoginRequest, );
void post_v1_pool_login;
// @ts-expect-error the request body is typed by the contract
void client.pool.login(42, )

const post_v1_pool_logout: Promise<void> = client.pool.logout({} as JsonObject, );
void post_v1_pool_logout;
// @ts-expect-error the request body is typed by the contract
void client.pool.logout(42, )

const post_v1_pool_password: Promise<void> = client.pool.changePassword({} as ChangePasswordRequest, );
void post_v1_pool_password;
// @ts-expect-error the request body is typed by the contract
void client.pool.changePassword(42, )

const post_v1_projects: Promise<ProjectView> = client.projects.create({} as CreateProjectRequest, );
void post_v1_projects;
// @ts-expect-error the request body is typed by the contract
void client.projects.create(42, )

const post_v1_projects_id_configuration_install: Promise<InstallResponse> = client.projects.installConfiguration("id-1", {} as ConfigurationRequest, );
void post_v1_projects_id_configuration_install;
// @ts-expect-error the request body is typed by the contract
void client.projects.installConfiguration("id-1", 42, )

const post_v1_projects_id_configuration_validate: Promise<ValidateResponse> = client.projects.validateConfiguration("id-1", {} as ConfigurationRequest, );
void post_v1_projects_id_configuration_validate;
// @ts-expect-error the request body is typed by the contract
void client.projects.validateConfiguration("id-1", 42, )

const post_v1_runs: Promise<ManualDispatchResponse> = client.runs.dispatch({} as ManualDispatchRequest, );
void post_v1_runs;
// @ts-expect-error the request body is typed by the contract
void client.runs.dispatch(42, )

const post_v1_saves: Promise<JsonObject> = client.game.saves.create({} as JsonObject, );
void post_v1_saves;
// @ts-expect-error the request body is typed by the contract
void client.game.saves.create(42, )

const post_v1_sessions_session_relay_events: Promise<RelayEvent> = client.relay.publish("id-1", {} as PublishBody, );
void post_v1_sessions_session_relay_events;
// @ts-expect-error the request body is typed by the contract
void client.relay.publish("id-1", 42, )

const post_v1_setup: Promise<PoolIdentityView> = client.setup.create({} as SetupRequest, );
void post_v1_setup;
// @ts-expect-error the request body is typed by the contract
void client.setup.create(42, )

const post_v1_socket_tickets: Promise<JsonObject> = client.game.socketTickets.create({} as JsonObject, );
void post_v1_socket_tickets;
// @ts-expect-error the request body is typed by the contract
void client.game.socketTickets.create(42, )

const post_v1_triggers_install: Promise<TriggerInstallResponse> = client.triggers.install({} as TriggerInstallRequest, );
void post_v1_triggers_install;
// @ts-expect-error the request body is typed by the contract
void client.triggers.install(42, )

const post_v1_triggers_validate: Promise<TriggerValidateResponse> = client.triggers.validate({} as TriggerDocument, );
void post_v1_triggers_validate;
// @ts-expect-error the request body is typed by the contract
void client.triggers.validate(42, )

const post_v1_webhooks_github_slug: Promise<WebhookResponse> = client.webhooks.github("id-1", {} as RawJson, );
void post_v1_webhooks_github_slug;

const post_v1_webhooks_slack_slug: Promise<ChallengeResponse | WebhookResponse> = client.webhooks.slack("id-1", {} as RawJson, );
void post_v1_webhooks_slack_slug;

const put_v1_games_game_id_settings: Promise<JsonObject> = client.game.games.updateSettings("id-1", {} as JsonObject, );
void put_v1_games_game_id_settings;
// @ts-expect-error the request body is typed by the contract
void client.game.games.updateSettings("id-1", 42, )

const put_v1_leaderboards_game_id_entries_player_id: Promise<JsonObject> = client.game.leaderboards.putEntry({} as { game_id: string; player_id: string; }, {} as JsonObject, );
void put_v1_leaderboards_game_id_entries_player_id;
// @ts-expect-error the request body is typed by the contract
void client.game.leaderboards.putEntry("id-1", 42, )

const put_v1_players_id: Promise<JsonObject> = client.game.players.update("id-1", {} as JsonObject, );
void put_v1_players_id;
// @ts-expect-error the request body is typed by the contract
void client.game.players.update("id-1", 42, )

const put_v1_sessions_session_relay: Promise<RelayState> = client.relay.enable("id-1", {} as JsonObject, );
void put_v1_sessions_session_relay;
// @ts-expect-error the request body is typed by the contract
void client.relay.enable("id-1", 42, )
