/**
 * Remembering a session across a reload.
 *
 * The pool issues a JWT with a lifetime; the browser keeps the token, the
 * operator and the lifetime in web storage so a reload can restore the session
 * without asking for a password again. Expiry and malformed values are handled
 * here, once, so the controller never signs a person in on a dead token.
 *
 * Storage is injected so this module is testable without a browser and so a
 * blocked or full storage degrades to "no session" rather than a crash.
 */

import {
  isRole,
  type AuthSession,
  type LoginResult,
  type SetupInput,
} from './types';

/** The storage key the control plane owns. */
export const SESSION_STORAGE_KEY = 'tidepool.control-plane.session';

/** The subset of `Storage` a session needs. */
export interface SessionStorage {
  /** Read a saved value. */
  getItem(key: string): string | null;
  /** Write a value. */
  setItem(key: string, value: string): void;
  /** Remove a value. */
  removeItem(key: string): void;
}

/** What was found in storage, and why it was not usable. */
export interface StoredSessionResult {
  /** The session to restore, or null. */
  session: AuthSession | null;
  /** True when a stored session was found but had passed its expiry. */
  expired: boolean;
  /** True when the stored value could not be parsed as a session. */
  malformed: boolean;
}

/** The version stamped into the stored record, so a shape change is explicit. */
const SESSION_RECORD_VERSION = 1;

/** Turn a sign-in response into a session with a local expiry instant. */
export function sessionFromLogin(result: LoginResult, now: number): AuthSession {
  if (!isRole(result.user.role)) {
    throw new TypeError(`the pool returned an unknown role: ${String(result.user.role)}`);
  }
  const lifetimeMs = Math.max(0, result.expires_in) * 1000;
  return {
    token: result.token,
    expiresAt: lifetimeMs > 0 ? now + lifetimeMs : null,
    user: { ...result.user, role: result.user.role },
    organization: null,
    mustChangePassword: result.must_change_password,
  };
}

/** True when a session may no longer be used at `now`. */
export function isSessionExpired(session: Pick<AuthSession, 'expiresAt'>, now: number): boolean {
  return session.expiresAt !== null && now >= session.expiresAt;
}

/** Attach the organization learned from `GET /v1/pool/me`. */
export function withIdentity(
  session: AuthSession,
  identity: { organization: AuthSession['organization']; user: AuthSession['user'] },
): AuthSession {
  return {
    ...session,
    user: identity.user,
    organization: identity.organization,
  };
}

/** The first-run form needs the same validation the pool applies. */
export function validateSetupInput(input: SetupInput): string | null {
  if (input.organization_name.trim() === '') return 'Give the organization a name.';
  if (input.display_name.trim() === '') return 'Give the owner a display name.';
  if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(input.email.trim())) {
    return 'Enter a valid email address.';
  }
  if (input.password.length < 12) {
    return 'Choose a password of at least 12 characters.';
  }
  return null;
}

/** Read a stored session, dropping an expired or unreadable record. */
export function readStoredSession(
  storage: SessionStorage | null,
  now: number,
): StoredSessionResult {
  if (!storage) return { session: null, expired: false, malformed: false };
  let raw: string | null;
  try {
    raw = storage.getItem(SESSION_STORAGE_KEY);
  } catch {
    return { session: null, expired: false, malformed: false };
  }
  if (raw === null || raw.trim() === '') {
    return { session: null, expired: false, malformed: false };
  }

  let record: unknown;
  try {
    record = JSON.parse(raw);
  } catch {
    removeQuietly(storage);
    return { session: null, expired: false, malformed: true };
  }

  const session = recordToSession(record);
  if (!session) {
    removeQuietly(storage);
    return { session: null, expired: false, malformed: true };
  }
  if (isSessionExpired(session, now)) {
    removeQuietly(storage);
    return { session: null, expired: true, malformed: false };
  }
  return { session, expired: false, malformed: false };
}

/** Save a session, ignoring storage failures. */
export function saveSession(storage: SessionStorage | null, session: AuthSession): void {
  if (!storage) return;
  const record = {
    v: SESSION_RECORD_VERSION,
    token: session.token,
    expiresAt: session.expiresAt,
    user: session.user,
    organization: session.organization,
    mustChangePassword: session.mustChangePassword,
  };
  try {
    storage.setItem(SESSION_STORAGE_KEY, JSON.stringify(record));
  } catch {
    // A blocked or full storage must not break a successful sign-in.
  }
}

/** Remove the stored session. */
export function clearSession(storage: SessionStorage | null): void {
  removeQuietly(storage);
}

/** Validate one parsed record, without trusting storage. */
function recordToSession(value: unknown): AuthSession | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  if (record['v'] !== SESSION_RECORD_VERSION) return null;
  const token = record['token'];
  if (typeof token !== 'string' || token.trim() === '') return null;
  const expiresAt =
    typeof record['expiresAt'] === 'number' && Number.isFinite(record['expiresAt'])
      ? record['expiresAt']
      : null;
  const user = record['user'];
  if (!user || typeof user !== 'object' || Array.isArray(user)) return null;
  const userRecord = user as Record<string, unknown>;
  const id = userRecord['id'];
  const email = userRecord['email'];
  const displayName = userRecord['display_name'];
  const role = userRecord['role'];
  if (typeof id !== 'string' || typeof email !== 'string' || typeof displayName !== 'string') {
    return null;
  }
  if (!isRole(role)) return null;

  const lastLoginRaw = userRecord['last_login_at'];
  return {
    token,
    expiresAt,
    user: {
      id,
      email,
      display_name: displayName,
      role,
      ...(typeof userRecord['password_change_required'] === 'boolean'
        ? { password_change_required: userRecord['password_change_required'] }
        : {}),
      ...(typeof lastLoginRaw === 'string' || lastLoginRaw === null
        ? { last_login_at: lastLoginRaw }
        : {}),
    },
    organization: readOrganization(record['organization']),
    mustChangePassword: record['mustChangePassword'] === true,
  };
}

/** Read the optional organization half of a stored session. */
function readOrganization(value: unknown): AuthSession['organization'] {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  if (
    typeof record['id'] !== 'string' ||
    typeof record['name'] !== 'string' ||
    typeof record['created_at'] !== 'string'
  ) {
    return null;
  }
  return { id: record['id'], name: record['name'], created_at: record['created_at'] };
}

/** Remove a key without letting a storage failure escape. */
function removeQuietly(storage: SessionStorage | null): void {
  if (!storage) return;
  try {
    storage.removeItem(SESSION_STORAGE_KEY);
  } catch {
    // Nothing to do: the record is unusable either way.
  }
}
