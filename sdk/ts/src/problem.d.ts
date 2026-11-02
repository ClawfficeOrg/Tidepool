import type { ProblemCode } from "./generated/types.js";

/** Base class for every error this SDK throws. */
export declare class TidepoolError extends Error {
  constructor(message: string, details?: { operationId?: string; cause?: unknown });
  /** Operation id that produced the error, when known. */
  readonly operationId?: string;
  readonly cause?: unknown;
}

/** The transport failed before an HTTP response existed. */
export declare class NetworkError extends TidepoolError {
  constructor(message: string, details?: { operationId?: string; cause?: unknown });
}

/** Fields of a `ProblemError`. */
export interface ProblemDetails {
  status: number;
  code: ProblemCode;
  detail?: string | null;
  title?: string | null;
  type?: string | null;
  requestId?: string | null;
  operationId?: string;
  headers?: Headers | null;
  attempts?: number;
  retryAfterMs?: number | null;
  body?: unknown;
}

/** A non-2xx response, carrying the RFC 9457 problem details. */
export declare class ProblemError extends TidepoolError {
  constructor(message: string, details: ProblemDetails);
  /** HTTP status code. */
  readonly status: number;
  /** Stable machine-readable code; unknown codes stay opaque strings. */
  readonly code: ProblemCode;
  /** Human-readable explanation, when the server sent one. */
  readonly detail: string | null;
  /** Short human-readable summary. */
  readonly title: string | null;
  /** Problem type URI. */
  readonly type: string | null;
  /** Correlation id from the body or the `x-request-id` header. */
  readonly requestId: string | null;
  /** Response headers. */
  readonly headers: Headers | null;
  /** HTTP attempts that produced this error (1 = no retry). */
  readonly attempts: number;
  /** `Retry-After` in milliseconds, when a 429 sent one. */
  readonly retryAfterMs: number | null;
  /** Parsed response body. */
  readonly body: unknown;
  /** True when the failure is a rate limit. */
  readonly rateLimited: boolean;
  toJSON(): {
    name: string;
    message: string;
    status: number;
    code: ProblemCode;
    detail: string | null;
    requestId: string | null;
    operationId?: string;
    attempts: number;
    retryAfterMs: number | null;
  };
}

/** Normalise any non-2xx payload into problem-details fields. */
export declare function normaliseProblem(
  body: unknown,
  meta: { status: number; headers?: Headers | null },
): ProblemDetails;

/** Parse a `Retry-After` header (seconds or HTTP-date) into milliseconds. */
export declare function retryAfterMs(value: string | null | undefined, now?: () => number): number | null;

/** True when `error` is a `ProblemError`. */
export declare function isProblemError(error: unknown): error is ProblemError;
