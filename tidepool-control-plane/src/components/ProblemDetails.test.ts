import { describe, expect, it } from 'vitest';
import ProblemDetails from './ProblemDetails.svelte';
import { presentProblem } from '$lib/api/problem';
import { NetworkError, ProblemError } from '@clawffice/tidepool-sdk';
import { renderComponent } from '$lib/testing/render';

describe('ProblemDetails', () => {
  it('renders a pool failure as a readable message with its request id', () => {
    const problem = presentProblem(
      new ProblemError('request failed', {
        status: 503,
        code: 'database_unavailable',
        title: 'Database unavailable',
        detail: 'The pool has no database connection.',
        requestId: 'req_01HSAMPLE',
      }),
    );
    const { html, text } = renderComponent(ProblemDetails, { problem });
    expect(text).toContain('Database unavailable');
    expect(text).toContain('The pool has no database connection.');
    expect(text).toContain('Request id req_01HSAMPLE');
    expect(text).toContain('req_01HSAMPLE');
    expect(html).toContain('role="alert"');
    expect(html).toContain('data-problem-kind="problem"');
    expect(html).toContain('data-status="danger"');
    expect(text).toContain('Failed');
  });

  it('says so plainly when the pool returned no request id', () => {
    const problem = presentProblem(new NetworkError('fetch failed'));
    const { text } = renderComponent(ProblemDetails, { problem });
    expect(text).toContain('The pool could not be reached');
    expect(text).toContain('No request id: the request never reached the pool.');
    expect(text).toContain('Request id');
    expect(text).toContain('none');
  });

  it('keeps the machine-readable details available for a bug report', () => {
    const problem = presentProblem(
      new ProblemError('request failed', {
        status: 422,
        code: 'invalid_trigger',
        title: 'Invalid trigger',
        detail: 'The trigger has no agent.',
        requestId: 'req_02',
      }),
    );
    const { text } = renderComponent(ProblemDetails, { problem });
    expect(text).toContain('422');
    expect(text).toContain('invalid_trigger');
  });

  it('offers a retry only when one is wired', () => {
    const problem = presentProblem(new NetworkError('fetch failed'));
    const lonely = renderComponent(ProblemDetails, { problem });
    expect(lonely.text).not.toContain('Try again');
    const retryable = renderComponent(ProblemDetails, { problem, onRetry: () => undefined });
    expect(retryable.text).toContain('Try again');
  });
});
