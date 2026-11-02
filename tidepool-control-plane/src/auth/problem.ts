/**
 * Turning an auth failure into the shell's readable problem shape.
 *
 * The pool answers failures with RFC 9457 problem details, and the SDK's
 * `presentProblem` renders them. The session controller deliberately does not
 * import the SDK at runtime: it is exercised by headless tests (including a
 * local HTTP pool) and must not pull a browser bundle into that path. This
 * module therefore normalises the few fields an auth form needs and produces
 * the same `PoolProblem` shape, so the existing `ProblemDetails` component
 * renders it unchanged.
 */

import type { PoolProblem } from '../lib/api/problem';

/** An HTTP failure from the pool, carrying the problem-details fields. */
export class PoolRequestError extends Error {
  /** HTTP status. */
  readonly status: number;
  /** Stable machine-readable code, or `http_<status>` when absent. */
  readonly code: string;
  /** The pool's explanation, when it sent one. */
  readonly detail: string | null;
  /** The pool's short title, when it sent one. */
  readonly title: string | null;
  /** Correlation id from the body or the `x-request-id` header. */
  readonly requestId: string | null;
  /** Problem type URI, when the pool sent one. */
  readonly type: string | null;

  constructor(
    status: number,
    fields: {
      code?: string | null;
      detail?: string | null;
      title?: string | null;
      requestId?: string | null;
      type?: string | null;
    } = {},
  ) {
    const title = fields.title ?? null;
    const detail = fields.detail ?? null;
    super(detail ?? title ?? `The pool answered ${status}`);
    this.name = 'PoolRequestError';
    this.status = status;
    this.code = fields.code ?? `http_${status}`;
    this.detail = detail;
    this.title = title;
    this.requestId = fields.requestId ?? null;
    this.type = fields.type ?? null;
  }

  /** True when the pool rejected the credential rather than the request. */
  get unauthenticated(): boolean {
    return this.status === 401;
  }
}

/** Headings for the statuses an auth form can meet. */
const TITLES: Readonly<Record<number, string>> = {
  400: 'The pool refused that request',
  401: 'The pool did not accept those credentials',
  403: 'That is not allowed for your role',
  404: 'The pool has no such endpoint',
  409: 'The pool is in a conflicting state',
  410: 'That invitation is no longer usable',
  422: 'The pool rejected that input',
  429: 'Too many attempts',
  500: 'The pool failed to complete that',
  502: 'The pool is unreachable',
  503: 'The pool is unavailable',
  504: 'The pool did not answer in time',
};

/** Present any thrown value as a `PoolProblem`. */
export function describeAuthFailure(error: unknown): PoolProblem {
  if (error instanceof PoolRequestError) {
    const title = error.title ?? TITLES[error.status] ?? `The pool answered ${error.status}`;
    return {
      kind: 'problem',
      title,
      message: error.detail?.trim() || title,
      requestId: error.requestId,
      status: error.status,
      code: error.code,
      type: error.type,
      retryAfterMs: null,
    };
  }
  if (error instanceof Error) {
    const network = error.name === 'NetworkError';
    return {
      kind: network ? 'network' : 'app',
      title: network ? 'The pool could not be reached' : 'Something went wrong in the control plane',
      message: error.message.trim() === '' ? 'No further detail was given.' : error.message,
      requestId: null,
      status: null,
      code: null,
      type: null,
      retryAfterMs: null,
    };
  }
  return {
    kind: 'app',
    title: 'Something went wrong in the control plane',
    message: 'The failure was not an error object, so there is nothing more to show.',
    requestId: null,
    status: null,
    code: null,
    type: null,
    retryAfterMs: null,
  };
}

/** True when the failure means the stored token is no longer usable. */
export function isCredentialRejection(problem: Pick<PoolProblem, 'status'>): boolean {
  return problem.status === 401 || problem.status === 403;
}
