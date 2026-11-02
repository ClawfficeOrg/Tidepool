//! The Tidepool API client.
//!
//! Generated methods live in `src/generated/resources.js`; this module owns the
//! transport, authentication, problem mapping, bounded retries and the stream
//! entry point. It uses `fetch` only, so the same module runs in a browser and
//! in Node without a Node-only dependency.

import { ROUTES } from "./generated/routes.js";
import { buildResources } from "./generated/resources.js";
import { NetworkError, ProblemError, normaliseProblem, retryAfterMs as parseRetryAfter } from "./problem.js";
import { openChatStream } from "./stream.js";

/** Default retry policy: bounded, 429-only, honouring `Retry-After`. */
export const DEFAULT_RETRY = Object.freeze({
  /** Maximum retries after the first attempt. Never unbounded. */
  maxRetries: 2,
  /** First exponential delay when no `Retry-After` was sent. */
  baseDelayMs: 250,
  /** Ceiling for exponential delays. */
  maxDelayMs: 4000,
  /** Add up to 50% jitter (off by default for deterministic tests). */
  jitter: false,
  /** A `Retry-After` longer than this is surfaced, not slept through. */
  maxRetryAfterMs: 60_000,
});

const AUTH_HEADERS = Object.freeze({
  token: "authorization",
  apiKey: "x-api-key",
  hostCredential: "x-host-credential",
});

/** Error thrown for programmer mistakes, not HTTP failures. */
export class TidepoolUsageError extends TypeError {
  constructor(message) {
    super(message);
    this.name = "TidepoolUsageError";
  }
}

/**
 * The Tidepool API client.
 *
 * Credentials are ambient: set `token`, `apiKey` or `hostCredential` and every
 * call sends the matching header. `refreshToken` is called at most once per
 * request, either proactively when the cached expiry passed or reactively on a
 * 401, so a stale token cannot loop.
 */
export class TidepoolClient {
  /**
   * @param {{
   *   baseUrl: string,
   *   token?: string | null,
   *   apiKey?: string | null,
   *   hostCredential?: string | null,
   *   refreshToken?: (() => Promise<string | { token: string, expiresIn?: number } | null>) | null,
   *   headers?: Record<string, string>,
   *   fetch?: typeof fetch,
   *   retry?: Partial<typeof DEFAULT_RETRY>,
   *   sleep?: (ms: number) => Promise<void>,
   *   now?: () => number,
   *   timeoutMs?: number,
   *   WebSocket?: typeof WebSocket | null,
   * }} options
   */
  constructor(options) {
    if (!options || typeof options.baseUrl !== "string" || !options.baseUrl) {
      throw new TidepoolUsageError("baseUrl is required");
    }
    /** Absolute base URL, no trailing slash. */
    this.baseUrl = options.baseUrl.replace(/\/+$/, "");
    // Bound: browsers throw "Illegal invocation" when fetch is called on another receiver.
    this.fetchImpl = options.fetch ?? globalThis.fetch?.bind(globalThis);
    if (typeof this.fetchImpl !== "function") {
      throw new TidepoolUsageError("no fetch implementation; pass options.fetch");
    }
    this.token = options.token ?? null;
    this.apiKey = options.apiKey ?? null;
    this.hostCredential = options.hostCredential ?? null;
    this.refreshToken = options.refreshToken ?? null;
    this.headers = { ...(options.headers ?? {}) };
    this.retry = { ...DEFAULT_RETRY, ...(options.retry ?? {}) };
    this.sleep = options.sleep ?? ((ms) => new Promise((resolve) => setTimeout(resolve, ms)));
    this.now = options.now ?? (() => Date.now());
    this.timeoutMs = options.timeoutMs ?? 0;
    this.WebSocket = options.WebSocket ?? null;
    /** Epoch milliseconds when `token` stops working, if the server said. */
    this.tokenExpiresAt = null;
    /** Route metadata by operation id, read-only. */
    this.routes = ROUTES;
    /** Typed resource namespaces, also spread onto the instance. */
    this.api = buildResources(this);
    Object.assign(this, this.api);
  }

  /** Store a token, optionally with the lifetime the server reported. */
  setToken(token, options = {}) {
    this.token = token ?? null;
    this.tokenExpiresAt =
      typeof options.expiresIn === "number" && options.expiresIn > 0 ? this.now() + options.expiresIn * 1000 : null;
    return this;
  }

  /** Forget the current token and expiry. */
  clearToken() {
    this.token = null;
    this.tokenExpiresAt = null;
    return this;
  }

  /** Replace the API key sent as `x-api-key`. */
  setApiKey(key) {
    this.apiKey = key ?? null;
    return this;
  }

  /** Replace the host credential sent as `X-Host-Credential`. */
  setHostCredential(credential) {
    this.hostCredential = credential ?? null;
    return this;
  }

  /** True when the cached token expiry has passed. */
  get tokenExpired() {
    return this.tokenExpiresAt !== null && this.now() >= this.tokenExpiresAt;
  }

  /**
   * Sign in and adopt the returned token.
   *
   * @param {{ email: string, password: string }} credentials
   */
  async login(credentials) {
    const response = await this.api.pool.login(credentials);
    this.setToken(response.token, { expiresIn: response.expires_in });
    return response;
  }

  /**
   * Route metadata for one operation: auth model, required scope and role.
   *
   * @param {string} operationId
   */
  describe(operationId) {
    const route = ROUTES[operationId];
    if (!route) throw new TidepoolUsageError(`unknown operation: ${operationId}`);
    return route;
  }

  /**
   * Ask the contract whether a principal may call an operation, before sending
   * anything (docs/API_CONTRACT.md §4).
   *
   * @param {string} operationId
   * @param {{ role?: string | null, scopes?: readonly string[] | null, authenticated?: boolean }} [principal]
   */
  allows(operationId, principal = {}) {
    const route = this.describe(operationId);
    const role = principal.role ?? null;
    const scopes = principal.scopes ?? [];
    if (route.role && role !== route.role) {
      // An owner-only operation still accepts an API key only when it declares
      // a scope; none of the owner-only routes do.
      return false;
    }
    if (route.auth === "public") return true;
    if (route.scope && scopes.includes(route.scope)) return true;
    if (route.auth === "api_key") return route.scope ? scopes.includes(route.scope) : false;
    // `member` and `member_or_api_key`: any role for reads, non-viewer writes.
    if (!role) return false;
    if (route.method !== "GET" && role === "viewer") return false;
    return true;
  }

  /**
   * Build an absolute URL for one operation.
   *
   * @param {string} operationId
   * @param {Record<string, string | number>} [pathParams]
   * @param {Record<string, string | number | boolean | undefined>} [query]
   */
  urlFor(operationId, pathParams = {}, query = {}) {
    const route = this.describe(operationId);
    let path = route.path;
    for (const name of route.pathParams) {
      const value = pathParams?.[name];
      if (value === undefined || value === null || value === "") {
        throw new TidepoolUsageError(`${operationId}: missing path parameter "${name}"`);
      }
      path = path.replace(`{${name}}`, encodeURIComponent(String(value)));
    }
    const search = new URLSearchParams();
    for (const [key, value] of Object.entries(query ?? {})) {
      if (value === undefined || value === null) continue;
      search.append(key, String(value));
    }
    const suffix = search.toString();
    return `${this.baseUrl}${path}${suffix ? `?${suffix}` : ""}`;
  }

  /** Auth headers for the ambient credentials, in the contract's precedence. */
  #authHeaders() {
    /** @type {Record<string, string>} */
    const headers = {};
    if (this.token) headers[AUTH_HEADERS.token] = `Bearer ${this.token}`;
    if (this.apiKey) headers[AUTH_HEADERS.apiKey] = this.apiKey;
    if (this.hostCredential) headers[AUTH_HEADERS.hostCredential] = this.hostCredential;
    return headers;
  }

  /** Refresh the token at most once per call; a hook returning null leaves it. */
  async #refresh() {
    if (typeof this.refreshToken !== "function") return false;
    const next = await this.refreshToken();
    if (!next) return false;
    if (typeof next === "string") {
      this.setToken(next);
    } else if (typeof next === "object" && typeof next.token === "string") {
      this.setToken(next.token, { expiresIn: next.expiresIn });
    } else {
      return false;
    }
    return true;
  }

  /**
   * Call an operation by id. Generated methods are thin wrappers over this.
   *
   * @param {string} operationId
   * @param {{
   *   query?: Record<string, string | number | boolean | undefined>,
   *   requestId?: string,
   *   headers?: Record<string, string>,
   *   signal?: AbortSignal,
   *   timeoutMs?: number,
   *   retry?: Partial<typeof DEFAULT_RETRY> | false,
   * }} [options]
   * @param {Record<string, string | number>} [pathParams]
   * @param {unknown} [body]
   */
  async _call(operationId, options = {}, pathParams = {}, body = undefined) {
    const route = this.describe(operationId);
    const url = this.urlFor(operationId, normalisePathParams(route, pathParams), options.query ?? {});
    const retry = options.retry === false ? { ...this.retry, maxRetries: 0 } : { ...this.retry, ...(options.retry ?? {}) };
    let refreshAttempted = false;
    let attempt = 0;
    /** @type {unknown} */
    let responseBody;
    for (;;) {
      if (!refreshAttempted && this.tokenExpired && this.token) {
        refreshAttempted = true;
        await this.#refresh();
      }
      const headers = {
        accept: "application/json",
        ...this.headers,
        ...this.#authHeaders(),
        ...(options.headers ?? {}),
      };
      const init = { method: route.method, headers };
      if (options.requestId) headers["x-request-id"] = options.requestId;
      if (body !== undefined) {
        if (!headers["content-type"]) headers["content-type"] = "application/json";
        init.body = JSON.stringify(body);
      }
      if (options.signal) init.signal = options.signal;
      let response;
      try {
        response = await this.#send(url, init, options.timeoutMs);
      } catch (error) {
        if (error?.name === "AbortError") throw error;
        throw new NetworkError(`${route.method} ${route.path} failed: ${error?.message ?? error}`, {
          operationId,
          cause: error,
        });
      }
      responseBody = await readResponseBody(response);
      if (response.ok) return responseBody;

      const retryAfter = parseRetryAfter(response.headers.get("retry-after"), this.now);
      if (response.status === 401 && !refreshAttempted) {
        refreshAttempted = true;
        if (await this.#refresh()) continue;
      }
      const canRetry =
        response.status === 429 &&
        attempt < retry.maxRetries &&
        (retryAfter === null || retryAfter <= retry.maxRetryAfterMs);
      if (canRetry) {
        await this.sleep(delayFor(attempt, retryAfter, retry));
        attempt += 1;
        continue;
      }
      const problem = normaliseProblem(responseBody, { status: response.status, headers: response.headers });
      throw new ProblemError(problemMessage(route, problem), {
        ...problem,
        operationId,
        attempts: attempt + 1,
        retryAfterMs: retryAfter,
      });
    }
  }

  /** Send with an optional client-side timeout, without leaking a timer. */
  async #send(url, init, timeoutMs) {
    const limit = timeoutMs ?? this.timeoutMs;
    if (!limit || init.signal) return this.fetchImpl(url, init);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(new Error("timeout")), limit);
    try {
      return await this.fetchImpl(url, { ...init, signal: controller.signal });
    } finally {
      clearTimeout(timer);
    }
  }

  /**
   * Call an operation by id with a single options object, for callers that
   * prefer not to use the generated resource namespaces.
   *
   * @param {string} operationId
   * @param {{ pathParams?: Record<string, string | number>, body?: unknown } & Parameters<TidepoolClient["_call"]>[1]} [options]
   */
  async request(operationId, options = {}) {
    const { pathParams, body, ...rest } = options;
    return this._call(operationId, rest, pathParams ?? {}, body);
  }

  /**
   * Open the chatroom live stream for one room.
   *
   * @param {string} operationId
   * @param {{ token?: string, after?: number, onFrame?: (frame: any) => void, onClose?: () => void, webSocket?: typeof WebSocket }} [options]
   * @param {Record<string, string | number>} [pathParams]
   */
  _stream(operationId, options = {}, pathParams = {}) {
    const route = this.describe(operationId);
    const token = options.token ?? this.token;
    if (!token) throw new TidepoolUsageError(`${operationId}: a token is required for the live stream`);
    const httpUrl = this.urlFor(operationId, normalisePathParams(route, pathParams), {
      token,
      after: options.after,
      ...(options.query ?? {}),
    });
    const url = httpUrl.replace(/^http/, "ws");
    return openChatStream({
      url,
      WebSocketImpl: options.webSocket ?? this.WebSocket,
      onFrame: options.onFrame,
      onClose: options.onClose,
    });
  }
}

/** Exponential backoff, replaced by `Retry-After` when the server sent one. */
export function delayFor(attempt, retryAfter, retry) {  if (retryAfter !== null) return retryAfter;
  const base = Math.min(retry.baseDelayMs * 2 ** attempt, retry.maxDelayMs);
  return retry.jitter ? Math.round(base * (0.5 + Math.random() * 0.5)) : base;
}

/** Accept `"id"` for a single-param route and an object otherwise. */
function normalisePathParams(route, pathParams) {
  if (pathParams && typeof pathParams === "object") return pathParams;
  if (pathParams === undefined || pathParams === null) return {};
  if (route.pathParams.length !== 1) {
    throw new TidepoolUsageError(
      `${route.operationId}: path parameters must be an object with ${route.pathParams.join(", ")}`,
    );
  }
  return { [route.pathParams[0]]: pathParams };
}

/** One-line message including the correlation id. */
function problemMessage(route, problem) {
  const suffix = problem.requestId ? ` (requestId ${problem.requestId})` : "";
  const detail = problem.detail ? `: ${problem.detail}` : "";
  return `${route.method} ${route.path} -> ${problem.status} ${problem.code}${detail}${suffix}`;
}

/** Parse a response body according to its content type. */
async function readResponseBody(response) {
  if (response.status === 204 || response.status === 205) return undefined;
  const contentType = response.headers.get("content-type") ?? "";
  const text = await response.text();
  if (!text) return undefined;
  if (contentType.includes("json")) {
    try {
      return JSON.parse(text);
    } catch {
      return text;
    }
  }
  return text;
}
