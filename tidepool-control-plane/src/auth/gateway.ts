/**
 * The pool's auth surface, as the browser needs it.
 *
 * Four routes come straight from the hub contract:
 *
 * - `GET  /v1/setup/status`
 * - `POST /v1/setup`
 * - `POST /v1/pool/login`
 * - `GET  /v1/pool/me`
 *
 * Sign-out additionally needs the pool to end the session server-side. The
 * contract has no logout route yet, so the gateway calls the natural companion
 * of `/v1/pool/login` — `POST /v1/pool/logout` with the caller's bearer token —
 * and reports a missing route as a named failure instead of pretending the
 * session ended. The controller still clears the local session, and the notice
 * says the pool could not confirm it. See `docs/memory.md` (2026-10-29).
 *
 * The gateway is a plain `fetch` client rather than the generated SDK because
 * logout is not in the generated surface, and because the session lifecycle is
 * small enough to keep in one readable place. The SDK remains the typed client
 * for the pool's resource routes.
 */

import type { Identity, LoginInput, LoginResult, SetupInput, SetupStatus } from './types';
import { PoolRequestError } from './problem';

/** The operations the session controller needs. */
export interface AuthGateway {
  /** Ask whether the pool still needs first-run setup. */
  setupStatus(): Promise<SetupStatus>;
  /** Create the organization and first owner. */
  setup(input: SetupInput): Promise<void>;
  /** Exchange credentials for a token. */
  login(input: LoginInput): Promise<LoginResult>;
  /** Fetch the signed-in operator and organization. */
  me(token: string): Promise<Identity>;
  /** Ask the pool to end this token's session. */
  logout(token: string): Promise<void>;
}

/** Construction options for the HTTP gateway. */
export interface PoolAuthGatewayOptions {
  /** Transport override, for tests. */
  fetch?: typeof fetch;
}

/** The path the control plane uses to end a session. */
export const LOGOUT_PATH = '/v1/pool/logout';

/** A `fetch`-based gateway for one pool. */
export class PoolAuthGateway implements AuthGateway {
  /** Absolute pool base URL, without a trailing slash. */
  readonly baseUrl: string;
  private readonly transport: typeof fetch;

  constructor(baseUrl: string, options: PoolAuthGatewayOptions = {}) {
    this.baseUrl = baseUrl.replace(/\/+$/, '');
    const transport = options.fetch ?? globalThis.fetch;
    if (typeof transport !== 'function') {
      throw new TypeError('no fetch implementation; pass options.fetch');
    }
    this.transport = transport;
  }

  async setupStatus(): Promise<SetupStatus> {
    return this.request<SetupStatus>('GET', '/v1/setup/status');
  }

  async setup(input: SetupInput): Promise<void> {
    await this.request('POST', '/v1/setup', input);
  }

  async login(input: LoginInput): Promise<LoginResult> {
    return this.request<LoginResult>('POST', '/v1/pool/login', input);
  }

  async me(token: string): Promise<Identity> {
    return this.request<Identity>('GET', '/v1/pool/me', undefined, token);
  }

  async logout(token: string): Promise<void> {
    try {
      await this.request('POST', LOGOUT_PATH, {}, token);
    } catch (error) {
      if (error instanceof PoolRequestError && (error.status === 404 || error.status === 405)) {
        throw new PoolRequestError(error.status, {
          code: 'session_revocation_unavailable',
          title: 'The pool cannot end sessions yet',
          detail:
            'This pool does not expose a logout route, so the session may remain valid until it expires. The browser has forgotten the token.',
          requestId: error.requestId,
          type: error.type,
        });
      }
      throw error;
    }
  }

  /** Send one request and normalise its success or its problem details. */
  private async request<T>(
    method: 'GET' | 'POST',
    path: string,
    body?: unknown,
    token?: string,
  ): Promise<T> {
    const headers: Record<string, string> = { accept: 'application/json' };
    if (token) headers['authorization'] = `Bearer ${token}`;
    if (body !== undefined) headers['content-type'] = 'application/json';

    let response: Response;
    try {
      response = await this.transport(`${this.baseUrl}${path}`, {
        method,
        headers,
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : 'the request never reached the pool';
      const network = new Error(message);
      network.name = 'NetworkError';
      throw network;
    }

    const requestId = response.headers.get('x-request-id');
    if (!response.ok) {
      const payload = await readJson(response);
      const record =
        payload && typeof payload === 'object' && !Array.isArray(payload)
          ? (payload as Record<string, unknown>)
          : {};
      const bodyRequestId =
        typeof record['requestId'] === 'string'
          ? record['requestId']
          : typeof record['id'] === 'string'
            ? record['id']
            : null;
      throw new PoolRequestError(response.status, {
        code: typeof record['code'] === 'string' ? record['code'] : null,
        detail: typeof record['detail'] === 'string' ? record['detail'] : null,
        title: typeof record['title'] === 'string' ? record['title'] : null,
        requestId: bodyRequestId ?? requestId,
        type: typeof record['type'] === 'string' ? record['type'] : null,
      });
    }

    if (response.status === 204) return undefined as T;
    return (await readJson(response)) as T;
  }
}

/** Build a gateway for one pool. */
export function createPoolAuthGateway(
  config: { baseUrl: string | null },
  options: PoolAuthGatewayOptions = {},
): PoolAuthGateway {
  if (!config.baseUrl) throw new TypeError('a pool URL is required for sign-in');
  return new PoolAuthGateway(config.baseUrl, options);
}

/** Read a JSON body, tolerating an empty or malformed one. */
async function readJson(response: Response): Promise<unknown> {
  const text = await response.text();
  if (text.trim() === '') return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}
