/**
 * The pool connection: configuration and the typed client.
 *
 * No pool is the normal first state, not an error. This module therefore keeps
 * "unconfigured" as an explicit value that the shell can explain, and only
 * builds the SDK's typed client once there is an absolute pool URL. All hub
 * calls go through that client, so every response and every failure already
 * has a typed method and a problem-details error behind it.
 */

import { createClient } from '@clawffice/tidepool-sdk';
import type { TidepoolClient } from '@clawffice/tidepool-sdk';
import { presentProblem, type PoolProblem } from './problem';

/** What the app currently knows about the pool. */
export interface PoolConfig {
  /** Absolute base URL, or null when no pool has been configured. */
  baseUrl: string | null;
}

/** Storage keys the control plane owns. */
export const POOL_URL_STORAGE_KEY = 'tidepool.control-plane.pool-url';

/** The subset of `Storage` this module needs. */
export interface KeyValueStorage {
  /** Read a saved value. */
  getItem(key: string): string | null;
}

/** The message shown when no pool has been configured. */
export const NO_POOL_MESSAGE =
  'No pool is configured yet, so this control plane has nothing to show. Point it at a Tidepool pool URL to sign in, see hosts, and dispatch runs.';

/** Raised when a call needs a pool and none is configured. */
export class PoolNotConfiguredError extends Error {
  /** Stable code, for logs and tests. */
  readonly code = 'pool_not_configured';

  constructor() {
    super(NO_POOL_MESSAGE);
    this.name = 'PoolNotConfiguredError';
  }
}

/**
 * Normalise a pool URL.
 *
 * Trims, strips trailing slashes and requires an absolute http(s) URL; anything
 * else is "not configured" rather than a request to the wrong place.
 */
export function normalisePoolUrl(raw: string | null | undefined): string | null {
  if (typeof raw !== 'string') return null;
  const trimmed = raw.trim();
  if (trimmed === '') return null;
  let url: URL;
  try {
    url = new URL(trimmed);
  } catch {
    return null;
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') return null;
  const path = url.pathname.replace(/\/+$/, '');
  return `${url.origin}${path}`;
}

/** True when the app has a pool to talk to. */
export function isPoolConfigured(config: PoolConfig): config is { baseUrl: string } {
  return normalisePoolUrl(config.baseUrl) !== null;
}

/**
 * Read the pool configuration.
 *
 * The build-time `VITE_POOL_URL` wins, so a self-hosted pool can bake in its own
 * origin; otherwise the value saved by settings is used. An unparseable value
 * is treated as unconfigured.
 */
export function poolConfigFrom(sources: {
  env?: Record<string, string | undefined>;
  storage?: KeyValueStorage | null;
}): PoolConfig {
  const fromEnv = normalisePoolUrl(sources.env?.['VITE_POOL_URL']);
  if (fromEnv) return { baseUrl: fromEnv };
  let fromStorage: string | null = null;
  try {
    fromStorage = sources.storage?.getItem(POOL_URL_STORAGE_KEY) ?? null;
  } catch {
    fromStorage = null;
  }
  return { baseUrl: normalisePoolUrl(fromStorage) };
}

/** Options the shell may pass to the typed client. */
export interface PoolClientOptions {
  /** Operator JWT, once signed in. */
  token?: string | null;
  /** Transport override, for tests. */
  fetch?: typeof fetch;
}

/**
 * Build the typed SDK client.
 *
 * @throws PoolNotConfiguredError when no absolute pool URL is configured
 */
export function poolClient(config: PoolConfig, options: PoolClientOptions = {}): TidepoolClient {
  const baseUrl = normalisePoolUrl(config.baseUrl);
  if (!baseUrl) throw new PoolNotConfiguredError();
  return createClient({
    baseUrl,
    token: options.token ?? null,
    ...(options.fetch ? { fetch: options.fetch } : {}),
  });
}

/** The result of a health check. */
export type PoolHealthResult =
  | { ok: true; baseUrl: string; health: unknown }
  | { ok: false; problem: PoolProblem };

/** Ask the pool whether it is alive. Failures come back readable, not thrown. */
export async function fetchPoolHealth(
  config: PoolConfig,
  options: PoolClientOptions = {},
): Promise<PoolHealthResult> {
  const baseUrl = normalisePoolUrl(config.baseUrl);
  if (!baseUrl) {
    return {
      ok: false,
      problem: {
        kind: 'app',
        title: 'No pool is configured',
        message: NO_POOL_MESSAGE,
        requestId: null,
        status: null,
        code: 'pool_not_configured',
        type: null,
        retryAfterMs: null,
      },
    };
  }
  try {
    const client = poolClient({ baseUrl }, options);
    const health = await client.api.system.health();
    return { ok: true, baseUrl, health };
  } catch (error) {
    return { ok: false, problem: presentProblem(error) };
  }
}
