//! Typed errors for the Tidepool API.
//!
//! Every hub failure is an RFC 9457 `application/problem+json` document
//! (`docs/HUB_API_ERRORS.md`). `ProblemError` carries that document as typed
//! fields, keeps `requestId` even when the body omits it, and preserves an
//! unknown `code` verbatim so a client can survive an additive contract change.

/** Base class for every error this SDK throws. */
export class TidepoolError extends Error {
  /**
   * @param {string} message
   * @param {{ operationId?: string, cause?: unknown }} [details]
   */
  constructor(message, details = {}) {
    super(message);
    this.name = "TidepoolError";
    /** Operation id that produced the error, when known. */
    this.operationId = details.operationId;
    if (details.cause !== undefined) this.cause = details.cause;
  }
}

/** The transport failed before an HTTP response existed. */
export class NetworkError extends TidepoolError {
  /**
   * @param {string} message
   * @param {{ operationId?: string, cause?: unknown }} [details]
   */
  constructor(message, details = {}) {
    super(message, details);
    this.name = "NetworkError";
  }
}

/** A non-2xx response. Carries the problem details fields. */
export class ProblemError extends TidepoolError {
  /**
   * @param {string} message
   * @param {{
   *   status: number,
   *   code: string,
   *   detail?: string | null,
   *   title?: string | null,
   *   type?: string | null,
   *   requestId?: string | null,
   *   operationId?: string,
   *   headers?: Headers | null,
   *   attempts?: number,
   *   retryAfterMs?: number | null,
   *   body?: unknown,
   * }} details
   */
  constructor(message, details) {
    super(message, { operationId: details.operationId });
    this.name = "ProblemError";
    /** HTTP status code. */
    this.status = details.status;
    /** Stable machine-readable code; unknown codes stay opaque strings. */
    this.code = details.code;
    /** Human-readable explanation, when the server sent one. */
    this.detail = details.detail ?? null;
    /** Short human-readable summary. */
    this.title = details.title ?? null;
    /** Problem type URI. */
    this.type = details.type ?? null;
    /** Correlation id from the body or the `x-request-id` header. */
    this.requestId = details.requestId ?? null;
    /** Response headers, for `Retry-After` and other operational data. */
    this.headers = details.headers ?? null;
    /** How many HTTP attempts produced this error (1 = no retry). */
    this.attempts = details.attempts ?? 1;
    /** Server-requested delay in milliseconds, when a 429 sent `Retry-After`. */
    this.retryAfterMs = details.retryAfterMs ?? null;
    /** Parsed response body (problem document, or the raw value). */
    this.body = details.body ?? null;
  }

  /** True when the failure is a rate limit. */
  get rateLimited() {
    return this.status === 429;
  }

  /** JSON-safe view, useful in logs. */
  toJSON() {
    return {
      name: this.name,
      message: this.message,
      status: this.status,
      code: this.code,
      detail: this.detail,
      requestId: this.requestId,
      operationId: this.operationId,
      attempts: this.attempts,
      retryAfterMs: this.retryAfterMs,
    };
  }
}

/**
 * Normalise any non-2xx payload into the problem-details fields.
 *
 * Hub routes send `Problem`; the game surface sends `GameError`, which has no
 * `requestId`, so the header wins. Unknown shapes become `internal_error`-ish
 * opaque values rather than an untyped exception.
 *
 * @param {unknown} body
 * @param {{ status: number, headers?: Headers | null }} meta
 */
export function normaliseProblem(body, meta) {
  const record =
    body && typeof body === "object" && !Array.isArray(body) ? /** @type {Record<string, unknown>} */ (body) : {};
  const headerRequestId = meta.headers?.get?.("x-request-id") ?? null;
  const code =
    typeof record.code === "string"
      ? record.code
      : typeof record.error === "string"
        ? record.error
        : `http_${meta.status}`;
  const detail =
    typeof record.detail === "string"
      ? record.detail
      : typeof record.message === "string"
        ? record.message
        : null;
  const title = typeof record.title === "string" ? record.title : null;
  const type = typeof record.type === "string" ? record.type : null;
  const requestId =
    typeof record.requestId === "string" ? record.requestId : typeof record.id === "string" ? record.id : headerRequestId;
  return {
    status: meta.status,
    code,
    detail,
    title,
    type,
    requestId,
    headers: meta.headers ?? null,
    body,
  };
}

/**
 * Parse a `Retry-After` header into milliseconds.
 *
 * Accepts both forms RFC 7231 allows: delta-seconds, or an HTTP-date.
 * Returns `null` when the header is absent or nonsense.
 *
 * @param {string | null | undefined} value
 * @param {() => number} now
 */
export function retryAfterMs(value, now = () => Date.now()) {
  if (value === null || value === undefined) return null;
  const trimmed = String(value).trim();
  if (!trimmed) return null;
  if (/^\d+$/.test(trimmed)) return Number(trimmed) * 1000;
  const at = Date.parse(trimmed);
  if (Number.isNaN(at)) return null;
  return Math.max(0, at - now());
}

/** True when `error` is a `ProblemError`. */
export function isProblemError(error) {
  return error instanceof ProblemError;
}
