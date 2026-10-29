/**
 * End to end across the client boundary, without a server.
 *
 * The real TypeScript SDK talks to an in-memory pool through an injected
 * `fetch`, so this test exercises the same URLs, bodies, auth header and
 * problem-details parsing the browser uses. It walks the task's Slack
 * socket-mode flow: create a connection from a credential reference, verify
 * it, rename it, disable it, delete it — and checks that no response, and no
 * view state, ever carries the stored secret.
 */

import { describe, expect, it } from 'vitest';
import { createClient } from '@clawffice/tidepool-sdk';
import type { ConnectionView } from '@clawffice/tidepool-sdk';
import {
  ConnectionsController,
  connectionsTransport,
  readTransportPreferences,
  type ConnectionStorage,
} from '$lib/connections';

const BASE_URL = 'https://pool.example.com';
const TOKEN = 'token-1';
const SLACK_SECRET = 'xoxb-1234567890-super-secret';

/** One connection in the fake pool. */
interface FakeConnection {
  id: string;
  provider: 'github' | 'slack' | 'discord';
  slug: string;
  display_name: string;
  status: 'active' | 'disabled';
  managed_by: 'database' | 'environment';
  credential_ref: string;
  resources: { resource_kind: string; external_id: string; display_name: string | null }[];
  created_at: string;
}

/** The fake pool's state. */
interface FakePool {
  connections: FakeConnection[];
  secrets: Map<string, string>;
  calls: string[];
  bodies: unknown[];
  responses: string[];
  nextId: number;
}

/** A fresh fake pool with one Slack credential in its secret store. */
function fakePool(): FakePool {
  return {
    connections: [],
    secrets: new Map([['SLACK_ACME_CREDS', SLACK_SECRET]]),
    calls: [],
    bodies: [],
    responses: [],
    nextId: 1,
  };
}

/** The public projection: never carries `credential_ref` or its value. */
function view(connection: FakeConnection, pool: FakePool): ConnectionView {
  return {
    id: connection.id,
    provider: connection.provider,
    slug: connection.slug,
    display_name: connection.display_name,
    status: connection.status,
    managed_by: connection.managed_by,
    credential_configured: pool.secrets.has(connection.credential_ref),
    resources: connection.resources.map((resource) => ({
      resource_kind: resource.resource_kind,
      external_id: resource.external_id,
      display_name: resource.display_name,
    })),
    created_at: connection.created_at,
  };
}

/** A JSON response, recorded so the test can prove no secret came back. */
function json(pool: FakePool, body: unknown, status = 200): Response {
  const raw = JSON.stringify(body);
  pool.responses.push(raw);
  return new Response(raw, {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

/** A problem-details response, in the pool's words. */
function problem(pool: FakePool, status: number, code: string, detail: string): Response {
  return json(
    pool,
    { status, code, detail, title: 'The pool rejected that input', requestId: 'req_e2e_connections' },
    status,
  );
}

/** The request body, parsed as an object. */
function bodyOf(init: RequestInit | undefined): Record<string, unknown> {
  const raw = init?.body;
  if (typeof raw !== 'string' || raw.trim() === '') return {};
  const parsed: unknown = JSON.parse(raw);
  return parsed !== null && typeof parsed === 'object' && !Array.isArray(parsed)
    ? (parsed as Record<string, unknown>)
    : {};
}

/**
 * The fake pool's `fetch`.
 *
 * Mirrors the contract: 200 for list/get/patch, 201 for create, 204 for
 * delete, 401 without the token, 404 for an unknown id and 422 for a credential
 * reference the pool cannot resolve.
 */
function fakeFetch(pool: FakePool): typeof fetch {
  return async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const url =
      typeof input === 'string' ? new URL(input) : input instanceof URL ? input : new URL(input.url);
    const method = (init?.method ?? 'GET').toUpperCase();
    const path = url.pathname;
    pool.calls.push(`${method} ${path}`);

    const authorization = new Headers(init?.headers).get('authorization');
    if (authorization !== `Bearer ${TOKEN}`) {
      return problem(pool, 401, 'unauthorized', 'invalid or missing token');
    }

    if (method === 'GET' && path === '/v1/connections') {
      return json(pool, { connections: pool.connections.map((item) => view(item, pool)) });
    }

    if (method === 'POST' && path === '/v1/connections') {
      const body = bodyOf(init);
      pool.bodies.push(body);
      const reference = typeof body['credential_ref'] === 'string' ? body['credential_ref'] : '';
      if (!pool.secrets.has(reference)) {
        return problem(
          pool,
          422,
          'validation_error',
          `credential reference '${reference}' is not set`,
        );
      }
      const connection: FakeConnection = {
        id: `connection-${pool.nextId}`,
        provider: body['provider'] as FakeConnection['provider'],
        slug: String(body['slug']),
        display_name: String(body['display_name']),
        status: 'active',
        managed_by: 'database',
        credential_ref: reference,
        resources: Array.isArray(body['resources'])
          ? (body['resources'] as FakeConnection['resources'])
          : [],
        created_at: '2026-10-29T09:00:00.000Z',
      };
      pool.nextId += 1;
      pool.connections.push(connection);
      return json(pool, view(connection, pool), 201);
    }

    const match = /^\/v1\/connections\/([^/]+)$/.exec(path);
    if (match) {
      const id = decodeURIComponent(match[1] ?? '');
      const connection = pool.connections.find((item) => item.id === id);
      if (!connection) return problem(pool, 404, 'not_found', 'connection not found');
      if (method === 'GET') return json(pool, view(connection, pool));
      if (method === 'PATCH') {
        const body = bodyOf(init);
        pool.bodies.push(body);
        if (typeof body['display_name'] === 'string') connection.display_name = body['display_name'];
        if (body['status'] === 'active' || body['status'] === 'disabled') connection.status = body['status'];
        return json(pool, view(connection, pool));
      }
      if (method === 'DELETE') {
        pool.connections = pool.connections.filter((item) => item.id !== id);
        return new Response(null, { status: 204 });
      }
    }

    return problem(pool, 404, 'not_found', 'no such route');
  };
}

/** An in-memory store for the browser preferences. */
class MemoryStorage implements ConnectionStorage {
  readonly values = new Map<string, string>();

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }
}

/** The controller wired to the real SDK and the fake pool. */
function controllerFor(pool: FakePool, storage: ConnectionStorage = new MemoryStorage()) {
  const client = createClient({
    baseUrl: BASE_URL,
    token: TOKEN,
    fetch: fakeFetch(pool),
  });
  return {
    controller: new ConnectionsController({
      transport: connectionsTransport(client),
      poolUrl: BASE_URL,
      storage,
    }),
    storage,
  };
}

describe('the Slack socket-mode connection, end to end', () => {
  it('creates, verifies, renames, disables and deletes it without reading the secret', async () => {
    const pool = fakePool();
    const { controller, storage } = controllerFor(pool);

    const loaded = await controller.load();
    expect(loaded.phase).toBe('ready');
    expect(loaded.connections).toEqual([]);

    const created = await controller.create({
      provider: 'slack',
      transport: 'slack_socket',
      slug: 'Acme Team',
      displayName: 'Acme Team',
      credentialRef: 'SLACK_ACME_CREDS',
      resources: [{ kind: 'workspace', externalId: 'T012345', displayName: 'Acme' }],
    });

    expect(created.problem).toBeNull();
    expect(created.connections).toHaveLength(1);
    const row = created.connections[0];
    expect(row?.slug).toBe('acme-team');
    expect(row?.transport).toBe('slack_socket');
    expect(row?.callback.kind).toBe('outbound');
    expect(row?.callback.url).toBeNull();
    expect(row?.credential.word).toBe('Credential set');
    expect(row?.credential.value).toBeNull();
    expect(created.verification?.ok).toBe(true);
    expect(readTransportPreferences(storage)).toEqual({ 'connection-1': 'slack_socket' });

    // The create body names the reference, never a credential value.
    const createBody = (pool.bodies[0] ?? {}) as Record<string, unknown>;
    expect(createBody['provider']).toBe('slack');
    expect(createBody['credential_ref']).toBe('SLACK_ACME_CREDS');
    expect(JSON.stringify(createBody)).not.toContain(SLACK_SECRET);

    // No response ever carried the secret or the reference.
    expect(pool.responses.length).toBeGreaterThan(0);
    for (const response of pool.responses) {
      expect(response).not.toContain(SLACK_SECRET);
      expect(response).not.toContain('SLACK_ACME_CREDS');
    }
    expect(JSON.stringify(created)).not.toContain(SLACK_SECRET);
    expect(JSON.stringify(created)).not.toContain('SLACK_ACME_CREDS');

    const renamed = await controller.rename('connection-1', 'Acme Renamed');
    expect(renamed.connections[0]?.displayName).toBe('Acme Renamed');
    expect(pool.bodies.at(-1)).toEqual({ display_name: 'Acme Renamed' });

    const disabled = await controller.setStatus('connection-1', 'disabled');
    expect(disabled.connections[0]?.status.value).toBe('disabled');
    expect(pool.bodies.at(-1)).toEqual({ status: 'disabled' });

    const removed = await controller.remove('connection-1');
    expect(removed.connections).toEqual([]);
    expect(pool.connections).toEqual([]);
    expect(pool.calls).toEqual([
      'GET /v1/connections',
      'POST /v1/connections',
      'GET /v1/connections/connection-1',
      'PATCH /v1/connections/connection-1',
      'PATCH /v1/connections/connection-1',
      'DELETE /v1/connections/connection-1',
    ]);
  });

  it('reports a pool verification failure in the pool’s own words', async () => {
    const pool = fakePool();
    const { controller } = controllerFor(pool);
    await controller.load();
    await controller.create({
      provider: 'slack',
      transport: 'slack_socket',
      slug: 'acme-team',
      displayName: 'Acme Team',
      credentialRef: 'SLACK_ACME_CREDS',
      resources: [{ kind: 'workspace', externalId: 'T012345' }],
    });

    // The pool's secret-store entry is gone; the next read reports it.
    pool.secrets.delete('SLACK_ACME_CREDS');
    const verified = await controller.verify('connection-1');
    expect(verified.verification?.ok).toBe(false);
    expect(verified.verification?.failed[0]?.id).toBe('credential_configured');
    expect(verified.verification?.summary).toContain('The credential is configured');
    expect(verified.verification?.summary).toContain('bot_token, signing_secret, app_token');
  });

  it('surfaces an unresolvable credential reference as the pool stated it', async () => {
    const pool = fakePool();
    const { controller } = controllerFor(pool);
    const state = await controller.create({
      provider: 'github',
      transport: 'github_webhook',
      slug: 'acme',
      displayName: 'Acme',
      credentialRef: 'MISSING_GITHUB_CREDS',
      resources: [{ kind: 'repository', externalId: 'acme/app' }],
    });
    expect(state.problem?.message).toBe("credential reference 'MISSING_GITHUB_CREDS' is not set");
    expect(state.problem?.status).toBe(422);
    expect(state.connections).toEqual([]);
    expect(JSON.stringify(state)).not.toContain(SLACK_SECRET);
  });

  it('prints the callback URL from the current origin for a webhook', async () => {
    const pool = fakePool();
    pool.secrets.set('GITHUB_ACME_CREDS', 'github-private-key');
    const { controller } = controllerFor(pool);
    const state = await controller.create({
      provider: 'github',
      transport: 'github_webhook',
      slug: 'acme',
      displayName: 'Acme',
      credentialRef: 'GITHUB_ACME_CREDS',
      resources: [{ kind: 'repository', externalId: 'acme/app' }],
    });
    expect(state.connections[0]?.callback.url).toBe(
      `${BASE_URL}/v1/webhooks/github/acme`,
    );
    expect(state.verification?.ok).toBe(true);
  });
});
