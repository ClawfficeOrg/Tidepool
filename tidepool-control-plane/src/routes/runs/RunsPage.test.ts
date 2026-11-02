/**
 * The activity filters, the failure sentences, the live detail and the step
 * output, asserted without a browser.
 *
 * Component tests render through `svelte/server`; the controller tests inject a
 * transport and a scheduler, so "live" means the same timer callback the
 * browser would run, not a real timer. The end-to-end file beside this one walks
 * a manual dispatch through the real SDK against a fake pool.
 */

import { describe, expect, it } from 'vitest';
import RunDetail from './RunDetail.svelte';
import RunsPage from './RunsPage.svelte';
import { renderComponent } from '$lib/testing/render';
import type {
  ActivityEntry,
  ActivityRunDetail,
  ProjectView,
  RunStepView,
  RunView,
} from '@clawffice/tidepool-sdk';
import {
  RunsController,
  activityQuery,
  arrivalSentence,
  copyOutput,
  dispatchSentence,
  emptyActivityFilters,
  emptyRunsState,
  failureExplanation,
  filterActivityEntries,
  manualDeliveryKey,
  manualRunIssues,
  runFailureExplanation,
  runIsLive,
  stepOutputText,
  type ActivityFilters,
  type RunsScheduler,
  type RunsTransport,
  type RunsViewState,
} from '$lib/runs';

const POOL = { baseUrl: 'https://pool.example.com' };

const PROJECT_A: ProjectView = {
  id: 'project-a',
  slug: 'nightly',
  name: 'Nightly',
  created_at: '2026-10-28T20:00:00.000Z',
  updated_at: '2026-10-28T20:00:00.000Z',
};

const PROJECT_B: ProjectView = {
  id: 'project-b',
  slug: 'other',
  name: 'Other project',
  created_at: '2026-10-28T20:00:00.000Z',
  updated_at: '2026-10-28T20:00:00.000Z',
};

/** One step, succeeded unless overridden. */
function step(overrides: Partial<RunStepView> = {}): RunStepView {
  return {
    id: 'step-1',
    step: 'triage',
    position: 0,
    status: 'succeeded',
    condition: null,
    executionId: 'exec-1',
    environment: 'default',
    host: 'host-1',
    directory: '/srv/tidepool',
    branch: null,
    output: 'All good.',
    error: null,
    reason: null,
    startedAt: '2026-10-29T05:00:01.000Z',
    completedAt: '2026-10-29T05:00:05.000Z',
    ...overrides,
  };
}

/** One run, succeeded unless overridden. */
function run(overrides: Partial<RunView> = {}): RunView {
  return {
    id: 'run-a',
    project: PROJECT_A.id,
    trigger: 'trigger-a',
    triggerName: 'auto-fix',
    workflow: 'triage',
    origin: 'event',
    actor: 'alice',
    deliveryKey: 'delivery-a',
    status: 'succeeded',
    requestText: 'Fix the failing test',
    inputs: {},
    deadlineAt: null,
    startedAt: '2026-10-29T05:00:01.000Z',
    completedAt: '2026-10-29T05:00:05.000Z',
    createdAt: '2026-10-29T05:00:00.000Z',
    steps: [step()],
    ...overrides,
  };
}

/** One run plus its arrival record. */
function detail(
  runView: RunView,
  arrival: Partial<ActivityRunDetail['arrival']> = {},
): ActivityRunDetail {
  return {
    run: runView,
    arrival: {
      mode: 'created',
      provider: 'github',
      conversationKey: null,
      steered: false,
      receipt: 'receipt-a',
      outputLimits: [],
      ...arrival,
    },
  };
}

/** One activity entry. */
function entry(overrides: Partial<ActivityEntry>): ActivityEntry {
  return {
    id: 'entry-1',
    kind: 'run',
    status: 'succeeded',
    at: '2026-10-29T05:00:05.000Z',
    ...overrides,
  };
}

const RUN_A = entry({
  id: 'run-a-entry',
  kind: 'run',
  project: PROJECT_A.id,
  provider: 'github',
  trigger: 'trigger-a',
  triggerName: 'auto-fix',
  run: 'run-a',
  status: 'succeeded',
});

const RECEIPT_A = entry({
  id: 'receipt-a',
  kind: 'receipt',
  project: PROJECT_A.id,
  provider: 'github',
  status: 'matched',
  eventType: 'issue.comment_created',
  actor: 'alice',
  matched: true,
  duplicates: 1,
});

const STEP_A = entry({
  id: 'step-a',
  kind: 'step',
  project: PROJECT_A.id,
  provider: 'github',
  trigger: 'trigger-a',
  run: 'run-a',
  step: 'triage',
  status: 'succeeded',
});

const RUN_B = entry({
  id: 'run-b-entry',
  kind: 'run',
  project: PROJECT_B.id,
  provider: 'slack',
  trigger: 'trigger-b',
  triggerName: 'other-workflow',
  run: 'run-b',
  status: 'failed',
});

const ALL_ENTRIES = [RUN_A, RECEIPT_A, STEP_A, RUN_B];

/** A transport a server-rendered test must not call. */
const UNUSED_TRANSPORT: RunsTransport = {
  listProjects: () => Promise.reject(new Error('the page renders from its initial state')),
  list: () => Promise.reject(new Error('not called')),
  run: () => Promise.reject(new Error('not called')),
  dispatch: () => Promise.reject(new Error('not called')),
};

/** A deterministic scheduler that records what would have fired. */
function testScheduler(): RunsScheduler & {
  pending: { callback: () => void | Promise<unknown>; delayMs: number }[];
  fireNext: () => Promise<void>;
} {
  const pending: { callback: () => void | Promise<unknown>; delayMs: number }[] = [];
  const handle = {
    pending,
    schedule(callback: () => void | Promise<unknown>, delayMs: number): unknown {
      const item = { callback, delayMs };
      pending.push(item);
      return item;
    },
    cancel(candidate: unknown): void {
      const index = pending.findIndex((item) => item === candidate);
      if (index >= 0) pending.splice(index, 1);
    },
    async fireNext(): Promise<void> {
      const next = pending.shift();
      if (!next) throw new Error('nothing was scheduled');
      await next.callback();
    },
  };
  return handle;
}

/** A state with the ready feed and detail, for rendering. */
function state(overrides: Partial<RunsViewState> = {}): RunsViewState {
  return {
    ...emptyRunsState(),
    phase: 'ready',
    projects: [PROJECT_A, PROJECT_B],
    items: ALL_ENTRIES,
    ...overrides,
  };
}

/** Render the page from a supplied state. */
function renderPage(overrides: Partial<RunsViewState> = {}) {
  return renderComponent(RunsPage, {
    pool: POOL,
    token: 'token-1',
    role: 'member',
    transport: UNUSED_TRANSPORT,
    initial: state(overrides),
  });
}

/** A controller over a fake pool. */
function controllerFor(
  transport: RunsTransport,
  options: { scheduler?: RunsScheduler; initial?: Partial<RunsViewState> } = {},
): RunsController {
  return new RunsController({
    transport,
    scheduler: options.scheduler ?? testScheduler(),
    initial: { ...emptyRunsState(), ...options.initial },
    now: () => Date.parse('2026-10-29T06:00:00.000Z'),
    pollIntervalMs: 1_500,
  });
}

describe('activity filters combine and stay inside one project', () => {
  const filters: ActivityFilters = {
    project: PROJECT_A.id,
    provider: 'github',
    trigger: 'auto-fix',
    status: 'succeeded',
    kind: 'run',
  };

  it('keeps only the entry every chosen filter accepts', () => {
    expect(filterActivityEntries(ALL_ENTRIES, filters).map((item) => item.id)).toEqual([
      'run-a-entry',
    ]);
  });

  it('never lets another project through, even when another filter is empty', () => {
    const projectOnly: ActivityFilters = { ...emptyActivityFilters(), project: PROJECT_A.id };
    const ids = filterActivityEntries(ALL_ENTRIES, projectOnly).map((item) => item.id);
    expect(ids).toEqual(['run-a-entry', 'receipt-a', 'step-a']);
    expect(ids).not.toContain('run-b-entry');

    const impossible: ActivityFilters = {
      ...emptyActivityFilters(),
      project: PROJECT_A.id,
      provider: 'slack',
    };
    expect(filterActivityEntries(ALL_ENTRIES, impossible)).toEqual([]);
  });

  it('treats a missing project or provider as no match, not as a wildcard', () => {
    const unowned = entry({ id: 'unowned', kind: 'run', status: 'succeeded' });
    expect(filterActivityEntries([unowned], filters)).toEqual([]);
    expect(
      filterActivityEntries([entry({ id: 'slack-receipt', provider: 'slack' })], {
        ...emptyActivityFilters(),
        provider: 'github',
      }),
    ).toEqual([]);
  });

  it('builds the pool query from only the non-empty filters', () => {
    expect(activityQuery(filters, { limit: 25 })).toEqual({
      project: PROJECT_A.id,
      provider: 'github',
      trigger: 'auto-fix',
      status: 'succeeded',
      kind: 'run',
      limit: 25,
    });
    expect(activityQuery(emptyActivityFilters())).toEqual({});
    expect(activityQuery(emptyActivityFilters(), { cursor: 'c2' })).toEqual({ cursor: 'c2' });
  });

  it('renders only the chosen project, even from a mixed supplied state', () => {
    const rendered = renderPage({
      filters: { ...emptyActivityFilters(), project: PROJECT_A.id },
      items: ALL_ENTRIES,
    });

    expect(rendered.html).toContain('data-activity-project="project-a"');
    expect(rendered.html).not.toContain('data-activity-project="project-b"');
    expect(rendered.html).not.toContain('run-b');
    expect(rendered.text).toContain('Nightly');
  });

  it('sends the combined filter to the pool and drops a project the pool mixes in', async () => {
    const calls: unknown[] = [];
    const transport: RunsTransport = {
      listProjects: () => Promise.resolve({ projects: [PROJECT_A, PROJECT_B] }),
      list: (query) => {
        calls.push(query);
        return Promise.resolve({ items: ALL_ENTRIES, hasMore: false, nextCursor: null });
      },
      run: () => Promise.reject(new Error('not called')),
      dispatch: () => Promise.reject(new Error('not called')),
    };
    const controller = controllerFor(transport);

    await controller.load();
    const next = await controller.setFilter('project', PROJECT_A.id);

    expect(calls[1]).toMatchObject({ project: PROJECT_A.id, limit: 25 });
    expect(next.items.map((item) => item.project)).toEqual([
      PROJECT_A.id,
      PROJECT_A.id,
      PROJECT_A.id,
    ]);
  });
});

describe('a named failure code is a sentence with the code kept', () => {
  it('says the host went offline when the code is daemon_not_connected', () => {
    const failure = failureExplanation(
      "daemon_not_connected: host 'swaygent' is offline",
      null,
    );
    expect(failure).not.toBeNull();
    expect(failure?.code).toBe('daemon_not_connected');
    expect(failure?.sentence).toContain('The host went offline');
    expect(failure?.detail).toBe("host 'swaygent' is offline");
  });

  it('recognises a bare code with no detail', () => {
    const failure = failureExplanation('daemon_not_connected', null);
    expect(failure?.code).toBe('daemon_not_connected');
    expect(failure?.sentence).toContain('The host went offline');
    expect(failure?.detail).toBeNull();
  });

  it('explains a step reason when the error carries no code', () => {
    const failure = failureExplanation(null, 'execution_failed');
    expect(failure?.code).toBe('execution_failed');
    expect(failure?.sentence).toContain('The agent on the host reported');
  });

  it('keeps an unknown code instead of hiding it', () => {
    const failure = failureExplanation('mystery_code: the pool said no', null);
    expect(failure?.code).toBe('mystery_code');
    expect(failure?.sentence).toContain("mystery_code");
    expect(failure?.sentence).toContain('the pool said no');
  });

  it('finds the first failing step for the run-level alert', () => {
    const failed = run({
      status: 'failed',
      steps: [
        step(),
        step({
          id: 'step-2',
          step: 'apply',
          position: 1,
          status: 'failed',
          error: "daemon_not_connected: host 'swaygent' is offline",
        }),
      ],
    });
    expect(runFailureExplanation(failed)?.code).toBe('daemon_not_connected');
  });

  it('renders the sentence before the code in the run detail', () => {
    const failed = run({
      status: 'failed',
      steps: [
        step({
          status: 'failed',
          error: "daemon_not_connected: host 'swaygent' is offline",
          reason: 'daemon_not_connected',
        }),
      ],
    });
    const rendered = renderComponent(RunDetail, {
      phase: 'ready',
      detail: detail(failed, { mode: 'continued', steered: true }),
      problem: null,
      live: false,
      updatedAt: '2026-10-29T06:00:00.000Z',
      projects: [PROJECT_A],
      onRetry: () => {},
      onClose: () => {},
    });

    expect(rendered.text).toContain('The host went offline');
    expect(rendered.html).toContain('data-failure-code="daemon_not_connected"');
    expect(rendered.text).toContain('daemon_not_connected');
    expect(rendered.text).toContain('continued the agent already serving');
  });
});

describe('a step output is readable and copied exactly', () => {
  const output = 'first line\n  <tag> & more\n\nlast line';
  const analysed = step({ id: 'step-analyse', step: 'analyse', output });

  it('keeps every character of the agent’s text', () => {
    expect(stepOutputText(analysed)).toBe(output);
  });

  it('renders it in a pre block with the markup escaped and the newlines kept', () => {
    const rendered = renderComponent(RunDetail, {
      phase: 'ready',
      detail: detail(run({ steps: [analysed] })),
      problem: null,
      live: false,
      updatedAt: null,
      projects: [PROJECT_A],
      onRetry: () => {},
      onClose: () => {},
    });

    expect(rendered.html).toContain('data-step-output="analyse"');
    expect(rendered.html).toContain('first line\n  &lt;tag> &amp; more\n\nlast line');
    expect(rendered.html).not.toContain('<tag>');
    expect(rendered.html).toContain('data-copy-step="step-analyse"');
  });

  it('writes the exact text to the clipboard', async () => {
    const written: string[] = [];
    const ok = await copyOutput(output, (text) => {
      written.push(text);
    });
    expect(ok).toBe(true);
    expect(written).toEqual([output]);
  });

  it('reports a refused copy without failing the page', async () => {
    const ok = await copyOutput(output, () => Promise.reject(new Error('denied')));
    expect(ok).toBe(false);
  });

  it('says a step has not written output yet', () => {
    const rendered = renderComponent(RunDetail, {
      phase: 'ready',
      detail: detail(run({ steps: [step({ output: null })] })),
      problem: null,
      live: false,
      updatedAt: null,
      projects: [PROJECT_A],
      onRetry: () => {},
      onClose: () => {},
    });
    expect(rendered.text).toContain('has not written any output yet');
    expect(rendered.html).toContain('data-step-output-empty="true"');
    expect(rendered.html).toMatch(/data-copy-step="[^"]+"[^>]*disabled/);
  });

  it('offers the copy control only when there is output to copy', () => {
    const rendered = renderComponent(RunDetail, {
      phase: 'ready',
      detail: detail(run({ steps: [analysed] })),
      problem: null,
      live: false,
      updatedAt: null,
      projects: [PROJECT_A],
      onRetry: () => {},
      onClose: () => {},
    });
    expect(rendered.html).toMatch(/data-copy-step="step-analyse"[^>]*>/);
    expect(rendered.html).not.toMatch(/data-copy-step="step-analyse"[^>]*disabled/);
  });
});

describe('a running execution updates live', () => {
  it('reads the detail again on the timer and stops when the run finishes', async () => {
    const passed = run({ id: 'run-live', status: 'pending', steps: [step({ status: 'pending' })] });
    const processing = run({
      id: 'run-live',
      status: 'running',
      steps: [step({ status: 'running', output: null })],
    });
    const finished = run({ id: 'run-live', status: 'succeeded' });

    const details = [passed, processing, finished];
    const calls: string[] = [];
    const transport: RunsTransport = {
      listProjects: () => Promise.resolve({ projects: [PROJECT_A] }),
      list: () => Promise.resolve({ items: [], hasMore: false, nextCursor: null }),
      run: (runId) => {
        calls.push(runId);
        const next = details[Math.min(calls.length - 1, details.length - 1)] ?? finished;
        return Promise.resolve(detail(next));
      },
      dispatch: () => Promise.reject(new Error('not called')),
    };
    const scheduler = testScheduler();
    const controller = controllerFor(transport, {
      scheduler,
      initial: { phase: 'ready', projects: [PROJECT_A] },
    });

    const opened = await controller.selectRun('run-live');
    expect(opened.live).toBe(true);
    expect(opened.detail?.run.status).toBe('pending');
    expect(scheduler.pending).toHaveLength(1);
    expect(scheduler.pending[0]?.delayMs).toBe(1_500);

    await scheduler.fireNext();
    expect(controller.snapshot().detail?.run.status).toBe('running');
    expect(controller.snapshot().live).toBe(true);
    expect(scheduler.pending).toHaveLength(1);

    await scheduler.fireNext();
    expect(controller.snapshot().detail?.run.status).toBe('succeeded');
    expect(controller.snapshot().live).toBe(false);
    expect(scheduler.pending).toHaveLength(0);
    expect(calls).toEqual(['run-live', 'run-live', 'run-live']);
  });

  it('does not schedule a timer for a run that already finished', async () => {
    const transport: RunsTransport = {
      listProjects: () => Promise.resolve({ projects: [PROJECT_A] }),
      list: () => Promise.resolve({ items: [], hasMore: false, nextCursor: null }),
      run: () => Promise.resolve(detail(run())),
      dispatch: () => Promise.reject(new Error('not called')),
    };
    const scheduler = testScheduler();
    const controller = controllerFor(transport, { scheduler });

    await controller.selectRun('run-a');

    expect(controller.snapshot().live).toBe(false);
    expect(scheduler.pending).toHaveLength(0);
  });

  it('treats a run with a running step as live even when the run word is ahead', () => {
    const lagging = run({ status: 'succeeded', steps: [step({ status: 'running' })] });
    expect(runIsLive(lagging)).toBe(true);
  });

  it('shows the live state on the page', () => {
    const rendered = renderPage({
      selectedRunId: 'run-live',
      detailPhase: 'ready',
      live: true,
      detailUpdatedAt: '2026-10-29T06:00:00.000Z',
      detail: detail(run({ id: 'run-live', status: 'running' })),
    });
    expect(rendered.html).toContain('data-run-live="true"');
    expect(rendered.text).toContain('Following this run live');
  });
});

describe('the arrival says created, continued or restored', () => {
  it('maps each mode to its own sentence', () => {
    expect(arrivalSentence(detail(run()).arrival)).toContain('created a new agent');
    expect(
      arrivalSentence(detail(run(), { mode: 'continued' }).arrival),
    ).toContain('continued the agent already serving');
    expect(
      arrivalSentence(detail(run(), { mode: 'continued', steered: true }).arrival),
    ).toContain('steered it while it was busy');
    expect(
      arrivalSentence(detail(run(), { mode: 'restored' }).arrival),
    ).toContain('restored it with a new agent');
  });
});

describe('manual dispatch input', () => {
  it('refuses an incomplete request before any pool call', async () => {
    let dispatched = false;
    const transport: RunsTransport = {
      listProjects: () => Promise.resolve({ projects: [PROJECT_A] }),
      list: () => Promise.resolve({ items: [], hasMore: false, nextCursor: null }),
      run: () => Promise.reject(new Error('not called')),
      dispatch: () => {
        dispatched = true;
        return Promise.reject(new Error('not called'));
      },
    };
    const controller = controllerFor(transport);

    const next = await controller.dispatchManual({
      project: '',
      trigger: '',
      actor: '',
      input: '',
      deliveryKey: '',
    });

    expect(dispatched).toBe(false);
    expect(next.dispatchPhase).toBe('failed');
    expect(next.dispatchProblem?.message).toContain('Choose the project');
    expect(manualRunIssues({
      project: PROJECT_A.id,
      trigger: 'auto-fix',
      actor: 'alice',
      input: '',
      deliveryKey: 'manual-1',
    })).toEqual([]);
  });

  it('builds a stable delivery key from the actor, time and press', () => {
    expect(manualDeliveryKey('Alice Summers', 1_700_000_000_000, 2)).toBe(
      'control-plane-alice-summers-1700000000000-2',
    );
    expect(manualDeliveryKey('  ', 1, 1)).toBe('control-plane-operator-1-1');
  });

  it('says whether a dispatch created a run or matched an earlier one', () => {
    expect(dispatchSentence({ runId: 'run-a', deduplicated: false })).toContain('started run run-a');
    expect(dispatchSentence({ runId: 'run-a', deduplicated: true })).toContain(
      'returned the existing run run-a',
    );
  });
});
