import { describe, expect, it } from 'vitest';
import {
  SESSION_STORAGE_KEY,
  clearSession,
  isSessionExpired,
  readStoredSession,
  saveSession,
  sessionFromLogin,
  validateSetupInput,
  withIdentity,
  type SessionStorage,
} from './session';
import type { AuthSession, LoginResult, SetupInput } from './types';

/** An in-memory `Storage` with the three methods a session uses. */
function memoryStorage(seed: Record<string, string> = {}): SessionStorage & {
  values: Map<string, string>;
} {
  const values = new Map<string, string>(Object.entries(seed));
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

const LOGIN: LoginResult = {
  token: 'token-1',
  expires_in: 3600,
  must_change_password: false,
  user: {
    id: 'user-1',
    email: 'owner@example.com',
    display_name: 'Owner',
    role: 'owner',
  },
};

const SESSION: AuthSession = {
  token: 'token-1',
  expiresAt: 3_601_000,
  user: {
    id: 'user-1',
    email: 'owner@example.com',
    display_name: 'Owner',
    role: 'owner',
  },
  organization: { id: 'org-1', name: 'Pool', created_at: '2026-10-29T00:00:00Z' },
  mustChangePassword: false,
};

const SETUP: SetupInput = {
  organization_name: 'Pool',
  display_name: 'Owner',
  email: 'owner@example.com',
  password: 'twelve-characters',
};

describe('session expiry', () => {
  it('derives the local expiry from the lifetime the pool reported', () => {
    expect(sessionFromLogin(LOGIN, 1000)).toMatchObject({
      token: 'token-1',
      expiresAt: 3_601_000,
      mustChangePassword: false,
    });
  });

  it('treats a missing lifetime as no local expiry', () => {
    expect(sessionFromLogin({ ...LOGIN, expires_in: 0 }, 1000).expiresAt).toBeNull();
  });

  it('refuses a role the pool does not define', () => {
    expect(() =>
      sessionFromLogin(
        { ...LOGIN, user: { ...LOGIN.user, role: 'superuser' as unknown as 'owner' } },
        1000,
      ),
    ).toThrow(/unknown role/);
  });

  it('knows when an expiry has passed', () => {
    expect(isSessionExpired({ expiresAt: 1000 }, 999)).toBe(false);
    expect(isSessionExpired({ expiresAt: 1000 }, 1000)).toBe(true);
    expect(isSessionExpired({ expiresAt: null }, 1_000_000)).toBe(false);
  });
});

describe('stored sessions', () => {
  it('round-trips a session', () => {
    const storage = memoryStorage();
    saveSession(storage, SESSION);
    expect(storage.values.has(SESSION_STORAGE_KEY)).toBe(true);
    expect(readStoredSession(storage, 1000)).toEqual({
      session: SESSION,
      expired: false,
      malformed: false,
    });
  });

  it('drops and reports an expired session', () => {
    const storage = memoryStorage();
    saveSession(storage, SESSION);
    expect(readStoredSession(storage, 3_601_001)).toEqual({
      session: null,
      expired: true,
      malformed: false,
    });
    expect(storage.values.has(SESSION_STORAGE_KEY)).toBe(false);
  });

  it('drops an unreadable record instead of crashing', () => {
    const storage = memoryStorage({ [SESSION_STORAGE_KEY]: 'not json' });
    expect(readStoredSession(storage, 0)).toEqual({
      session: null,
      expired: false,
      malformed: true,
    });
    expect(storage.values.has(SESSION_STORAGE_KEY)).toBe(false);
  });

  it('drops a record from another version or with a bad role', () => {
    const wrongVersion = memoryStorage({
      [SESSION_STORAGE_KEY]: JSON.stringify({ v: 99, token: 't' }),
    });
    expect(readStoredSession(wrongVersion, 0).malformed).toBe(true);

    const badRole = memoryStorage({
      [SESSION_STORAGE_KEY]: JSON.stringify({
        v: 1,
        token: 't',
        expiresAt: null,
        user: { id: 'u', email: 'e', display_name: 'n', role: 'root' },
        organization: null,
        mustChangePassword: false,
      }),
    });
    expect(readStoredSession(badRole, 0).malformed).toBe(true);
  });

  it('treats an absent or blank record as no session', () => {
    expect(readStoredSession(memoryStorage(), 0).session).toBeNull();
    expect(readStoredSession(memoryStorage({ [SESSION_STORAGE_KEY]: '   ' }), 0).session).toBeNull();
    expect(readStoredSession(null, 0).session).toBeNull();
  });

  it('survives a storage that throws', () => {
    const broken: SessionStorage = {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('full');
      },
      removeItem: () => {
        throw new Error('blocked');
      },
    };
    expect(readStoredSession(broken, 0).session).toBeNull();
    expect(() => saveSession(broken, SESSION)).not.toThrow();
    expect(() => clearSession(broken)).not.toThrow();
  });

  it('clears a saved session on sign-out', () => {
    const storage = memoryStorage();
    saveSession(storage, SESSION);
    clearSession(storage);
    expect(storage.values.size).toBe(0);
  });

  it('adopts the organization and role from the identity endpoint', () => {
    const next = withIdentity(SESSION, {
      organization: { id: 'org-2', name: 'Renamed', created_at: '2026-10-29T00:00:00Z' },
      user: { ...SESSION.user, role: 'viewer', display_name: 'V' },
    });
    expect(next.organization?.id).toBe('org-2');
    expect(next.user.role).toBe('viewer');
    expect(next.user.display_name).toBe('V');
  });
});

describe('first-run validation', () => {
  it('accepts a complete setup form', () => {
    expect(validateSetupInput(SETUP)).toBeNull();
  });

  it('names the first missing or invalid field', () => {
    expect(validateSetupInput({ ...SETUP, organization_name: ' ' })).toContain('organization');
    expect(validateSetupInput({ ...SETUP, display_name: '' })).toContain('display name');
    expect(validateSetupInput({ ...SETUP, email: 'not-an-email' })).toContain('email');
    expect(validateSetupInput({ ...SETUP, password: 'short' })).toContain('12 characters');
  });
});
