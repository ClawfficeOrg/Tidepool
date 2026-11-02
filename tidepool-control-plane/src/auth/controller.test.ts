import { describe, expect, it } from 'vitest';
import {
  AuthController,
  MALFORMED_SESSION_NOTICE,
  SESSION_REJECTED_NOTICE,
  SETUP_COMPLETE_NOTICE,
  SIGNED_OUT_UNCONFIRMED_NOTICE,
} from './controller';
import { PoolRequestError } from './problem';
import { SESSION_STORAGE_KEY, saveSession, type SessionStorage } from './session';
import {
  SESSION_EXPIRED_NOTICE,
  SIGNED_OUT_NOTICE,
  type AuthUser,
  type Identity,
  type LoginInput,
  type LoginResult,
  type SetupInput,
  type SetupStatus,
} from './types';
import type { AuthGateway } from './gateway';

/** An in-memory storage shared by controllers across simulated reloads. */
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

const ORG = { id: 'org-1', name: 'Pool', created_at: '2026-10-29T00:00:00Z' };

const SETUP: SetupInput = {
  organization_name: 'Pool',
  display_name: 'Owner',
  email: 'owner@example.com',
  password: 'twelve-characters',
};

/** A pool that behaves like the hub's setup and session routes. */
class FakePool implements AuthGateway {
  needsSetup = true;
  revoked = new Set<string>();
  tokens = new Map<string, AuthUser>();
  users = new Map<string, { password: string; user: AuthUser }>();
  meCalls = 0;
  setupCalls = 0;
  logoutCalls = 0;
  failSetupStatus: Error | null = null;
  failLogout: Error | null = null;
  private counter = 0;

  async setupStatus(): Promise<SetupStatus> {
    if (this.failSetupStatus) throw this.failSetupStatus;
    return { needs_setup: this.needsSetup, bootstrap_configured: false };
  }

  async setup(input: SetupInput): Promise<void> {
    this.setupCalls += 1;
    if (!this.needsSetup) {
      throw new PoolRequestError(409, { code: 'conflict', detail: 'pool is already initialized' });
    }
    this.users.set(input.email, {
      password: input.password,
      user: {
        id: 'user-1',
        email: input.email,
        display_name: input.display_name,
        role: 'owner',
      },
    });
    this.needsSetup = false;
  }

  async login(input: LoginInput): Promise<LoginResult> {
    const found = this.users.get(input.email);
    if (!found || found.password !== input.password) {
      throw new PoolRequestError(401, {
        code: 'unauthorized',
        detail: 'invalid credentials',
      });
    }
    this.counter += 1;
    const token = `token-${this.counter}`;
    this.tokens.set(token, found.user);
    return {
      token,
      expires_in: 3600,
      must_change_password: false,
      user: found.user,
    };
  }

  async me(token: string): Promise<Identity> {
    this.meCalls += 1;
    const user = this.tokens.get(token);
    if (!user || this.revoked.has(token)) {
      throw new PoolRequestError(401, { code: 'unauthorized', detail: 'invalid or expired token' });
    }
    return { organization: ORG, user };
  }

  async logout(token: string): Promise<void> {
    this.logoutCalls += 1;
    if (this.failLogout) throw this.failLogout;
    if (!this.tokens.has(token)) {
      throw new PoolRequestError(401, { code: 'unauthorized', detail: 'invalid token' });
    }
    this.revoked.add(token);
  }
}

function signedInPool(): FakePool {
  const pool = new FakePool();
  pool.needsSetup = false;
  pool.users.set(SETUP.email, {
    password: SETUP.password,
    user: {
      id: 'user-1',
      email: SETUP.email,
      display_name: SETUP.display_name,
      role: 'owner',
    },
  });
  return pool;
}

describe('first-run setup', () => {
  it('walks setup and then signs the new operator in', async () => {
    const pool = new FakePool();
    const storage = memoryStorage();
    const controller = new AuthController({ gateway: pool, storage, now: () => 1000 });

    const start = await controller.start();
    expect(start.status).toBe('setup');

    const done = await controller.completeSetup(SETUP);
    expect(done.status).toBe('signed-in');
    if (done.status !== 'signed-in') return;
    expect(done.session.user.role).toBe('owner');
    expect(done.session.user.email).toBe(SETUP.email);
    expect(done.session.expiresAt).toBe(1000 + 3600 * 1000);
    expect(pool.setupCalls).toBe(1);
    expect(storage.values.has(SESSION_STORAGE_KEY)).toBe(true);
    expect(pool.revoked.size).toBe(0);
  });

  it('keeps the setup form and its reason when the input is invalid', async () => {
    const pool = new FakePool();
    const controller = new AuthController({ gateway: pool, storage: memoryStorage() });
    const result = await controller.completeSetup({ ...SETUP, password: 'short' });
    expect(result.status).toBe('setup');
    if (result.status !== 'setup') return;
    expect(result.notice).toContain('12 characters');
    expect(pool.setupCalls).toBe(0);
  });

  it('keeps setup with the pool failure when setup itself fails', async () => {
    const pool = new FakePool();
    pool.needsSetup = false;
    const controller = new AuthController({ gateway: pool, storage: memoryStorage() });
    const result = await controller.completeSetup(SETUP);
    expect(result.status).toBe('setup');
    if (result.status !== 'setup') return;
    expect(result.problem?.status).toBe(409);
  });

  it('returns to sign-in with a notice when setup works but sign-in does not', async () => {
    const pool = new FakePool();
    pool.login = async () => {
      throw new PoolRequestError(503, { code: 'service_unavailable', detail: 'try again' });
    };
    const controller = new AuthController({ gateway: pool, storage: memoryStorage() });
    const result = await controller.completeSetup(SETUP);
    expect(result.status).toBe('signed-out');
    if (result.status !== 'signed-out') return;
    expect(result.notice).toBe(SETUP_COMPLETE_NOTICE);
    expect(result.problem?.status).toBe(503);
  });
});

describe('sign-in and reload', () => {
  it('signs in on an initialized pool and survives a reload', async () => {
    const pool = signedInPool();
    const storage = memoryStorage();
    const first = new AuthController({ gateway: pool, storage, now: () => 1000 });

    expect((await first.start()).status).toBe('signed-out');
    const signedIn = await first.signIn({ email: SETUP.email, password: SETUP.password });
    expect(signedIn.status).toBe('signed-in');
    if (signedIn.status !== 'signed-in') return;
    const token = signedIn.session.token;

    // A reload constructs a new controller over the same storage.
    const second = new AuthController({ gateway: pool, storage, now: () => 2000 });
    const reloaded = await second.start();
    expect(reloaded.status).toBe('signed-in');
    if (reloaded.status !== 'signed-in') return;
    expect(reloaded.session.token).toBe(token);
    expect(reloaded.session.organization?.id).toBe(ORG.id);
    expect(pool.meCalls).toBe(1);
  });

  it('returns an expired stored session to sign-in without a request', async () => {
    const pool = signedInPool();
    const storage = memoryStorage();
    saveSession(storage, {
      token: 'token-old',
      expiresAt: 5_000,
      user: {
        id: 'user-1',
        email: SETUP.email,
        display_name: SETUP.display_name,
        role: 'owner',
      },
      organization: ORG,
      mustChangePassword: false,
    });

    const controller = new AuthController({ gateway: pool, storage, now: () => 6_000 });
    const result = await controller.start();
    expect(result.status).toBe('signed-out');
    if (result.status !== 'signed-out') return;
    expect(result.notice).toBe(SESSION_EXPIRED_NOTICE);
    expect(pool.meCalls).toBe(0);
    expect(storage.values.has(SESSION_STORAGE_KEY)).toBe(false);
  });

  it('returns a session the pool no longer accepts to sign-in', async () => {
    const pool = signedInPool();
    const storage = memoryStorage();
    saveSession(storage, {
      token: 'token-revoked',
      expiresAt: 9_000_000,
      user: {
        id: 'user-1',
        email: SETUP.email,
        display_name: SETUP.display_name,
        role: 'owner',
      },
      organization: ORG,
      mustChangePassword: false,
    });

    const controller = new AuthController({ gateway: pool, storage, now: () => 1000 });
    const result = await controller.start();
    expect(result.status).toBe('signed-out');
    if (result.status !== 'signed-out') return;
    expect(result.notice).toBe(SESSION_REJECTED_NOTICE);
    expect(storage.values.has(SESSION_STORAGE_KEY)).toBe(false);
  });

  it('keeps a remembered session through a transient pool failure', async () => {
    const pool = signedInPool();
    pool.me = async () => {
      const error = new Error('offline');
      error.name = 'NetworkError';
      throw error;
    };
    const storage = memoryStorage();
    saveSession(storage, {
      token: 'token-live',
      expiresAt: 9_000_000,
      user: {
        id: 'user-1',
        email: SETUP.email,
        display_name: SETUP.display_name,
        role: 'owner',
      },
      organization: ORG,
      mustChangePassword: false,
    });

    const controller = new AuthController({ gateway: pool, storage, now: () => 1000 });
    const result = await controller.start();
    expect(result.status).toBe('signed-in');
    expect(storage.values.has(SESSION_STORAGE_KEY)).toBe(true);
  });

  it('reports bad credentials instead of inventing a session', async () => {
    const pool = signedInPool();
    const storage = memoryStorage();
    const controller = new AuthController({ gateway: pool, storage });
    const result = await controller.signIn({ email: SETUP.email, password: 'wrong-password' });
    expect(result.status).toBe('signed-out');
    if (result.status !== 'signed-out') return;
    expect(result.problem?.status).toBe(401);
    expect(result.problem?.code).toBe('unauthorized');
    expect(storage.values.has(SESSION_STORAGE_KEY)).toBe(false);
  });

  it('reports a malformed stored record without trying it', async () => {
    const pool = signedInPool();
    const storage = memoryStorage();
    storage.values.set(SESSION_STORAGE_KEY, '{');
    const controller = new AuthController({ gateway: pool, storage });
    const result = await controller.start();
    expect(result.status).toBe('signed-out');
    if (result.status !== 'signed-out') return;
    expect(result.notice).toBe(MALFORMED_SESSION_NOTICE);
    expect(pool.meCalls).toBe(0);
  });

  it('reports a pool that cannot answer setup status', async () => {
    const pool = new FakePool();
    pool.failSetupStatus = new PoolRequestError(503, {
      code: 'service_unavailable',
      detail: 'database unavailable',
    });
    const controller = new AuthController({ gateway: pool, storage: memoryStorage() });
    const result = await controller.start();
    expect(result.status).toBe('signed-out');
    if (result.status !== 'signed-out') return;
    expect(result.problem?.status).toBe(503);
  });
});

describe('sign-out', () => {
  it('ends the session in the pool, not only locally', async () => {
    const pool = signedInPool();
    const storage = memoryStorage();
    const controller = new AuthController({ gateway: pool, storage });
    const signedIn = await controller.signIn({ email: SETUP.email, password: SETUP.password });
    expect(signedIn.status).toBe('signed-in');
    if (signedIn.status !== 'signed-in') return;
    const token = signedIn.session.token;

    const result = await controller.signOut();
    expect(result.status).toBe('signed-out');
    if (result.status !== 'signed-out') return;
    expect(result.notice).toBe(SIGNED_OUT_NOTICE);
    expect(pool.logoutCalls).toBe(1);
    expect(pool.revoked.has(token)).toBe(true);
    expect(storage.values.has(SESSION_STORAGE_KEY)).toBe(false);
    await expect(pool.me(token)).rejects.toMatchObject({ status: 401 });

    // The next mount finds no session and stays signed out.
    const reloaded = new AuthController({ gateway: pool, storage });
    expect((await reloaded.start()).status).toBe('signed-out');
  });

  it('forgets the token even when the pool cannot confirm the sign-out', async () => {
    const pool = signedInPool();
    pool.failLogout = new PoolRequestError(503, {
      code: 'service_unavailable',
      detail: 'database unavailable',
    });
    const storage = memoryStorage();
    const controller = new AuthController({ gateway: pool, storage });
    await controller.signIn({ email: SETUP.email, password: SETUP.password });

    const result = await controller.signOut();
    expect(result.status).toBe('signed-out');
    if (result.status !== 'signed-out') return;
    expect(result.notice).toBe(SIGNED_OUT_UNCONFIRMED_NOTICE);
    expect(result.problem?.status).toBe(503);
    expect(storage.values.has(SESSION_STORAGE_KEY)).toBe(false);
  });

  it('is a quiet no-op when nothing is signed in', async () => {
    const pool = signedInPool();
    const controller = new AuthController({ gateway: pool, storage: memoryStorage() });
    const result = await controller.signOut();
    expect(result.status).toBe('signed-out');
    expect(pool.logoutCalls).toBe(0);
  });
});
