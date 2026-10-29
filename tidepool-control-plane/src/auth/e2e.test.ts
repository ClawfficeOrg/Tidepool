import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { once } from 'node:events';
import type { AddressInfo } from 'node:net';
import { AuthController } from './controller';
import { createPoolAuthGateway } from './gateway';
import { SESSION_STORAGE_KEY, type SessionStorage } from './session';
import { SIGNED_OUT_NOTICE, type LoginInput, type SetupInput, type AuthUser } from './types';

/** A tiny but real HTTP pool, so the test crosses the network boundary. */
interface FakePool {
  needsSetup: boolean;
  users: Map<string, { password: string; user: AuthUser }>;
  tokens: Map<string, AuthUser>;
  revoked: Set<string>;
  calls: string[];
}

const ORG = { id: 'org-1', name: 'Pool', created_at: '2026-10-29T00:00:00Z' };

const SETUP: SetupInput = {
  organization_name: 'Pool',
  display_name: 'Owner',
  email: 'owner@example.com',
  password: 'twelve-characters',
};

/** Read and parse a request body. */
async function readJson(request: IncomingMessage): Promise<Record<string, unknown>> {
  const chunks: Buffer[] = [];
  for await (const chunk of request) chunks.push(chunk as Buffer);
  const text = Buffer.concat(chunks).toString('utf8');
  if (text.trim() === '') return {};
  try {
    const parsed: unknown = JSON.parse(text);
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : {};
  } catch {
    return {};
  }
}

/** Write a problem-details response. */
function problem(
  response: ServerResponse,
  status: number,
  code: string,
  detail: string,
): void {
  response.writeHead(status, {
    'content-type': 'application/problem+json',
    'x-request-id': 'req_e2e',
  });
  response.end(JSON.stringify({ status, code, detail, requestId: 'req_e2e' }));
}

/** Bearer token from a request, when there is one. */
function bearer(request: IncomingMessage): string | null {
  const header = request.headers.authorization;
  if (typeof header !== 'string' || !header.startsWith('Bearer ')) return null;
  return header.slice('Bearer '.length);
}

/** The fake pool, with the five routes the browser uses. */
function createFakePool(): FakePool {
  return {
    needsSetup: true,
    users: new Map(),
    tokens: new Map(),
    revoked: new Set(),
    calls: [],
  };
}

/** Handle one request against the fake pool's state. */
async function handle(
  pool: FakePool,
  request: IncomingMessage,
  response: ServerResponse,
): Promise<void> {
  const method = request.method ?? 'GET';
  const url = request.url ?? '/';
  pool.calls.push(`${method} ${url}`);

  if (method === 'GET' && url === '/v1/setup/status') {
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end(
      JSON.stringify({ needs_setup: pool.needsSetup, bootstrap_configured: false }),
    );
    return;
  }

  if (method === 'POST' && url === '/v1/setup') {
    const body = await readJson(request);
    if (!pool.needsSetup) {
      problem(response, 409, 'conflict', 'pool is already initialized');
      return;
    }
    const email = String(body['email'] ?? '');
    const user: AuthUser = {
      id: 'user-1',
      email,
      display_name: String(body['display_name'] ?? ''),
      role: 'owner',
    };
    pool.users.set(email, { password: String(body['password'] ?? ''), user });
    pool.needsSetup = false;
    response.writeHead(201, { 'content-type': 'application/json' });
    response.end(JSON.stringify({ organization: ORG, user }));
    return;
  }

  if (method === 'POST' && url === '/v1/pool/login') {
    const body = (await readJson(request)) as Partial<LoginInput>;
    const found = pool.users.get(body.email ?? '');
    if (!found || found.password !== body.password) {
      problem(response, 401, 'unauthorized', 'invalid credentials');
      return;
    }
    const token = `token-${pool.tokens.size + 1}`;
    pool.tokens.set(token, found.user);
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end(
      JSON.stringify({
        token,
        expires_in: 3600,
        must_change_password: false,
        user: found.user,
      }),
    );
    return;
  }

  if (method === 'GET' && url === '/v1/pool/me') {
    const token = bearer(request);
    const user = token ? pool.tokens.get(token) : undefined;
    if (!token || !user || pool.revoked.has(token)) {
      problem(response, 401, 'unauthorized', 'invalid or expired token');
      return;
    }
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end(JSON.stringify({ organization: ORG, user }));
    return;
  }

  if (method === 'POST' && url === '/v1/pool/logout') {
    const token = bearer(request);
    if (!token || !pool.tokens.has(token) || pool.revoked.has(token)) {
      problem(response, 401, 'unauthorized', 'invalid token');
      return;
    }
    pool.revoked.add(token);
    response.writeHead(204);
    response.end();
    return;
  }

  problem(response, 404, 'not_found', `no such route: ${method} ${url}`);
}

/** An in-memory storage shared by the simulated reloads. */
function memoryStorage(): SessionStorage & { values: Map<string, string> } {
  const values = new Map<string, string>();
  return {
    values,
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => {
      values.set(key, value);
    },
    removeItem: (key) => {
      values.delete(key);
    },
  };
}

describe('setup, sign-in, reload and sign-out over HTTP', () => {
  let pool: FakePool;
  let server: ReturnType<typeof createServer>;
  let baseUrl = '';
  let storage: SessionStorage & { values: Map<string, string> };

  beforeAll(async () => {
    pool = createFakePool();
    server = createServer((request, response) => {
      void handle(pool, request, response);
    });
    server.listen(0, '127.0.0.1');
    await once(server, 'listening');
    const address = server.address() as AddressInfo;
    baseUrl = `http://127.0.0.1:${address.port}`;
    storage = memoryStorage();
  });

  afterAll(async () => {
    server.closeAllConnections();
    server.close();
    await once(server, 'close');
  });

  it('walks setup, signs in, survives a reload and ends the session server-side', async () => {
    const gateway = createPoolAuthGateway({ baseUrl });
    const first = new AuthController({ gateway, storage, now: () => 1000 });

    expect((await first.start()).status).toBe('setup');
    expect(pool.calls).toContain('GET /v1/setup/status');

    const done = await first.completeSetup(SETUP);
    expect(done.status).toBe('signed-in');
    if (done.status !== 'signed-in') return;
    expect(pool.calls).toContain('POST /v1/setup');
    expect(pool.calls).toContain('POST /v1/pool/login');
    const token = done.session.token;
    expect(storage.values.has(SESSION_STORAGE_KEY)).toBe(true);

    // Reload: a new controller, same storage, fresh clock.
    const second = new AuthController({ gateway, storage, now: () => 2000 });
    const reloaded = await second.start();
    expect(reloaded.status).toBe('signed-in');
    if (reloaded.status !== 'signed-in') return;
    expect(reloaded.session.token).toBe(token);
    expect(reloaded.session.organization?.name).toBe(ORG.name);
    expect(pool.calls).toContain('GET /v1/pool/me');

    const out = await second.signOut();
    expect(out.status).toBe('signed-out');
    if (out.status !== 'signed-out') return;
    expect(out.notice).toBe(SIGNED_OUT_NOTICE);
    expect(pool.calls).toContain('POST /v1/pool/logout');
    expect(pool.revoked.has(token)).toBe(true);

    // The token is dead for anyone who kept it, not just in this browser.
    const replay = await fetch(`${baseUrl}/v1/pool/me`, {
      headers: { authorization: `Bearer ${token}` },
    });
    expect(replay.status).toBe(401);
    expect(storage.values.has(SESSION_STORAGE_KEY)).toBe(false);

    // A reload after sign-out lands on sign-in.
    const third = new AuthController({ gateway, storage, now: () => 3000 });
    const afterSignOut = await third.start();
    expect(afterSignOut.status).toBe('signed-out');
  });
});
