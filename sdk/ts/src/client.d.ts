import type { PoolLoginResponse } from "./generated/types.js";
import type { Resources } from "./generated/resources.js";
import type { RouteInfo } from "./generated/routes.js";
import type { RetryOptions } from "./stream.js";

/** Construction options. */
export interface ClientOptions {
  /** Absolute base URL, e.g. `https://pool.example.com`. */
  baseUrl: string;
  /** Operator JWT. */
  token?: string | null;
  /** API key, sent as `x-api-key`. */
  apiKey?: string | null;
  /** Enrolled host credential, sent as `X-Host-Credential`. */
  hostCredential?: string | null;
  /**
   * Called at most once per request: proactively when the cached expiry
   * passed, reactively on a 401. Return a new token, or `null` to give up.
   */
  refreshToken?: (() => Promise<string | { token: string; expiresIn?: number } | null>) | null;
  /** Headers sent on every call. */
  headers?: Record<string, string>;
  /** Transport override; defaults to the global `fetch`. */
  fetch?: typeof fetch;
  /** Retry policy override. */
  retry?: Partial<RetryOptions>;
  /** Sleep implementation, injectable for tests. */
  sleep?: (ms: number) => Promise<void>;
  /** Clock, injectable for tests. */
  now?: () => number;
  /** Default per-call timeout, in milliseconds (0 disables it). */
  timeoutMs?: number;
  /** WebSocket implementation for the live stream. */
  WebSocket?: typeof WebSocket | null;
}

/** Default retry policy. */
export declare const DEFAULT_RETRY: Readonly<RetryOptions>;

/** Thrown for programmer mistakes, not HTTP failures. */
export declare class TidepoolUsageError extends TypeError {
  constructor(message: string);
}

/** The Tidepool API client, with one typed namespace per resource. */
export declare class TidepoolClient {
  constructor(options: ClientOptions);

  /** Absolute base URL, no trailing slash. */
  readonly baseUrl: string;
  /** Route metadata by operation id. */
  readonly routes: Readonly<Record<string, RouteInfo>>;
  /** All typed namespaces under one object. */
  readonly api: Resources;
  /** Operator JWT, when set. */
  token: string | null;
  /** API key, when set. */
  apiKey: string | null;
  /** Host credential, when set. */
  hostCredential: string | null;
  /** Epoch milliseconds at which `token` expires, when known. */
  tokenExpiresAt: number | null;
  /** Refresh hook, when set. */
  refreshToken: (() => Promise<string | { token: string; expiresIn?: number } | null>) | null;
  /** Effective retry policy. */
  readonly retry: RetryOptions;
  /** True when the cached token expiry passed. */
  readonly tokenExpired: boolean;

  /** Store a token, optionally with the lifetime the server reported. */
  setToken(token: string | null, options?: { expiresIn?: number }): this;
  /** Forget the token and its expiry. */
  clearToken(): this;
  /** Replace the API key. */
  setApiKey(key: string | null): this;
  /** Replace the host credential. */
  setHostCredential(credential: string | null): this;
  /** Sign in and adopt the returned token. */
  login(credentials: { email: string; password: string }): Promise<PoolLoginResponse>;
  /** Route metadata for one operation. */
  describe(operationId: string): RouteInfo;
  /** Ask the contract whether a principal may call an operation. */
  allows(
    operationId: string,
    principal?: { role?: string | null; scopes?: readonly string[] | null },
  ): boolean;
  /** Build an absolute URL for one operation. */
  urlFor(
    operationId: string,
    pathParams?: Record<string, string | number>,
    query?: Record<string, string | number | boolean | undefined>,
  ): string;
  /** Call an operation by id with a single options object. */
  request<T = unknown>(
    operationId: string,
    options?: {
      pathParams?: Record<string, string | number>;
      body?: unknown;
      query?: Record<string, string | number | boolean | undefined>;
      requestId?: string;
      headers?: Record<string, string>;
      signal?: AbortSignal;
      timeoutMs?: number;
      retry?: Partial<RetryOptions> | false;
    },
  ): Promise<T>;
  /** Open an operation's live stream (used by `chatrooms.stream`). */
  _stream(
    operationId: string,
    options?: {
      token?: string;
      after?: number;
      onFrame?: (frame: unknown) => void;
      onClose?: () => void;
      webSocket?: typeof WebSocket;
    },
    pathParams?: Record<string, string | number>,
  ): import("./stream.js").ChatStream;
}

/** The generated resource namespaces are members of every client. */
export interface TidepoolClient extends Resources {}

/** Exponential backoff, replaced by `Retry-After` when the server sent one. */
export declare function delayFor(
  attempt: number,
  retryAfter: number | null,
  retry: RetryOptions,
): number;
