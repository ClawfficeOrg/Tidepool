import { describe, expect, it } from 'vitest';
import type { HostView as ContractHostView } from '@clawffice/tidepool-sdk';
import { CONTROLS } from '../auth/roles';
import {
  ENROLLMENT_TOKEN_WARNING,
  GRANT_CONSEQUENCE,
  HOST_STATUSES,
  HOST_STATUS_DESCRIPTORS,
  LAST_SEEN_UNREPORTED,
  REVOKE_CONSEQUENCE,
  describeHostStatus,
  emptyHostsState,
  enrollmentCommand,
  enrollmentResult,
  executionAction,
  executionState,
  hostCounts,
  hostRow,
  hostSummary,
  hostUrls,
  isHostStatus,
  lastSeenView,
  relativeAge,
  shellQuote,
  sortHostRows,
  validateEnrollInput,
  type HostRow,
} from './hosts';

const NOW = new Date('2026-10-29T12:00:00.000Z');

/** A contract-shaped host, overridable per test. */
function contractHost(overrides: Partial<ContractHostView> = {}): ContractHostView {
  return {
    id: 'host-1',
    slug: 'swaygent',
    display_name: 'Swaygent',
    hostname: 'swaygent.local',
    status: 'connected',
    execution_enabled: false,
    created_at: '2026-10-29T09:00:00.000Z',
    revoked_at: null,
    ...overrides,
  };
}

/** A rendered row, overridable per test. */
function row(overrides: Partial<HostRow> = {}): HostRow {
  return { ...hostRow(contractHost(), NOW), ...overrides };
}

describe('host statuses', () => {
  it('has all four contract statuses, reachable by name', () => {
    expect(HOST_STATUSES).toEqual([
      'pending_approval',
      'connected',
      'offline',
      'revoked',
    ]);
    for (const status of HOST_STATUSES) {
      expect(isHostStatus(status)).toBe(true);
      expect(describeHostStatus(status).status).toBe(status);
    }
  });

  it('gives every status an icon, a word and a description', () => {
    for (const status of HOST_STATUSES) {
      const descriptor = HOST_STATUS_DESCRIPTORS[status];
      expect(descriptor.word.trim()).not.toBe('');
      expect(descriptor.description.trim()).not.toBe('');
      expect(descriptor.icon).toMatch(/^M/);
      expect(descriptor.icon.length).toBeGreaterThanOrEqual(8);
      expect(descriptor.tone).not.toBe('unknown');
    }
  });

  it('gives each status a distinct word, so two states never read alike', () => {
    const words = HOST_STATUSES.map((status) =>
      HOST_STATUS_DESCRIPTORS[status].word.toLowerCase(),
    );
    expect(new Set(words).size).toBe(words.length);
    expect(HOST_STATUS_DESCRIPTORS.pending_approval.word).toBe('Approval required');
    expect(HOST_STATUS_DESCRIPTORS.connected.word).toBe('Connected');
    expect(HOST_STATUS_DESCRIPTORS.offline.word).toBe('Offline');
    expect(HOST_STATUS_DESCRIPTORS.revoked.word).toBe('Revoked');
  });

  it('falls back to a readable Unknown rather than throwing', () => {
    for (const value of ['gremlin', '', null, undefined, 42]) {
      const descriptor = describeHostStatus(value as string | null | undefined);
      expect(descriptor.word).toBe('Unknown');
      expect(descriptor.status).toBeNull();
      expect(descriptor.tone).toBe('unknown');
    }
    expect(isHostStatus('Connected')).toBe(false);
  });
});

describe('execution permission', () => {
  it('says a granted host may run agents', () => {
    const state = executionState('connected', true);
    expect(state.enabled).toBe(true);
    expect(state.word).toBe('May run agents');
    expect(state.description).toContain('host.execute');
  });

  it('labels a connected host without the grant as connected only', () => {
    const state = executionState('connected', false);
    expect(state.enabled).toBe(false);
    expect(state.word).toBe('Connected only');
    expect(state.description).toContain('not granted');
  });

  it('does not call an offline or pending host connected only', () => {
    for (const status of ['offline', 'pending_approval', 'revoked']) {
      expect(executionState(status, false).word).toBe('Execution not granted');
    }
  });
});

describe('last seen', () => {
  it('says the pool did not report it rather than inventing a time', () => {
    for (const value of [null, undefined, '', 'not-a-date']) {
      const view = lastSeenView(value, NOW);
      expect(view.reported).toBe(false);
      expect(view.at).toBeNull();
      expect(view.text).toBe(LAST_SEEN_UNREPORTED);
    }
  });

  it('reads a reported timestamp as a relative age', () => {
    expect(lastSeenView('2026-10-29T11:59:30.000Z', NOW).text).toBe('Last seen just now');
    expect(lastSeenView('2026-10-29T11:58:00.000Z', NOW).text).toBe('Last seen 2 minutes ago');
    expect(lastSeenView('2026-10-29T08:00:00.000Z', NOW).text).toBe('Last seen 4 hours ago');
    expect(lastSeenView('2026-10-27T12:00:00.000Z', NOW).text).toBe('Last seen 2 days ago');
    expect(lastSeenView('2026-10-29T11:58:00.000Z', NOW).at).toBe('2026-10-29T11:58:00.000Z');
  });

  it('handles a future timestamp from clock skew as just now', () => {
    expect(relativeAge('2026-10-29T12:05:00.000Z', NOW)).toBe('just now');
  });

  it('uses a date for anything older than a month', () => {
    expect(relativeAge('2026-08-01T00:00:00.000Z', NOW)).toBe('2026-08-01');
  });
});

describe('host rows', () => {
  it('maps the contract view into what the page renders', () => {
    const rendered = hostRow(contractHost(), NOW);
    expect(rendered).toMatchObject({
      id: 'host-1',
      slug: 'swaygent',
      displayName: 'Swaygent',
      hostname: 'swaygent.local',
      execution: { enabled: false, word: 'Connected only' },
    });
    expect(rendered.status.word).toBe('Connected');
    expect(rendered.lastSeen.reported).toBe(false);
  });

  it('reads last_seen_at when a pool sends it', () => {
    const rendered = hostRow(
      { ...contractHost(), last_seen_at: '2026-10-29T11:55:00.000Z' } as ContractHostView,
      NOW,
    );
    expect(rendered.lastSeen.text).toBe('Last seen 5 minutes ago');
    expect(rendered.lastSeen.reported).toBe(true);
  });

  it('orders approval required first and revoked last, then by slug', () => {
    const rows = [
      hostRow(contractHost({ id: '4', slug: 'd', status: 'revoked' }), NOW),
      hostRow(contractHost({ id: '2', slug: 'b', status: 'offline' }), NOW),
      hostRow(contractHost({ id: '3', slug: 'c', status: 'connected' }), NOW),
      hostRow(contractHost({ id: '1', slug: 'a', status: 'pending_approval' }), NOW),
      hostRow(contractHost({ id: '5', slug: 'a2', status: 'pending_approval' }), NOW),
    ];
    expect(sortHostRows(rows).map((entry) => entry.slug)).toEqual([
      'a',
      'a2',
      'c',
      'b',
      'd',
    ]);
  });

  it('counts what the summary line says', () => {
    const rows = [
      hostRow(contractHost({ id: '1', status: 'pending_approval' }), NOW),
      hostRow(contractHost({ id: '2', status: 'connected', execution_enabled: true }), NOW),
      hostRow(contractHost({ id: '3', status: 'connected', execution_enabled: false }), NOW),
      hostRow(contractHost({ id: '4', status: 'offline' }), NOW),
      hostRow(contractHost({ id: '5', status: 'revoked' }), NOW),
    ];
    const counts = hostCounts(rows);
    expect(counts).toEqual({
      pending: 1,
      connected: 2,
      offline: 1,
      revoked: 1,
      mayRunAgents: 1,
      connectedOnly: 1,
    });
    expect(hostSummary(counts)).toBe(
      '5 hosts: 1 approval required, 2 connected, 1 offline, 1 revoked, 1 connected only.',
    );
    expect(hostSummary(hostCounts([]))).toBe('No hosts are enrolled yet.');
  });
});

describe('execution actions', () => {
  it('offers the grant to a connected-only host and states the consequence', () => {
    const action = executionAction(row());
    expect(action).not.toBeNull();
    expect(action?.id).toBe('grant-execution');
    expect(action?.operationId).toBe('post_v1_hosts_id_execution');
    expect(action?.consequence).toBe(GRANT_CONSEQUENCE);
    expect(action?.consequence).toContain('run agents');
    expect(action?.consequence).toContain('dispatch runs');
    expect(action?.confirmConsequence).toContain('swaygent');
    expect(action?.confirmLabel).toContain('swaygent');
  });

  it('offers revoke to a host that already has the grant', () => {
    const granted = row({ execution: executionState('connected', true) });
    const action = executionAction(granted);
    expect(action?.id).toBe('revoke-execution');
    expect(action?.operationId).toBe('delete_v1_hosts_id_execution');
    expect(action?.consequence).toBe(REVOKE_CONSEQUENCE);
    expect(action?.consequence).toContain('stays enrolled and connected');
    expect(action?.confirmConsequence).toContain('can no longer run agents');
  });

  it('offers nothing for a revoked host', () => {
    const revoked = row({
      status: describeHostStatus('revoked'),
      execution: executionState('revoked', false),
    });
    expect(executionAction(revoked)).toBeNull();
  });

  it('gates each action by the role-table control for the same operation', () => {
    const actions = [
      executionAction(row()),
      executionAction(row({ execution: executionState('connected', true) })),
    ];
    for (const action of actions) {
      expect(action).not.toBeNull();
      if (!action) continue;
      const control = CONTROLS.find((candidate) => candidate.id === action.controlId);
      expect(control?.operationId).toBe(action.operationId);
      expect(control?.mutating).toBe(true);
    }
  });
});

describe('the enrolment form', () => {
  it('accepts a hostname and an optional display name, trimmed', () => {
    expect(validateEnrollInput({ hostname: ' swaygent.local ' })).toEqual({
      ok: true,
      input: { hostname: 'swaygent.local', display_name: null },
    });
    expect(
      validateEnrollInput({ hostname: 'swaygent.local', display_name: ' Swaygent ' }),
    ).toEqual({
      ok: true,
      input: { hostname: 'swaygent.local', display_name: 'Swaygent' },
    });
  });

  it('refuses an empty, overlong or path-like hostname', () => {
    expect(validateEnrollInput({ hostname: '   ' })).toMatchObject({
      ok: false,
      field: 'hostname',
    });
    expect(validateEnrollInput({ hostname: 'a'.repeat(254) })).toMatchObject({ ok: false });
    for (const hostname of ['a b', 'a/b', 'a\\b']) {
      expect(validateEnrollInput({ hostname })).toMatchObject({ ok: false, field: 'hostname' });
    }
  });
});

describe('host URLs and the enrolment command', () => {
  it('derives the WebSocket URL from the pool URL', () => {
    expect(hostUrls('https://pool.example.com')).toEqual({
      http: 'https://pool.example.com',
      ws: 'wss://pool.example.com/ws/host',
    });
    expect(hostUrls('http://127.0.0.1:8080/')).toEqual({
      http: 'http://127.0.0.1:8080',
      ws: 'ws://127.0.0.1:8080/ws/host',
    });
    expect(hostUrls('https://example.com/tidepool/')).toEqual({
      http: 'https://example.com/tidepool',
      ws: 'wss://example.com/tidepool/ws/host',
    });
    for (const value of ['', 'pool.example.com', 'ftp://example.com', null]) {
      expect(hostUrls(value)).toBeNull();
    }
  });

  it('quotes values so a token cannot break out of the shell', () => {
    expect(shellQuote('plain')).toBe("'plain'");
    expect(shellQuote("it's")).toBe("'it'\\''s'");
  });

  it('builds the exact command: token, both URLs, a kept credential, the agent', () => {
    const command = enrollmentCommand({
      poolUrl: 'https://pool.example.com',
      token: 'pub.secret',
    });
    expect(command).not.toBeNull();
    expect(command).toContain("ENROLLMENT_TOKEN='pub.secret'");
    expect(command).toContain("export POOL_HTTP_URL='https://pool.example.com'");
    expect(command).toContain("export POOL_WS_URL='wss://pool.example.com/ws/host'");
    expect(command).toContain('CREDENTIAL_FILE="${HOME}/.tidepool/host-credential"');
    expect(command).toContain('openssl rand -hex 32');
    expect(command).toContain('cargo run -p tidepool-host-agent');
    expect(command?.split('\n').length).toBeGreaterThanOrEqual(6);
  });

  it('refuses to build a command without a usable pool URL or token', () => {
    expect(enrollmentCommand({ poolUrl: null, token: 'pub.secret' })).toBeNull();
    expect(enrollmentCommand({ poolUrl: 'not a url', token: 'pub.secret' })).toBeNull();
    expect(enrollmentCommand({ poolUrl: 'https://pool.example.com', token: '  ' })).toBeNull();
  });

  it('builds the result with the warning and the created host', () => {
    const result = enrollmentResult({
      host: contractHost({ status: 'pending_approval' }),
      token: 'pub.secret',
      expiresAt: '2026-10-29T12:10:00.000Z',
      poolUrl: 'https://pool.example.com',
      now: NOW,
    });
    expect(result).not.toBeNull();
    expect(result?.token).toBe('pub.secret');
    expect(result?.host.status.word).toBe('Approval required');
    expect(result?.warning).toBe(ENROLLMENT_TOKEN_WARNING);
    expect(result?.warning).toContain('only time it can be shown');
    expect(result?.command).toContain('pub.secret');
    expect(result?.expiresAt).toBe('2026-10-29T12:10:00.000Z');
  });

  it('refuses a result when no command can be built', () => {
    expect(
      enrollmentResult({
        host: contractHost(),
        token: 'pub.secret',
        expiresAt: '2026-10-29T12:10:00.000Z',
        poolUrl: '',
        now: NOW,
      }),
    ).toBeNull();
  });
});

describe('the empty page state', () => {
  it('starts idle with no hosts, no token and no failure', () => {
    expect(emptyHostsState()).toEqual({
      phase: 'idle',
      hosts: [],
      enrollment: null,
      problem: null,
      busyHostId: null,
      busy: false,
    });
  });
});
