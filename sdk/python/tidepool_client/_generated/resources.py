"""Generated from docs/api/openapi-v1.json. Do not edit by hand.

Regenerate: python sdk/python/scripts/generate.py
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Dict, Mapping, Optional, Union

if TYPE_CHECKING:
    from ..client import AsyncClient, CallOptions, SyncClient
    from ..stream import ChatStream
    from .models import AcceptInvitationRequest
    from .models import ActivityPage
    from .models import ActivityRunDetail
    from .models import ApiKeyView
    from .models import ApiKeysResponse
    from .models import CapabilityReportRequest
    from .models import ChallengeResponse
    from .models import ChangePasswordRequest
    from .models import ConfigurationRequest
    from .models import ConnectionView
    from .models import ConnectionsResponse
    from .models import CreateApiKeyRequest
    from .models import CreateApiKeyResponse
    from .models import CreateConnectionRequest
    from .models import CreateExecutionRequest
    from .models import CreateExecutionResponse
    from .models import CreateHostRequest
    from .models import CreateProjectRequest
    from .models import EnrollHostRequest
    from .models import HistoryResponse
    from .models import HostCapabilityReport
    from .models import HostView
    from .models import HostsResponse
    from .models import InstallResponse
    from .models import InvitationsResponse
    from .models import InviteRequest
    from .models import InviteResponse
    from .models import ManualDispatchRequest
    from .models import ManualDispatchResponse
    from .models import MembersResponse
    from .models import MessageView
    from .models import MintEnrollmentTokenRequest
    from .models import MintEnrollmentTokenResponse
    from .models import OpenApiDocument
    from .models import PoolIdentityView
    from .models import PoolLoginRequest
    from .models import PoolLoginResponse
    from .models import PoolUserView
    from .models import ProjectView
    from .models import ProjectsResponse
    from .models import PublishBody
    from .models import RawJson
    from .models import RelayEvent
    from .models import RelayPage
    from .models import RelayState
    from .models import RenameHostRequest
    from .models import RenameProjectRequest
    from .models import RevisionView
    from .models import RevisionsResponse
    from .models import RoomMemberViewList
    from .models import RoomViewList
    from .models import SendMessageBody
    from .models import SetupRequest
    from .models import SetupStatusResponse
    from .models import TriggerDocument
    from .models import TriggerInstallRequest
    from .models import TriggerInstallResponse
    from .models import TriggerValidateResponse
    from .models import UpdateConnectionRequest
    from .models import UpdateRoleRequest
    from .models import ValidateResponse
    from .models import WebhookResponse


class SyncResources:
    """Contract operations for the root namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client
        self.activity = SyncActivity(client)
        self.api_keys = SyncApiKeys(client)
        self.chatrooms = SyncChatrooms(client)
        self.connections = SyncConnections(client)
        self.game = SyncGame(client)
        self.hosts = SyncHosts(client)
        self.pool = SyncPool(client)
        self.projects = SyncProjects(client)
        self.relay = SyncRelay(client)
        self.runs = SyncRuns(client)
        self.setup = SyncSetup(client)
        self.system = SyncSystem(client)
        self.triggers = SyncTriggers(client)
        self.webhooks = SyncWebhooks(client)

class SyncActivity:
    """Contract operations for the activity namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def list(self, project: Optional[str] = None, provider: Optional[str] = None, trigger: Optional[str] = None, status: Optional[str] = None, kind: Optional[str] = None, cursor: Optional[str] = None, limit: Optional[int] = None, options: "CallOptions" = None) -> ActivityPage:
        """List one page of receipts, runs, steps and executions"""
        return self._client.call(
            'get_v1_activity',
            query={'project': project, 'provider': provider, 'trigger': trigger, 'status': status, 'kind': kind, 'cursor': cursor, 'limit': limit},
            options=options,
        )

    def run(self, run: str, options: "CallOptions" = None) -> ActivityRunDetail:
        """Fetch one run with the arrival record that started it"""
        return self._client.call(
            'get_v1_activity_runs_run',
            path_params={'run': run},
            options=options,
        )

class SyncApiKeys:
    """Contract operations for the api_keys namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def create(self, body: Union["CreateApiKeyRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> CreateApiKeyResponse:
        """Create an API key; the full secret is shown once"""
        return self._client.call(
            'post_v1_api_keys',
            body=body,
            options=options,
        )

    def list(self, options: "CallOptions" = None) -> ApiKeysResponse:
        """List organization API keys (owner only; prefix only)"""
        return self._client.call(
            'get_v1_api_keys',
            options=options,
        )

    def remove(self, id: str, options: "CallOptions" = None) -> None:
        """Revoke an API key (owner only; effective immediately)"""
        return self._client.call(
            'delete_v1_api_keys_id',
            path_params={'id': id},
            options=options,
        )

    def whoami(self, options: "CallOptions" = None) -> ApiKeyView:
        """Introspect an API key (requires projects:read)"""
        return self._client.call(
            'get_v1_api_keys_whoami',
            options=options,
        )

class SyncChatrooms:
    """Contract operations for the chatrooms namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def list(self, options: "CallOptions" = None) -> RoomViewList:
        """List rooms visible to the operator"""
        return self._client.call(
            'get_v1_chatrooms',
            options=options,
        )

    def members(self, room: str, options: "CallOptions" = None) -> RoomMemberViewList:
        """List a room's members"""
        return self._client.call(
            'get_v1_chatrooms_room_members',
            path_params={'room': room},
            options=options,
        )

    def messages(self, room: str, after: Optional[int] = None, limit: Optional[int] = None, options: "CallOptions" = None) -> HistoryResponse:
        """Read room history after a sequence cursor"""
        return self._client.call(
            'get_v1_chatrooms_room_messages',
            path_params={'room': room},
            query={'after': after, 'limit': limit},
            options=options,
        )

    def send(self, room: str, body: Union["SendMessageBody", Mapping[str, Any]] = None, options: "CallOptions" = None) -> MessageView:
        """Post a message to a room"""
        return self._client.call(
            'post_v1_chatrooms_room_messages',
            path_params={'room': room},
            body=body,
            options=options,
        )

    def stream(self, room: str, token: Optional[str] = None, after: Optional[int] = None) -> ChatStream:
        """Upgrade to the room's live message stream"""
        return self._client.stream(
            'get_v1_chatrooms_room_stream',
            path_params={'room': room},
            query={'token': token, 'after': after},
        )

class SyncConnections:
    """Contract operations for the connections namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def create(self, body: Union["CreateConnectionRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> ConnectionView:
        """Create a provider connection from a credential reference"""
        return self._client.call(
            'post_v1_connections',
            body=body,
            options=options,
        )

    def get(self, id: str, options: "CallOptions" = None) -> ConnectionView:
        """Fetch a connection by id or env:<slug>"""
        return self._client.call(
            'get_v1_connections_id',
            path_params={'id': id},
            options=options,
        )

    def list(self, options: "CallOptions" = None) -> ConnectionsResponse:
        """List effective provider connections (environment overlay applied)"""
        return self._client.call(
            'get_v1_connections',
            options=options,
        )

    def remove(self, id: str, options: "CallOptions" = None) -> None:
        """Delete a database connection"""
        return self._client.call(
            'delete_v1_connections_id',
            path_params={'id': id},
            options=options,
        )

    def update(self, id: str, body: Union["UpdateConnectionRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> ConnectionView:
        """Rename, enable or disable a database connection"""
        return self._client.call(
            'patch_v1_connections_id',
            path_params={'id': id},
            body=body,
            options=options,
        )

class SyncGame:
    """Contract operations for the game namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client
        self.auth = SyncGameAuth(client)
        self.events = SyncGameEvents(client)
        self.feedback = SyncGameFeedback(client)
        self.games = SyncGameGames(client)
        self.leaderboards = SyncGameLeaderboards(client)
        self.players = SyncGamePlayers(client)
        self.saves = SyncGameSaves(client)
        self.socket_tickets = SyncGameSocketTickets(client)

class SyncGameAuth:
    """Contract operations for the game.auth namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def login(self, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Verify credentials and issue a signed JWT"""
        return self._client.call(
            'post_v1_auth_login',
            body=body,
            options=options,
        )

    def register(self, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Register a new player and return the public profile"""
        return self._client.call(
            'post_v1_auth_register',
            body=body,
            options=options,
        )

class SyncGameEvents:
    """Contract operations for the game.events namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def create(self, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Ingest a batch of analytics events (fire-and-forget)"""
        return self._client.call(
            'post_v1_events',
            body=body,
            options=options,
        )

class SyncGameFeedback:
    """Contract operations for the game.feedback namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def create(self, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Submit player feedback"""
        return self._client.call(
            'post_v1_feedback',
            body=body,
            options=options,
        )

class SyncGameGames:
    """Contract operations for the game.games namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def settings(self, game_id: str, options: "CallOptions" = None) -> Dict[str, Any]:
        """Fetch a game's settings"""
        return self._client.call(
            'get_v1_games_game_id_settings',
            path_params={'game_id': game_id},
            options=options,
        )

    def update_settings(self, game_id: str, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Set a game's settings (upsert)"""
        return self._client.call(
            'put_v1_games_game_id_settings',
            path_params={'game_id': game_id},
            body=body,
            options=options,
        )

class SyncGameLeaderboards:
    """Contract operations for the game.leaderboards namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def get(self, game_id: str, page: Optional[int] = None, per_page: Optional[int] = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Fetch one ranked page of a leaderboard"""
        return self._client.call(
            'get_v1_leaderboards_game_id',
            path_params={'game_id': game_id},
            query={'page': page, 'per_page': per_page},
            options=options,
        )

    def put_entry(self, game_id: str, player_id: str, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Update the authenticated player's own leaderboard entry"""
        return self._client.call(
            'put_v1_leaderboards_game_id_entries_player_id',
            path_params={'game_id': game_id, 'player_id': player_id},
            body=body,
            options=options,
        )

    def submit_entry(self, game_id: str, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Submit a leaderboard entry for the authenticated player"""
        return self._client.call(
            'post_v1_leaderboards_game_id_entries',
            path_params={'game_id': game_id},
            body=body,
            options=options,
        )

class SyncGamePlayers:
    """Contract operations for the game.players namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def create_subaccount(self, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Create a subaccount under the authenticated player"""
        return self._client.call(
            'post_v1_players_subaccount',
            body=body,
            options=options,
        )

    def get(self, id: str, options: "CallOptions" = None) -> Dict[str, Any]:
        """Fetch a player's public profile"""
        return self._client.call(
            'get_v1_players_id',
            path_params={'id': id},
            options=options,
        )

    def remove(self, id: str, options: "CallOptions" = None) -> None:
        """Soft-delete the authenticated player's own account"""
        return self._client.call(
            'delete_v1_players_id',
            path_params={'id': id},
            options=options,
        )

    def subaccounts(self, id: str, options: "CallOptions" = None) -> Dict[str, Any]:
        """List the authenticated player's subaccounts"""
        return self._client.call(
            'get_v1_players_id_subaccounts',
            path_params={'id': id},
            options=options,
        )

    def update(self, id: str, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Update the authenticated player's own profile"""
        return self._client.call(
            'put_v1_players_id',
            path_params={'id': id},
            body=body,
            options=options,
        )

class SyncGameSaves:
    """Contract operations for the game.saves namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def create(self, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Create a game save"""
        return self._client.call(
            'post_v1_saves',
            body=body,
            options=options,
        )

    def get(self, player_id: str, save_id: str, options: "CallOptions" = None) -> Dict[str, Any]:
        """Fetch a single game save"""
        return self._client.call(
            'get_v1_saves_player_id_save_id',
            path_params={'player_id': player_id, 'save_id': save_id},
            options=options,
        )

    def list(self, player_id: str, options: "CallOptions" = None) -> Dict[str, Any]:
        """List the authenticated player's saves, newest first"""
        return self._client.call(
            'get_v1_saves_player_id',
            path_params={'player_id': player_id},
            options=options,
        )

    def remove(self, player_id: str, save_id: str, options: "CallOptions" = None) -> None:
        """Delete a single game save"""
        return self._client.call(
            'delete_v1_saves_player_id_save_id',
            path_params={'player_id': player_id, 'save_id': save_id},
            options=options,
        )

class SyncGameSocketTickets:
    """Contract operations for the game.socket_tickets namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def create(self, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Issue a one-shot WebSocket connection ticket"""
        return self._client.call(
            'post_v1_socket_tickets',
            body=body,
            options=options,
        )

class SyncHosts:
    """Contract operations for the hosts namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def capabilities(self, id: str, options: "CallOptions" = None) -> HostCapabilityReport:
        """Read the host's last capability report and offered providers"""
        return self._client.call(
            'get_v1_hosts_id_capabilities',
            path_params={'id': id},
            options=options,
        )

    def create(self, body: Union["CreateHostRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> HostView:
        """Register a host; the slug is derived from the hostname"""
        return self._client.call(
            'post_v1_hosts',
            body=body,
            options=options,
        )

    def create_execution(self, id: str, body: Union["CreateExecutionRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> CreateExecutionResponse:
        """Create or replay a create-agent intent with a stable execution id"""
        return self._client.call(
            'post_v1_hosts_id_executions',
            path_params={'id': id},
            body=body,
            options=options,
        )

    def enroll(self, body: Union["EnrollHostRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> HostView:
        """Exchange an enrollment token for a host relationship credential"""
        return self._client.call(
            'post_v1_hosts_enroll',
            body=body,
            options=options,
        )

    def get(self, id: str, options: "CallOptions" = None) -> HostView:
        """Fetch a live host by immutable id"""
        return self._client.call(
            'get_v1_hosts_id',
            path_params={'id': id},
            options=options,
        )

    def list(self, options: "CallOptions" = None) -> HostsResponse:
        """List live hosts in the pool"""
        return self._client.call(
            'get_v1_hosts',
            options=options,
        )

    def mint_enrollment_token(self, body: Union["MintEnrollmentTokenRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> MintEnrollmentTokenResponse:
        """Mint a ten-minute single-use host enrollment token"""
        return self._client.call(
            'post_v1_hosts_enrollment_tokens',
            body=body,
            options=options,
        )

    def remove(self, id: str, options: "CallOptions" = None) -> None:
        """Revoke the pool relationship for a host"""
        return self._client.call(
            'delete_v1_hosts_id',
            path_params={'id': id},
            options=options,
        )

    def rename(self, id: str, body: Union["RenameHostRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> HostView:
        """Rename a host slug or display name; the id is unchanged"""
        return self._client.call(
            'patch_v1_hosts_id',
            path_params={'id': id},
            body=body,
            options=options,
        )

    def report_capabilities(self, id: str, body: Union["CapabilityReportRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> HostCapabilityReport:
        """Host reports providers, models, modes and thinking options"""
        return self._client.call(
            'post_v1_hosts_id_capabilities',
            path_params={'id': id},
            body=body,
            options=options,
        )

    def start_execution(self, id: str, body: Any = None, options: "CallOptions" = None) -> HostView:
        """Grant host.execute so the host may run agents"""
        return self._client.call(
            'post_v1_hosts_id_execution',
            path_params={'id': id},
            body=body,
            options=options,
        )

    def stop_execution(self, id: str, options: "CallOptions" = None) -> HostView:
        """Revoke host.execute; the host becomes connected-only"""
        return self._client.call(
            'delete_v1_hosts_id_execution',
            path_params={'id': id},
            options=options,
        )

class SyncPool:
    """Contract operations for the pool namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def accept_invitation(self, body: Union["AcceptInvitationRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> PoolLoginResponse:
        """Accept an invitation and create the member account"""
        return self._client.call(
            'post_v1_pool_invitations_accept',
            body=body,
            options=options,
        )

    def change_password(self, body: Union["ChangePasswordRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> None:
        """Replace the operator's password (clears first-sign-in requirement)"""
        return self._client.call(
            'post_v1_pool_password',
            body=body,
            options=options,
        )

    def invitations(self, options: "CallOptions" = None) -> InvitationsResponse:
        """List pending team invitations (owner only)"""
        return self._client.call(
            'get_v1_pool_invitations',
            options=options,
        )

    def invite(self, body: Union["InviteRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> InviteResponse:
        """Invite an operator by email; the token is shown once"""
        return self._client.call(
            'post_v1_pool_invitations',
            body=body,
            options=options,
        )

    def login(self, body: Union["PoolLoginRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> PoolLoginResponse:
        """Sign an operator in and issue a JWT"""
        return self._client.call(
            'post_v1_pool_login',
            body=body,
            options=options,
        )

    def me(self, options: "CallOptions" = None) -> PoolIdentityView:
        """Fetch the signed-in operator and organization"""
        return self._client.call(
            'get_v1_pool_me',
            options=options,
        )

    def members(self, options: "CallOptions" = None) -> MembersResponse:
        """List the organization's operators"""
        return self._client.call(
            'get_v1_pool_members',
            options=options,
        )

    def remove_member(self, id: str, options: "CallOptions" = None) -> None:
        """Remove a member (owner only; last owner protected)"""
        return self._client.call(
            'delete_v1_pool_members_id',
            path_params={'id': id},
            options=options,
        )

    def revoke_invitation(self, id: str, options: "CallOptions" = None) -> None:
        """Revoke a pending invitation (owner only)"""
        return self._client.call(
            'delete_v1_pool_invitations_id',
            path_params={'id': id},
            options=options,
        )

    def update_member(self, id: str, body: Union["UpdateRoleRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> PoolUserView:
        """Change a member's role (owner only; last owner protected)"""
        return self._client.call(
            'patch_v1_pool_members_id',
            path_params={'id': id},
            body=body,
            options=options,
        )

class SyncProjects:
    """Contract operations for the projects namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def configuration(self, id: str, options: "CallOptions" = None) -> RevisionView:
        """Fetch a project's active configuration revision"""
        return self._client.call(
            'get_v1_projects_id_configuration',
            path_params={'id': id},
            options=options,
        )

    def create(self, body: Union["CreateProjectRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> ProjectView:
        """Create a project"""
        return self._client.call(
            'post_v1_projects',
            body=body,
            options=options,
        )

    def get(self, id: str, options: "CallOptions" = None) -> ProjectView:
        """Fetch a project by id"""
        return self._client.call(
            'get_v1_projects_id',
            path_params={'id': id},
            options=options,
        )

    def install_configuration(self, id: str, body: Union["ConfigurationRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> InstallResponse:
        """Install a configuration bundle as a new immutable revision"""
        return self._client.call(
            'post_v1_projects_id_configuration_install',
            path_params={'id': id},
            body=body,
            options=options,
        )

    def list(self, options: "CallOptions" = None) -> ProjectsResponse:
        """List projects in the organization"""
        return self._client.call(
            'get_v1_projects',
            options=options,
        )

    def remove(self, id: str, options: "CallOptions" = None) -> None:
        """Delete a project and its revisions"""
        return self._client.call(
            'delete_v1_projects_id',
            path_params={'id': id},
            options=options,
        )

    def rename(self, id: str, body: Union["RenameProjectRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> ProjectView:
        """Rename a project slug or name; the id is unchanged"""
        return self._client.call(
            'patch_v1_projects_id',
            path_params={'id': id},
            body=body,
            options=options,
        )

    def revision(self, id: str, revision_id: str, options: "CallOptions" = None) -> RevisionView:
        """Fetch one configuration revision with its files"""
        return self._client.call(
            'get_v1_projects_id_revisions_revision_id',
            path_params={'id': id, 'revision_id': revision_id},
            options=options,
        )

    def revisions(self, id: str, options: "CallOptions" = None) -> RevisionsResponse:
        """List a project's configuration revisions"""
        return self._client.call(
            'get_v1_projects_id_revisions',
            path_params={'id': id},
            options=options,
        )

    def validate_configuration(self, id: str, body: Union["ConfigurationRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> ValidateResponse:
        """Validate a configuration bundle without writing pool state"""
        return self._client.call(
            'post_v1_projects_id_configuration_validate',
            path_params={'id': id},
            body=body,
            options=options,
        )

class SyncRelay:
    """Contract operations for the relay namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def enable(self, session: str, body: Any = None, options: "CallOptions" = None) -> RelayState:
        """Opt a session into publication"""
        return self._client.call(
            'put_v1_sessions_session_relay',
            path_params={'session': session},
            body=body,
            options=options,
        )

    def events(self, session: str, after: Optional[int] = None, limit: Optional[int] = None, options: "CallOptions" = None) -> RelayPage:
        """Backfill relayed events after a sequence cursor"""
        return self._client.call(
            'get_v1_sessions_session_relay_events',
            path_params={'session': session},
            query={'after': after, 'limit': limit},
            options=options,
        )

    def get(self, session: str, options: "CallOptions" = None) -> RelayState:
        """Read a session's relay publication state"""
        return self._client.call(
            'get_v1_sessions_session_relay',
            path_params={'session': session},
            options=options,
        )

    def publish(self, session: str, body: Union["PublishBody", Mapping[str, Any]] = None, options: "CallOptions" = None) -> RelayEvent:
        """Publish one allow-listed session event"""
        return self._client.call(
            'post_v1_sessions_session_relay_events',
            path_params={'session': session},
            body=body,
            options=options,
        )

    def stop(self, session: str, options: "CallOptions" = None) -> RelayState:
        """Stop publication immediately and permanently"""
        return self._client.call(
            'delete_v1_sessions_session_relay',
            path_params={'session': session},
            options=options,
        )

class SyncRuns:
    """Contract operations for the runs namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def dispatch(self, body: Union["ManualDispatchRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> ManualDispatchResponse:
        """Dispatch a workflow run from a manual request"""
        return self._client.call(
            'post_v1_runs',
            body=body,
            options=options,
        )

class SyncSetup:
    """Contract operations for the setup namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def create(self, body: Union["SetupRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> PoolIdentityView:
        """Create the organization and first owner from the browser"""
        return self._client.call(
            'post_v1_setup',
            body=body,
            options=options,
        )

    def status(self, options: "CallOptions" = None) -> SetupStatusResponse:
        """Report whether the pool still needs first-run setup"""
        return self._client.call(
            'get_v1_setup_status',
            options=options,
        )

class SyncSystem:
    """Contract operations for the system namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def health(self, options: "CallOptions" = None) -> Dict[str, Any]:
        """Liveness probe with DB status"""
        return self._client.call(
            'get_healthz',
            options=options,
        )

    def metrics(self, options: "CallOptions" = None) -> str:
        """Prometheus text-format metrics"""
        return self._client.call(
            'get_metrics',
            options=options,
        )

    def openapi(self, options: "CallOptions" = None) -> OpenApiDocument:
        """This OpenAPI document"""
        return self._client.call(
            'get_openapi_json',
            options=options,
        )

    def openapi_v1(self, options: "CallOptions" = None) -> OpenApiDocument:
        """This OpenAPI document at its pinned major-version path"""
        return self._client.call(
            'get_openapi_v1_json',
            options=options,
        )

class SyncTriggers:
    """Contract operations for the triggers namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def install(self, body: Union["TriggerInstallRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> TriggerInstallResponse:
        """Install or replace a trigger; keeps its id and bumps the revision"""
        return self._client.call(
            'post_v1_triggers_install',
            body=body,
            options=options,
        )

    def validate(self, body: Union["TriggerDocument", Mapping[str, Any]] = None, options: "CallOptions" = None) -> TriggerValidateResponse:
        """Validate one trigger document without writing pool state"""
        return self._client.call(
            'post_v1_triggers_validate',
            body=body,
            options=options,
        )

class SyncWebhooks:
    """Contract operations for the webhooks namespace."""

    def __init__(self, client: "SyncClient") -> None:
        self._client = client

    def github(self, slug: str, body: Union["RawJson", Mapping[str, Any]] = None, options: "CallOptions" = None) -> WebhookResponse:
        """Receive a GitHub App webhook; signature verified and delivery deduplicated"""
        return self._client.call(
            'post_v1_webhooks_github_slug',
            path_params={'slug': slug},
            body=body,
            options=options,
        )

    def slack(self, slug: str, body: Union["RawJson", Mapping[str, Any]] = None, options: "CallOptions" = None) -> Union[ChallengeResponse, WebhookResponse]:
        """Receive a Slack Events API callback; signature verified and event deduplicated"""
        return self._client.call(
            'post_v1_webhooks_slack_slug',
            path_params={'slug': slug},
            body=body,
            options=options,
        )


class AsyncResources:
    """Contract operations for the root namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client
        self.activity = AsyncActivity(client)
        self.api_keys = AsyncApiKeys(client)
        self.chatrooms = AsyncChatrooms(client)
        self.connections = AsyncConnections(client)
        self.game = AsyncGame(client)
        self.hosts = AsyncHosts(client)
        self.pool = AsyncPool(client)
        self.projects = AsyncProjects(client)
        self.relay = AsyncRelay(client)
        self.runs = AsyncRuns(client)
        self.setup = AsyncSetup(client)
        self.system = AsyncSystem(client)
        self.triggers = AsyncTriggers(client)
        self.webhooks = AsyncWebhooks(client)

class AsyncActivity:
    """Contract operations for the activity namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def list(self, project: Optional[str] = None, provider: Optional[str] = None, trigger: Optional[str] = None, status: Optional[str] = None, kind: Optional[str] = None, cursor: Optional[str] = None, limit: Optional[int] = None, options: "CallOptions" = None) -> ActivityPage:
        """List one page of receipts, runs, steps and executions"""
        return await self._client.call(
            'get_v1_activity',
            query={'project': project, 'provider': provider, 'trigger': trigger, 'status': status, 'kind': kind, 'cursor': cursor, 'limit': limit},
            options=options,
        )

    async def run(self, run: str, options: "CallOptions" = None) -> ActivityRunDetail:
        """Fetch one run with the arrival record that started it"""
        return await self._client.call(
            'get_v1_activity_runs_run',
            path_params={'run': run},
            options=options,
        )

class AsyncApiKeys:
    """Contract operations for the api_keys namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def create(self, body: Union["CreateApiKeyRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> CreateApiKeyResponse:
        """Create an API key; the full secret is shown once"""
        return await self._client.call(
            'post_v1_api_keys',
            body=body,
            options=options,
        )

    async def list(self, options: "CallOptions" = None) -> ApiKeysResponse:
        """List organization API keys (owner only; prefix only)"""
        return await self._client.call(
            'get_v1_api_keys',
            options=options,
        )

    async def remove(self, id: str, options: "CallOptions" = None) -> None:
        """Revoke an API key (owner only; effective immediately)"""
        return await self._client.call(
            'delete_v1_api_keys_id',
            path_params={'id': id},
            options=options,
        )

    async def whoami(self, options: "CallOptions" = None) -> ApiKeyView:
        """Introspect an API key (requires projects:read)"""
        return await self._client.call(
            'get_v1_api_keys_whoami',
            options=options,
        )

class AsyncChatrooms:
    """Contract operations for the chatrooms namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def list(self, options: "CallOptions" = None) -> RoomViewList:
        """List rooms visible to the operator"""
        return await self._client.call(
            'get_v1_chatrooms',
            options=options,
        )

    async def members(self, room: str, options: "CallOptions" = None) -> RoomMemberViewList:
        """List a room's members"""
        return await self._client.call(
            'get_v1_chatrooms_room_members',
            path_params={'room': room},
            options=options,
        )

    async def messages(self, room: str, after: Optional[int] = None, limit: Optional[int] = None, options: "CallOptions" = None) -> HistoryResponse:
        """Read room history after a sequence cursor"""
        return await self._client.call(
            'get_v1_chatrooms_room_messages',
            path_params={'room': room},
            query={'after': after, 'limit': limit},
            options=options,
        )

    async def send(self, room: str, body: Union["SendMessageBody", Mapping[str, Any]] = None, options: "CallOptions" = None) -> MessageView:
        """Post a message to a room"""
        return await self._client.call(
            'post_v1_chatrooms_room_messages',
            path_params={'room': room},
            body=body,
            options=options,
        )

    def stream(self, room: str, token: Optional[str] = None, after: Optional[int] = None) -> ChatStream:
        """Upgrade to the room's live message stream"""
        return self._client.stream(
            'get_v1_chatrooms_room_stream',
            path_params={'room': room},
            query={'token': token, 'after': after},
        )

class AsyncConnections:
    """Contract operations for the connections namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def create(self, body: Union["CreateConnectionRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> ConnectionView:
        """Create a provider connection from a credential reference"""
        return await self._client.call(
            'post_v1_connections',
            body=body,
            options=options,
        )

    async def get(self, id: str, options: "CallOptions" = None) -> ConnectionView:
        """Fetch a connection by id or env:<slug>"""
        return await self._client.call(
            'get_v1_connections_id',
            path_params={'id': id},
            options=options,
        )

    async def list(self, options: "CallOptions" = None) -> ConnectionsResponse:
        """List effective provider connections (environment overlay applied)"""
        return await self._client.call(
            'get_v1_connections',
            options=options,
        )

    async def remove(self, id: str, options: "CallOptions" = None) -> None:
        """Delete a database connection"""
        return await self._client.call(
            'delete_v1_connections_id',
            path_params={'id': id},
            options=options,
        )

    async def update(self, id: str, body: Union["UpdateConnectionRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> ConnectionView:
        """Rename, enable or disable a database connection"""
        return await self._client.call(
            'patch_v1_connections_id',
            path_params={'id': id},
            body=body,
            options=options,
        )

class AsyncGame:
    """Contract operations for the game namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client
        self.auth = AsyncGameAuth(client)
        self.events = AsyncGameEvents(client)
        self.feedback = AsyncGameFeedback(client)
        self.games = AsyncGameGames(client)
        self.leaderboards = AsyncGameLeaderboards(client)
        self.players = AsyncGamePlayers(client)
        self.saves = AsyncGameSaves(client)
        self.socket_tickets = AsyncGameSocketTickets(client)

class AsyncGameAuth:
    """Contract operations for the game.auth namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def login(self, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Verify credentials and issue a signed JWT"""
        return await self._client.call(
            'post_v1_auth_login',
            body=body,
            options=options,
        )

    async def register(self, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Register a new player and return the public profile"""
        return await self._client.call(
            'post_v1_auth_register',
            body=body,
            options=options,
        )

class AsyncGameEvents:
    """Contract operations for the game.events namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def create(self, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Ingest a batch of analytics events (fire-and-forget)"""
        return await self._client.call(
            'post_v1_events',
            body=body,
            options=options,
        )

class AsyncGameFeedback:
    """Contract operations for the game.feedback namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def create(self, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Submit player feedback"""
        return await self._client.call(
            'post_v1_feedback',
            body=body,
            options=options,
        )

class AsyncGameGames:
    """Contract operations for the game.games namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def settings(self, game_id: str, options: "CallOptions" = None) -> Dict[str, Any]:
        """Fetch a game's settings"""
        return await self._client.call(
            'get_v1_games_game_id_settings',
            path_params={'game_id': game_id},
            options=options,
        )

    async def update_settings(self, game_id: str, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Set a game's settings (upsert)"""
        return await self._client.call(
            'put_v1_games_game_id_settings',
            path_params={'game_id': game_id},
            body=body,
            options=options,
        )

class AsyncGameLeaderboards:
    """Contract operations for the game.leaderboards namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def get(self, game_id: str, page: Optional[int] = None, per_page: Optional[int] = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Fetch one ranked page of a leaderboard"""
        return await self._client.call(
            'get_v1_leaderboards_game_id',
            path_params={'game_id': game_id},
            query={'page': page, 'per_page': per_page},
            options=options,
        )

    async def put_entry(self, game_id: str, player_id: str, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Update the authenticated player's own leaderboard entry"""
        return await self._client.call(
            'put_v1_leaderboards_game_id_entries_player_id',
            path_params={'game_id': game_id, 'player_id': player_id},
            body=body,
            options=options,
        )

    async def submit_entry(self, game_id: str, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Submit a leaderboard entry for the authenticated player"""
        return await self._client.call(
            'post_v1_leaderboards_game_id_entries',
            path_params={'game_id': game_id},
            body=body,
            options=options,
        )

class AsyncGamePlayers:
    """Contract operations for the game.players namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def create_subaccount(self, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Create a subaccount under the authenticated player"""
        return await self._client.call(
            'post_v1_players_subaccount',
            body=body,
            options=options,
        )

    async def get(self, id: str, options: "CallOptions" = None) -> Dict[str, Any]:
        """Fetch a player's public profile"""
        return await self._client.call(
            'get_v1_players_id',
            path_params={'id': id},
            options=options,
        )

    async def remove(self, id: str, options: "CallOptions" = None) -> None:
        """Soft-delete the authenticated player's own account"""
        return await self._client.call(
            'delete_v1_players_id',
            path_params={'id': id},
            options=options,
        )

    async def subaccounts(self, id: str, options: "CallOptions" = None) -> Dict[str, Any]:
        """List the authenticated player's subaccounts"""
        return await self._client.call(
            'get_v1_players_id_subaccounts',
            path_params={'id': id},
            options=options,
        )

    async def update(self, id: str, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Update the authenticated player's own profile"""
        return await self._client.call(
            'put_v1_players_id',
            path_params={'id': id},
            body=body,
            options=options,
        )

class AsyncGameSaves:
    """Contract operations for the game.saves namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def create(self, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Create a game save"""
        return await self._client.call(
            'post_v1_saves',
            body=body,
            options=options,
        )

    async def get(self, player_id: str, save_id: str, options: "CallOptions" = None) -> Dict[str, Any]:
        """Fetch a single game save"""
        return await self._client.call(
            'get_v1_saves_player_id_save_id',
            path_params={'player_id': player_id, 'save_id': save_id},
            options=options,
        )

    async def list(self, player_id: str, options: "CallOptions" = None) -> Dict[str, Any]:
        """List the authenticated player's saves, newest first"""
        return await self._client.call(
            'get_v1_saves_player_id',
            path_params={'player_id': player_id},
            options=options,
        )

    async def remove(self, player_id: str, save_id: str, options: "CallOptions" = None) -> None:
        """Delete a single game save"""
        return await self._client.call(
            'delete_v1_saves_player_id_save_id',
            path_params={'player_id': player_id, 'save_id': save_id},
            options=options,
        )

class AsyncGameSocketTickets:
    """Contract operations for the game.socket_tickets namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def create(self, body: Any = None, options: "CallOptions" = None) -> Dict[str, Any]:
        """Issue a one-shot WebSocket connection ticket"""
        return await self._client.call(
            'post_v1_socket_tickets',
            body=body,
            options=options,
        )

class AsyncHosts:
    """Contract operations for the hosts namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def capabilities(self, id: str, options: "CallOptions" = None) -> HostCapabilityReport:
        """Read the host's last capability report and offered providers"""
        return await self._client.call(
            'get_v1_hosts_id_capabilities',
            path_params={'id': id},
            options=options,
        )

    async def create(self, body: Union["CreateHostRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> HostView:
        """Register a host; the slug is derived from the hostname"""
        return await self._client.call(
            'post_v1_hosts',
            body=body,
            options=options,
        )

    async def create_execution(self, id: str, body: Union["CreateExecutionRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> CreateExecutionResponse:
        """Create or replay a create-agent intent with a stable execution id"""
        return await self._client.call(
            'post_v1_hosts_id_executions',
            path_params={'id': id},
            body=body,
            options=options,
        )

    async def enroll(self, body: Union["EnrollHostRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> HostView:
        """Exchange an enrollment token for a host relationship credential"""
        return await self._client.call(
            'post_v1_hosts_enroll',
            body=body,
            options=options,
        )

    async def get(self, id: str, options: "CallOptions" = None) -> HostView:
        """Fetch a live host by immutable id"""
        return await self._client.call(
            'get_v1_hosts_id',
            path_params={'id': id},
            options=options,
        )

    async def list(self, options: "CallOptions" = None) -> HostsResponse:
        """List live hosts in the pool"""
        return await self._client.call(
            'get_v1_hosts',
            options=options,
        )

    async def mint_enrollment_token(self, body: Union["MintEnrollmentTokenRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> MintEnrollmentTokenResponse:
        """Mint a ten-minute single-use host enrollment token"""
        return await self._client.call(
            'post_v1_hosts_enrollment_tokens',
            body=body,
            options=options,
        )

    async def remove(self, id: str, options: "CallOptions" = None) -> None:
        """Revoke the pool relationship for a host"""
        return await self._client.call(
            'delete_v1_hosts_id',
            path_params={'id': id},
            options=options,
        )

    async def rename(self, id: str, body: Union["RenameHostRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> HostView:
        """Rename a host slug or display name; the id is unchanged"""
        return await self._client.call(
            'patch_v1_hosts_id',
            path_params={'id': id},
            body=body,
            options=options,
        )

    async def report_capabilities(self, id: str, body: Union["CapabilityReportRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> HostCapabilityReport:
        """Host reports providers, models, modes and thinking options"""
        return await self._client.call(
            'post_v1_hosts_id_capabilities',
            path_params={'id': id},
            body=body,
            options=options,
        )

    async def start_execution(self, id: str, body: Any = None, options: "CallOptions" = None) -> HostView:
        """Grant host.execute so the host may run agents"""
        return await self._client.call(
            'post_v1_hosts_id_execution',
            path_params={'id': id},
            body=body,
            options=options,
        )

    async def stop_execution(self, id: str, options: "CallOptions" = None) -> HostView:
        """Revoke host.execute; the host becomes connected-only"""
        return await self._client.call(
            'delete_v1_hosts_id_execution',
            path_params={'id': id},
            options=options,
        )

class AsyncPool:
    """Contract operations for the pool namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def accept_invitation(self, body: Union["AcceptInvitationRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> PoolLoginResponse:
        """Accept an invitation and create the member account"""
        return await self._client.call(
            'post_v1_pool_invitations_accept',
            body=body,
            options=options,
        )

    async def change_password(self, body: Union["ChangePasswordRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> None:
        """Replace the operator's password (clears first-sign-in requirement)"""
        return await self._client.call(
            'post_v1_pool_password',
            body=body,
            options=options,
        )

    async def invitations(self, options: "CallOptions" = None) -> InvitationsResponse:
        """List pending team invitations (owner only)"""
        return await self._client.call(
            'get_v1_pool_invitations',
            options=options,
        )

    async def invite(self, body: Union["InviteRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> InviteResponse:
        """Invite an operator by email; the token is shown once"""
        return await self._client.call(
            'post_v1_pool_invitations',
            body=body,
            options=options,
        )

    async def login(self, body: Union["PoolLoginRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> PoolLoginResponse:
        """Sign an operator in and issue a JWT"""
        return await self._client.call(
            'post_v1_pool_login',
            body=body,
            options=options,
        )

    async def me(self, options: "CallOptions" = None) -> PoolIdentityView:
        """Fetch the signed-in operator and organization"""
        return await self._client.call(
            'get_v1_pool_me',
            options=options,
        )

    async def members(self, options: "CallOptions" = None) -> MembersResponse:
        """List the organization's operators"""
        return await self._client.call(
            'get_v1_pool_members',
            options=options,
        )

    async def remove_member(self, id: str, options: "CallOptions" = None) -> None:
        """Remove a member (owner only; last owner protected)"""
        return await self._client.call(
            'delete_v1_pool_members_id',
            path_params={'id': id},
            options=options,
        )

    async def revoke_invitation(self, id: str, options: "CallOptions" = None) -> None:
        """Revoke a pending invitation (owner only)"""
        return await self._client.call(
            'delete_v1_pool_invitations_id',
            path_params={'id': id},
            options=options,
        )

    async def update_member(self, id: str, body: Union["UpdateRoleRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> PoolUserView:
        """Change a member's role (owner only; last owner protected)"""
        return await self._client.call(
            'patch_v1_pool_members_id',
            path_params={'id': id},
            body=body,
            options=options,
        )

class AsyncProjects:
    """Contract operations for the projects namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def configuration(self, id: str, options: "CallOptions" = None) -> RevisionView:
        """Fetch a project's active configuration revision"""
        return await self._client.call(
            'get_v1_projects_id_configuration',
            path_params={'id': id},
            options=options,
        )

    async def create(self, body: Union["CreateProjectRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> ProjectView:
        """Create a project"""
        return await self._client.call(
            'post_v1_projects',
            body=body,
            options=options,
        )

    async def get(self, id: str, options: "CallOptions" = None) -> ProjectView:
        """Fetch a project by id"""
        return await self._client.call(
            'get_v1_projects_id',
            path_params={'id': id},
            options=options,
        )

    async def install_configuration(self, id: str, body: Union["ConfigurationRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> InstallResponse:
        """Install a configuration bundle as a new immutable revision"""
        return await self._client.call(
            'post_v1_projects_id_configuration_install',
            path_params={'id': id},
            body=body,
            options=options,
        )

    async def list(self, options: "CallOptions" = None) -> ProjectsResponse:
        """List projects in the organization"""
        return await self._client.call(
            'get_v1_projects',
            options=options,
        )

    async def remove(self, id: str, options: "CallOptions" = None) -> None:
        """Delete a project and its revisions"""
        return await self._client.call(
            'delete_v1_projects_id',
            path_params={'id': id},
            options=options,
        )

    async def rename(self, id: str, body: Union["RenameProjectRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> ProjectView:
        """Rename a project slug or name; the id is unchanged"""
        return await self._client.call(
            'patch_v1_projects_id',
            path_params={'id': id},
            body=body,
            options=options,
        )

    async def revision(self, id: str, revision_id: str, options: "CallOptions" = None) -> RevisionView:
        """Fetch one configuration revision with its files"""
        return await self._client.call(
            'get_v1_projects_id_revisions_revision_id',
            path_params={'id': id, 'revision_id': revision_id},
            options=options,
        )

    async def revisions(self, id: str, options: "CallOptions" = None) -> RevisionsResponse:
        """List a project's configuration revisions"""
        return await self._client.call(
            'get_v1_projects_id_revisions',
            path_params={'id': id},
            options=options,
        )

    async def validate_configuration(self, id: str, body: Union["ConfigurationRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> ValidateResponse:
        """Validate a configuration bundle without writing pool state"""
        return await self._client.call(
            'post_v1_projects_id_configuration_validate',
            path_params={'id': id},
            body=body,
            options=options,
        )

class AsyncRelay:
    """Contract operations for the relay namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def enable(self, session: str, body: Any = None, options: "CallOptions" = None) -> RelayState:
        """Opt a session into publication"""
        return await self._client.call(
            'put_v1_sessions_session_relay',
            path_params={'session': session},
            body=body,
            options=options,
        )

    async def events(self, session: str, after: Optional[int] = None, limit: Optional[int] = None, options: "CallOptions" = None) -> RelayPage:
        """Backfill relayed events after a sequence cursor"""
        return await self._client.call(
            'get_v1_sessions_session_relay_events',
            path_params={'session': session},
            query={'after': after, 'limit': limit},
            options=options,
        )

    async def get(self, session: str, options: "CallOptions" = None) -> RelayState:
        """Read a session's relay publication state"""
        return await self._client.call(
            'get_v1_sessions_session_relay',
            path_params={'session': session},
            options=options,
        )

    async def publish(self, session: str, body: Union["PublishBody", Mapping[str, Any]] = None, options: "CallOptions" = None) -> RelayEvent:
        """Publish one allow-listed session event"""
        return await self._client.call(
            'post_v1_sessions_session_relay_events',
            path_params={'session': session},
            body=body,
            options=options,
        )

    async def stop(self, session: str, options: "CallOptions" = None) -> RelayState:
        """Stop publication immediately and permanently"""
        return await self._client.call(
            'delete_v1_sessions_session_relay',
            path_params={'session': session},
            options=options,
        )

class AsyncRuns:
    """Contract operations for the runs namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def dispatch(self, body: Union["ManualDispatchRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> ManualDispatchResponse:
        """Dispatch a workflow run from a manual request"""
        return await self._client.call(
            'post_v1_runs',
            body=body,
            options=options,
        )

class AsyncSetup:
    """Contract operations for the setup namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def create(self, body: Union["SetupRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> PoolIdentityView:
        """Create the organization and first owner from the browser"""
        return await self._client.call(
            'post_v1_setup',
            body=body,
            options=options,
        )

    async def status(self, options: "CallOptions" = None) -> SetupStatusResponse:
        """Report whether the pool still needs first-run setup"""
        return await self._client.call(
            'get_v1_setup_status',
            options=options,
        )

class AsyncSystem:
    """Contract operations for the system namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def health(self, options: "CallOptions" = None) -> Dict[str, Any]:
        """Liveness probe with DB status"""
        return await self._client.call(
            'get_healthz',
            options=options,
        )

    async def metrics(self, options: "CallOptions" = None) -> str:
        """Prometheus text-format metrics"""
        return await self._client.call(
            'get_metrics',
            options=options,
        )

    async def openapi(self, options: "CallOptions" = None) -> OpenApiDocument:
        """This OpenAPI document"""
        return await self._client.call(
            'get_openapi_json',
            options=options,
        )

    async def openapi_v1(self, options: "CallOptions" = None) -> OpenApiDocument:
        """This OpenAPI document at its pinned major-version path"""
        return await self._client.call(
            'get_openapi_v1_json',
            options=options,
        )

class AsyncTriggers:
    """Contract operations for the triggers namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def install(self, body: Union["TriggerInstallRequest", Mapping[str, Any]] = None, options: "CallOptions" = None) -> TriggerInstallResponse:
        """Install or replace a trigger; keeps its id and bumps the revision"""
        return await self._client.call(
            'post_v1_triggers_install',
            body=body,
            options=options,
        )

    async def validate(self, body: Union["TriggerDocument", Mapping[str, Any]] = None, options: "CallOptions" = None) -> TriggerValidateResponse:
        """Validate one trigger document without writing pool state"""
        return await self._client.call(
            'post_v1_triggers_validate',
            body=body,
            options=options,
        )

class AsyncWebhooks:
    """Contract operations for the webhooks namespace."""

    def __init__(self, client: "AsyncClient") -> None:
        self._client = client

    async def github(self, slug: str, body: Union["RawJson", Mapping[str, Any]] = None, options: "CallOptions" = None) -> WebhookResponse:
        """Receive a GitHub App webhook; signature verified and delivery deduplicated"""
        return await self._client.call(
            'post_v1_webhooks_github_slug',
            path_params={'slug': slug},
            body=body,
            options=options,
        )

    async def slack(self, slug: str, body: Union["RawJson", Mapping[str, Any]] = None, options: "CallOptions" = None) -> Union[ChallengeResponse, WebhookResponse]:
        """Receive a Slack Events API callback; signature verified and event deduplicated"""
        return await self._client.call(
            'post_v1_webhooks_slack_slug',
            path_params={'slug': slug},
            body=body,
            options=options,
        )

