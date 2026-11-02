import { describe, expect, it } from 'vitest';
import { NetworkError, ProblemError } from '@clawffice/tidepool-sdk';
import { isRetryable, presentProblem, requestIdLine } from './problem';

/** A problem details failure, as the SDK throws it. */
function problemError(
  overrides: {
    status?: number;
    code?: string;
    title?: string | null;
    detail?: string | null;
    requestId?: string | null;
  } = {},
): ProblemError {
  return new ProblemError('request failed', {
    status: overrides.status ?? 503,
    code: overrides.code ?? 'pool_unavailable',
    title: overrides.title ?? null,
    detail: overrides.detail ?? null,
    requestId: overrides.requestId ?? null,
    type: 'https://tidepool.example/problems/pool-unavailable',
  });
}

describe('presentProblem', () => {
  it('renders problem details as a readable title, sentence and request id', () => {
    const problem = presentProblem(
      problemError({ detail: 'No database connection is available.', requestId: 'req_01HXYZ' }),
    );
    expect(problem).toMatchObject({
      kind: 'problem',
      title: 'The pool is unavailable',
      message: 'No database connection is available.',
      requestId: 'req_01HXYZ',
      status: 503,
      code: 'pool_unavailable',
    });
  });

  it('prefers the pool title and does not repeat it as the sentence', () => {
    const problem = presentProblem(problemError({ title: 'Maintenance window', detail: 'Retry at 02:00.' }));
    expect(problem.title).toBe('Maintenance window');
    expect(problem.message).toBe('Retry at 02:00.');
    const repeated = presentProblem(problemError({ title: 'Maintenance window', detail: 'Maintenance window' }));
    expect(repeated.message).toBe('Maintenance window');
  });

  it('falls back to a sentence for the status when the pool sent no detail', () => {
    expect(presentProblem(problemError({ status: 401 })).title).toBe('The pool needs you to sign in');
    expect(presentProblem(problemError({ status: 429 })).title).toBe('Too many requests');
    expect(presentProblem(problemError({ status: 418 })).title).toBe('The pool answered 418');
  });

  it('keeps the request id line explicit, including when there is none', () => {
    expect(requestIdLine({ kind: 'problem', requestId: 'req_01' })).toBe('Request id req_01');
    expect(requestIdLine({ kind: 'problem', requestId: null })).toContain('No request id');
  });

  it('names a transport failure as an unreachable pool with no request id', () => {
    const problem = presentProblem(new NetworkError('fetch failed: ECONNREFUSED'));
    expect(problem.kind).toBe('network');
    expect(problem.title).toBe('The pool could not be reached');
    expect(problem.message).toBe('fetch failed: ECONNREFUSED');
    expect(problem.requestId).toBeNull();
    expect(requestIdLine(problem)).toBe('No request id: the request never reached the pool.');
  });

  it('handles an ordinary error without leaking a stack trace', () => {
    const problem = presentProblem(new Error('Unexpected token < in JSON'));
    expect(problem.kind).toBe('app');
    expect(problem.message).toBe('Unexpected token < in JSON');
    expect(problem.message).not.toContain('at ');
  });

  it('handles a value that is not an error at all', () => {
    const problem = presentProblem('boom');
    expect(problem.kind).toBe('app');
    expect(problem.message.length).toBeGreaterThan(0);
    expect(problem.requestId).toBeNull();
  });

  it('knows which failures are worth retrying', () => {
    expect(isRetryable({ status: 503 })).toBe(true);
    expect(isRetryable({ status: 429 })).toBe(true);
    expect(isRetryable({ status: null })).toBe(true);
    expect(isRetryable({ status: 404 })).toBe(false);
    expect(isRetryable({ status: 422 })).toBe(false);
  });
});
