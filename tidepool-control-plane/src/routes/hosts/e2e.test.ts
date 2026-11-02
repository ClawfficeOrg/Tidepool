/**
 * End to end across the client boundary, without a server.
 *
 * The real TypeScript SDK talks to an in-memory pool through an injected
 * `fetch`, so this test exercises the same URLs, bodies, auth header and
 * problem-details parsing the browser uses. It walks the task's flow: enrol a
 * host, read the one-time token and command, have the host machine exchange the
 * token, and watch the host move from approval required to connected — then
 * grant and revoke execution.
 */

import { beforeEach, describe, expect, it } from 'vitest';
import { createClient } from '@clawffice/tidepool-sdk';
import type { HostView as ContractHostView } from '@clawffice/tidepool-sdk';
import { HostsController, hostsTransport } from '$lib/hosts';

const NOW = new Date('2026-10-29T12:00:00.000Z');
const BASE_URL = 'https://pool.example.com';
const TOKEN = 'token-1';

/** One host in the fake pool. */
interface FakeHost {
  id: string;
  slug: string;
  display_name: string;
  hostname: string;
  status: 'pending_approval' | 'connected' | 'offline' | 'revoked';
  execution_enabled: boolean;
  created_at: string;
  revoked_at: string | null;
}

/** One enrolled token in the fake pool. */
interface FakeToken {
  hostId: string;
  expiresAt: string;
  consumed: boolean;
}

/** The fake pool's state. */
interface FakePool {
  hosts: FakeHost[];
  tokens: Map<string, FakeToken>;
  calls: string[];
  unauthorized: number;
  nextId: number;
}

/** A fresh fake pool. */
function fakePool(): FakePool {
  return { hosts: [], tokens: new Map(), calls: [], unauthorized: 0, nextId: 1 };
}

/** The contract projection of a fake host. */
function view(host: FakeHost): ContractHostView {
  return {
    id: host.id,
    slug: host.slug,
    display_name: host.display_name,
    hostname: host.hostname,
    status: host.status,
    execution_enabled: host.execution_enabled,
    created_at: host.created_at,
    revoked_at: host.revoked_at,
  };
}

/** A JSON response. */
function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

/** A problem-details response. */
function problem(status: number, code: string, detail: string): Response {
  return new Response(JSON.stringify({ status, code, detail, requestId: 'req_e2e_hosts' }), {
    status,
    headers: { 'content-type': 'application/problem+json', 'x-request-id': 'req_e2e_hosts' },
  });
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
 * Every answer mirrors the contract: 201 for create and mint, 200 for a list
 * or an execution change, 410 for a consumed token, and problem details for
 * everything else.
 */
function fakeFetch(pool: FakePool): typeof fetch {
  return async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const url =
      typeof input === 'string' ? new URL(input) : input instanceof URL ? input : new URL(input.url);
    const method = (init?.method ?? 'GET').toUpperCase();
    const path = url.pathname;
    pool.calls.push(`${method} ${path}`);

    const authorization = new Headers(init?.headers).get('authorization');
    const operatorRoute =
      !path.endsWith('/enroll') && path.startsWith('/v1/hosts');
    if (operatorRoute && authorization !== `Bearer ${TOKEN}`) {
      pool.unauthorized += 1;
      return problem(401, 'unauthorized', 'invalid or missing token');
    }

    if (method === 'GET' && path === '/v1/hosts') {
      return json({ hosts: pool.hosts.map(view) });
    }

    if (method === 'POST' && path === '/v1/hosts') {
      const body = bodyOf(init);
      const hostname = String(body['hostname'] ?? '');
      if (hostname === '') return problem(422, 'validation_failed', 'hostname is required');
      if (pool.hosts.some((host) => host.hostname === hostname)) {
        return problem(409, 'conflict', 'a host with that hostname already exists');
      }
      const id = `host-${pool.nextId}`;
      pool.nextId += 1;
      const host: FakeHost = {
        id,
        slug: hostname.split('.')[0] ?? hostname,
        display_name: String(body['display_name'] ?? hostname),
        hostname,
        status: 'pending_approval',
        execution_enabled: false,
        created_at: NOW.toISOString(),
        revoked_at: null,
      };
      pool.hosts.push(host);
      return json(view(host), 201);
    }

    if (method === 'POST' && path === '/v1/hosts/enrollment-tokens') {
      const hostId = String(bodyOf(init)['host_id'] ?? '');
      const host = pool.hosts.find((candidate) => candidate.id === hostId);
      if (!host) return problem(404, 'not_found', 'no such host');
      const token = `tok-${pool.tokens.size + 1}.secret`;
      const expiresAt = new Date(NOW.getTime() + 10 * 60 * 1000).toISOString();
      pool.tokens.set(token, { hostId, expiresAt, consumed: false });
      return json(
        {
          enrollment_token: {
            id: `et-${pool.tokens.size}`,
            created_at: NOW.toISOString(),
            expires_at: expiresAt,
            consumed_at: null,
            revoked_at: null,
          },
          token,
        },
        201,
      );
    }

    if (method === 'POST' && path === '/v1/hosts/enroll') {
      const body = bodyOf(init);
      const token = String(body['token'] ?? '');
      const credential = String(body['credential'] ?? '');
      const record = pool.tokens.get(token);
      if (!record) return problem(410, 'enrollment_token_invalid', 'unknown or expired token');
      if (record.consumed) return problem(410, 'enrollment_token_used', 'token already used');
      if (credential.length < 32) {
        return problem(422, 'validation_failed', 'credential is too short');
      }
      record.consumed = true;
      const host = pool.hosts.find((candidate) => candidate.id === record.hostId);
      if (!host) return problem(404, 'not_found', 'no such host');
      host.status = 'connected';
      return json(view(host));
    }

    const execution = /^\/v1\/hosts\/([^/]+)\/execution$/.exec(path);
    if (execution && (method === 'POST' || method === 'DELETE')) {
      const host = pool.hosts.find((candidate) => candidate.id === execution[1]);
      if (!host) return problem(404, 'not_found', 'no such host');
      if (host.status === 'revoked') {
        return problem(409, 'host_revoked', 'the host relationship has ended');
      }
      host.execution_enabled = method === 'POST';
      return json(view(host));
    }

    return problem(404, 'not_found', `no such route: ${method} ${path}`);
  };
}

describe('enrolling a host moves it from approval required to connected', () => {
  let pool: FakePool;
  let controller: HostsController;

  beforeEach(() => {
    pool = fakePool();
    const client = createClient({
      baseUrl: BASE_URL,
      token: TOKEN,
      fetch: fakeFetch(pool),
      retry: { maxRetries: 0 },
    });
    controller = new HostsController({
      transport: hostsTransport(client),
      poolUrl: BASE_URL,
      now: () => NOW,
    });
  });

  it('walks the whole enrolment, connection and execution grant', async () => {
    const empty = await controller.load();
    expect(empty.phase).toBe('ready');
    expect(empty.hosts).toEqual([]);

    const enrolled = await controller.enroll({
      hostname: 'swaygent.local',
      display_name: 'Swaygent',
    });
    expect(enrolled.problem).toBeNull();
    expect(pool.calls).toContain('POST /v1/hosts');
    expect(pool.calls).toContain('POST /v1/hosts/enrollment-tokens');
    expect(pool.unauthorized).toBe(0);

    const result = enrolled.enrollment;
    expect(result).not.toBeNull();
    if (!result) return;

    // The page now has the single-use token and the exact command.
    expect(result.token).toMatch(/^tok-1\./);
    expect(result.host.status.word).toBe('Approval required');
    expect(result.command).toContain(`ENROLLMENT_TOKEN='${result.token}'`);
    expect(result.command).toContain(`POOL_HTTP_URL='${BASE_URL}'`);
    expect(result.command).toContain(`POOL_WS_URL='wss://pool.example.com/ws/host'`);
    expect(result.command).toContain('cargo run -p tidepool-host-agent');
    expect(enrolled.hosts.map((row) => row.status.word)).toEqual(['Approval required']);
    expect(pool.tokens.get(result.token)?.consumed).toBe(false);

    // The host machine runs that command: it exchanges the token for its
    // own credential, and the pool sees the host connect.
    const hostFetch = fakeFetch(pool);
    const exchange = await hostFetch(`${BASE_URL}/v1/hosts/enroll`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ token: result.token, credential: 'a'.repeat(64) }),
    });
    expect(exchange.status).toBe(200);
    expect(pool.tokens.get(result.token)?.consumed).toBe(true);
    expect(pool.hosts[0]?.status).toBe('connected');

    const connected = await controller.load();
    expect(connected.hosts).toHaveLength(1);
    expect(connected.hosts[0]?.status.word).toBe('Connected');
    expect(connected.hosts[0]?.status.status).toBe('connected');
    expect(connected.hosts[0]?.execution.word).toBe('Connected only');
    expect(connected.hosts[0]?.lastSeen.text).toContain('Last seen not reported');

    // The token is single use: the same exchange is refused, and the host
    // stays connected rather than being reset.
    const replay = await hostFetch(`${BASE_URL}/v1/hosts/enroll`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ token: result.token, credential: 'b'.repeat(64) }),
    });
    expect(replay.status).toBe(410);
    expect(pool.hosts[0]?.status).toBe('connected');

    // Granting execution changes what the page says about the same host.
    const hostId = connected.hosts[0]?.id ?? '';
    const granted = await controller.setExecution(hostId, true);
    expect(granted.problem).toBeNull();
    expect(pool.calls).toContain(`POST /v1/hosts/${hostId}/execution`);
    expect(granted.hosts[0]?.execution.word).toBe('May run agents');
    expect(pool.hosts[0]?.execution_enabled).toBe(true);

    // Revoking it leaves the host connected, now connected-only again.
    const revokedGrant = await controller.setExecution(hostId, false);
    expect(revokedGrant.hosts[0]?.status.word).toBe('Connected');
    expect(revokedGrant.hosts[0]?.execution.word).toBe('Connected only');
    expect(pool.calls).toContain(`DELETE /v1/hosts/${hostId}/execution`);
    expect(pool.hosts[0]?.execution_enabled).toBe(false);

    // The one-time token leaves the page when dismissed.
    expect(controller.dismissEnrollment().enrollment).toBeNull();
  });

  it('refuses an empty hostname without calling the pool', async () => {
    const refused = await controller.enroll({ hostname: '  ' });
    expect(refused.problem?.code).toBe('invalid_enrollment_input');
    expect(pool.calls).toEqual([]);
    expect(refused.enrollment).toBeNull();
  });

  it('reports a duplicate hostname as readable problem details', async () => {
    pool.hosts.push({
      id: 'host-1',
      slug: 'swaygent',
      display_name: 'Swaygent',
      hostname: 'swaygent.local',
      status: 'offline',
      execution_enabled: false,
      created_at: NOW.toISOString(),
      revoked_at: null,
    });
    const refused = await controller.enroll({ hostname: 'swaygent.local' });
    expect(refused.problem?.status).toBe(409);
    expect(refused.problem?.requestId).toBe('req_e2e_hosts');
    expect(refused.problem?.message).toContain('already exists');
  });
});
