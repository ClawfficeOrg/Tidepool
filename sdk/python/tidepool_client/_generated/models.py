"""Generated from docs/api/openapi-v1.json. Do not edit by hand.

Regenerate: python sdk/python/scripts/generate.py
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Dict, List, Optional

from ..model import Model, ModelList



@dataclass
class AcceptInvitationRequest(Model):
    """Accept an invitation and create the member account"""

    display_name: str = None
    # At least 12 characters
    password: str = None
    token: str = None


@dataclass
class ActivityEntry(Model):
    """One receipt, run, step or execution in the activity feed"""

    actor: Optional[str] = None
    at: str = None
    continuation: Optional[str] = None
    duplicates: Optional[int] = None
    error: Optional[str] = None
    event_type: Optional[str] = None
    execution: Optional[str] = None
    id: str = None
    # Known values: 'receipt', 'run', 'step', 'execution'. Unknown values are tolerated.
    kind: str = None
    matched: Optional[bool] = None
    output: Optional[str] = None
    project: Optional[str] = None
    provider: Optional[str] = None
    run: Optional[str] = None
    status: str = None
    step: Optional[str] = None
    trigger: Optional[str] = None
    trigger_name: Optional[str] = None

    __json_names__ = {'event_type': 'eventType', 'trigger_name': 'triggerName'}


@dataclass
class ActivityPage(Model):
    """One cursor-paged activity window"""

    has_more: bool = None
    items: List[ActivityEntry] = None
    next_cursor: Optional[str] = None

    __json_names__ = {'has_more': 'hasMore', 'next_cursor': 'nextCursor'}


@dataclass
class ActivityRunDetail(Model):
    """One run plus the arrival that started it"""

    arrival: ArrivalDetail = None
    run: RunView = None


@dataclass
class AgentCapability(Model):
    """One provider/model/mode/thinking combination a host can run"""

    execution_mode: str = None
    model: str = None
    options: Any = None
    provider: str = None
    thinking_option: Optional[str] = None


@dataclass
class AgentSelection(Model):
    """A named provider/model/mode choice"""

    execution_mode: str = None
    model: str = None
    provider: str = None
    thinking_option: Optional[str] = None


@dataclass
class ApiKeyView(Model):
    """API key metadata; the secret is never returned after creation"""

    created_at: str = None
    id: str = None
    last_used_at: Optional[str] = None
    name: str = None
    prefix: str = None
    revoked_at: Optional[str] = None
    scopes: List[str] = None
    # Known values: 'active', 'revoked'. Unknown values are tolerated.
    status: str = None


@dataclass
class ApiKeysResponse(Model):
    """Organization API keys, newest first"""

    api_keys: List[ApiKeyView] = None


@dataclass
class ArrivalDetail(Model):
    """How an arrival created, continued or restored an agent"""

    conversation_key: Optional[str] = None
    # Known values: 'created', 'continued', 'restored'. Unknown values are tolerated.
    mode: str = None
    output_limits: List[StepOutputLimits] = None
    provider: Optional[str] = None
    receipt: Optional[str] = None
    steered: bool = None

    __json_names__ = {'conversation_key': 'conversationKey', 'output_limits': 'outputLimits'}


@dataclass
class CapabilityReportRequest(Model):
    """A host's capability report"""

    capabilities: List[AgentCapability] = None


@dataclass
class ChallengeResponse(Model):
    """Slack URL-verification echo"""

    challenge: str = None


@dataclass
class ChangePasswordRequest(Model):
    """Replace the operator's password"""

    current_password: str = None
    # At least 12 characters
    new_password: str = None


@dataclass
class ConfigFileRequest(Model):
    """One configuration file as authored"""

    content: str = None
    path: str = None


@dataclass
class ConfigurationRequest(Model):
    """A configuration bundle (at most 100 files)"""

    files: List[ConfigFileRequest] = None


@dataclass
class ConnectionResourceView(Model):
    """One resource a connection may address"""

    display_name: Optional[str] = None
    external_id: str = None
    resource_kind: str = None


@dataclass
class ConnectionView(Model):
    """One provider connection"""

    created_at: str = None
    credential_configured: bool = None
    display_name: str = None
    id: str = None
    # Known values: 'database', 'environment'. Unknown values are tolerated.
    managed_by: str = None
    # Known values: 'github', 'slack', 'discord'. Unknown values are tolerated.
    provider: str = None
    resources: List[ConnectionResourceView] = None
    slug: str = None
    # Known values: 'active', 'disabled'. Unknown values are tolerated.
    status: str = None


@dataclass
class ConnectionsResponse(Model):
    """Effective connections after the environment overlay"""

    connections: List[ConnectionView] = None


@dataclass
class CreateApiKeyRequest(Model):
    """Create an API key"""

    name: str = None
    # Subset of projects:read, configuration:validate, configuration:install, runs:dispatch, daemons:enroll
    scopes: List[str] = None


@dataclass
class CreateApiKeyResponse(Model):
    """Key metadata plus the one-time secret"""

    api_key: ApiKeyView = None
    # `<prefix>.<secret>`; shown once
    secret: str = None


@dataclass
class CreateConnectionRequest(Model):
    """Create a connection from a credential reference"""

    # Environment variable name; the secret itself is never sent
    credential_ref: str = None
    display_name: str = None
    # Known values: 'github', 'slack', 'discord'. Unknown values are tolerated.
    provider: str = None
    resources: Optional[List[ResourceRequest]] = None
    slug: str = None


@dataclass
class CreateExecutionRequest(Model):
    """Create or replay a create-agent intent"""

    # Stable id; the replay key
    execution_id: str = None
    prompt: str = None


@dataclass
class CreateExecutionResponse(Model):
    """The stored execution and whether the call replayed an existing one"""

    execution: HostExecutionView = None
    replayed: bool = None


@dataclass
class CreateHostRequest(Model):
    """Register a host"""

    display_name: Optional[str] = None
    hostname: str = None


@dataclass
class CreateProjectRequest(Model):
    """Create a project"""

    name: str = None
    slug: Optional[str] = None


@dataclass
class EnrollHostRequest(Model):
    """Exchange an enrollment token for a host credential"""

    # Host-generated relationship secret
    credential: str = None
    token: str = None


@dataclass
class EnrollmentTokenView(Model):
    """Enrollment token metadata (never the token)"""

    consumed_at: Optional[str] = None
    created_at: str = None
    expires_at: str = None
    id: str = None
    revoked_at: Optional[str] = None


@dataclass
class FieldIssue(Model):
    """One named validation issue"""

    field: str = None
    message: str = None
    revision: int = None


@dataclass
class GameError(Model):
    """Error body used by the game surface (`{"error": "..."}`)"""

    error: str = None


@dataclass
class HistoryResponse(Model):
    """One page of chat history"""

    messages: List[MessageView] = None
    next: Optional[int] = None


@dataclass
class HostCapabilityReport(Model):
    """The last stored capability report, plus what the pool may offer"""

    capabilities: List[AgentCapability] = None
    host_id: str = None
    offered_providers: List[str] = None
    reported_at: Optional[str] = None


@dataclass
class HostExecutionView(Model):
    """One agent execution started on a host"""

    completed_at: Optional[str] = None
    id: str = None
    output: Optional[str] = None
    # Known values: 'queued', 'running', 'succeeded', 'failed', 'interrupted'. Unknown values are tolerated.
    status: str = None


@dataclass
class HostView(Model):
    """One enrolled machine"""

    created_at: str = None
    display_name: str = None
    execution_enabled: bool = None
    hostname: str = None
    id: str = None
    revoked_at: Optional[str] = None
    slug: str = None
    # Known values: 'pending_approval', 'connected', 'offline', 'revoked'. Unknown values are tolerated.
    status: str = None


@dataclass
class HostsResponse(Model):
    """Live hosts, ordered by slug"""

    hosts: List[HostView] = None


@dataclass
class InstallResponse(Model):
    """The installed revision and whether it became active"""

    activated: bool = None
    revision: RevisionView = None


@dataclass
class InterruptExecutionResponse(Model):
    """The execution as recorded when the interrupt was handled"""

    execution: HostExecutionView = None
    # True when interrupt_execution was sent; false when the execution was already terminal
    sent: bool = None


@dataclass
class InvitationsResponse(Model):
    """Invitations still awaiting acceptance"""

    invitations: List[PoolInvitationView] = None


@dataclass
class InviteRequest(Model):
    """Invite an operator by email"""

    email: str = None
    # Defaults to member Known values: 'owner', 'admin', 'member', 'viewer'. Unknown values are tolerated.
    role: str = None


@dataclass
class InviteResponse(Model):
    """Invitation metadata plus the one-time acceptance token"""

    invitation: PoolInvitationView = None
    # Shown once; not stored in plaintext
    token: str = None


@dataclass
class ManualDispatchRequest(Model):
    """A manual run request"""

    actor: str = None
    # Stable per request; a repeat returns the existing run
    delivery_key: str = None
    input: Optional[str] = None
    project: str = None
    revision: Optional[str] = None
    trigger: str = None

    __json_names__ = {'delivery_key': 'deliveryKey'}


@dataclass
class ManualDispatchResponse(Model):
    """The run a manual request created or matched"""

    # True on an idempotent replay
    deduplicated: bool = None
    run: RunView = None


@dataclass
class MembersResponse(Model):
    """Operators in the organization"""

    members: List[PoolUserView] = None


@dataclass
class MessageView(Model):
    """One chat message"""

    author: Optional[Principal] = None
    body: str = None
    created_at: str = None
    id: str = None
    project: str = None
    reply_to: Optional[str] = None
    sequence: int = None


@dataclass
class MintEnrollmentTokenRequest(Model):
    """Mint a single-use enrollment token for one host"""

    host_id: str = None


@dataclass
class MintEnrollmentTokenResponse(Model):
    """The one-time enrollment token"""

    enrollment_token: EnrollmentTokenView = None
    token: str = None


@dataclass
class OpenApiDocument(Model):
    """This OpenAPI 3.1 documents"""

    info: Any = None
    # OpenAPI version
    openapi: str = None
    paths: Any = None


@dataclass
class OrganizationView(Model):
    """One organization (one pool in v1)"""

    created_at: str = None
    id: str = None
    name: str = None


@dataclass
class OutputLimit(Model):
    """One declared output capability limit"""

    capability: str = None
    maximum: int = None
    required: bool = None


@dataclass
class PoolIdentityView(Model):
    """The signed-in operator and its organization"""

    organization: OrganizationView = None
    user: PoolUserView = None


@dataclass
class PoolInvitationView(Model):
    """Pending or historical invitation metadata (never the token)"""

    accepted_at: Optional[str] = None
    created_at: str = None
    email: str = None
    expires_at: str = None
    id: str = None
    revoked_at: Optional[str] = None
    # Known values: 'owner', 'admin', 'member', 'viewer'. Unknown values are tolerated.
    role: str = None


@dataclass
class PoolLoginRequest(Model):
    """Operator sign-in payload"""

    email: str = None
    password: str = None


@dataclass
class PoolLoginResponse(Model):
    """A freshly issued operator JWT"""

    # Token lifetime in seconds
    expires_in: int = None
    # True while the bootstrap password must be replaced
    must_change_password: bool = None
    # Signed JWT; subject is the operator public id
    token: str = None
    user: PoolUserView = None


@dataclass
class PoolUserView(Model):
    """Operator account, never carrying credential material"""

    created_at: str = None
    display_name: str = None
    email: str = None
    id: str = None
    last_login_at: Optional[str] = None
    password_change_required: bool = None
    # Known values: 'owner', 'admin', 'member', 'viewer'. Unknown values are tolerated.
    role: str = None


@dataclass
class Principal(Model):
    """The single principal behind a room member or message author"""

    id: str = None
    # Known values: 'user', 'session'. Unknown values are tolerated.
    kind: str = None


@dataclass
class Problem(Model):
    """RFC 9457 problem detail returned by every hub route on failure"""

    # Stable machine-readable code (docs/HUB_API_ERRORS.md) Known values: 'unauthorized', 'forbidden', 'missing_scope', 'invalid_request', 'validation_failed', 'not_found', 'conflict', 'gone', 'method_not_allowed', 'rate_limited', 'daemon_not_connected', 'configuration_invalid', 'service_unavailable', 'internal_error'. Unknown values are tolerated.
    code: str = None
    # Human-readable explanation
    detail: Optional[str] = None
    # Correlation id, also echoed in x-request-id
    request_id: Optional[str] = None
    status: Optional[int] = None
    # Short human-readable summary
    title: Optional[str] = None
    # Problem type URI
    type_: Optional[str] = None

    __json_names__ = {'request_id': 'requestId', 'type_': 'type'}


@dataclass
class ProjectView(Model):
    """One project"""

    created_at: str = None
    id: str = None
    name: str = None
    slug: str = None
    updated_at: str = None


@dataclass
class ProjectsResponse(Model):
    """Projects in the organization"""

    projects: List[ProjectView] = None


@dataclass
class PublishBody(Model):
    """Publish one relayed event"""

    # Known values: 'started', 'status', 'output', 'tool', 'message', 'stopped'. Unknown values are tolerated.
    kind: str = None
    # At most 8 KiB; credential-like keys are refused
    payload: Any = None


@dataclass
class RelayEvent(Model):
    """One relayed session event; never carries file contents"""

    created_at: str = None
    id: str = None
    # Known values: 'started', 'status', 'output', 'tool', 'message', 'stopped'. Unknown values are tolerated.
    kind: str = None
    payload: Any = None
    sequence: int = None
    session: str = None

    __json_names__ = {'created_at': 'createdAt'}


@dataclass
class RelayPage(Model):
    """One page of relayed events"""

    events: List[RelayEvent] = None
    next: Optional[int] = None


@dataclass
class RelayState(Model):
    """A session's relay publication state (off by default)"""

    enabled: bool = None
    session: str = None
    started_at: Optional[str] = None
    stopped_at: Optional[str] = None

    __json_names__ = {'started_at': 'startedAt', 'stopped_at': 'stoppedAt'}


@dataclass
class RenameHostRequest(Model):
    """Rename a host; at least one field is required"""

    display_name: Optional[str] = None
    slug: Optional[str] = None


@dataclass
class RenameProjectRequest(Model):
    """Rename a project; the id is unchanged"""

    name: Optional[str] = None
    slug: Optional[str] = None


@dataclass
class ResourceRequest(Model):
    """A resource a new connection may address"""

    display_name: Optional[str] = None
    external_id: str = None
    resource_kind: str = None


@dataclass
class RevisionFileView(Model):
    """One file of a stored revision"""

    content: str = None
    path: str = None


@dataclass
class RevisionMetaView(Model):
    """Revision metadata without file bodies"""

    active: bool = None
    created_at: str = None
    error_count: int = None
    id: str = None
    # Known values: 'valid', 'invalid'. Unknown values are tolerated.
    status: str = None
    version: int = None


@dataclass
class RevisionView(Model):
    """One immutable configuration revision"""

    active: bool = None
    created_at: str = None
    errors: List[FieldIssue] = None
    files: List[RevisionFileView] = None
    id: str = None
    # Known values: 'valid', 'invalid'. Unknown values are tolerated.
    status: str = None
    version: int = None


@dataclass
class RevisionsResponse(Model):
    """A project's revisions, newest first"""

    revisions: List[RevisionMetaView] = None


@dataclass
class RoomMemberView(Model):
    """One room member"""

    id: str = None
    joined_at: str = None
    principal: Principal = None
    # Known values: 'owner', 'member', 'viewer'. Unknown values are tolerated.
    role: str = None


class RoomMemberViewList(ModelList):
    """A room's members"""

    item_type = RoomMemberView


@dataclass
class RoomView(Model):
    """One chatroom"""

    created_at: str = None
    id: str = None
    name: str = None
    origin_pool: Optional[str] = None
    updated_at: str = None
    # Known values: 'private', 'unlisted', 'public'. Unknown values are tolerated.
    visibility: str = None


class RoomViewList(ModelList):
    """Rooms visible to the caller"""

    item_type = RoomView


@dataclass
class RunStepView(Model):
    """One step of a workflow run"""

    branch: Optional[str] = None
    completed_at: Optional[str] = None
    condition: Optional[bool] = None
    directory: Optional[str] = None
    environment: Optional[str] = None
    error: Optional[str] = None
    execution_id: Optional[str] = None
    host: Optional[str] = None
    id: str = None
    output: Optional[str] = None
    position: int = None
    reason: Optional[str] = None
    started_at: Optional[str] = None
    # Known values: 'pending', 'running', 'succeeded', 'failed', 'skipped', 'interrupted'. Unknown values are tolerated.
    status: str = None
    step: str = None

    __json_names__ = {'completed_at': 'completedAt', 'execution_id': 'executionId', 'started_at': 'startedAt'}


@dataclass
class RunView(Model):
    """One workflow run with its steps"""

    actor: Optional[str] = None
    completed_at: Optional[str] = None
    created_at: str = None
    deadline_at: Optional[str] = None
    # Idempotency key for the arrival
    delivery_key: str = None
    id: str = None
    inputs: Dict[str, str] = None
    # Known values: 'event', 'manual'. Unknown values are tolerated.
    origin: str = None
    project: str = None
    request_text: str = None
    started_at: Optional[str] = None
    # Known values: 'pending', 'running', 'succeeded', 'failed', 'interrupted'. Unknown values are tolerated.
    status: str = None
    steps: List[RunStepView] = None
    trigger: str = None
    trigger_name: str = None
    workflow: str = None

    __json_names__ = {'completed_at': 'completedAt', 'created_at': 'createdAt', 'deadline_at': 'deadlineAt', 'delivery_key': 'deliveryKey', 'request_text': 'requestText', 'started_at': 'startedAt', 'trigger_name': 'triggerName'}


@dataclass
class SendMessageBody(Model):
    """Post a message to a room"""

    body: str = None
    reply_to: Optional[str] = None

    __json_names__ = {'reply_to': 'replyTo'}


@dataclass
class SetupRequest(Model):
    """First-run browser setup payload"""

    display_name: str = None
    email: str = None
    organization_name: str = None
    # At least 12 characters
    password: str = None


@dataclass
class SetupStatusResponse(Model):
    """First-run setup state"""

    bootstrap_configured: bool = None
    needs_setup: bool = None


@dataclass
class StepOutputLimits(Model):
    """Output limits declared by one step"""

    limits: List[OutputLimit] = None
    step: str = None


@dataclass
class TriggerDocument(Model):
    """One authored trigger document"""

    event: TriggerEvent = None
    filters: TriggerFilters = None
    name: str = None
    run: TriggerRun = None


@dataclass
class TriggerEvent(Model):
    """The provider event that may start a trigger"""

    # Known values: 'github', 'slack', 'discord'. Unknown values are tolerated.
    provider: str = None
    # Provider event type
    type_: str = None

    __json_names__ = {'type_': 'type'}


@dataclass
class TriggerFilters(Model):
    """Which deliveries of that event are allowed through"""

    channel: Optional[str] = None
    connection: str = None
    contains: Optional[str] = None
    # Required allowlist; an empty list is rejected
    from_users: List[str] = None
    label: Optional[str] = None
    pattern: Optional[str] = None
    resource: TriggerResourceFilter = None


@dataclass
class TriggerInstallRequest(Model):
    """A trigger document plus the project it belongs to"""

    event: TriggerEvent = None
    filters: TriggerFilters = None
    name: str = None
    # Public id of the owning project
    project: str = None
    run: TriggerRun = None


@dataclass
class TriggerInstallResponse(Model):
    """The installed trigger and the revision the install produced"""

    active: bool = None
    # Immutable trigger id; a replacement keeps it
    id: str = None
    name: str = None
    replaced: bool = None
    revision: str = None
    version: int = None


@dataclass
class TriggerIssue(Model):
    """One named trigger validation issue"""

    field: str = None
    message: str = None


@dataclass
class TriggerResourceFilter(Model):
    """The provider resource a trigger is scoped to"""

    id: str = None
    kind: str = None


@dataclass
class TriggerRun(Model):
    """Where and with which agent a matched trigger runs"""

    agent: AgentSelection = None
    host: str = None


@dataclass
class TriggerValidateResponse(Model):
    """Trigger validation result; nothing was written"""

    errors: List[TriggerIssue] = None
    name: str = None
    valid: bool = None


@dataclass
class UpdateConnectionRequest(Model):
    """Rename, enable or disable a database connection"""

    display_name: Optional[str] = None
    slug: Optional[str] = None
    status: Optional[str] = None


@dataclass
class UpdateRoleRequest(Model):
    """Change a member's role"""

    # Known values: 'owner', 'admin', 'member', 'viewer'. Unknown values are tolerated.
    role: str = None


@dataclass
class ValidateResponse(Model):
    """Validation result; nothing was written"""

    errors: List[FieldIssue] = None
    valid: bool = None
    version: int = None


@dataclass
class WebhookResponse(Model):
    """A verified provider delivery"""

    accepted: bool = None
    action: Optional[str] = None
    duplicate: bool = None
    event: str = None
    received: bool = None


RawJson = Any
"""Any JSON value; the contract does not constrain it."""


MODEL_TYPES: Dict[str, Any] = {
    'AcceptInvitationRequest': AcceptInvitationRequest,
    'ActivityEntry': ActivityEntry,
    'ActivityPage': ActivityPage,
    'ActivityRunDetail': ActivityRunDetail,
    'AgentCapability': AgentCapability,
    'AgentSelection': AgentSelection,
    'ApiKeyView': ApiKeyView,
    'ApiKeysResponse': ApiKeysResponse,
    'ArrivalDetail': ArrivalDetail,
    'CapabilityReportRequest': CapabilityReportRequest,
    'ChallengeResponse': ChallengeResponse,
    'ChangePasswordRequest': ChangePasswordRequest,
    'ConfigFileRequest': ConfigFileRequest,
    'ConfigurationRequest': ConfigurationRequest,
    'ConnectionResourceView': ConnectionResourceView,
    'ConnectionView': ConnectionView,
    'ConnectionsResponse': ConnectionsResponse,
    'CreateApiKeyRequest': CreateApiKeyRequest,
    'CreateApiKeyResponse': CreateApiKeyResponse,
    'CreateConnectionRequest': CreateConnectionRequest,
    'CreateExecutionRequest': CreateExecutionRequest,
    'CreateExecutionResponse': CreateExecutionResponse,
    'CreateHostRequest': CreateHostRequest,
    'CreateProjectRequest': CreateProjectRequest,
    'EnrollHostRequest': EnrollHostRequest,
    'EnrollmentTokenView': EnrollmentTokenView,
    'FieldIssue': FieldIssue,
    'GameError': GameError,
    'HistoryResponse': HistoryResponse,
    'HostCapabilityReport': HostCapabilityReport,
    'HostExecutionView': HostExecutionView,
    'HostView': HostView,
    'HostsResponse': HostsResponse,
    'InstallResponse': InstallResponse,
    'InterruptExecutionResponse': InterruptExecutionResponse,
    'InvitationsResponse': InvitationsResponse,
    'InviteRequest': InviteRequest,
    'InviteResponse': InviteResponse,
    'ManualDispatchRequest': ManualDispatchRequest,
    'ManualDispatchResponse': ManualDispatchResponse,
    'MembersResponse': MembersResponse,
    'MessageView': MessageView,
    'MintEnrollmentTokenRequest': MintEnrollmentTokenRequest,
    'MintEnrollmentTokenResponse': MintEnrollmentTokenResponse,
    'OpenApiDocument': OpenApiDocument,
    'OrganizationView': OrganizationView,
    'OutputLimit': OutputLimit,
    'PoolIdentityView': PoolIdentityView,
    'PoolInvitationView': PoolInvitationView,
    'PoolLoginRequest': PoolLoginRequest,
    'PoolLoginResponse': PoolLoginResponse,
    'PoolUserView': PoolUserView,
    'Principal': Principal,
    'Problem': Problem,
    'ProjectView': ProjectView,
    'ProjectsResponse': ProjectsResponse,
    'PublishBody': PublishBody,
    'RelayEvent': RelayEvent,
    'RelayPage': RelayPage,
    'RelayState': RelayState,
    'RenameHostRequest': RenameHostRequest,
    'RenameProjectRequest': RenameProjectRequest,
    'ResourceRequest': ResourceRequest,
    'RevisionFileView': RevisionFileView,
    'RevisionMetaView': RevisionMetaView,
    'RevisionView': RevisionView,
    'RevisionsResponse': RevisionsResponse,
    'RoomMemberView': RoomMemberView,
    'RoomMemberViewList': RoomMemberViewList,
    'RoomView': RoomView,
    'RoomViewList': RoomViewList,
    'RunStepView': RunStepView,
    'RunView': RunView,
    'SendMessageBody': SendMessageBody,
    'SetupRequest': SetupRequest,
    'SetupStatusResponse': SetupStatusResponse,
    'StepOutputLimits': StepOutputLimits,
    'TriggerDocument': TriggerDocument,
    'TriggerEvent': TriggerEvent,
    'TriggerFilters': TriggerFilters,
    'TriggerInstallRequest': TriggerInstallRequest,
    'TriggerInstallResponse': TriggerInstallResponse,
    'TriggerIssue': TriggerIssue,
    'TriggerResourceFilter': TriggerResourceFilter,
    'TriggerRun': TriggerRun,
    'TriggerValidateResponse': TriggerValidateResponse,
    'UpdateConnectionRequest': UpdateConnectionRequest,
    'UpdateRoleRequest': UpdateRoleRequest,
    'ValidateResponse': ValidateResponse,
    'WebhookResponse': WebhookResponse,
}


__all__ = [
    'AcceptInvitationRequest',
    'ActivityEntry',
    'ActivityPage',
    'ActivityRunDetail',
    'AgentCapability',
    'AgentSelection',
    'ApiKeyView',
    'ApiKeysResponse',
    'ArrivalDetail',
    'CapabilityReportRequest',
    'ChallengeResponse',
    'ChangePasswordRequest',
    'ConfigFileRequest',
    'ConfigurationRequest',
    'ConnectionResourceView',
    'ConnectionView',
    'ConnectionsResponse',
    'CreateApiKeyRequest',
    'CreateApiKeyResponse',
    'CreateConnectionRequest',
    'CreateExecutionRequest',
    'CreateExecutionResponse',
    'CreateHostRequest',
    'CreateProjectRequest',
    'EnrollHostRequest',
    'EnrollmentTokenView',
    'FieldIssue',
    'GameError',
    'HistoryResponse',
    'HostCapabilityReport',
    'HostExecutionView',
    'HostView',
    'HostsResponse',
    'InstallResponse',
    'InterruptExecutionResponse',
    'InvitationsResponse',
    'InviteRequest',
    'InviteResponse',
    'ManualDispatchRequest',
    'ManualDispatchResponse',
    'MembersResponse',
    'MessageView',
    'MintEnrollmentTokenRequest',
    'MintEnrollmentTokenResponse',
    'OpenApiDocument',
    'OrganizationView',
    'OutputLimit',
    'PoolIdentityView',
    'PoolInvitationView',
    'PoolLoginRequest',
    'PoolLoginResponse',
    'PoolUserView',
    'Principal',
    'Problem',
    'ProjectView',
    'ProjectsResponse',
    'PublishBody',
    'RawJson',
    'RelayEvent',
    'RelayPage',
    'RelayState',
    'RenameHostRequest',
    'RenameProjectRequest',
    'ResourceRequest',
    'RevisionFileView',
    'RevisionMetaView',
    'RevisionView',
    'RevisionsResponse',
    'RoomMemberView',
    'RoomMemberViewList',
    'RoomView',
    'RoomViewList',
    'RunStepView',
    'RunView',
    'SendMessageBody',
    'SetupRequest',
    'SetupStatusResponse',
    'StepOutputLimits',
    'TriggerDocument',
    'TriggerEvent',
    'TriggerFilters',
    'TriggerInstallRequest',
    'TriggerInstallResponse',
    'TriggerIssue',
    'TriggerResourceFilter',
    'TriggerRun',
    'TriggerValidateResponse',
    'UpdateConnectionRequest',
    'UpdateRoleRequest',
    'ValidateResponse',
    'WebhookResponse',
    'MODEL_TYPES',
]
