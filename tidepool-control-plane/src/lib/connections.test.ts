/**
 * The connections model, tested without a DOM and without a pool.
 *
 * These tests pin the three things the page promises: the callback URL is the
 * exact URL for this origin, a credential has no read path (only a boolean and
 * a replace control), and a failed verification names the check that failed.
 */

import { describe, expect, it } from 'vitest';
import type {
  ConnectionView as ContractConnectionView,
  ConnectionsResponse,
  CreateConnectionRequest,
  UpdateConnectionRequest,
} from '@clawffice/tidepool-sdk';
import {
  CONNECTION_PROVIDERS,
  CONNECTION_PROVIDERS_CATALOG,
  CONNECTION_TRANSPORTS,
  ConnectionsController,
  POOL_ORIGIN_STORAGE_KEY,
  TRANSPORT_PREFERENCE_STORAGE_KEY,
  callbackChangeSummary,
  callbackChanges,
  callbackPath,
  callbackUrl,
  callbackView,
  canLoadConnections,
  connectionCounts,
  connectionRow,
  connectionStatus,
  connectionSummary,
  connectionsTransport,
  credentialRequirement,
  defaultTransport,
  describeProvider,
  describeTransport,
  emptyConnectionsState,
  failedVerification,
  isConnectionProvider,
  isConnectionTransport,
  isConnectionStatus,
  managedByLabel,
  normaliseSlug,
  poolOrigin,
  previousPoolOrigin,
  readTransportPreferences,
  rememberPoolOrigin,
  secretFieldView,
  sortConnectionRows,
  transportBelongsTo,
  transportsFor,
  validateConnectionInput,
  verificationChecks,
  verificationReport,
  verifyConnection,
  writeTransportPreference,
  type ConnectionRow,
  type ConnectionStorage,
  type ConnectionsTransport,
} from './connections';
import type { PoolProblem } from './api/problem';

const POOL = 'https://pool.example.com';

/** A contract connection view; tests override one field at a time. */
function view(overrides: Partial<ContractConnectionView> = {}): ContractConnectionView {
  return {
    id: 'connection-1',
    provider: 'slack',
    slug: 'acme',
    display_name: 'Acme',
    status: 'active',
    managed_by: 'database',
    credential_configured: true,
    resources: [{ resource_kind: 'workspace', external_id: 'T012345', display_name: 'Acme' }],
    created_at: '2026-10-29T09:00:00.000Z',
    ...overrides,
  };
}

/** One row for the given transport and overrides. */
function row(
  transport: 'github_webhook' | 'slack_webhook' | 'slack_socket' | 'discord_gateway',
  overrides: Partial<ContractConnectionView> = {},
): ConnectionRow {
  const provider =
    transport === 'github_webhook'
      ? 'github'
      : transport === 'discord_gateway'
        ? 'discord'
        : 'slack';
  const resources =
    provider === 'github'
      ? [{ resource_kind: 'repository', external_id: 'acme/app', display_name: 'acme/app' }]
      : provider === 'discord'
        ? [{ resource_kind: 'guild', external_id: '900000000000000001', display_name: 'Acme' }]
        : [{ resource_kind: 'workspace', external_id: 'T012345', display_name: 'Acme' }];
  return connectionRow(view({ provider, resources, ...overrides }), {
    poolUrl: POOL,
    transport,
  });
}

/** An in-memory storage that records what the page remembered. */
class MemoryStorage implements ConnectionStorage {
  readonly values = new Map<string, string>();

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }
}

/** A pool problem with the pool's own sentence. */
function problem(detail: string): PoolProblem {
  return {
    kind: 'problem',
    title: 'The pool rejected that input',
    message: detail,
    requestId: 'req_connections',
    status: 422,
    code: 'validation_error',
    type: 'about:blank',
    retryAfterMs: null,
  };
}

/** A transport whose calls a test can observe. */
function fakeTransport(overrides: Partial<ConnectionsTransport> = {}): ConnectionsTransport {
  return {
    list: async () => ({ connections: [view()] }),
    create: async (input: CreateConnectionRequest) =>
      view({
        id: 'connection-2',
        provider: input.provider,
        slug: input.slug,
        display_name: input.display_name,
        credential_configured: true,
      }),
    get: async (id: string) => view({ id }),
    update: async (id: string, input: UpdateConnectionRequest) =>
      view({
        id,
        display_name: input.display_name ?? 'Acme',
        status: input.status ?? 'active',
      }),
    remove: async () => undefined,
    ...overrides,
  };
}

describe('the provider catalogue', () => {
  it('offers GitHub and Slack webhooks, Slack socket mode and the Discord gateway', () => {
    expect(CONNECTION_PROVIDERS).toEqual(['github', 'slack', 'discord']);
    expect(CONNECTION_TRANSPORTS.map((transport) => transport.id)).toEqual([
      'github_webhook',
      'slack_webhook',
      'slack_socket',
      'discord_gateway',
    ]);
    expect(transportsFor('slack').map((transport) => transport.id)).toEqual([
      'slack_webhook',
      'slack_socket',
    ]);
    expect(transportsFor('discord').map((transport) => transport.id)).toEqual([
      'discord_gateway',
    ]);
  });

  it('says which transports need a public address and which dial out', () => {
    expect(describeTransport('github_webhook')?.needsPublicAddress).toBe(true);
    expect(describeTransport('slack_webhook')?.needsPublicAddress).toBe(true);
    expect(describeTransport('slack_socket')?.needsPublicAddress).toBe(false);
    expect(describeTransport('discord_gateway')?.needsPublicAddress).toBe(false);
    expect(describeTransport('slack_socket')?.callbackTemplate).toBeNull();
    expect(describeTransport('discord_gateway')?.callbackTemplate).toBeNull();
  });

  it('matches the credential fields the pool provider implementations read', () => {
    expect(CONNECTION_PROVIDERS_CATALOG.github.credentialFields.map((field) => field.key)).toEqual(
      ['app_id', 'private_key', 'webhook_secret'],
    );
    expect(CONNECTION_PROVIDERS_CATALOG.slack.credentialFields.map((field) => field.key)).toEqual([
      'bot_token',
      'signing_secret',
      'app_token',
    ]);
    expect(CONNECTION_PROVIDERS_CATALOG.discord.credentialFields.map((field) => field.key)).toEqual([
      'bot_token',
    ]);
  });

  it('guards every lookup instead of throwing on an unknown value', () => {
    expect(isConnectionProvider('github')).toBe(true);
    expect(isConnectionProvider('gitlab')).toBe(false);
    expect(describeProvider('gitlab')).toBeNull();
    expect(isConnectionTransport('slack_socket')).toBe(true);
    expect(isConnectionTransport('carrier_pigeon')).toBe(false);
    expect(describeTransport('carrier_pigeon')).toBeNull();
    expect(isConnectionStatus('active')).toBe(true);
    expect(isConnectionStatus('sleeping')).toBe(false);
  });
});

describe('slugs and callback URLs', () => {
  it('normalises a slug the way the pool does', () => {
    expect(normaliseSlug('  Acme / App!! ')).toBe('acme-app');
    expect(normaliseSlug('---')).toBe('');
    expect(normaliseSlug('a'.repeat(100))).toHaveLength(63);
  });

  it('builds the exact GitHub and Slack callback URLs for the current origin', () => {
    expect(
      callbackUrl({ poolUrl: POOL, transport: 'github_webhook', slug: 'acme' }),
    ).toBe('https://pool.example.com/v1/webhooks/github/acme');
    expect(callbackUrl({ poolUrl: `${POOL}/`, transport: 'slack_webhook', slug: 'team-1' })).toBe(
      'https://pool.example.com/v1/webhooks/slack/team-1',
    );
    expect(callbackPath('slack_socket', 'acme')).toBeNull();
    expect(callbackPath('discord_gateway', 'acme')).toBeNull();
  });

  it('uses the origin only, so a pool path never leaks into the callback', () => {
    expect(poolOrigin('https://pool.example.com/some/mount')).toBe('https://pool.example.com');
    expect(
      callbackUrl({
        poolUrl: 'https://pool.example.com/some/mount',
        transport: 'github_webhook',
        slug: 'acme',
      }),
    ).toBe('https://pool.example.com/v1/webhooks/github/acme');
  });

  it('returns no URL for an unusable pool URL or a dial-out transport', () => {
    expect(callbackUrl({ poolUrl: null, transport: 'github_webhook', slug: 'acme' })).toBeNull();
    expect(callbackUrl({ poolUrl: 'not a url', transport: 'github_webhook', slug: 'acme' })).toBeNull();
    expect(callbackUrl({ poolUrl: POOL, transport: 'slack_socket', slug: 'acme' })).toBeNull();
    expect(callbackUrl({ poolUrl: POOL, transport: 'discord_gateway', slug: 'acme' })).toBeNull();
  });

  it('explains outbound transports instead of showing an empty callback field', () => {
    const socket = callbackView({ poolUrl: POOL, transport: 'slack_socket', slug: 'acme' });
    expect(socket.kind).toBe('outbound');
    expect(socket.url).toBeNull();
    expect(socket.note).toContain('socket mode dials Slack');

    const gateway = callbackView({ poolUrl: POOL, transport: 'discord_gateway', slug: 'acme' });
    expect(gateway.kind).toBe('outbound');
    expect(gateway.note).toContain('bot gateway');

    const missing = callbackView({ poolUrl: null, transport: 'slack_webhook', slug: 'acme' });
    expect(missing.kind).toBe('unavailable');
    expect(missing.note).toContain('Use socket mode instead');
  });

  it('lists every webhook URL that changes with the origin, and only those', () => {
    const rows = [
      row('github_webhook', { id: 'gh', slug: 'acme' }),
      row('slack_webhook', { id: 'sl', slug: 'team' }),
      row('slack_socket', { id: 'sock', slug: 'socket-team' }),
      row('discord_gateway', { id: 'dc', slug: 'guild' }),
    ];
    const changes = callbackChanges('http://localhost:8080', POOL, rows);
    expect(changes.map((change) => change.connectionId)).toEqual(['gh', 'sl']);
    expect(changes[0]?.previousUrl).toBe('http://localhost:8080/v1/webhooks/github/acme');
    expect(changes[0]?.currentUrl).toBe('https://pool.example.com/v1/webhooks/github/acme');
    expect(changes[1]?.currentUrl).toBe('https://pool.example.com/v1/webhooks/slack/team');
    expect(changes.some((change) => change.transport === 'slack_socket')).toBe(false);
  });

  it('returns no changes when the origin did not move', () => {
    expect(callbackChanges(POOL, POOL, [row('github_webhook')])).toEqual([]);
    expect(callbackChanges(null, POOL, [row('github_webhook')])).toEqual([]);
    expect(callbackChanges(POOL, null, [row('github_webhook')])).toEqual([]);
  });

  it('states both sides of the move', () => {
    const sentence = callbackChangeSummary('http://localhost:8080', POOL);
    expect(sentence).toContain('http://localhost:8080');
    expect(sentence).toContain(POOL);
    expect(sentence).toContain('callback URLs');
  });
});

describe('the write-only credential field', () => {
  it('shows only that a credential is set and offers replacement', () => {
    const field = secretFieldView(true);
    expect(field.configured).toBe(true);
    expect(field.word).toBe('Credential set');
    expect(field.actionLabel).toBe('Replace credential');
    expect(field.value).toBeNull();
    expect(field.description).toContain('never sends it back');
  });

  it('offers to set a credential when none exists, never to reveal one', () => {
    const field = secretFieldView(false);
    expect(field.word).toBe('No credential set');
    expect(field.actionLabel).toBe('Set credential');
    expect(field.value).toBeNull();
    expect(field.description).toContain('will not accept events');
  });

  it('carries no secret through a connection row', () => {
    const built = row('slack_socket');
    expect(built.credential.value).toBeNull();
    expect(JSON.stringify(built)).not.toContain('xoxb-1234567890-super-secret');
  });
});

describe('validating the create form', () => {
  const base = {
    provider: 'slack',
    transport: 'slack_socket',
    slug: 'Acme Team!',
    displayName: 'Acme Team',
    credentialRef: 'SLACK_ACME_CREDS',
    resources: [{ kind: 'workspace', externalId: 'T012345', displayName: 'Acme' }],
  };

  it('accepts a Slack socket-mode connection and normalises the slug', () => {
    const result = validateConnectionInput(base);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.input.slug).toBe('acme-team');
    expect(result.input.transport).toBe('slack_socket');
    expect(result.input.credentialRef).toBe('SLACK_ACME_CREDS');
    expect(result.input.resources).toEqual([
      { resource_kind: 'workspace', external_id: 'T012345', display_name: 'Acme' },
    ]);
  });

  it('refuses a transport the provider does not offer', () => {
    const result = validateConnectionInput({ ...base, provider: 'discord' });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.field).toBe('transport');
    expect(result.message).toContain('Discord does not support');
  });

  it('uses the pool’s own words for an invalid resource kind', () => {
    const result = validateConnectionInput({
      ...base,
      resources: [{ kind: 'guild', externalId: 'guild-1' }],
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.field).toBe('resources');
    expect(result.message).toBe("slack resource_kind must be 'workspace' or 'channel', got 'guild'");
  });

  it('refuses duplicate resources and empty credential references', () => {
    const duplicate = validateConnectionInput({
      ...base,
      resources: [
        { kind: 'workspace', externalId: 'T1' },
        { kind: 'workspace', externalId: 't1' },
      ],
    });
    expect(duplicate.ok).toBe(false);
    if (!duplicate.ok) expect(duplicate.message).toBe('duplicate resource identity');

    const missing = validateConnectionInput({ ...base, credentialRef: '   ' });
    expect(missing.ok).toBe(false);
    if (!missing.ok) expect(missing.message).toBe('credential_ref must be 1-255 characters');
  });

  it('requires a slug and a display name', () => {
    expect(validateConnectionInput({ ...base, slug: '!!!' }).ok).toBe(false);
    expect(validateConnectionInput({ ...base, displayName: '  ' }).ok).toBe(false);
  });
});

describe('verification', () => {
  it('passes every check for an active, configured webhook with a resource', () => {
    const report = verifyConnection(row('github_webhook'), { publicOrigin: POOL });
    expect(report.ok).toBe(true);
    expect(report.summary).toContain('all');
    expect(report.checks.map((check) => check.id)).toEqual([
      'pool_answered',
      'status_active',
      'credential_configured',
      'resource_scope',
      'transport_ready',
    ]);
    expect(report.checks.every((check) => check.ok)).toBe(true);
  });

  it('names the credential check and uses the provider’s own requirement when it fails', () => {
    const report = verifyConnection(
      row('slack_socket', { credential_configured: false }),
      { publicOrigin: POOL },
    );
    expect(report.ok).toBe(false);
    expect(report.failed.map((check) => check.id)).toEqual(['credential_configured']);
    expect(report.summary).toContain('The credential is configured');
    expect(report.summary).toContain('bot_token, signing_secret, app_token');
    expect(credentialRequirement('github', 'github_webhook')).toBe(
      'GitHub credentials need app_id, private_key, webhook_secret.',
    );
  });

  it('names a missing resource with the provider and the kinds it accepts', () => {
    const report = verifyConnection(
      row('discord_gateway', { resources: [] }),
      { publicOrigin: POOL },
    );
    expect(report.ok).toBe(false);
    expect(report.failed[0]?.id).toBe('resource_scope');
    expect(report.summary).toContain('The resource scope is registered');
    expect(report.summary).toContain("'guild'");
  });

  it('fails the transport check when a webhook has no public origin', () => {
    const checks = verificationChecks(row('slack_webhook'), { publicOrigin: null });
    const transport = checks.find((check) => check.id === 'transport_ready');
    expect(transport?.ok).toBe(false);
    expect(transport?.detail).toContain('Use socket mode instead');
  });

  it('passes the transport check for socket mode without a public address', () => {
    const checks = verificationChecks(row('slack_socket'), { publicOrigin: null });
    const transport = checks.find((check) => check.id === 'transport_ready');
    expect(transport?.ok).toBe(true);
    expect(transport?.detail).toContain('no public address');
  });

  it('keeps a failed pool request as a failed check in the pool’s own words', () => {
    const report = failedVerification(
      { connectionId: 'connection-1', provider: 'github', transport: 'github_webhook' },
      problem("credential reference 'GITHUB_ACME_CREDS' is not set"),
    );
    expect(report.ok).toBe(false);
    expect(report.failed[0]?.id).toBe('pool_answered');
    expect(report.summary).toContain('The pool answered');
    expect(report.summary).toContain("credential reference 'GITHUB_ACME_CREDS' is not set");
  });

  it('builds the same report through verificationReport', () => {
    const built = row('github_webhook');
    const report = verificationReport(built, verificationChecks(built, { publicOrigin: POOL }));
    expect(report.connectionId).toBe('connection-1');
    expect(report.provider).toBe('github');
    expect(report.ok).toBe(true);
  });
});

describe('rows and summaries', () => {
  it('reads a contract view without inventing fields', () => {
    const built = connectionRow(view({ status: 'disabled', managed_by: 'environment' }), {
      poolUrl: POOL,
      transport: 'slack_webhook',
    });
    expect(built.displayName).toBe('Acme');
    expect(built.status.value).toBe('disabled');
    expect(built.status.word).toBe('Disabled');
    expect(built.managedBy).toBe('environment');
    expect(built.editable).toBe(false);
    expect(built.resources[0]?.externalId).toBe('T012345');
    expect(built.createdAt).toBe('2026-10-29T09:00:00.000Z');
  });

  it('falls back to readable values for unknown status, provider and management', () => {
    expect(connectionStatus('sleeping')).toEqual({
      value: null,
      word: 'Unknown',
      tone: 'unknown',
    });
    expect(managedByLabel('elsewhere')).toBe('unknown');
    const built = connectionRow(
      {
        id: 'connection-1',
        provider: 'gitlab',
        slug: 'acme',
        display_name: 'Acme',
        status: 'active',
        managed_by: 'elsewhere',
        credential_configured: true,
        resources: [],
        created_at: null,
      },
      { poolUrl: POOL },
    );
    expect(built.provider).toBeNull();
    expect(built.managedBy).toBe('unknown');
    expect(built.editable).toBe(false);
  });

  it('orders by provider then slug and counts what the page summarises', () => {
    const rows = [
      row('discord_gateway', { id: 'd', slug: 'guild' }),
      row('slack_socket', { id: 's', slug: 'z-team' }),
      row('slack_webhook', { id: 's2', slug: 'a-team', status: 'disabled' }),
      row('github_webhook', { id: 'g', slug: 'acme' }),
    ];
    const sorted = sortConnectionRows(rows);
    expect(sorted.map((item) => item.id)).toEqual(['g', 's2', 's', 'd']);
    const counts = connectionCounts(sorted);
    expect(counts.total).toBe(4);
    expect(counts.active).toBe(3);
    expect(counts.disabled).toBe(1);
    expect(counts.webhook).toBe(2);
    expect(connectionSummary(counts)).toContain('4 connections');
    expect(connectionSummary(connectionCounts([]))).toBe('No connections are configured yet.');
  });

  it('decides whether a page has enough to load', () => {
    expect(canLoadConnections({ baseUrl: POOL }, 'token')).toBe(true);
    expect(canLoadConnections({ baseUrl: null }, 'token')).toBe(false);
    expect(canLoadConnections({ baseUrl: POOL }, '  ')).toBe(false);
  });

  it('maps transport choices back to their provider', () => {
    expect(transportBelongsTo('slack', 'slack_socket')).toBe(true);
    expect(transportBelongsTo('github', 'slack_socket')).toBe(false);
    expect(defaultTransport('slack')).toBe('slack_webhook');
    expect(defaultTransport('discord')).toBe('discord_gateway');
  });
});

describe('remembered transports and origins', () => {
  it('stores a transport preference per connection id and ignores junk', () => {
    const storage = new MemoryStorage();
    const preferences = writeTransportPreference(storage, 'connection-1', 'slack_socket');
    expect(preferences).toEqual({ 'connection-1': 'slack_socket' });
    expect(readTransportPreferences(storage)).toEqual({ 'connection-1': 'slack_socket' });

    storage.setItem(TRANSPORT_PREFERENCE_STORAGE_KEY, '{"connection-1":"slack_socket","x":"bad"}');
    expect(readTransportPreferences(storage)).toEqual({ 'connection-1': 'slack_socket' });
    storage.setItem(TRANSPORT_PREFERENCE_STORAGE_KEY, 'not json');
    expect(readTransportPreferences(storage)).toEqual({});
    expect(readTransportPreferences(null)).toEqual({});
  });

  it('remembers the origin shown and reports the previous one only after a move', () => {
    const storage = new MemoryStorage();
    expect(previousPoolOrigin(storage, POOL)).toBeNull();
    rememberPoolOrigin(storage, 'http://localhost:8080');
    expect(storage.getItem(POOL_ORIGIN_STORAGE_KEY)).toBe('http://localhost:8080');
    expect(previousPoolOrigin(storage, POOL)).toBe('http://localhost:8080');
    rememberPoolOrigin(storage, POOL);
    expect(previousPoolOrigin(storage, POOL)).toBeNull();
    expect(previousPoolOrigin(null, POOL)).toBeNull();
  });
});

describe('the connections controller', () => {
  it('lists connections and starts idle, loading and ready in order', async () => {
    const controller = new ConnectionsController({
      transport: fakeTransport(),
      poolUrl: POOL,
    });
    expect(controller.snapshot().phase).toBe('idle');
    const state = await controller.load();
    expect(state.phase).toBe('ready');
    expect(state.connections).toHaveLength(1);
    expect(state.connections[0]?.callback.url).toBe(
      'https://pool.example.com/v1/webhooks/slack/acme',
    );
  });

  it('creates a Slack socket-mode connection, remembers the transport and verifies it', async () => {
    const storage = new MemoryStorage();
    const controller = new ConnectionsController({
      transport: fakeTransport(),
      poolUrl: POOL,
      storage,
    });
    const state = await controller.create({
      provider: 'slack',
      transport: 'slack_socket',
      slug: 'Acme Team',
      displayName: 'Acme Team',
      credentialRef: 'SLACK_ACME_CREDS',
      resources: [{ kind: 'workspace', externalId: 'T012345' }],
    });
    expect(state.busy).toBe(false);
    expect(state.connections).toHaveLength(1);
    expect(state.connections[0]?.transport).toBe('slack_socket');
    expect(state.connections[0]?.callback.kind).toBe('outbound');
    expect(state.verification?.ok).toBe(true);
    expect(readTransportPreferences(storage)).toEqual({ 'connection-2': 'slack_socket' });
  });

  it('never carries a credential value into the view state', async () => {
    const secret = 'xoxb-super-secret-value';
    const refs: string[] = [];
    const controller = new ConnectionsController({
      transport: fakeTransport({
        create: async (input) => {
          refs.push(input.credential_ref);
          return view({ id: 'connection-9', credential_configured: true });
        },
      }),
      poolUrl: POOL,
    });
    const state = await controller.create({
      provider: 'slack',
      transport: 'slack_socket',
      slug: 'acme',
      displayName: 'Acme',
      credentialRef: 'SLACK_ACME_CREDS',
      resources: [],
    });
    expect(refs).toEqual(['SLACK_ACME_CREDS']);
    const serialised = JSON.stringify(state);
    expect(serialised).not.toContain(secret);
    expect(serialised).not.toContain('SLACK_ACME_CREDS');
    expect(state.connections[0]?.credential.word).toBe('Credential set');
    expect(state.connections[0]?.credential.value).toBeNull();
  });

  it('verifies a failed credential and names the failing check', async () => {
    const controller = new ConnectionsController({
      transport: fakeTransport({
        list: async () => ({ connections: [view({ credential_configured: false })] }),
        get: async (id) => view({ id, credential_configured: false }),
      }),
      poolUrl: POOL,
    });
    await controller.load();
    const state = await controller.verify('connection-1');
    expect(state.verification?.ok).toBe(false);
    expect(state.verification?.failed[0]?.id).toBe('credential_configured');
    expect(state.verification?.summary).toContain('The credential is configured');
  });

  it('reports a failed pool read as a failed check in the pool’s own words', async () => {
    const controller = new ConnectionsController({
      transport: fakeTransport({
        get: async () => {
          throw new Error('credential reference is not set');
        },
      }),
      poolUrl: POOL,
    });
    await controller.load();
    const state = await controller.verify('connection-1');
    expect(state.verification?.failed[0]?.id).toBe('pool_answered');
    expect(state.verification?.summary).toContain('credential reference is not set');
  });

  it('renames, disables and deletes a connection', async () => {
    const calls: string[] = [];
    const controller = new ConnectionsController({
      transport: fakeTransport({
        update: async (id, input) => {
          calls.push(`update ${id} ${JSON.stringify(input)}`);
          return view({
            id,
            display_name: input.display_name ?? 'Acme',
            status: input.status ?? 'active',
          });
        },
        remove: async (id) => {
          calls.push(`remove ${id}`);
        },
      }),
      poolUrl: POOL,
    });
    await controller.load();
    const renamed = await controller.rename('connection-1', 'Acme Renamed');
    expect(renamed.connections[0]?.displayName).toBe('Acme Renamed');
    const disabled = await controller.setStatus('connection-1', 'disabled');
    expect(disabled.connections[0]?.status.value).toBe('disabled');
    const removed = await controller.remove('connection-1');
    expect(removed.connections).toEqual([]);
    expect(calls).toEqual([
      'update connection-1 {"display_name":"Acme Renamed"}',
      'update connection-1 {"status":"disabled"}',
      'remove connection-1',
    ]);
  });

  it('refuses an empty rename before sending it', async () => {
    let updates = 0;
    const controller = new ConnectionsController({
      transport: fakeTransport({
        update: async (id) => {
          updates += 1;
          return view({ id });
        },
      }),
      poolUrl: POOL,
    });
    await controller.load();
    const state = await controller.rename('connection-1', '   ');
    expect(updates).toBe(0);
    expect(state.problem?.code).toBe('invalid_connection_name');
  });

  it('offers replacement but reports the contract gap instead of faking a write', async () => {
    const controller = new ConnectionsController({
      transport: fakeTransport(),
      poolUrl: POOL,
    });
    await controller.load();
    const state = await controller.replaceCredential('connection-1', 'SLACK_NEW_CREDS');
    expect(state.problem?.code).toBe('credential_replacement_unavailable');
    expect(state.problem?.message).toContain('Rotate the secret');
    expect(JSON.stringify(state)).not.toContain('SLACK_NEW_CREDS');
  });

  it('uses a replacement operation when a pool provides one, then re-verifies', async () => {
    const replaced: string[] = [];
    const controller = new ConnectionsController({
      transport: fakeTransport({
        replaceCredential: async (id, credentialRef) => {
          replaced.push(`${id}:${credentialRef}`);
          return view({ id, credential_configured: true });
        },
      }),
      poolUrl: POOL,
    });
    await controller.load();
    const state = await controller.replaceCredential('connection-1', 'SLACK_NEW_CREDS');
    expect(replaced).toEqual(['connection-1:SLACK_NEW_CREDS']);
    expect(state.verification?.ok).toBe(true);
  });

  it('remembers a Slack transport and changes what the card shows', async () => {
    const storage = new MemoryStorage();
    const controller = new ConnectionsController({
      transport: fakeTransport(),
      poolUrl: POOL,
      storage,
    });
    await controller.load();
    const state = controller.rememberTransport('connection-1', 'slack_socket');
    expect(state.connections[0]?.transport).toBe('slack_socket');
    expect(state.connections[0]?.callback.kind).toBe('outbound');
    expect(readTransportPreferences(storage)).toEqual({ 'connection-1': 'slack_socket' });
  });

  it('shows an empty state before loading and keeps a problem readable', async () => {
    const state = emptyConnectionsState();
    expect(state.phase).toBe('idle');
    expect(state.connections).toEqual([]);
    expect(state.verification).toBeNull();

    const controller = new ConnectionsController({
      transport: fakeTransport({
        list: async () => {
          throw new Error('network down');
        },
      }),
      poolUrl: POOL,
    });
    const failed = await controller.load();
    expect(failed.phase).toBe('error');
    expect(failed.problem?.message).toBe('network down');
  });

  it('builds a transport from the SDK client', () => {
    const calls: string[] = [];
    const client = {
      api: {
        connections: {
          list: async () => {
            calls.push('list');
            return { connections: [] } satisfies ConnectionsResponse;
          },
          create: async (input: CreateConnectionRequest) => {
            calls.push(`create ${input.slug}`);
            return view({ slug: input.slug });
          },
          get: async (id: string) => {
            calls.push(`get ${id}`);
            return view({ id });
          },
          update: async (id: string, input: UpdateConnectionRequest) => {
            calls.push(`update ${id} ${input.display_name ?? ''}`);
            return view({ id });
          },
          remove: async (id: string) => {
            calls.push(`remove ${id}`);
          },
        },
      },
    };
    const transport = connectionsTransport(client as never);
    void transport.list();
    void transport.create({
      provider: 'slack',
      slug: 'acme',
      display_name: 'Acme',
      credential_ref: 'SLACK_ACME_CREDS',
    });
    void transport.get('connection-1');
    void transport.update('connection-1', { display_name: 'Renamed' });
    void transport.remove('connection-1');
    expect(calls).toEqual([
      'list',
      'create acme',
      'get connection-1',
      'update connection-1 Renamed',
      'remove connection-1',
    ]);
  });
});
