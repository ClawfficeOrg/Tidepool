/**
 * Problem details, presented so a person can read them.
 *
 * The pool answers failures with RFC 9457 problem details (`docs/HUB_API_ERRORS.md`),
 * and the SDK throws them as `ProblemError` with the correlation id already
 * lifted out of the body or the `x-request-id` header. This module turns any
 * thrown value into one plain sentence plus the request id, so the shell's
 * error boundary never shows a stack trace or a bare status code, and a person
 * can quote the id to whoever runs the pool.
 */

import { isProblemError } from '@clawffice/tidepool-sdk';

/** A failure, in the shape the UI renders. */
export interface PoolProblem {
  /** How the failure arrived. */
  kind: 'problem' | 'network' | 'app';
  /** Short heading: what went wrong. */
  title: string;
  /** One readable sentence. Never a stack trace. */
  message: string;
  /** Correlation id from the pool, or null when the failure never reached it. */
  requestId: string | null;
  /** HTTP status, when there was a response. */
  status: number | null;
  /** Stable machine-readable code, when the pool sent one. */
  code: string | null;
  /** Problem type URI, when the pool sent one. */
  type: string | null;
  /** Milliseconds until a retry is allowed, for a rate limit. */
  retryAfterMs: number | null;
}

const TITLES: Readonly<Record<number, string>> = {
  400: 'The pool refused that request',
  401: 'The pool needs you to sign in',
  403: 'That is not allowed for your role',
  404: 'The pool has no such resource',
  409: 'The pool is in a conflicting state',
  422: 'The pool rejected that input',
  429: 'Too many requests',
  500: 'The pool failed to complete that',
  502: 'The pool is unreachable',
  503: 'The pool is unavailable',
  504: 'The pool did not answer in time',
};

/** The heading for a status code. */
function titleForStatus(status: number): string {
  return TITLES[status] ?? `The pool answered ${status}`;
}

/** Present any thrown value. */
export function presentProblem(error: unknown): PoolProblem {
  if (isProblemError(error)) {
    const status = error.status;
    const title = error.title ?? titleForStatus(status);
    const detail = error.detail?.trim();
    return {
      kind: 'problem',
      title,
      message: detail && detail !== title ? detail : title,
      requestId: error.requestId,
      status,
      code: typeof error.code === 'string' ? error.code : null,
      type: error.type,
      retryAfterMs: error.retryAfterMs,
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

/** The line to show for a failure's request id. */
export function requestIdLine(problem: Pick<PoolProblem, 'kind' | 'requestId'>): string {
  if (problem.requestId) return `Request id ${problem.requestId}`;
  if (problem.kind === 'network') return 'No request id: the request never reached the pool.';
  return 'No request id was returned with this failure.';
}

/** True when the failure is worth retrying as-is. */
export function isRetryable(problem: Pick<PoolProblem, 'status'>): boolean {
  return problem.status === null || problem.status === 429 || problem.status >= 500;
}
