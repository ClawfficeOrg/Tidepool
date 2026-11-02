/**
 * The shapes the control plane's auth layer works in.
 *
 * These mirror the hub contract (`docs/api/openapi-v1.json`) but are declared
 * locally and structurally: the browser only needs the fields it renders, and
 * the session has one extra field the pool does not send — `expiresAt`, the
 * local instant after which the token must not be used.
 */

import type { PoolProblem } from '../lib/api/problem';

/** The four operator roles the pool enforces. */
export type Role = 'owner' | 'admin' | 'member' | 'viewer';

/** Every role, most privileged first. */
export const ROLES: readonly Role[] = ['owner', 'admin', 'member', 'viewer'];

/** True when a value is one of the four roles. */
export function isRole(value: unknown): value is Role {
  return typeof value === 'string' && (ROLES as readonly string[]).includes(value);
}

/** One operator account, as the pool projects it. */
export interface AuthUser {
  /** Stable public id. */
  id: string;
  /** Login email. */
  email: string;
  /** Name a person sees. */
  display_name: string;
  /** Role the pool enforces on every request. */
  role: Role;
  /** True while the pool still requires a password change. */
  password_change_required?: boolean;
  /** Last successful sign-in, when the pool reports one. */
  last_login_at?: string | null;
}

/** The organization a pool serves. Single-tenant in v1. */
export interface AuthOrganization {
  /** Stable public id. */
  id: string;
  /** Display name. */
  name: string;
  /** Creation timestamp. */
  created_at: string;
}

/** `GET /v1/setup/status`. */
export interface SetupStatus {
  /** True when no organization exists and browser setup is allowed. */
  needs_setup: boolean;
  /** True when the pool will bootstrap its first operator at startup. */
  bootstrap_configured: boolean;
}

/** `POST /v1/setup` — the first-run form. */
export interface SetupInput {
  /** Organization display name. */
  organization_name: string;
  /** Owner display name. */
  display_name: string;
  /** Owner login email. */
  email: string;
  /** Chosen password; the pool requires at least twelve characters. */
  password: string;
}

/** `POST /v1/pool/login`. */
export interface LoginInput {
  /** Operator login email. */
  email: string;
  /** Plaintext password. */
  password: string;
}

/** The pool's sign-in response, before it becomes a session. */
export interface LoginResult {
  /** Signed JWT. */
  token: string;
  /** Token lifetime in seconds. */
  expires_in: number;
  /** True while the bootstrap password must be replaced. */
  must_change_password: boolean;
  /** The signed-in operator. */
  user: AuthUser;
}

/** `GET /v1/pool/me`. */
export interface Identity {
  /** The pool's organization. */
  organization: AuthOrganization;
  /** The signed-in operator. */
  user: AuthUser;
}

/**
 * A browser session.
 *
 * The token is the pool credential; `expiresAt` is derived from the lifetime
 * the pool reported so a reload can reject a stale token without a request.
 */
export interface AuthSession {
  /** Signed JWT. */
  token: string;
  /** Epoch milliseconds after which the token must not be used, or null. */
  expiresAt: number | null;
  /** The operator the token belongs to. */
  user: AuthUser;
  /** The organization the operator belongs to, when known. */
  organization: AuthOrganization | null;
  /** True while the pool still requires a password change. */
  mustChangePassword: boolean;
}

/** Minimum operator password length the pool enforces. */
export const MIN_SETUP_PASSWORD_LENGTH = 12;

/** The state the shell renders from. */
export type AuthState =
  | { status: 'loading' }
  | {
      status: 'setup';
      setup: SetupStatus;
      problem: PoolProblem | null;
      notice: string | null;
    }
  | {
      status: 'signed-out';
      problem: PoolProblem | null;
      notice: string | null;
    }
  | { status: 'signed-in'; session: AuthSession };

/** The notice shown when a stored session is no longer usable. */
export const SESSION_EXPIRED_NOTICE =
  'Your session expired. Sign in again to continue where you left off.';

/** The notice shown after a deliberate sign-out. */
export const SIGNED_OUT_NOTICE = 'Signed out. The pool ended this session.';
