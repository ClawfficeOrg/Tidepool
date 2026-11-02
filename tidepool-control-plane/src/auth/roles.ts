/**
 * The role-to-control mapping.
 *
 * The hub contract is the source of truth: every control names the operation
 * it will call, and the pool enforces the same rule on the request. This module
 * only decides what a signed-in role may be offered *before* submitting, so a
 * viewer never meets a button that would come back 403. The unit test
 * cross-checks every entry against the generated SDK route table, so a contract
 * change that narrows a route fails the mapping rather than drifting from it.
 *
 * A control the role cannot use is not rendered as a control. It is listed as
 * hidden with the reason, so a person understands the absence instead of
 * assuming the feature is broken.
 */

import type { Role } from './types';

/** One action the shell may offer, tied to the contract operation behind it. */
export interface ControlSpec {
  /** Stable id, used in markup and tests. */
  id: string;
  /** Short label a person reads. */
  label: string;
  /** One line on what the control does. */
  description: string;
  /** Operation id from the hub contract. */
  operationId: string;
  /** True when the operation changes pool state. */
  mutating: boolean;
  /** The role the contract narrows the operation to, or null for any member. */
  requiredRole: Role | null;
}

/** The controls this task's shell knows how to describe. */
export const CONTROLS: readonly ControlSpec[] = [
  {
    id: 'view-hosts',
    label: 'View hosts',
    description: 'Machines enrolled with this pool and what they may run.',
    operationId: 'get_v1_hosts',
    mutating: false,
    requiredRole: null,
  },
  {
    id: 'read-activity',
    label: 'Read activity',
    description: 'What arrived, what matched a trigger, and what ran.',
    operationId: 'get_v1_activity',
    mutating: false,
    requiredRole: null,
  },
  {
    id: 'list-members',
    label: 'List members',
    description: 'The operators in this organization.',
    operationId: 'get_v1_pool_members',
    mutating: false,
    requiredRole: null,
  },
  {
    id: 'invite-member',
    label: 'Invite a member',
    description: 'Send an operator invitation with a role.',
    operationId: 'post_v1_pool_invitations',
    mutating: true,
    requiredRole: 'owner',
  },
  {
    id: 'revoke-invitation',
    label: 'Revoke an invitation',
    description: 'Withdraw a pending invitation.',
    operationId: 'delete_v1_pool_invitations_id',
    mutating: true,
    requiredRole: 'owner',
  },
  {
    id: 'change-member-role',
    label: 'Change a member role',
    description: 'Move an operator between owner, admin, member and viewer.',
    operationId: 'patch_v1_pool_members_id',
    mutating: true,
    requiredRole: 'owner',
  },
  {
    id: 'remove-member',
    label: 'Remove a member',
    description: 'End an operator’s access to this pool.',
    operationId: 'delete_v1_pool_members_id',
    mutating: true,
    requiredRole: 'owner',
  },
  {
    id: 'create-api-key',
    label: 'Create an API key',
    description: 'Mint a scoped key for automation; the secret is shown once.',
    operationId: 'post_v1_api_keys',
    mutating: true,
    requiredRole: 'owner',
  },
  {
    id: 'revoke-api-key',
    label: 'Revoke an API key',
    description: 'Stop an automation key immediately.',
    operationId: 'delete_v1_api_keys_id',
    mutating: true,
    requiredRole: 'owner',
  },
  {
    id: 'enrol-host',
    label: 'Enrol a host',
    description: 'Register a machine with this pool.',
    operationId: 'post_v1_hosts',
    mutating: true,
    requiredRole: null,
  },
  {
    id: 'mint-enrolment-token',
    label: 'Mint an enrolment token',
    description: 'Create the one-time token a new host exchanges.',
    operationId: 'post_v1_hosts_enrollment_tokens',
    mutating: true,
    requiredRole: null,
  },
  {
    id: 'revoke-host',
    label: 'Revoke a host',
    description: 'End a machine’s relationship with this pool.',
    operationId: 'delete_v1_hosts_id',
    mutating: true,
    requiredRole: null,
  },
  {
    id: 'grant-host-execution',
    label: 'Grant execution',
    description: 'Allow a host to run agents.',
    operationId: 'post_v1_hosts_id_execution',
    mutating: true,
    requiredRole: null,
  },
  {
    id: 'revoke-host-execution',
    label: 'Revoke execution',
    description: 'Leave a host connected but unable to run agents.',
    operationId: 'delete_v1_hosts_id_execution',
    mutating: true,
    requiredRole: null,
  },
  {
    id: 'create-connection',
    label: 'Add a connection',
    description: 'Connect a provider account by credential reference.',
    operationId: 'post_v1_connections',
    mutating: true,
    requiredRole: null,
  },
  {
    id: 'update-connection',
    label: 'Update a connection',
    description: 'Rename, enable or disable a provider connection.',
    operationId: 'patch_v1_connections_id',
    mutating: true,
    requiredRole: null,
  },
  {
    id: 'delete-connection',
    label: 'Delete a connection',
    description: 'Remove a provider connection from the pool.',
    operationId: 'delete_v1_connections_id',
    mutating: true,
    requiredRole: null,
  },
  {
    id: 'create-project',
    label: 'Create a project',
    description: 'Group a configuration with the hosts and connections it uses.',
    operationId: 'post_v1_projects',
    mutating: true,
    requiredRole: null,
  },
  {
    id: 'install-configuration',
    label: 'Install a configuration',
    description: 'Store a validated configuration as a new revision.',
    operationId: 'post_v1_projects_id_configuration_install',
    mutating: true,
    requiredRole: null,
  },
  {
    id: 'install-trigger',
    label: 'Install a trigger',
    description: 'Make a validated trigger live for a project.',
    operationId: 'post_v1_triggers_install',
    mutating: true,
    requiredRole: null,
  },
  {
    id: 'dispatch-run',
    label: 'Dispatch a run',
    description: 'Start a workflow manually.',
    operationId: 'post_v1_runs',
    mutating: true,
    requiredRole: null,
  },
  {
    id: 'change-password',
    label: 'Change your password',
    description: 'Replace the password for your own operator account.',
    operationId: 'post_v1_pool_password',
    mutating: true,
    requiredRole: null,
  },
  {
    id: 'stop-relay',
    label: 'Stop session relay',
    description: 'End publication for a session; stopping is permanent.',
    operationId: 'delete_v1_sessions_session_relay',
    mutating: true,
    requiredRole: null,
  },
  {
    id: 'publish-relay-event',
    label: 'Publish a relay event',
    description: 'Send one allow-listed event for a published session.',
    operationId: 'post_v1_sessions_session_relay_events',
    mutating: true,
    requiredRole: null,
  },
];

/** A control the role cannot use, with the sentence that explains why. */
export interface HiddenControl {
  /** The control that is not offered. */
  control: ControlSpec;
  /** Why it is absent, in one readable sentence. */
  reason: string;
}

/** The role label a person reads. */
export function roleLabel(role: Role): string {
  switch (role) {
    case 'owner':
      return 'Owner';
    case 'admin':
      return 'Admin';
    case 'member':
      return 'Member';
    case 'viewer':
      return 'Viewer';
  }
}

/** One line on what a role may do. */
export function roleDescription(role: Role): string {
  switch (role) {
    case 'owner':
      return 'Full access, including members, API keys and the last-owner rules.';
    case 'admin':
      return 'Can change hosts, connections, projects, triggers and runs, but not members or keys.';
    case 'member':
      return 'Can change hosts, connections, projects, triggers and runs, but not members or keys.';
    case 'viewer':
      return 'Read-only: everything can be seen, nothing can be changed.';
  }
}

/** True when the role may use the control. */
export function canUseControl(role: Role | null, control: ControlSpec): boolean {
  if (role === null) return false;
  if (control.requiredRole !== null && role !== control.requiredRole) return false;
  if (control.mutating && role === 'viewer') return false;
  return true;
}

/** Why a role cannot use a control. Empty when it can. */
export function denialReason(role: Role | null, control: ControlSpec): string {
  if (role === null) return `${control.label} needs a signed-in member.`;
  if (control.requiredRole === 'owner' && role !== 'owner') {
    return `${control.label} is owner-only; only an owner can do it.`;
  }
  if (control.mutating && role === 'viewer') {
    return `${control.label} changes the pool, and the viewer role is read-only.`;
  }
  return '';
}

/** The controls the role may use, in table order. */
export function availableControls(role: Role | null): ControlSpec[] {
  return CONTROLS.filter((control) => canUseControl(role, control));
}

/** The controls the role may not use, each with its reason. */
export function hiddenControls(role: Role | null): HiddenControl[] {
  return CONTROLS.filter((control) => !canUseControl(role, control)).map((control) => ({
    control,
    reason: denialReason(role, control),
  }));
}

/** The controls that change pool state. */
export function mutatingControls(): ControlSpec[] {
  return CONTROLS.filter((control) => control.mutating);
}

/** Look one control up by id. */
export function controlById(id: string): ControlSpec | null {
  return CONTROLS.find((control) => control.id === id) ?? null;
}
