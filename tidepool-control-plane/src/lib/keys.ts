/**
 * Members and API keys: the model behind the settings page.
 *
 * Everything a person reads here — the role table, the last-owner rule with the
 * sentence that explains it, an invitation's expiry, a key's scopes, its
 * prefix, when it was last used, whether it is revoked, and the one-time
 * secret with its warning — is decided in this module. The components only
 * render it. That keeps every rule testable without a DOM, without a network
 * and without a pool.
 *
 * The contract is the source of truth (`docs/api/openapi-v1.json`):
 *
 * - A member's role is one of `owner`, `admin`, `member`, `viewer`. The pool
 *   keeps at least one owner, so the last owner cannot be removed or demoted.
 *   The page refuses first and prints why; a stale count still comes back as a
 *   409 problem from the pool and is shown as the pool's own words.
 * - An invitation carries an expiry and a one-time acceptance token. The pool
 *   stores only the token's hash, so the page shows the token once and says so.
 * - An API key's secret (`<prefix>.<secret>`) is returned exactly once, by
 *   creation. A revoked key stays in the list, marked revoked, because the
 *   criterion is that it is marked rather than disappearing.
 */

import type {
  ApiKeysResponse,
  CreateApiKeyRequest,
  CreateApiKeyResponse,
  InvitationsResponse,
  InviteRequest,
  InviteResponse,
  MembersResponse,
  PoolInvitationView,
  PoolUserView,
  TidepoolClient,
  UpdateRoleRequest,
} from '@clawffice/tidepool-sdk';
import { isRole, ROLES, type Role } from '../auth/types';
import { roleLabel } from '../auth/roles';
import { presentProblem, type PoolProblem } from './api/problem';
import { isPoolConfigured, type PoolConfig } from './api/pool';
import type { StatusKind } from './status';

/* ------------------------------------------------------------------------- */
/* API key scopes                                                            */
/* ------------------------------------------------------------------------- */

/** The scopes the hub contract allows on an API key. */
export type ApiKeyScope =
  | 'projects:read'
  | 'configuration:validate'
  | 'configuration:install'
  | 'runs:dispatch'
  | 'daemons:enroll';

/** One scope, in every form the page renders it. */
export interface ScopeDescriptor {
  /** The scope string the pool enforces. */
  scope: string;
  /** Short label a person reads. */
  label: string;
  /** One line saying what the scope permits. */
  description: string;
  /** True when the scope permits a change rather than a read. */
  mutating: boolean;
  /** True when this revision knows the scope. */
  known: boolean;
}

/** The scope catalog, in the contract's order. */
export const API_KEY_SCOPES: readonly ScopeDescriptor[] = [
  {
    scope: 'projects:read',
    label: 'Read projects',
    description: 'See projects, their configuration revisions and their activity.',
    mutating: false,
    known: true,
  },
  {
    scope: 'configuration:validate',
    label: 'Validate configuration',
    description: 'Run the pool’s validator without installing anything.',
    mutating: false,
    known: true,
  },
  {
    scope: 'configuration:install',
    label: 'Install configuration',
    description: 'Store a validated configuration as a new project revision.',
    mutating: true,
    known: true,
  },
  {
    scope: 'runs:dispatch',
    label: 'Dispatch runs',
    description: 'Start agent runs on this pool.',
    mutating: true,
    known: true,
  },
  {
    scope: 'daemons:enroll',
    label: 'Enrol daemons',
    description: 'Register automation daemons with this pool.',
    mutating: true,
    known: true,
  },
];

/** The scope names, in the contract's order. */
export const API_KEY_SCOPE_NAMES: readonly ApiKeyScope[] = API_KEY_SCOPES.map(
  (descriptor) => descriptor.scope as ApiKeyScope,
);

/** True when `value` names a scope this control plane knows. */
export function isApiKeyScope(value: unknown): value is ApiKeyScope {
  return typeof value === 'string' && (API_KEY_SCOPE_NAMES as readonly string[]).includes(value);
}

/**
 * Describe a scope.
 *
 * An unknown scope is never dropped from a key's row: it is shown by its raw
 * name with the note that the pool still enforces it. Hiding it would make a
 * key look less powerful than it is.
 */
export function describeScope(scope: string): ScopeDescriptor {
  const known = API_KEY_SCOPES.find((descriptor) => descriptor.scope === scope);
  if (known) return known;
  return {
    scope,
    label: scope,
    description: 'A scope this control plane does not know; the pool still enforces it.',
    mutating: false,
    known: false,
  };
}

/**
 * Keep only known scopes, remove duplicates and restore the contract's order.
 *
 * The request sends this result, so the same set of ticks always produces the
 * same body.
 */
export function normaliseScopes(scopes: readonly string[]): ApiKeyScope[] {
  const wanted = new Set(scopes);
  return API_KEY_SCOPE_NAMES.filter((scope) => wanted.has(scope));
}

/* ------------------------------------------------------------------------- */
/* Moments and expiry                                                        */
/* ------------------------------------------------------------------------- */

/** A timestamp as the page renders it. */
export interface MomentView {
  /** What the page shows; never empty. */
  text: string;
  /** The ISO instant, when one was reported. */
  at: string | null;
  /** True when a usable timestamp was reported. */
  reported: boolean;
}

/** A duration in the words the page uses. */
function durationText(milliseconds: number): string {
  const seconds = Math.max(1, Math.round(milliseconds / 1000));
  if (seconds < 60) return `${seconds} second${seconds === 1 ? '' : 's'}`;
  const minutes = Math.max(1, Math.round(seconds / 60));
  if (minutes < 60) return `${minutes} minute${minutes === 1 ? '' : 's'}`;
  const hours = Math.max(1, Math.round(minutes / 60));
  if (hours < 24) return `${hours} hour${hours === 1 ? '' : 's'}`;
  const days = Math.max(1, Math.round(hours / 24));
  return `${days} day${days === 1 ? '' : 's'}`;
}

/** Parse a timestamp, or null when it is missing or unreadable. */
function parseInstant(value: string | null | undefined): Date | null {
  if (value === null || value === undefined || value.trim() === '') return null;
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? null : parsed;
}

/** Build "verb duration" from a past timestamp. */
function momentView(
  value: string | null | undefined,
  now: number | Date,
  verb: string,
  neverText: string,
): MomentView {
  const then = parseInstant(value);
  if (!then) return { text: neverText, at: null, reported: false };
  const current = now instanceof Date ? now.getTime() : now;
  let elapsed = current - then.getTime();
  if (elapsed < 0) elapsed = 0; // clock skew reads as "just now", not "in the future"
  if (elapsed < 45_000) return { text: `${verb} just now`, at: then.toISOString(), reported: true };
  return {
    text: `${verb} ${durationText(elapsed)} ago`,
    at: then.toISOString(),
    reported: true,
  };
}

/** When a key was last used, in words; "Never used" when it has not been. */
export function keyUsageView(value: string | null | undefined, now: number | Date): MomentView {
  return momentView(value, now, 'Last used', 'Never used');
}

/** When a member last signed in, in words. */
export function memberLoginView(value: string | null | undefined, now: number | Date): MomentView {
  return momentView(value, now, 'Last signed in', 'Never signed in');
}

/** When an invitation stops being usable. */
export interface ExpiryView {
  /** What the page shows; never empty. */
  text: string;
  /** True when the instant has passed. */
  expired: boolean;
  /** The ISO instant, when one was reported. */
  at: string | null;
}

/**
 * Describe an invitation's expiry.
 *
 * The page shows both this sentence and the absolute instant in a `<time>`, so
 * "soon" never hides the exact moment.
 */
export function expiryView(value: string | null | undefined, now: number | Date): ExpiryView {
  const then = parseInstant(value);
  if (!then) return { text: 'Expiry not reported', expired: false, at: null };
  const current = now instanceof Date ? now.getTime() : now;
  const remaining = then.getTime() - current;
  if (remaining <= 0) {
    return {
      text: `Expired ${durationText(-remaining)} ago`,
      expired: true,
      at: then.toISOString(),
    };
  }
  return {
    text: `Expires in ${durationText(remaining)}`,
    expired: false,
    at: then.toISOString(),
  };
}

/* ------------------------------------------------------------------------- */
/* Members                                                                   */
/* ------------------------------------------------------------------------- */

/** The sentence shown when the last owner is about to be removed or demoted. */
export const LAST_OWNER_REASON =
  'This is the pool’s last owner. Removing or demoting them would leave no one who can manage members or API keys, so the pool refuses it. Invite another owner or promote someone first.';

/** One operator, as the page renders them. */
export interface MemberRow {
  /** Stable public id. */
  id: string;
  /** Login email. */
  email: string;
  /** Name a person sees. */
  displayName: string;
  /** Role the pool enforces. */
  role: Role;
  /** The word shown in the role column. */
  roleLabel: string;
  /** True while the pool still requires a password change. */
  passwordChangeRequired: boolean;
  /** Last successful sign-in, in words. */
  lastLogin: MomentView;
  /** When the operator was created. */
  createdAt: string | null;
}

/** The member shape this module reads; a superset of the contract's view. */
export interface MemberLike {
  id: string;
  email: string;
  display_name: string;
  role: string;
  password_change_required?: boolean;
  last_login_at?: string | null;
  created_at?: string | null;
}

/** Build one member row. */
export function memberRow(member: MemberLike, now: number | Date): MemberRow {
  const role: Role = isRole(member.role) ? member.role : 'viewer';
  return {
    id: member.id,
    email: member.email,
    displayName: member.display_name,
    role,
    roleLabel: isRole(member.role) ? roleLabel(member.role) : member.role,
    passwordChangeRequired: member.password_change_required ?? false,
    lastLogin: memberLoginView(member.last_login_at, now),
    createdAt: member.created_at ?? null,
  };
}

/** Order members by privilege, then name, so owners are always visible first. */
export function sortMemberRows(rows: readonly MemberRow[]): MemberRow[] {
  const rank = (row: MemberRow): number => {
    const index = ROLES.indexOf(row.role);
    return index === -1 ? ROLES.length : index;
  };
  return [...rows].sort((left, right) => {
    const byRole = rank(left) - rank(right);
    if (byRole !== 0) return byRole;
    return left.displayName.localeCompare(right.displayName);
  });
}

/** The last-owner rule for one member, and the sentence that explains it. */
export interface OwnerRule {
  /** True when this member is an owner and the only one. */
  isLastOwner: boolean;
  /** One sentence to show beside the disabled controls, or null. */
  reason: string | null;
}

/** Count the owners in a list of rows. */
export function ownerCount(members: readonly MemberRow[]): number {
  return members.filter((member) => member.role === 'owner').length;
}

/** Whether removing this member would leave the pool without an owner. */
export function removalRule(
  member: MemberRow,
  members: readonly MemberRow[],
): OwnerRule {
  const isLastOwner = member.role === 'owner' && ownerCount(members) <= 1;
  return {
    isLastOwner,
    reason: isLastOwner
      ? LAST_OWNER_REASON
      : null,
  };
}

/**
 * Whether changing this member's role to `next` would leave the pool without
 * an owner. Promoting, or changing a non-owner, is never blocked by this rule.
 */
export function roleChangeRule(
  member: MemberRow,
  members: readonly MemberRow[],
  next: Role,
): OwnerRule {
  if (member.role !== 'owner' || next === 'owner') {
    return { isLastOwner: false, reason: null };
  }
  const isLastOwner = ownerCount(members) <= 1;
  return {
    isLastOwner,
    reason: isLastOwner
      ? LAST_OWNER_REASON
      : null,
  };
}

/* ------------------------------------------------------------------------- */
/* Invitations                                                               */
/* ------------------------------------------------------------------------- */

/** The warning shown beside a newly minted invitation token. */
export const INVITATION_TOKEN_WARNING =
  'This acceptance token is shown once. Copy it and send it to the person you invited; the pool stores only its hash, so it cannot be shown again.';

/** One pending invitation, as the page renders it. */
export interface InvitationRow {
  /** Stable public id. */
  id: string;
  /** Who was invited. */
  email: string;
  /** The role they will hold. */
  role: Role;
  /** The word shown in the role column. */
  roleLabel: string;
  /** When the invitation was created. */
  createdAt: string | null;
  /** When it stops working, as the pool reported it. */
  expiresAt: string | null;
  /** The expiry in words, plus whether it has passed. */
  expiry: ExpiryView;
}

/** Build one invitation row. */
export function invitationRow(
  invitation: PoolInvitationView,
  now: number | Date,
): InvitationRow {
  const role: Role = isRole(invitation.role) ? invitation.role : 'viewer';
  return {
    id: invitation.id,
    email: invitation.email,
    role,
    roleLabel: isRole(invitation.role) ? roleLabel(invitation.role) : invitation.role,
    createdAt: invitation.created_at ?? null,
    expiresAt: invitation.expires_at ?? null,
    expiry: expiryView(invitation.expires_at, now),
  };
}

/** Order invitations by soonest expiry, so the one about to lapse is first. */
export function sortInvitationRows(rows: readonly InvitationRow[]): InvitationRow[] {
  return [...rows].sort((left, right) => {
    const leftAt = left.expiresAt ? Date.parse(left.expiresAt) : Number.POSITIVE_INFINITY;
    const rightAt = right.expiresAt ? Date.parse(right.expiresAt) : Number.POSITIVE_INFINITY;
    if (leftAt !== rightAt) return leftAt - rightAt;
    return left.email.localeCompare(right.email);
  });
}

/** What a successful invite returns: the row plus the one-time token. */
export interface CreatedInvitation {
  /** The pending invitation, with its expiry. */
  invitation: InvitationRow;
  /** The one-time acceptance token; shown until dismissed. */
  token: string;
  /** The copy-me-now warning. */
  warning: string;
}

/** Build the invite result from the pool's answer. */
export function createdInvitation(
  response: InviteResponse,
  now: number | Date,
): CreatedInvitation {
  return {
    invitation: invitationRow(response.invitation, now),
    token: response.token,
    warning: INVITATION_TOKEN_WARNING,
  };
}

/* ------------------------------------------------------------------------- */
/* API keys                                                                  */
/* ------------------------------------------------------------------------- */

/** The two states a key can be in. */
export type ApiKeyStatus = 'active' | 'revoked';

/** Every key status, in the order the page groups them. */
export const API_KEY_STATUSES: readonly ApiKeyStatus[] = ['active', 'revoked'];

/** One key status, in every form the page renders it. */
export interface ApiKeyStatusDescriptor {
  /** The status, or null for a value this revision does not know. */
  status: ApiKeyStatus | null;
  /** The word shown beside the icon. Never empty. */
  word: string;
  /** One line saying what the state means. */
  description: string;
  /** 24x24 SVG path, drawn with the palette's status colour. */
  icon: string;
  /** Colour role from `$lib/status`; state is never colour alone. */
  tone: StatusKind;
}

/** The key status table. */
export const API_KEY_STATUS_DESCRIPTORS: Readonly<
  Record<ApiKeyStatus, ApiKeyStatusDescriptor>
> = {
  active: {
    status: 'active',
    word: 'Active',
    description: 'This key authenticates requests until it is revoked.',
    icon: 'M12 3a9 9 0 100 18 9 9 0 000-18zM8 12.5l3 3 5-6',
    tone: 'success',
  },
  revoked: {
    status: 'revoked',
    word: 'Revoked',
    description:
      'Revoked keys are kept here for the audit trail but no longer authenticate anything.',
    icon: 'M12 3a9 9 0 100 18 9 9 0 000-18zM8 8l8 8M16 8l-8 8',
    tone: 'danger',
  },
};

/** The descriptor used for a status this revision cannot read. */
const UNKNOWN_KEY_STATUS: ApiKeyStatusDescriptor = {
  status: null,
  word: 'Unknown',
  description: 'The pool reported a status this control plane does not know.',
  icon: 'M9.5 9a2.5 2.5 0 115 .5c0 1.5-2.5 1.5-2.5 3.5M12 17h.01',
  tone: 'unknown',
};

/** True when `value` names one of the two contract statuses. */
export function isApiKeyStatus(value: unknown): value is ApiKeyStatus {
  return typeof value === 'string' && (API_KEY_STATUSES as readonly string[]).includes(value);
}

/** Look a key status up, falling back to a readable Unknown. */
export function describeApiKeyStatus(
  value: string | null | undefined,
): ApiKeyStatusDescriptor {
  return isApiKeyStatus(value) ? API_KEY_STATUS_DESCRIPTORS[value] : UNKNOWN_KEY_STATUS;
}

/** One API key, as the page renders it. */
export interface ApiKeyRow {
  /** Immutable public id. */
  id: string;
  /** Name the operator gave it. */
  name: string;
  /** The visible prefix; never the secret. */
  prefix: string;
  /** Scopes with their labels. */
  scopes: ScopeDescriptor[];
  /** Status with its icon and word. */
  status: ApiKeyStatusDescriptor;
  /** When the key was created. */
  createdAt: string | null;
  /** When it was last used, in words. */
  lastUsed: MomentView;
  /** When it was revoked, when it has been. */
  revokedAt: string | null;
}

/** The key shape this module reads; a superset of the contract's view. */
export interface ApiKeyLike {
  id: string;
  name: string;
  prefix: string;
  scopes: string[];
  status: string;
  created_at?: string | null;
  last_used_at?: string | null;
  revoked_at?: string | null;
}

/** Build one key row from a pool response. */
export function apiKeyRow(key: ApiKeyLike, now: number | Date): ApiKeyRow {
  return {
    id: key.id,
    name: key.name,
    prefix: key.prefix,
    scopes: key.scopes.map(describeScope),
    status: describeApiKeyStatus(key.status),
    createdAt: key.created_at ?? null,
    lastUsed: keyUsageView(key.last_used_at, now),
    revokedAt: key.revoked_at ?? null,
  };
}

/**
 * Order keys with active ones first, then revoked ones — both newest first.
 *
 * A revoked key is never filtered out: the page keeps it in place so a person
 * can see that revocation happened rather than guess why a row disappeared.
 */
export function sortApiKeyRows(rows: readonly ApiKeyRow[]): ApiKeyRow[] {
  const rank = (row: ApiKeyRow): number => (row.status.status === 'revoked' ? 1 : 0);
  const instant = (row: ApiKeyRow): number =>
    row.createdAt ? Date.parse(row.createdAt) : Number.NEGATIVE_INFINITY;
  return [...rows].sort((left, right) => {
    const byStatus = rank(left) - rank(right);
    if (byStatus !== 0) return byStatus;
    const byCreated = instant(right) - instant(left);
    if (Number.isFinite(byCreated) && byCreated !== 0) return byCreated;
    return left.name.localeCompare(right.name);
  });
}

/** The warning shown above a newly created key's secret. */
export const API_KEY_SECRET_WARNING =
  'The secret is shown once. Copy it now: the pool stores only a hash, so it will not be shown again — not after you leave this page, not after a reload, and not by anyone else.';

/** The instruction that makes copying the obvious next action. */
export const API_KEY_SECRET_NEXT_ACTION =
  'Copy the secret and store it in the place your automation reads it from.';

/** What a successful key creation returns. */
export interface CreatedApiKey {
  /** The key's metadata, already in the list. */
  key: ApiKeyRow;
  /** The full `<prefix>.<secret>` value, shown once. */
  secret: string;
  /** The copy-me-now warning. */
  warning: string;
  /** The sentence that says what to do next. */
  nextAction: string;
}

/** Build the creation result from the pool's answer. */
export function createdApiKey(
  response: CreateApiKeyResponse,
  now: number | Date,
): CreatedApiKey {
  return {
    key: apiKeyRow(response.api_key, now),
    secret: response.secret,
    warning: API_KEY_SECRET_WARNING,
    nextAction: API_KEY_SECRET_NEXT_ACTION,
  };
}

/** The clipboard surface this module needs. */
export interface ClipboardWriter {
  /** Write text to the clipboard; rejects when the browser refuses. */
  writeText(text: string): Promise<void>;
}

/**
 * Copy a secret exactly as the pool returned it.
 *
 * Returns false rather than throwing when no clipboard exists or the browser
 * refuses, so the page can say "copy it by hand" beside the text instead of
 * showing a failure for a permission the person can grant.
 */
export async function copySecret(
  secret: string,
  writer: ClipboardWriter | null | undefined,
): Promise<boolean> {
  if (!writer || secret === '') return false;
  try {
    await writer.writeText(secret);
    return true;
  } catch {
    return false;
  }
}

/* ------------------------------------------------------------------------- */
/* Form validation                                                           */
/* ------------------------------------------------------------------------- */

/** The longest key name the page sends. */
export const MAX_KEY_NAME_LENGTH = 80;

/** A validated invite form, or the reason it is not valid. */
export type InviteValidation =
  | { ok: true; input: { email: string; role: Role } }
  | { ok: false; field: 'email' | 'role'; message: string };

/** A deliberately simple email check: the pool is still the authority. */
const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

/** Validate the invite form before any request is made. */
export function validateInviteInput(input: {
  email: string;
  role: string;
}): InviteValidation {
  const email = input.email.trim();
  if (email === '') {
    return { ok: false, field: 'email', message: 'Enter the email address of the person to invite.' };
  }
  if (!EMAIL_PATTERN.test(email)) {
    return {
      ok: false,
      field: 'email',
      message: 'Enter a single email address, such as ada@example.com.',
    };
  }
  if (!isRole(input.role)) {
    return {
      ok: false,
      field: 'role',
      message: 'Choose one of owner, admin, member or viewer.',
    };
  }
  return { ok: true, input: { email, role: input.role } };
}

/** A validated key form, or the reason it is not valid. */
export type KeyValidation =
  | { ok: true; input: { name: string; scopes: ApiKeyScope[] } }
  | { ok: false; field: 'name' | 'scopes'; message: string };

/** Validate the key creation form before any request is made. */
export function validateKeyInput(input: {
  name: string;
  scopes: readonly string[];
}): KeyValidation {
  const name = input.name.trim();
  if (name === '') {
    return { ok: false, field: 'name', message: 'Give the key a name, so its use is obvious later.' };
  }
  if (name.length > MAX_KEY_NAME_LENGTH) {
    return {
      ok: false,
      field: 'name',
      message: `A key name is at most ${MAX_KEY_NAME_LENGTH} characters.`,
    };
  }
  if (input.scopes.length === 0) {
    return {
      ok: false,
      field: 'scopes',
      message: 'Choose at least one scope. A key with no scopes can do nothing.',
    };
  }
  const unknown = input.scopes.filter((scope) => !isApiKeyScope(scope));
  if (unknown.length > 0) {
    return {
      ok: false,
      field: 'scopes',
      message: `This revision does not know the scope ${unknown[0] ?? ''}.`,
    };
  }
  const scopes = normaliseScopes(input.scopes);
  if (scopes.length === 0) {
    return {
      ok: false,
      field: 'scopes',
      message: 'Choose at least one scope. A key with no scopes can do nothing.',
    };
  }
  return { ok: true, input: { name, scopes } };
}

/* ------------------------------------------------------------------------- */
/* Transport                                                                 */
/* ------------------------------------------------------------------------- */

/** The pool calls the settings page makes, injectable for tests. */
export interface MembersKeysTransport {
  /** List the organization's operators. */
  listMembers(): Promise<MembersResponse>;
  /** Invite an operator; the answer carries the one-time token. */
  inviteMember(input: InviteRequest): Promise<InviteResponse>;
  /** Change one member's role. */
  changeMemberRole(id: string, role: Role): Promise<PoolUserView>;
  /** Remove one member. */
  removeMember(id: string): Promise<void>;
  /** List pending invitations. */
  listInvitations(): Promise<InvitationsResponse>;
  /** Withdraw a pending invitation. */
  revokeInvitation(id: string): Promise<void>;
  /** List the organization's API keys (metadata only). */
  listKeys(): Promise<ApiKeysResponse>;
  /** Create a key; the answer carries the one-time secret. */
  createKey(input: CreateApiKeyRequest): Promise<CreateApiKeyResponse>;
  /** Revoke a key immediately. */
  revokeKey(id: string): Promise<void>;
}

/** Build the transport from the typed SDK client. */
export function membersKeysTransport(client: TidepoolClient): MembersKeysTransport {
  return {
    listMembers: () => client.api.pool.members(),
    inviteMember: (input) => client.api.pool.invite(input),
    changeMemberRole: (id, role) => {
      const body: UpdateRoleRequest = { role };
      return client.api.pool.updateMember(id, body);
    },
    removeMember: (id) => client.api.pool.removeMember(id),
    listInvitations: () => client.api.pool.invitations(),
    revokeInvitation: (id) => client.api.pool.revokeInvitation(id),
    listKeys: () => client.api.apiKeys.list(),
    createKey: (input) => client.api.apiKeys.create(input),
    revokeKey: (id) => client.api.apiKeys.remove(id),
  };
}

/* ------------------------------------------------------------------------- */
/* Controller                                                                */
/* ------------------------------------------------------------------------- */

/** What the settings page is doing. */
export type MembersKeysPhase = 'idle' | 'loading' | 'ready' | 'error';

/** Everything the settings page renders. */
export interface MembersKeysState {
  /** The current load state. */
  phase: MembersKeysPhase;
  /** Operators, owners first. */
  members: MemberRow[];
  /** Pending invitations, soonest expiry first. */
  invitations: InvitationRow[];
  /** API keys, active first and revoked kept. */
  keys: ApiKeyRow[];
  /** The one-time key secret, until dismissed. */
  createdKey: CreatedApiKey | null;
  /** The one-time invitation token, until dismissed. */
  createdInvitation: CreatedInvitation | null;
  /** The last failure, readable. */
  problem: PoolProblem | null;
  /** A successful change, in one sentence. */
  notice: string | null;
  /** True while a request is in flight. */
  busy: boolean;
  /** The member or key whose row is busy, when one is. */
  busyId: string | null;
}

/** The state before anything has been loaded. */
export function emptyMembersKeysState(): MembersKeysState {
  return {
    phase: 'idle',
    members: [],
    invitations: [],
    keys: [],
    createdKey: null,
    createdInvitation: null,
    problem: null,
    notice: null,
    busy: false,
    busyId: null,
  };
}

/** How the controller reaches the pool, and what clock it uses. */
export interface MembersKeysControllerOptions {
  /** The pool calls. */
  transport: MembersKeysTransport;
  /** Clock, injectable so expiry and last-used tests are deterministic. */
  now?: () => number | Date;
}

/** Build an app-level refusal in the shape the page already renders. */
function refusal(title: string, message: string, code: string): PoolProblem {
  return {
    kind: 'app',
    title,
    message,
    requestId: null,
    status: null,
    code,
    type: null,
    retryAfterMs: null,
  };
}

/**
 * The settings page's controller.
 *
 * Every method returns the new state rather than mutating what the caller
 * holds, so the page can assign the result and a test can assert on it.
 * Failures are caught and presented as problem details; nothing throws across
 * the boundary. The last-owner rule is checked here before the request, with
 * the same sentence the row shows beside its disabled controls.
 */
export class MembersKeysController {
  private readonly transport: MembersKeysTransport;
  private readonly now: () => number | Date;
  private state: MembersKeysState = emptyMembersKeysState();

  constructor(options: MembersKeysControllerOptions) {
    this.transport = options.transport;
    this.now = options.now ?? (() => Date.now());
  }

  /** The current state, without touching the pool. */
  snapshot(): MembersKeysState {
    return this.state;
  }

  /**
   * Load members, invitations and keys.
   *
   * `GET /v1/pool/members` allows any signed-in member, but invitations and
   * API keys are owner-only. `includeOwnerData` therefore defaults to true for
   * owners and is false for everyone else, who would otherwise meet a 403 just
   * by opening the page. A viewer still sees the member list, read-only.
   *
   * A reload drops any secret or acceptance token still on screen: the pool
   * cannot return them again, so keeping a stale one would suggest otherwise.
   */
  async load(options: { includeOwnerData?: boolean } = {}): Promise<MembersKeysState> {
    const includeOwnerData = options.includeOwnerData ?? true;
    this.state = {
      ...this.state,
      phase: this.state.phase === 'ready' ? 'ready' : 'loading',
      busy: true,
      createdKey: null,
      createdInvitation: null,
    };
    try {
      const [members, invitations, keys] = await Promise.all([
        this.transport.listMembers(),
        includeOwnerData
          ? this.transport.listInvitations()
          : Promise.resolve({ invitations: [] }),
        includeOwnerData ? this.transport.listKeys() : Promise.resolve({ api_keys: [] }),
      ]);
      const now = this.now();
      this.state = {
        ...this.state,
        phase: 'ready',
        busy: false,
        problem: null,
        members: sortMemberRows(members.members.map((member) => memberRow(member, now))),
        invitations: sortInvitationRows(
          invitations.invitations.map((invitation) => invitationRow(invitation, now)),
        ),
        keys: sortApiKeyRows(keys.api_keys.map((key) => apiKeyRow(key, now))),
      };
    } catch (error) {
      this.state = {
        ...this.state,
        phase: 'error',
        busy: false,
        problem: presentProblem(error),
      };
    }
    return this.state;
  }

  /** Invite an operator and hold the one-time token until it is dismissed. */
  async invite(input: { email: string; role: string }): Promise<MembersKeysState> {
    const validation = validateInviteInput(input);
    if (!validation.ok) {
      this.state = {
        ...this.state,
        problem: refusal('That invitation is not complete', validation.message, 'invalid_invite_input'),
      };
      return this.state;
    }
    this.state = { ...this.state, busy: true, busyId: null, problem: null, notice: null };
    try {
      const response = await this.transport.inviteMember(validation.input);
      const created = createdInvitation(response, this.now());
      this.state = {
        ...this.state,
        busy: false,
        phase: 'ready',
        createdInvitation: created,
        invitations: sortInvitationRows([...this.state.invitations, created.invitation]),
        notice: `Invitation created for ${created.invitation.email}. ${created.invitation.expiry.text}.`,
      };
    } catch (error) {
      this.state = { ...this.state, busy: false, problem: presentProblem(error) };
    }
    return this.state;
  }

  /** Change one member's role, refusing to demote the last owner. */
  async changeRole(memberId: string, role: Role): Promise<MembersKeysState> {
    const member = this.state.members.find((row) => row.id === memberId);
    if (!member) {
      this.state = {
        ...this.state,
        problem: refusal('That member is not in the list', 'Reload the members and try again.', 'member_not_found'),
      };
      return this.state;
    }
    const rule = roleChangeRule(member, this.state.members, role);
    if (rule.reason) {
      this.state = {
        ...this.state,
        problem: refusal(
          'That change would leave the pool without an owner',
          rule.reason,
          'last_owner',
        ),
      };
      return this.state;
    }
    if (member.role === role) return this.state;
    this.state = { ...this.state, busy: true, busyId: memberId, problem: null, notice: null };
    try {
      const updated = await this.transport.changeMemberRole(memberId, role);
      const row = memberRow(updated, this.now());
      this.state = {
        ...this.state,
        busy: false,
        busyId: null,
        members: sortMemberRows(
          this.state.members.map((candidate) => (candidate.id === memberId ? row : candidate)),
        ),
        notice: `${row.displayName} is now ${row.roleLabel.toLowerCase()}.`,
      };
    } catch (error) {
      this.state = {
        ...this.state,
        busy: false,
        busyId: null,
        problem: presentProblem(error),
      };
    }
    return this.state;
  }

  /** Remove a member, refusing to remove the last owner. */
  async removeMember(memberId: string): Promise<MembersKeysState> {
    const member = this.state.members.find((row) => row.id === memberId);
    if (!member) {
      this.state = {
        ...this.state,
        problem: refusal('That member is not in the list', 'Reload the members and try again.', 'member_not_found'),
      };
      return this.state;
    }
    const rule = removalRule(member, this.state.members);
    if (rule.reason) {
      this.state = {
        ...this.state,
        problem: refusal('That member is the last owner', rule.reason, 'last_owner'),
      };
      return this.state;
    }
    this.state = { ...this.state, busy: true, busyId: memberId, problem: null, notice: null };
    try {
      await this.transport.removeMember(memberId);
      this.state = {
        ...this.state,
        busy: false,
        busyId: null,
        members: this.state.members.filter((candidate) => candidate.id !== memberId),
        notice: `${member.displayName} no longer has access to this pool.`,
      };
    } catch (error) {
      this.state = {
        ...this.state,
        busy: false,
        busyId: null,
        problem: presentProblem(error),
      };
    }
    return this.state;
  }

  /** Withdraw a pending invitation. */
  async revokeInvitation(invitationId: string): Promise<MembersKeysState> {
    const invitation = this.state.invitations.find((row) => row.id === invitationId);
    if (!invitation) return this.state;
    this.state = { ...this.state, busy: true, busyId: invitationId, problem: null, notice: null };
    try {
      await this.transport.revokeInvitation(invitationId);
      this.state = {
        ...this.state,
        busy: false,
        busyId: null,
        invitations: this.state.invitations.filter((row) => row.id !== invitationId),
        createdInvitation:
          this.state.createdInvitation?.invitation.id === invitationId
            ? null
            : this.state.createdInvitation,
        notice: `Invitation for ${invitation.email} revoked.`,
      };
    } catch (error) {
      this.state = {
        ...this.state,
        busy: false,
        busyId: null,
        problem: presentProblem(error),
      };
    }
    return this.state;
  }

  /** Mint a key and hold the one-time secret until it is dismissed. */
  async createKey(input: { name: string; scopes: readonly string[] }): Promise<MembersKeysState> {
    const validation = validateKeyInput(input);
    if (!validation.ok) {
      this.state = {
        ...this.state,
        problem: refusal('That key is not complete', validation.message, 'invalid_key_input'),
      };
      return this.state;
    }
    this.state = { ...this.state, busy: true, busyId: null, problem: null, notice: null };
    try {
      const response = await this.transport.createKey(validation.input);
      const created = createdApiKey(response, this.now());
      this.state = {
        ...this.state,
        busy: false,
        phase: 'ready',
        createdKey: created,
        keys: sortApiKeyRows([created.key, ...this.state.keys]),
        notice: `Key ${created.key.name} created. Its secret is shown once.`,
      };
    } catch (error) {
      this.state = { ...this.state, busy: false, problem: presentProblem(error) };
    }
    return this.state;
  }

  /** Forget the one-time secret: it is shown once, then gone. */
  dismissCreatedKey(): MembersKeysState {
    this.state = { ...this.state, createdKey: null };
    return this.state;
  }

  /** Forget the one-time invitation token. */
  dismissCreatedInvitation(): MembersKeysState {
    this.state = { ...this.state, createdInvitation: null };
    return this.state;
  }

  /**
   * Revoke a key and keep its row, marked revoked.
   *
   * The 204 carries no body, so the row is updated locally with the revocation
   * instant. It stays in the list: the point of the criterion is that a person
   * sees the key was revoked, not that it vanished.
   */
  async revokeKey(keyId: string): Promise<MembersKeysState> {
    const key = this.state.keys.find((row) => row.id === keyId);
    if (!key) return this.state;
    this.state = { ...this.state, busy: true, busyId: keyId, problem: null, notice: null };
    try {
      await this.transport.revokeKey(keyId);
      const now = this.now();
      const revokedAt = new Date(now).toISOString();
      this.state = {
        ...this.state,
        busy: false,
        busyId: null,
        keys: sortApiKeyRows(
          this.state.keys.map((row) =>
            row.id === keyId
              ? { ...row, status: API_KEY_STATUS_DESCRIPTORS.revoked, revokedAt }
              : row,
          ),
        ),
        notice: `Key ${key.name} revoked. It no longer authenticates anything.`,
      };
    } catch (error) {
      this.state = {
        ...this.state,
        busy: false,
        busyId: null,
        problem: presentProblem(error),
      };
    }
    return this.state;
  }

  /** Forget a failure, e.g. after reading the reason. */
  clearProblem(): MembersKeysState {
    this.state = { ...this.state, problem: null };
    return this.state;
  }

  /** Forget a success notice. */
  clearNotice(): MembersKeysState {
    this.state = { ...this.state, notice: null };
    return this.state;
  }
}

/** True when this pool config and session can load members and keys. */
export function canLoadMembersKeys(
  config: PoolConfig,
  token: string | null | undefined,
): boolean {
  return isPoolConfigured(config) && typeof token === 'string' && token.trim() !== '';
}

/** The counts shown in the members heading. */
export interface MemberCounts {
  /** Owners in the organization. */
  owners: number;
  /** Every operator. */
  members: number;
  /** Invitations still awaiting acceptance. */
  invitations: number;
  /** Keys that still authenticate. */
  activeKeys: number;
  /** Keys that no longer authenticate but remain listed. */
  revokedKeys: number;
}

/** Count members, invitations and keys for the summary line. */
export function membersKeysCounts(state: MembersKeysState): MemberCounts {
  return {
    owners: state.members.filter((member) => member.role === 'owner').length,
    members: state.members.length,
    invitations: state.invitations.length,
    activeKeys: state.keys.filter((key) => key.status.status === 'active').length,
    revokedKeys: state.keys.filter((key) => key.status.status === 'revoked').length,
  };
}

/** The one-line summary shown above the members list. */
export function membersSummary(counts: MemberCounts): string {
  if (counts.members === 0) return 'No operators yet.';
  const parts = [
    `${counts.members} operator${counts.members === 1 ? '' : 's'}`,
    `${counts.owners} owner${counts.owners === 1 ? '' : 's'}`,
  ];
  if (counts.invitations > 0) {
    parts.push(`${counts.invitations} pending invitation${counts.invitations === 1 ? '' : 's'}`);
  }
  return `${parts.join(', ')}.`;
}

/** The one-line summary shown above the key list. */
export function keysSummary(counts: MemberCounts): string {
  if (counts.activeKeys === 0 && counts.revokedKeys === 0) return 'No API keys yet.';
  const parts = [`${counts.activeKeys} active`];
  if (counts.revokedKeys > 0) {
    parts.push(`${counts.revokedKeys} revoked`);
  }
  return `${parts.join(', ')} key${counts.activeKeys + counts.revokedKeys === 1 ? '' : 's'}.`;
}

/** The consequence printed beside removing a member. */
export const REMOVE_MEMBER_CONSEQUENCE =
  'Removing a member ends their access immediately: they are signed out everywhere and cannot sign in again until invited.';

/** The consequence printed beside revoking a key. */
export const REVOKE_KEY_CONSEQUENCE =
  'Revoking a key stops it immediately and cannot be undone. The key stays in this list, marked revoked, for the audit trail.';
