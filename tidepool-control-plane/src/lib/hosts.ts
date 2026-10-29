/**
 * Hosts: the model behind the hosts page.
 *
 * The page itself is a rendering of this module. Everything a person reads —
 * the four statuses with their words and icons, whether a host may run agents,
 * the consequence of granting or revoking that permission, and the exact
 * command a new host runs — is decided here, so it can be tested without a
 * DOM, without a network and without a pool. The controller is the only part
 * that touches the pool, and it takes the transport by injection so the
 * enrolment flow can be walked end to end against a fake pool.
 *
 * The contract is the source of truth (`docs/HOST_PROTOCOL.md`, `HostView` in
 * `docs/api/openapi-v1.json`). Two consequences are worth naming:
 *
 * - `execution_enabled` is a separate grant from enrolment and connection.
 *   A host with status `connected` and no grant is *connected-only*: the page
 *   says so in those words and offers the grant, not a dispatch control.
 * - This contract revision carries no last-seen timestamp. The page still
 *   shows a last-seen field for every host (the goal asks for it), reading
 *   `last_seen_at` when a pool sends one and saying plainly that the pool did
 *   not report it when it does not. It never invents a time from `created_at`.
 */

import type {
  CreateHostRequest,
  HostView as ContractHostView,
  HostsResponse,
  MintEnrollmentTokenResponse,
  TidepoolClient,
} from '@clawffice/tidepool-sdk';
import { presentProblem, type PoolProblem } from './api/problem';
import { isPoolConfigured, normalisePoolUrl, type PoolConfig } from './api/pool';
import type { StatusKind } from './status';

/** The four lifecycle states the pool can report for a host. */
export type HostStatus = 'pending_approval' | 'connected' | 'offline' | 'revoked';

/** Every host status, in the order the page lists them. */
export const HOST_STATUSES: readonly HostStatus[] = [
  'pending_approval',
  'connected',
  'offline',
  'revoked',
];

/** One host status, in every form the page renders it. */
export interface HostStatusDescriptor {
  /** The status, or null for a value this revision does not know. */
  status: HostStatus | null;
  /** The word shown beside the icon. Never empty. */
  word: string;
  /** One line saying what the state means for the host. */
  description: string;
  /** 24x24 SVG path, drawn with the palette's status colour. */
  icon: string;
  /** Colour role from `$lib/status`; state is never colour alone. */
  tone: StatusKind;
}

/**
 * The host status table.
 *
 * The words are the page's own, not the generic `StatusBadge` words: "Approval
 * required" tells an operator what to do, where "Pending" does not. Each entry
 * still carries a `tone`, so the design rule that state needs colour + icon +
 * word holds.
 */
export const HOST_STATUS_DESCRIPTORS: Readonly<Record<HostStatus, HostStatusDescriptor>> = {
  pending_approval: {
    status: 'pending_approval',
    word: 'Approval required',
    description:
      'Registered with the pool, but no host has exchanged its enrolment token yet.',
    icon: 'M12 3a9 9 0 100 18 9 9 0 000-18zM12 7.5V12l3 2',
    tone: 'pending',
  },
  connected: {
    status: 'connected',
    word: 'Connected',
    description: 'The host holds a live connection to the pool.',
    icon: 'M9.5 14.5l5-5M7.5 17.5a4 4 0 01-1-6.5l2-2a4 4 0 015.5 0M16.5 6.5a4 4 0 011 6.5l-2 2a4 4 0 01-5.5 0',
    tone: 'success',
  },
  offline: {
    status: 'offline',
    word: 'Offline',
    description: 'Enrolled, but the pool has no live connection to it.',
    icon: 'M12 3a9 9 0 100 18 9 9 0 000-18zM5 5l14 14',
    tone: 'warning',
  },
  revoked: {
    status: 'revoked',
    word: 'Revoked',
    description: 'The relationship is over; the pool refuses this host everywhere.',
    icon: 'M12 3a9 9 0 100 18 9 9 0 000-18zM8 8l8 8M16 8l-8 8',
    tone: 'danger',
  },
};

/** The descriptor used for a status this revision cannot read. */
const UNKNOWN_HOST_STATUS: HostStatusDescriptor = {
  status: null,
  word: 'Unknown',
  description: 'The pool reported a status this control plane does not know.',
  icon: 'M9.5 9a2.5 2.5 0 115 .5c0 1.5-2.5 1.5-2.5 3.5M12 17h.01',
  tone: 'unknown',
};

/** True when `value` names one of the four contract statuses. */
export function isHostStatus(value: unknown): value is HostStatus {
  return typeof value === 'string' && (HOST_STATUSES as readonly string[]).includes(value);
}

/** Look a host status up, falling back to a readable Unknown. */
export function describeHostStatus(value: string | null | undefined): HostStatusDescriptor {
  return isHostStatus(value) ? HOST_STATUS_DESCRIPTORS[value] : UNKNOWN_HOST_STATUS;
}

/** Whether a host may run agents, and the words for it. */
export interface ExecutionState {
  /** True when the pool holds the `host.execute` grant. */
  enabled: boolean;
  /** Short label: "May run agents" or "Connected only". */
  word: string;
  /** One line for the definition list and the accessible label. */
  description: string;
}

/**
 * Whether this host may run agents.
 *
 * A host that is connected without the grant is *connected-only*, the term the
 * host protocol uses. A host that is not connected at all is not called
 * connected-only: it is enrolled but not permitted to execute, which is a
 * different thing to fix.
 */
export function executionState(
  status: string | null | undefined,
  enabled: boolean,
): ExecutionState {
  if (enabled) {
    return {
      enabled: true,
      word: 'May run agents',
      description: 'host.execute is granted: the pool may dispatch runs to this host.',
    };
  }
  if (isHostStatus(status) && status === 'connected') {
    return {
      enabled: false,
      word: 'Connected only',
      description:
        'Connected, but host.execute is not granted, so the pool will not dispatch runs here.',
    };
  }
  return {
    enabled: false,
    word: 'Execution not granted',
    description: 'host.execute is not granted, so the pool will not dispatch runs here.',
  };
}

/** The last-seen field for one host. */
export interface LastSeenView {
  /** What the page shows; never empty. */
  text: string;
  /** The ISO instant, when the pool reported one. */
  at: string | null;
  /** True when the pool reported no timestamp for this contract revision. */
  reported: boolean;
}

/** The sentence used when the pool does not send a last-seen time. */
export const LAST_SEEN_UNREPORTED =
  'Last seen not reported: this contract revision has no host last-seen timestamp.';

/**
 * Format a timestamp as a short relative age.
 *
 * Deterministic given `now`, so the tests can pin it. Future timestamps (clock
 * skew between pool and browser) read as "just now" rather than "in -3
 * minutes".
 */
export function relativeAge(timestamp: string | number | Date, now: number | Date): string {
  const then = timestamp instanceof Date ? timestamp.getTime() : new Date(timestamp).getTime();
  const current = now instanceof Date ? now.getTime() : now;
  if (!Number.isFinite(then) || !Number.isFinite(current)) return 'unknown';
  let seconds = Math.round((current - then) / 1000);
  if (seconds < 0) seconds = 0;
  if (seconds < 60) return 'just now';
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} minute${minutes === 1 ? '' : 's'} ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} hour${hours === 1 ? '' : 's'} ago`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${days} day${days === 1 ? '' : 's'} ago`;
  return new Date(then).toISOString().slice(0, 10);
}

/**
 * Build the last-seen field.
 *
 * @param value the timestamp the pool sent, or null
 * @param now the instant to measure against
 */
export function lastSeenView(
  value: string | null | undefined,
  now: number | Date,
): LastSeenView {
  if (value === null || value === undefined || value.trim() === '') {
    return { text: LAST_SEEN_UNREPORTED, at: null, reported: false };
  }
  const then = new Date(value);
  if (Number.isNaN(then.getTime())) {
    return { text: LAST_SEEN_UNREPORTED, at: null, reported: false };
  }
  return {
    text: `Last seen ${relativeAge(then, now)}`,
    at: then.toISOString(),
    reported: true,
  };
}

/** One host, as the page renders it. */
export interface HostRow {
  /** Immutable public id. */
  id: string;
  /** Mutable friendly handle. */
  slug: string;
  /** Name a person sees in the list. */
  displayName: string;
  /** Hostname reported at registration. */
  hostname: string;
  /** Lifecycle status with its icon and word. */
  status: HostStatusDescriptor;
  /** Whether the host may run agents. */
  execution: ExecutionState;
  /** Last connection observed, or the reason the pool did not report one. */
  lastSeen: LastSeenView;
  /** When the relationship ended, when it has. */
  revokedAt: string | null;
  /** When the host was registered. */
  createdAt: string | null;
}

/** The host shape this module reads; a superset of the contract's `HostView`. */
export interface HostLike {
  id: string;
  slug: string;
  display_name: string;
  hostname: string;
  status: string;
  execution_enabled: boolean;
  revoked_at?: string | null;
  created_at?: string | null;
  /** Not in the contract today; read when a pool starts sending it. */
  last_seen_at?: string | null;
}

/**
 * Build one row from a pool response.
 *
 * @param host the contract view, or anything shaped like it
 * @param now the instant used for the last-seen age
 */
export function hostRow(host: HostLike, now: number | Date): HostRow {
  return {
    id: host.id,
    slug: host.slug,
    displayName: host.display_name,
    hostname: host.hostname,
    status: describeHostStatus(host.status),
    execution: executionState(host.status, host.execution_enabled),
    lastSeen: lastSeenView(host.last_seen_at, now),
    revokedAt: host.revoked_at ?? null,
    createdAt: host.created_at ?? null,
  };
}

/**
 * Order rows by attention first: approval required, connected, offline,
 * revoked, then by slug so the list is stable between refreshes.
 */
export function sortHostRows(rows: readonly HostRow[]): HostRow[] {
  const rank = (row: HostRow): number => {
    switch (row.status.status) {
      case 'pending_approval':
        return 0;
      case 'connected':
        return 1;
      case 'offline':
        return 2;
      case 'revoked':
        return 3;
      default:
        return 4;
    }
  };
  return [...rows].sort((left, right) => {
    const byRank = rank(left) - rank(right);
    if (byRank !== 0) return byRank;
    return left.slug.localeCompare(right.slug);
  });
}

/** An action offered on one host row. */
export interface HostAction {
  /** Stable id, used in markup and tests. */
  id: 'grant-execution' | 'revoke-execution';
  /** The role-table control this action is gated by. */
  controlId: 'grant-host-execution' | 'revoke-host-execution';
  /** The operation id from the hub contract. */
  operationId: string;
  /** Button label before confirming. */
  label: string;
  /** What the action does, shown before it is confirmed. */
  consequence: string;
  /** The consequence restated at the point of confirmation. */
  confirmConsequence: string;
  /** The button label that actually performs it. */
  confirmLabel: string;
}

/** What granting execution does, in one sentence. */
export const GRANT_CONSEQUENCE =
  'Granting execution lets this host run agents: the pool may start agent sessions on it and dispatch runs to it. Enrolment and connection alone do not permit that.';

/** What revoking execution does, in one sentence. */
export const REVOKE_CONSEQUENCE =
  'Revoking execution stops new agent sessions on this host. It stays enrolled and connected, but the pool will no longer dispatch runs to it until execution is granted again.';

/**
 * The execution action for a row, or null when none applies.
 *
 * A revoked host has no execution action: granting it would be meaningless and
 * revoking it cannot change anything.
 */
export function executionAction(row: HostRow): HostAction | null {
  if (row.status.status === 'revoked') return null;
  if (row.execution.enabled) {
    return {
      id: 'revoke-execution',
      controlId: 'revoke-host-execution',
      operationId: 'delete_v1_hosts_id_execution',
      label: 'Revoke execution',
      consequence: REVOKE_CONSEQUENCE,
      confirmConsequence: `Confirm: ${row.slug} stays connected but can no longer run agents.`,
      confirmLabel: `Revoke execution for ${row.slug}`,
    };
  }
  return {
    id: 'grant-execution',
    controlId: 'grant-host-execution',
    operationId: 'post_v1_hosts_id_execution',
    label: 'Grant execution',
    consequence: GRANT_CONSEQUENCE,
    confirmConsequence: `Confirm: the pool may start agents on ${row.slug} from now on.`,
    confirmLabel: `Grant execution to ${row.slug}`,
  };
}

/** A validated enrolment form, or the reason it is not valid. */
export type EnrollValidation =
  | { ok: true; input: { hostname: string; display_name: string | null } }
  | { ok: false; field: 'hostname' | 'display_name'; message: string };

/** The longest hostname the pool will accept. */
export const MAX_HOSTNAME_LENGTH = 253;

/**
 * Validate the enrolment form before any request is made.
 *
 * The pool is still the authority (a duplicate hostname comes back as a
 * problem-details conflict); this only stops obvious mistakes from leaving the
 * browser.
 */
export function validateEnrollInput(input: {
  hostname: string;
  display_name?: string | null;
}): EnrollValidation {
  const hostname = input.hostname.trim();
  if (hostname === '') {
    return { ok: false, field: 'hostname', message: 'Enter the hostname of the machine to enrol.' };
  }
  if (hostname.length > MAX_HOSTNAME_LENGTH) {
    return {
      ok: false,
      field: 'hostname',
      message: `A hostname is at most ${MAX_HOSTNAME_LENGTH} characters.`,
    };
  }
  if (/[\s/\\]/.test(hostname)) {
    return {
      ok: false,
      field: 'hostname',
      message: 'A hostname cannot contain spaces, slashes or backslashes.',
    };
  }
  const displayName = input.display_name?.trim() ?? '';
  return {
    ok: true,
    input: {
      hostname,
      display_name: displayName === '' ? null : displayName,
    },
  };
}

/** Quote a value for a POSIX shell, so a token cannot break the command. */
export function shellQuote(value: string): string {
  return `'${value.replace(/'/g, "'\\''")}'`;
}

/** The HTTP and WebSocket origins a host uses for one pool. */
export interface HostUrls {
  /** Pool HTTP origin, used for enrolment and capability reports. */
  http: string;
  /** Pool WebSocket URL, used for the host connection. */
  ws: string;
}

/**
 * Derive a host's two pool URLs from the operator-facing pool URL.
 *
 * @returns null when the pool URL is not an absolute http(s) URL
 */
export function hostUrls(poolUrl: string | null | undefined): HostUrls | null {
  const normalised = normalisePoolUrl(poolUrl);
  if (!normalised) return null;
  const url = new URL(normalised);
  const secure = url.protocol === 'https:';
  const path = url.pathname.replace(/\/+$/, '');
  return {
    http: `${url.origin}${path}`,
    ws: `${secure ? 'wss:' : 'ws:'}//${url.host}${path}/ws/host`,
  };
}

/** The note shown above the enrolment command. */
export const ENROLLMENT_COMMAND_NOTE =
  'Run this on the host machine, from a Tidepool checkout. The agent generates a credential, keeps it under your home directory, exchanges the token once, and connects.';

/** The one-time-token warning. */
export const ENROLLMENT_TOKEN_WARNING =
  'The token is single use and expires ten minutes after it is minted. The pool stores only a hash, so this is the only time it can be shown: copy the command now.';

/** What a successful enrolment shows. */
export interface EnrollmentResult {
  /** The host that was registered, still waiting for its first connection. */
  host: HostRow;
  /** The one-time token, shown once and never again. */
  token: string;
  /** When the token stops working, as the pool reported it. */
  expiresAt: string;
  /** The exact command to run on the host machine. */
  command: string;
  /** The pool the command points at. */
  poolUrl: string;
  /** The copy-me-now warning. */
  warning: string;
}

/**
 * Build the exact command a new host runs.
 *
 * The credential is generated on the host and kept in a file, because the
 * protocol requires the host to hold its own credential and the reference
 * agent reads it from the environment on every start. The enrolment token is
 * passed once; after the exchange the agent must be started with `HOST_ID`
 * instead, which the page says in prose beside the command.
 *
 * @returns null when the pool URL is not usable, so no misleading command is shown
 */
export function enrollmentCommand(input: {
  poolUrl: string | null | undefined;
  token: string;
}): string | null {
  const urls = hostUrls(input.poolUrl);
  if (!urls || input.token.trim() === '') return null;
  const lines = [
    '# On the host machine, from a Tidepool checkout:',
    `export POOL_HTTP_URL=${shellQuote(urls.http)}`,
    `export POOL_WS_URL=${shellQuote(urls.ws)}`,
    'CREDENTIAL_FILE="${HOME}/.tidepool/host-credential"',
    'mkdir -p "$(dirname "${CREDENTIAL_FILE}")"',
    '[ -s "${CREDENTIAL_FILE}" ] || openssl rand -hex 32 > "${CREDENTIAL_FILE}"',
    'ENROLLMENT_TOKEN=' +
      shellQuote(input.token) +
      ' CREDENTIAL="$(cat "${CREDENTIAL_FILE}")" cargo run -p tidepool-host-agent',
  ];
  return lines.join('\n');
}

/**
 * Build the enrolment result from a create + mint pair.
 *
 * @returns null when no command can be built (no usable pool URL), so the
 *   page can refuse rather than show a command with no origin
 */
export function enrollmentResult(input: {
  host: ContractHostView;
  token: string;
  expiresAt: string;
  poolUrl: string;
  now: number | Date;
}): EnrollmentResult | null {
  const command = enrollmentCommand({ poolUrl: input.poolUrl, token: input.token });
  if (!command) return null;
  return {
    host: hostRow(input.host, input.now),
    token: input.token,
    expiresAt: input.expiresAt,
    command,
    poolUrl: input.poolUrl,
    warning: ENROLLMENT_TOKEN_WARNING,
  };
}

/** The pool calls the hosts page makes, injectable for tests. */
export interface HostsTransport {
  /** List live hosts, ordered by slug. */
  list(): Promise<HostsResponse>;
  /** Register a host; it starts in `pending_approval`. */
  create(input: CreateHostRequest): Promise<ContractHostView>;
  /** Mint the single-use token a new host exchanges. */
  mintToken(hostId: string): Promise<MintEnrollmentTokenResponse>;
  /** Grant `host.execute`. */
  grantExecution(hostId: string): Promise<ContractHostView>;
  /** Revoke `host.execute`. */
  revokeExecution(hostId: string): Promise<ContractHostView>;
}

/** Build the transport from the typed SDK client. */
export function hostsTransport(client: TidepoolClient): HostsTransport {
  return {
    list: () => client.api.hosts.list(),
    create: (input) => client.api.hosts.create(input),
    mintToken: (hostId) =>
      client.api.hosts.mintEnrollmentToken({ host_id: hostId }) as Promise<MintEnrollmentTokenResponse>,
    grantExecution: (hostId) =>
      client.api.hosts.startExecution(hostId, {}) as Promise<ContractHostView>,
    revokeExecution: (hostId) =>
      client.api.hosts.stopExecution(hostId) as Promise<ContractHostView>,
  };
}

/** What the page is doing. */
export type HostsPhase = 'idle' | 'loading' | 'ready' | 'error';

/** Everything the hosts page renders. */
export interface HostsViewState {
  /** The current load state. */
  phase: HostsPhase;
  /** Hosts, in attention order. */
  hosts: HostRow[];
  /** The last failure, readable. */
  problem: PoolProblem | null;
  /** The one-time token and command, until dismissed. */
  enrollment: EnrollmentResult | null;
  /** The host whose execution grant is being changed, if any. */
  busyHostId: string | null;
  /** True while a form submission or grant is in flight. */
  busy: boolean;
}

/** The state before anything has been loaded. */
export function emptyHostsState(): HostsViewState {
  return {
    phase: 'idle',
    hosts: [],
    problem: null,
    enrollment: null,
    busyHostId: null,
    busy: false,
  };
}

/** How the controller reaches the pool, and what clock it uses. */
export interface HostsControllerOptions {
  /** The pool calls. */
  transport: HostsTransport;
  /** Absolute pool URL, used to build the enrolment command. */
  poolUrl: string;
  /** Clock, injectable so last-seen and tests are deterministic. */
  now?: () => number | Date;
}

/**
 * The hosts page's controller.
 *
 * Every method returns the new state rather than mutating what the caller
 * holds, so a Svelte page can assign the result and a test can assert on it.
 * Failures are caught and presented as problem details; nothing throws across
 * the boundary.
 */
export class HostsController {
  private readonly transport: HostsTransport;
  private readonly poolUrl: string;
  private readonly now: () => number | Date;
  private state: HostsViewState = emptyHostsState();

  constructor(options: HostsControllerOptions) {
    this.transport = options.transport;
    this.poolUrl = options.poolUrl;
    this.now = options.now ?? (() => Date.now());
  }

  /** The current state, without touching the pool. */
  snapshot(): HostsViewState {
    return this.state;
  }

  /** Load the host list. */
  async load(): Promise<HostsViewState> {
    this.state = { ...this.state, phase: this.state.phase === 'ready' ? 'ready' : 'loading' };
    try {
      const response = await this.transport.list();
      this.state = {
        ...this.state,
        phase: 'ready',
        hosts: sortHostRows(response.hosts.map((host) => hostRow(host, this.now()))),
        problem: null,
      };
    } catch (error) {
      this.state = { ...this.state, phase: 'error', problem: presentProblem(error) };
    }
    return this.state;
  }

  /**
   * Register a host and mint its one-time token.
   *
   * The token is held in state only until `dismissEnrollment`, so a reload or a
   * dismissal loses it — it cannot be shown again, which is the contract's
   * rule, not the page's.
   */
  async enroll(input: { hostname: string; display_name?: string | null }): Promise<HostsViewState> {
    const validation = validateEnrollInput(input);
    if (!validation.ok) {
      this.state = {
        ...this.state,
        problem: {
          kind: 'app',
          title: 'That enrolment form is not complete',
          message: validation.message,
          requestId: null,
          status: null,
          code: 'invalid_enrollment_input',
          type: null,
          retryAfterMs: null,
        },
      };
      return this.state;
    }
    this.state = { ...this.state, busy: true, problem: null };
    try {
      const host = await this.transport.create({
        hostname: validation.input.hostname,
        display_name: validation.input.display_name,
      });
      const minted = await this.transport.mintToken(host.id);
      const result = enrollmentResult({
        host,
        token: minted.token,
        expiresAt: minted.enrollment_token.expires_at,
        poolUrl: this.poolUrl,
        now: this.now(),
      });
      if (!result) {
        throw new Error(
          'The pool URL is not an absolute http(s) URL, so no enrolment command can be built.',
        );
      }
      this.state = {
        ...this.state,
        busy: false,
        phase: 'ready',
        enrollment: result,
        hosts: sortHostRows([...this.state.hosts, result.host]),
      };
    } catch (error) {
      this.state = { ...this.state, busy: false, problem: presentProblem(error) };
    }
    return this.state;
  }

  /** Grant or revoke `host.execute`, then refresh that row from the answer. */
  async setExecution(hostId: string, enabled: boolean): Promise<HostsViewState> {
    this.state = { ...this.state, busy: true, busyHostId: hostId, problem: null };
    try {
      const host = enabled
        ? await this.transport.grantExecution(hostId)
        : await this.transport.revokeExecution(hostId);
      const updated = hostRow(host, this.now());
      const hosts = this.state.hosts.some((row) => row.id === hostId)
        ? this.state.hosts.map((row) => (row.id === hostId ? updated : row))
        : [...this.state.hosts, updated];
      this.state = {
        ...this.state,
        busy: false,
        busyHostId: null,
        hosts: sortHostRows(hosts),
      };
    } catch (error) {
      this.state = {
        ...this.state,
        busy: false,
        busyHostId: null,
        problem: presentProblem(error),
      };
    }
    return this.state;
  }

  /** Forget the one-time token: it is shown once, then gone. */
  dismissEnrollment(): HostsViewState {
    this.state = { ...this.state, enrollment: null };
    return this.state;
  }

  /** Forget a failure, e.g. after retrying. */
  clearProblem(): HostsViewState {
    this.state = { ...this.state, problem: null };
    return this.state;
  }
}

/** The host counts the page summarises, in attention order. */
export interface HostCounts {
  /** Registered but never connected. */
  pending: number;
  /** Connected right now. */
  connected: number;
  /** Enrolled but not connected. */
  offline: number;
  /** Relationship ended. */
  revoked: number;
  /** Hosts that may run agents, regardless of presence. */
  mayRunAgents: number;
  /** Connected hosts that may not run agents. */
  connectedOnly: number;
}

/** Count rows for the summary line. */
export function hostCounts(rows: readonly HostRow[]): HostCounts {
  const counts: HostCounts = {
    pending: 0,
    connected: 0,
    offline: 0,
    revoked: 0,
    mayRunAgents: 0,
    connectedOnly: 0,
  };
  for (const row of rows) {
    if (row.status.status === 'pending_approval') counts.pending += 1;
    if (row.status.status === 'connected') counts.connected += 1;
    if (row.status.status === 'offline') counts.offline += 1;
    if (row.status.status === 'revoked') counts.revoked += 1;
    if (row.execution.enabled) counts.mayRunAgents += 1;
    if (row.status.status === 'connected' && !row.execution.enabled) counts.connectedOnly += 1;
  }
  return counts;
}

/** The one-line summary shown above the list. */
export function hostSummary(counts: HostCounts): string {
  const parts: string[] = [];
  if (counts.pending > 0) parts.push(`${counts.pending} approval required`);
  if (counts.connected > 0) parts.push(`${counts.connected} connected`);
  if (counts.offline > 0) parts.push(`${counts.offline} offline`);
  if (counts.revoked > 0) parts.push(`${counts.revoked} revoked`);
  if (counts.connectedOnly > 0) parts.push(`${counts.connectedOnly} connected only`);
  const total = counts.pending + counts.connected + counts.offline + counts.revoked;
  if (total === 0) return 'No hosts are enrolled yet.';
  return `${total} host${total === 1 ? '' : 's'}: ${parts.join(', ')}.`;
}

/** True when this pool config can load hosts. */
export function canLoadHosts(config: PoolConfig, token: string | null | undefined): boolean {
  return isPoolConfigured(config) && typeof token === 'string' && token.trim() !== '';
}
