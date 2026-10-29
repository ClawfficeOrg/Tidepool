/**
 * The activity page across the SDK boundary, without a server.
 *
 * The real TypeScript SDK talks to an in-memory pool through an injected
 * `fetch`, so this test exercises the URLs, query string, auth header and JSON
 * shapes the browser uses. It walks the task's end-to-end case: a manual run is
 * dispatched, its detail is read while it is pending and running, and the page
 * follows it to `succeeded` on the same timer a browser would use.
 */

import { describe, expect, it } from 'vitest';
import { createClient } from '@clawffice/tidepool-sdk';
import type {
  ActivityEntry,
  ActivityPage,
  ActivityRunDetail,
  ManualDispatchRequest,
  ProjectView,
  RunStepView,
  RunView,
} from '@clawffice/tidepool-sdk';
import {
  RunsController,
  emptyRunsState,
  runsTransport,
  type RunsScheduler,
} from '$lib/runs';

const BASE_URL = 'https://pool.example.com';
const TOKEN = 'token-1';

const PROJECT: ProjectView = {
  id: 'project-a',
  slug: 'nightly',
  name: 'Nightly',
  created_at: '2026-10-28T20:00:00.000Z',
  updated_at: '2026-10-28T20:00:00.000Z',
};

/** One step in the state the fake pool is currently reporting. */
function step(status: RunStepView['status'], output: string | null): RunStepView {
  return {
    id: 'step-1',
    step: 'triage',
    position: 0,
    status,
    condition: null,
    executionId: 'exec-1',
    environment: 'default',
    host: 'host-1',
    directory: '/srv/tidepool',
    branch: null,
    output,
    error: null,
    reason: null,
    startedAt: status === 'pending' ? null : '2026-10-29T05:00:01.000Z',
    completedAt: status === 'succeeded' ? '2026-10-29T05:00:05.000Z' : null,
  };
}

/** The run at one point in its life. */
function run(status: RunView['status'], stepStatus: RunStepView['status'], output: string | null): RunView {
  return {
    id: 'run-live',
    project: PROJECT.id,
    trigger: 'nightly',
    triggerName: 'nightly',
    workflow: 'nightly',
    origin: 'manual',
    actor: 'operator-1',
    deliveryKey: 'e2e-manual-1',
    status,
    requestText: 'run the nightly build',
    inputs: {},
    deadlineAt: null,
    startedAt: status === 'pending' ? null : '2026-10-29T05:00:01.000Z',
    completedAt: status === 'succeeded' ? '2026-10-29T05:00:05.000Z' : null,
    createdAt: '2026-10-29T05:00:00.000Z',
    steps: [step(stepStatus, output)],
  };
}

/** One run plus its arrival record. */
function detail(runView: RunView): ActivityRunDetail {
  return {
    run: runView,
    arrival: {
      mode: 'created',
      provider: null,
      conversationKey: null,
      steered: false,
      receipt: null,
      outputLimits: [],
    },
  };
}

/** One page entry for the run. */
function entry(status: string): ActivityEntry {
  return {
    id: 'run-live',
    kind: 'run',
    project: PROJECT.id,
    trigger: 'nightly',
    triggerName: 'nightly',
    run: 'run-live',
    status,
    at: '2026-10-29T05:00:00.000Z',
  };
}

/** What the fake pool remembers. */
interface FakePool {
  calls: { method: string; path: string; query: URLSearchParams }[];
  dispatchBodies: ManualDispatchRequest[];
  detailCalls: number;
  lifecycle: RunView[];
}

/** A fresh fake pool. */
function fakePool(): FakePool {
  return {
    calls: [],
    dispatchBodies: [],
    detailCalls: 0,
    lifecycle: [
      run('pending', 'pending', null),
      run('running', 'running', null),
      run('succeeded', 'succeeded', 'the nightly build is green'),
    ],
  };
}

/** A JSON response. */
function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

/** A problem-details response. */
function problem(status: number, code: string, detail: string): Response {
  return new Response(
    JSON.stringify({ status, code, title: detail, detail, requestId: 'req_e2e_runs' }),
    {
      status,
      headers: {
        'content-type': 'application/problem+json',
        'x-request-id': 'req_e2e_runs',
      },
    },
  );
}

/** Parse a request body as an object. */
function bodyOf(init: RequestInit | undefined): Record<string, unknown> {
  const raw = init?.body;
  if (typeof raw !== 'string' || raw.trim() === '') return {};
  const parsed: unknown = JSON.parse(raw);
  return parsed !== null && typeof parsed === 'object' && !Array.isArray(parsed)
    ? (parsed as Record<string, unknown>)
    : {};
}

/** The fake pool's `fetch`, mirroring the contract's statuses and shapes. */
function fakeFetch(pool: FakePool): typeof fetch {
  return async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const url =
      typeof input === 'string'
        ? new URL(input)
        : input instanceof URL
          ? input
          : new URL(input.url);
    const method = (init?.method ?? 'GET').toUpperCase();
    const path = url.pathname;
    pool.calls.push({ method, path, query: url.searchParams });

    if (new Headers(init?.headers).get('authorization') !== `Bearer ${TOKEN}`) {
      return problem(401, 'unauthorized', 'invalid or missing token');
    }

    if (method === 'GET' && path === '/v1/projects') {
      return json({ projects: [PROJECT] });
    }

    if (method === 'GET' && path === '/v1/activity') {
      const status = url.searchParams.get('status') ?? 'pending';
      const page: ActivityPage = {
        items: [entry(status)],
        hasMore: false,
        nextCursor: null,
      };
      return json(page);
    }

    const detailRoute = /^\/v1\/activity\/runs\/([^/]+)$/.exec(path);
    if (method === 'GET' && detailRoute) {
      const index = Math.min(pool.detailCalls, pool.lifecycle.length - 1);
      const runView = pool.lifecycle[index] ?? pool.lifecycle[0];
      pool.detailCalls += 1;
      return json(detail(runView as RunView));
    }

    if (method === 'POST' && path === '/v1/runs') {
      const body = bodyOf(init) as unknown as ManualDispatchRequest;
      pool.dispatchBodies.push(body);
      return json(
        { deduplicated: false, run: pool.lifecycle[0] },
        201,
      );
    }

    return problem(404, 'not_found', `no route for ${method} ${path}`);
  };
}

/** A deterministic scheduler that records what would have fired. */
function testScheduler(): RunsScheduler & {
  pending: { callback: () => void | Promise<unknown>; delayMs: number }[];
  fireNext: () => Promise<void>;
} {
  const pending: { callback: () => void | Promise<unknown>; delayMs: number }[] = [];
  return {
    pending,
    schedule(callback, delayMs) {
      const item = { callback, delayMs };
      pending.push(item);
      return item;
    },
    cancel(candidate) {
      const index = pending.findIndex((item) => item === candidate);
      if (index >= 0) pending.splice(index, 1);
    },
    async fireNext() {
      const next = pending.shift();
      if (!next) throw new Error('nothing was scheduled');
      await next.callback();
    },
  };
}

/** Build the page controller over the real SDK and the fake pool. */
function controllerFor(pool: FakePool, scheduler: RunsScheduler): RunsController {
  const client = createClient({
    baseUrl: BASE_URL,
    token: TOKEN,
    fetch: fakeFetch(pool),
  });
  return new RunsController({
    transport: runsTransport(client),
    scheduler,
    initial: emptyRunsState(),
    now: () => Date.parse('2026-10-29T06:00:00.000Z'),
  });
}

describe('a manual run followed to the end', () => {
  it('dispatches it through the SDK and follows pending, running and succeeded', async () => {
    const pool = fakePool();
    const scheduler = testScheduler();
    const controller = controllerFor(pool, scheduler);

    const loaded = await controller.load();
    expect(loaded.phase).toBe('ready');
    expect(loaded.projects).toEqual([PROJECT]);

    const dispatched = await controller.dispatchManual({
      project: PROJECT.id,
      trigger: 'nightly',
      actor: 'operator-1',
      input: 'run the nightly build',
      deliveryKey: 'e2e-manual-1',
    });

    expect(dispatched.dispatchPhase).toBe('dispatched');
    expect(dispatched.lastDispatch).toEqual({ runId: 'run-live', deduplicated: false });
    expect(pool.dispatchBodies).toHaveLength(1);
    expect(pool.dispatchBodies[0]).toEqual({
      project: 'project-a',
      trigger: 'nightly',
      actor: 'operator-1',
      deliveryKey: 'e2e-manual-1',
      input: 'run the nightly build',
    });
    expect(pool.calls.map((call) => `${call.method} ${call.path}`)).toContain('POST /v1/runs');
    expect(dispatched.detail?.run.status).toBe('pending');
    expect(dispatched.live).toBe(true);
    expect(scheduler.pending).toHaveLength(1);

    await scheduler.fireNext();
    expect(controller.snapshot().detail?.run.status).toBe('running');
    expect(controller.snapshot().live).toBe(true);

    await scheduler.fireNext();
    const finished = controller.snapshot();
    expect(finished.detail?.run.status).toBe('succeeded');
    expect(finished.detail?.run.steps[0]?.output).toBe('the nightly build is green');
    expect(finished.live).toBe(false);
    expect(finished.detailUpdatedAt).toBe('2026-10-29T06:00:00.000Z');
    expect(scheduler.pending).toHaveLength(0);
    expect(pool.detailCalls).toBe(3);
  });

  it('carries the project filter in the request URL and keeps the feed scoped', async () => {
    const pool = fakePool();
    const scheduler = testScheduler();
    const controller = controllerFor(pool, scheduler);

    await controller.load();
    const filtered = await controller.setFilter('project', PROJECT.id);
    const filteredCall = pool.calls
      .filter((call) => call.method === 'GET' && call.path === '/v1/activity')
      .at(-1);

    expect(filteredCall?.query.get('project')).toBe('project-a');
    expect(filteredCall?.query.get('limit')).toBe('25');
    expect(filtered.items.every((item) => item.project === PROJECT.id)).toBe(true);
  });
});
