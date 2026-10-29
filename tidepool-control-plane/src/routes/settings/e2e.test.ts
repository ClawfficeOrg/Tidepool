/**
 * End to end across the client boundary, without a server.
 *
 * The real TypeScript SDK talks to an in-memory pool through an injected
 * `fetch`, so this test exercises the same URLs, bodies, auth header and
 * problem-details parsing the browser uses. It walks the task's flow: invite a
 * member and read the expiry with its one-time token, create a key and read the
 * one-time secret, dismiss it, revoke the key and see it stay in the list
 * marked revoked — then check the last-owner rule refuses locally and that a
 * stale local count still shows the pool's own reason.
 */

import { describe, expect, it } from 'vitest';
import { createClient } from '@clawffice/tidepool-sdk';
import { MembersKeysController, membersKeysTransport } from '$lib/keys';

const NOW = new Date('2026-10-29T12:00:00.000Z');
const BASE_URL = 'https://pool.example.com';
const TOKEN = 'token-1';

/** One operator in the fake pool. */
interface FakeMember {
  id: string;
  email: string;
  display_name: string;
  role: 'owner' | 'admin' | 'member' | 'viewer';
  password_change_required: boolean;
  created_at: string;
  last_login_at: string | null;
}

/** One pending invitation in the fake pool. */
interface FakeInvitation {
  id: string;
  email: string;
  role: 'owner' | 'admin' | 'member' | 'viewer';
  created_at: string;
  expires_at: string;
}

/** One API key in the fake pool. */
interface FakeKey {
  id: string;
  name: string;
  prefix: string;
  scopes: string[];
  status: 'active' | 'revoked';
  created_at: string;
  last_used_at: string | null;
  revoked_at: string | null;
}

/** The fake pool's state. */
interface FakePool {
  members: FakeMember[];
  invitations: FakeInvitation[];
  keys: FakeKey[];
  calls: string[];
  unauthorized: number;
  nextInvitation: number;
  nextKey: number;
}

/** A fresh fake pool with one owner. */
function fakePool(): FakePool {
  return {
    members: [
      {
        id: 'user-1',
        email: 'owner@example.com',
        display_name: 'Ada',
        role: 'owner',
        password_change_required: false,
        created_at: '2026-10-01T09:00:00.000Z',
        last_login_at: '2026-10-29T09:00:00.000Z',
      },
    ],
    invitations: [],
    keys: [],
    calls: [],
    unauthorized: 0,
    nextInvitation: 1,
    nextKey: 1,
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
  return new Response(
    JSON.stringify({ status, code, detail, requestId: 'req_e2e_settings' }),
    {
      status,
      headers: { 'content-type': 'application/problem+json', 'x-request-id': 'req_e2e_settings' },
    },
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

/** The number of owners in the fake pool. */
function ownerCount(pool: FakePool): number {
  return pool.members.filter((member) => member.role === 'owner').length;
}

/**
 * The fake pool's `fetch`.
 *
 * Every answer mirrors the contract: 200 for a list, 201 for invite and key
 * creation, 204 for removal and revocation, and problem details (including the
 * last-owner conflict) for everything else.
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
      pool.unauthorized += 1;
      return problem(401, 'unauthorized', 'invalid or missing token');
    }

    if (method === 'GET' && path === '/v1/pool/members') {
      return json({ members: pool.members });
    }

    if (method === 'GET' && path === '/v1/pool/invitations') {
      return json({ invitations: pool.invitations });
    }

    if (method === 'POST' && path === '/v1/pool/invitations') {
      const body = bodyOf(init);
      const email = String(body['email'] ?? '');
      const role = String(body['role'] ?? 'member');
      if (!email.includes('@')) return problem(422, 'validation_failed', 'email is not valid');
      if (pool.invitations.some((invitation) => invitation.email === email)) {
        return problem(409, 'conflict', 'an invitation for that email is already pending');
      }
      const id = `inv-${pool.nextInvitation}`;
      pool.nextInvitation += 1;
      const invitation: FakeInvitation = {
        id,
        email,
        role: role as FakeInvitation['role'],
        created_at: NOW.toISOString(),
        expires_at: '2026-11-05T12:00:00.000Z',
      };
      pool.invitations.push(invitation);
      return json({ invitation, token: `invite-token-${id}` }, 201);
    }

    if (method === 'DELETE' && path.startsWith('/v1/pool/invitations/')) {
      const id = path.slice('/v1/pool/invitations/'.length);
      pool.invitations = pool.invitations.filter((invitation) => invitation.id !== id);
      return new Response(null, { status: 204 });
    }

    if (method === 'PATCH' && path.startsWith('/v1/pool/members/')) {
      const id = path.slice('/v1/pool/members/'.length);
      const member = pool.members.find((candidate) => candidate.id === id);
      if (!member) return problem(404, 'not_found', 'no such member');
      const role = String(bodyOf(init)['role'] ?? '');
      if (member.role === 'owner' && role !== 'owner' && ownerCount(pool) <= 1) {
        return problem(409, 'last_owner', 'The last owner cannot be demoted.');
      }
      member.role = role as FakeMember['role'];
      return json(member);
    }

    if (method === 'DELETE' && path.startsWith('/v1/pool/members/')) {
      const id = path.slice('/v1/pool/members/'.length);
      const member = pool.members.find((candidate) => candidate.id === id);
      if (!member) return problem(404, 'not_found', 'no such member');
      if (member.role === 'owner' && ownerCount(pool) <= 1) {
        return problem(409, 'last_owner', 'The last owner cannot be removed.');
      }
      pool.members = pool.members.filter((candidate) => candidate.id !== id);
      return new Response(null, { status: 204 });
    }

    if (method === 'GET' && path === '/v1/api-keys') {
      return json({ api_keys: pool.keys });
    }

    if (method === 'POST' && path === '/v1/api-keys') {
      const body = bodyOf(init);
      const name = String(body['name'] ?? '');
      const scopes = Array.isArray(body['scopes']) ? body['scopes'].map(String) : [];
      if (name === '') return problem(422, 'validation_failed', 'name is required');
      if (scopes.length === 0) return problem(422, 'validation_failed', 'scopes are required');
      const id = `key-${pool.nextKey}`;
      pool.nextKey += 1;
      const prefix = `tp_live_${id}`;
      const key: FakeKey = {
        id,
        name,
        prefix,
        scopes,
        status: 'active',
        created_at: NOW.toISOString(),
        last_used_at: null,
        revoked_at: null,
      };
      pool.keys.push(key);
      return json({ api_key: key, secret: `${prefix}.secret-value` }, 201);
    }

    if (method === 'DELETE' && path.startsWith('/v1/api-keys/')) {
      const id = path.slice('/v1/api-keys/'.length);
      const key = pool.keys.find((candidate) => candidate.id === id);
      if (!key) return problem(404, 'not_found', 'no such key');
      key.status = 'revoked';
      key.revoked_at = NOW.toISOString();
      return new Response(null, { status: 204 });
    }

    return problem(404, 'not_found', 'no such route');
  };
}

/** Build a controller talking to the fake pool through the real SDK. */
function controllerFor(pool: FakePool, now: Date = NOW): MembersKeysController {
  const client = createClient({ baseUrl: BASE_URL, token: TOKEN, fetch: fakeFetch(pool) });
  return new MembersKeysController({
    transport: membersKeysTransport(client),
    now: () => now,
  });
}

describe('members and keys, end to end through the SDK', () => {
  it('invites a member and shows the expiry and one-time token', async () => {
    const pool = fakePool();
    const controller = controllerFor(pool);

    const loaded = await controller.load();
    expect(loaded.phase).toBe('ready');
    expect(loaded.members.map((member) => member.email)).toEqual(['owner@example.com']);

    const invited = await controller.invite({ email: 'new@example.com', role: 'member' });
    expect(pool.calls).toContain('POST /v1/pool/invitations');
    expect(invited.createdInvitation?.token).toBe('invite-token-inv-1');
    expect(invited.createdInvitation?.invitation.expiry.text).toBe('Expires in 7 days');
    expect(invited.createdInvitation?.invitation.expiry.at).toBe('2026-11-05T12:00:00.000Z');
    expect(invited.invitations.map((invitation) => invitation.email)).toEqual([
      'new@example.com',
    ]);
    expect(invited.notice).toContain('Expires in 7 days');
    expect(pool.unauthorized).toBe(0);
  });

  it('creates a key, shows the secret once, then revokes it and keeps the row', async () => {
    const pool = fakePool();
    const controller = controllerFor(pool);
    await controller.load();

    const created = await controller.createKey({
      name: 'nightly',
      scopes: ['runs:dispatch', 'projects:read'],
    });
    expect(pool.calls).toContain('POST /v1/api-keys');
    expect(created.createdKey?.secret).toBe('tp_live_key-1.secret-value');
    expect(created.createdKey?.key.prefix).toBe('tp_live_key-1');
    expect(created.createdKey?.key.scopes.map((scope) => scope.scope)).toEqual([
      'projects:read',
      'runs:dispatch',
    ]);
    expect(created.createdKey?.key.lastUsed.text).toBe('Never used');
    expect(created.keys).toHaveLength(1);

    const dismissed = controller.dismissCreatedKey();
    expect(dismissed.createdKey).toBeNull();

    // A reload cannot bring the secret back; the key itself is still listed.
    const reloaded = await controller.load();
    expect(reloaded.createdKey).toBeNull();
    const keyId = reloaded.keys[0]?.id ?? '';
    expect(keyId).toBe('key-1');

    const revoked = await controller.revokeKey(keyId);
    expect(pool.calls).toContain(`DELETE /v1/api-keys/${keyId}`);
    expect(revoked.createdKey).toBeNull();
    expect(revoked.keys).toHaveLength(1);
    expect(revoked.keys[0]?.status.status).toBe('revoked');
    expect(revoked.keys[0]?.status.word).toBe('Revoked');
    expect(revoked.keys[0]?.revokedAt).toBe(NOW.toISOString());
    expect(revoked.notice).toContain('no longer authenticates');

    // The fake pool also records the revocation, so a later list agrees.
    expect(pool.keys[0]?.status).toBe('revoked');
    const after = await controller.load();
    expect(after.keys[0]?.status.status).toBe('revoked');
  });

  it('refuses to remove or demote the last owner without a request', async () => {
    const pool = fakePool();
    const controller = controllerFor(pool);
    await controller.load();

    const removed = await controller.removeMember('user-1');
    expect(removed.problem?.code).toBe('last_owner');
    expect(removed.problem?.message).toContain('last owner');
    expect(pool.calls.some((call) => call.startsWith('DELETE /v1/pool/members'))).toBe(false);

    const demoted = await controller.changeRole('user-1', 'admin');
    expect(demoted.problem?.code).toBe('last_owner');
    expect(pool.calls.some((call) => call.startsWith('PATCH /v1/pool/members'))).toBe(false);
    expect(pool.members[0]?.role).toBe('owner');
  });

  it('shows the pool’s own reason when the local owner count is stale', async () => {
    const pool = fakePool();
    pool.members.push({
      id: 'user-2',
      email: 'grace@example.com',
      display_name: 'Grace',
      role: 'owner',
      password_change_required: false,
      created_at: '2026-10-02T09:00:00.000Z',
      last_login_at: null,
    });
    const controller = controllerFor(pool);
    await controller.load();

    // Another session removes Ada, so the page's view is one owner out of
    // date; the local guard passes and the pool answers 409.
    pool.members = pool.members.filter((member) => member.id !== 'user-1');
    const refused = await controller.removeMember('user-2');

    expect(pool.calls.some((call) => call.startsWith('DELETE /v1/pool/members'))).toBe(true);
    expect(refused.problem?.status).toBe(409);
    expect(refused.problem?.message).toBe('The last owner cannot be removed.');
    expect(refused.members).toHaveLength(2);
  });

  it('refuses an invalid invitation without reaching the pool', async () => {
    const pool = fakePool();
    const controller = controllerFor(pool);
    await controller.load();

    const refused = await controller.invite({ email: 'not-an-email', role: 'member' });
    expect(refused.problem?.code).toBe('invalid_invite_input');
    expect(pool.calls).not.toContain('POST /v1/pool/invitations');
  });
});
