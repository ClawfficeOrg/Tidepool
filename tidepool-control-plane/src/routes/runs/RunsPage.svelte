<script lang="ts">
  /**
   * The runs and activity page.
   *
   * The feed is one cursor-paged union of receipts, runs, steps and executions;
   * the filters above it combine, and the page keeps its own filter guard on top
   * of the pool's, so another project's rows cannot appear under a project
   * filter even if a stale page arrives. A run's detail follows a live
   * execution on a timer and stops when the run is terminal; a failed step says
   * what the pool's code means in a sentence a person can act on.
   *
   * The page renders from a `RunsViewState`. It loads itself on mount when it
   * has a pool and a session token (or an injected transport), and it can be
   * rendered from an initial state in tests, which is how the filter
   * combination, the failure sentence and the output block are asserted without
   * a browser.
   */
  import ProblemDetails from '../../components/ProblemDetails.svelte';
  import StatusBadge from '../../components/StatusBadge.svelte';
  import ActivityFilters from './ActivityFilters.svelte';
  import ActivityList from './ActivityList.svelte';
  import RunDetail from './RunDetail.svelte';
  import { canUseControl, controlById, denialReason } from '../../auth/roles';
  import type { Role } from '../../auth/types';
  import { isPoolConfigured, poolClient, type PoolConfig } from '$lib/api/pool';
  import {
    RunsController,
    canLoadRuns,
    dispatchSentence,
    emptyRunsState,
    failureExplanation,
    filterActivityEntries,
    isNamedFailureCode,
    manualDeliveryKey,
    manualRunIssues,
    runsTransport,
    type ActivityFilterField,
    type ManualRunInput,
    type RunsTransport,
    type RunsViewState,
  } from '$lib/runs';

  interface Props {
    /** The pool this control plane is pointed at. */
    pool?: PoolConfig;
    /** The signed-in operator's token. */
    token?: string | null;
    /** The signed-in operator's role, for control gating. */
    role?: Role | null;
    /** The state to render before loading; also how tests render any state. */
    initial?: RunsViewState;
    /** Transport override, for tests; otherwise built from the pool and token. */
    transport?: RunsTransport | null;
  }

  let {
    pool = { baseUrl: null },
    token = null,
    role = null,
    initial = emptyRunsState(),
    transport = null,
  }: Props = $props();

  // svelte-ignore state_referenced_locally
  let view = $state<RunsViewState>(initial);

  /** Build the controller once: the page keeps one view of one pool. */
  // svelte-ignore state_referenced_locally
  const controller: RunsController | null = (() => {
    if (transport) return new RunsController({ transport });
    if (!canLoadRuns(pool, token)) return null;
    try {
      return new RunsController({ transport: runsTransport(poolClient(pool, { token })) });
    } catch {
      return null;
    }
  })();

  const configured = transport !== null || isPoolConfigured(pool);
  const dispatchControl = controlById('dispatch-run');
  const canDispatchRole = $derived(
    dispatchControl !== null && canUseControl(role, dispatchControl),
  );
  const dispatchDenial = $derived(
    dispatchControl !== null && !canUseControl(role, dispatchControl)
      ? denialReason(role, dispatchControl)
      : null,
  );

  let dispatchProject = $state('');
  let dispatchTrigger = $state('');
  let dispatchActor = $state('');
  let dispatchText = $state('');
  let dispatchSequence = 0;

  const dispatchInput = $derived<ManualRunInput>({
    project: dispatchProject || view.projects[0]?.id || '',
    trigger: dispatchTrigger,
    actor: dispatchActor,
    input: dispatchText,
    deliveryKey: 'pending',
  });
  const dispatchIssues = $derived(manualRunIssues(dispatchInput));
  const canDispatch = $derived(
    canDispatchRole &&
      dispatchIssues.length === 0 &&
      view.dispatchPhase !== 'dispatching' &&
      view.projects.length > 0,
  );
  const dispatchFailure = $derived(
    view.dispatchProblem !== null && isNamedFailureCode(view.dispatchProblem.code)
      ? failureExplanation(view.dispatchProblem.code, null)
      : null,
  );
  /**
   * The page's own filter guard, applied again at render time.
   *
   * The controller already dropped entries the filter excludes; this second
   * pass is what makes "never show another project's runs" a property of the
   * rendered page, including when it is rendered from a supplied state.
   */
  const visibleItems = $derived(filterActivityEntries(view.items, view.filters));

  /** Adopt a controller result. */
  function adopt(next: RunsViewState): void {
    view = next;
  }

  /** Load the project list and the first page. */
  async function refresh(): Promise<void> {
    if (!controller) return;
    adopt(await controller.refresh());
  }

  $effect(() => {
    // A page with a pool and no initial state loads once; a page rendered from
    // a supplied state keeps that state.
    if (controller && initial.phase === 'idle' && controller.snapshot().phase === 'idle') {
      void controller.load().then(adopt);
    }
  });

  $effect(() => () => controller?.dispose());

  /** Apply one filter and reload the first page. */
  async function filter(field: ActivityFilterField, value: string): Promise<void> {
    if (!controller) {
      view = { ...view, filters: { ...view.filters, [field]: value } };
      return;
    }
    adopt(await controller.setFilter(field, value));
  }

  /** Drop every filter. */
  async function clearFilters(): Promise<void> {
    if (!controller) {
      view = {
        ...view,
        filters: { project: '', provider: '', trigger: '', status: '', kind: '' },
      };
      return;
    }
    adopt(await controller.clearFilters());
  }

  /** Append the next page. */
  async function loadMore(): Promise<void> {
    if (!controller) return;
    adopt(await controller.loadMore());
  }

  /** Open one run and follow it while it is live. */
  async function openRun(runId: string): Promise<void> {
    if (!controller) {
      view = { ...view, selectedRunId: runId };
      return;
    }
    adopt(await controller.selectRun(runId));
  }

  /** Read the open run again. */
  async function refreshDetail(): Promise<void> {
    if (!controller) return;
    const runId = view.selectedRunId;
    if (runId === null) return;
    adopt(await controller.selectRun(runId));
  }

  /** Close the run detail. */
  function closeRun(): void {
    if (!controller) {
      view = { ...view, selectedRunId: null, detail: null, detailPhase: 'idle' };
      return;
    }
    adopt(controller.closeRun());
  }

  /** Dispatch one manual run and follow what it starts. */
  async function dispatch(): Promise<void> {
    if (!controller) return;
    const input: ManualRunInput = {
      ...dispatchInput,
      deliveryKey: manualDeliveryKey(dispatchActor, Date.now(), (dispatchSequence += 1)),
    };
    adopt(await controller.dispatchManual(input));
  }
</script>

<section class="tp-runs tp-page" aria-labelledby="tp-runs-title" data-runs-page="true">
  <h1 class="tp-page__title" id="tp-runs-title">Runs</h1>
  <p class="tp-page__lede">
    What arrived, whether it matched a trigger, what ran, and what each step
    wrote. Filters combine, the detail follows a running execution live, and a
    failure names the code the pool recorded beside the sentence that explains
    it.
  </p>

  {#if !configured}
    <p class="tp-page__state" data-runs-state="unconfigured">
      <StatusBadge kind="pending" />
      <span>
        No pool is configured, so there is no activity to read and no run to
        dispatch. Point the control plane at a pool and sign in.
      </span>
    </p>
  {:else}
    {#if view.problem}
      <ProblemDetails problem={view.problem} onRetry={() => void refresh()} />
    {/if}

    <section class="tp-runs__dispatch" aria-labelledby="tp-runs-dispatch-title">
      <div class="tp-runs__section-header">
        <h2 class="tp-runs__section-title" id="tp-runs-dispatch-title">Dispatch a manual run</h2>
        {#if !canDispatchRole && dispatchDenial}
          <p class="tp-runs__denial" data-runs-denial="dispatch-run">{dispatchDenial}</p>
        {/if}
      </div>
      <form
        class="tp-runs__dispatch-form"
        data-dispatch-form="true"
        onsubmit={(event) => {
          event.preventDefault();
          void dispatch();
        }}
      >
        <label class="tp-field">
          <span class="tp-field__label">Project</span>
          <select
            class="tp-input"
            value={dispatchInput.project}
            data-dispatch-field="project"
            disabled={!canDispatchRole || view.projects.length === 0}
            onchange={(event) =>
              (dispatchProject = (event.currentTarget as HTMLSelectElement).value)}
          >
            {#if view.projects.length === 0}
              <option value="">No projects loaded</option>
            {:else}
              {#each view.projects as project (project.id)}
                <option value={project.id}>{project.name} ({project.slug})</option>
              {/each}
            {/if}
          </select>
        </label>
        <label class="tp-field">
          <span class="tp-field__label">Trigger</span>
          <input
            class="tp-input"
            type="text"
            value={dispatchTrigger}
            placeholder="Installed trigger name"
            data-dispatch-field="trigger"
            disabled={!canDispatchRole}
            oninput={(event) =>
              (dispatchTrigger = (event.currentTarget as HTMLInputElement).value)}
          />
        </label>
        <label class="tp-field">
          <span class="tp-field__label">Actor</span>
          <input
            class="tp-input"
            type="text"
            value={dispatchActor}
            placeholder="Actor the trigger must allow"
            data-dispatch-field="actor"
            disabled={!canDispatchRole}
            oninput={(event) => (dispatchActor = (event.currentTarget as HTMLInputElement).value)}
          />
        </label>
        <label class="tp-field tp-runs__request-field">
          <span class="tp-field__label">Request</span>
          <textarea
            class="tp-input"
            rows="3"
            value={dispatchText}
            placeholder="key=value tokens followed by what the agent should do"
            data-dispatch-field="input"
            disabled={!canDispatchRole}
            oninput={(event) => (dispatchText = (event.currentTarget as HTMLTextAreaElement).value)}
          ></textarea>
        </label>
        <div class="tp-runs__dispatch-actions">
          <button
            class="tp-button"
            type="submit"
            data-dispatch-action="dispatch"
            disabled={!canDispatch || view.busy}
          >
            {view.dispatchPhase === 'dispatching' ? 'Dispatching…' : 'Dispatch run'}
          </button>
          {#if dispatchIssues.length > 0 && canDispatchRole}
            <p class="tp-runs__dispatch-note" data-dispatch-issues={dispatchIssues.length}>
              {dispatchIssues[0]}
            </p>
          {/if}
        </div>
      </form>

      {#if view.lastDispatch}
        <p class="tp-runs__dispatch-result" data-dispatch-result="true">
          <StatusBadge kind="success" />
          <span>{dispatchSentence(view.lastDispatch)}</span>
        </p>
      {/if}
      {#if view.dispatchProblem}
        {#if dispatchFailure}
          <p class="tp-runs__dispatch-failure" data-dispatch-failure-sentence="true">
            {dispatchFailure.sentence}
            {#if dispatchFailure.code}<code data-failure-code={dispatchFailure.code}>{dispatchFailure.code}</code>{/if}
          </p>
        {/if}
        <ProblemDetails problem={view.dispatchProblem} />
      {/if}
    </section>

    <ActivityFilters
      filters={view.filters}
      projects={view.projects}
      busy={view.busy}
      onFilter={(field, value) => void filter(field, value)}
      onClear={() => void clearFilters()}
    />

    {#if view.phase === 'loading' && view.items.length === 0}
      <p class="tp-page__state" data-runs-state="loading">
        <StatusBadge kind="pending" />
        <span>Reading the activity feed…</span>
      </p>
    {:else if view.phase === 'error' && view.problem}
      <p class="tp-page__state" data-runs-state="error">
        <StatusBadge kind="danger" />
        <span>The activity feed could not be loaded. Use the problem above to try again.</span>
      </p>
    {:else}
      <div class="tp-runs__layout">
        <section class="tp-runs__feed" aria-labelledby="tp-runs-feed-title">
          <div class="tp-runs__section-header">
            <h2 class="tp-runs__section-title" id="tp-runs-feed-title">Activity</h2>
            <button
              class="tp-button"
              type="button"
              data-runs-action="refresh"
              disabled={view.busy}
              onclick={() => void refresh()}
            >
              Refresh feed
            </button>
          </div>
          <ActivityList
            items={visibleItems}
            projects={view.projects}
            selectedRunId={view.selectedRunId}
            onOpen={(runId) => void openRun(runId)}
          />
          {#if view.hasMore}
            <button
              class="tp-button"
              type="button"
              data-runs-action="more"
              disabled={view.loadingMore}
              onclick={() => void loadMore()}
            >
              {view.loadingMore ? 'Loading…' : 'Load more'}
            </button>
          {/if}
        </section>

        <section class="tp-runs__detail" aria-labelledby="tp-runs-detail-title">
          <h2 class="tp-runs__section-title" id="tp-runs-detail-title">Run detail</h2>
          <RunDetail
            phase={view.detailPhase}
            detail={view.detail}
            problem={view.detailProblem}
            live={view.live}
            updatedAt={view.detailUpdatedAt}
            projects={view.projects}
            onRetry={() => void refreshDetail()}
            onClose={closeRun}
          />
        </section>
      </div>
    {/if}
  {/if}
</section>

<style>
  .tp-runs {
    display: grid;
    gap: var(--tp-space-3);
  }

  .tp-runs__layout {
    display: grid;
    gap: var(--tp-space-3);
    grid-template-columns: minmax(18rem, 1fr) minmax(20rem, 1.2fr);
    align-items: start;
  }

  @media (max-width: 60rem) {
    .tp-runs__layout {
      grid-template-columns: 1fr;
    }
  }

  .tp-runs__dispatch,
  .tp-runs__feed,
  .tp-runs__detail {
    display: grid;
    gap: var(--tp-space-2);
    align-content: start;
  }

  .tp-runs__section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--tp-space-2);
  }

  .tp-runs__section-title {
    margin: 0;
    font-family: var(--tp-font-heading);
    font-size: var(--tp-font-body);
    color: var(--tp-text-strong);
  }

  .tp-runs__dispatch-form {
    display: grid;
    gap: var(--tp-space-2);
    grid-template-columns: repeat(auto-fit, minmax(12rem, 1fr));
    align-items: end;
  }

  .tp-runs__request-field {
    grid-column: 1 / -1;
  }

  .tp-field {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-field__label {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .tp-input {
    padding: var(--tp-space-1) var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    background: var(--tp-surface);
    color: var(--tp-text-strong);
    font: inherit;
  }

  .tp-runs__dispatch-actions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--tp-space-2);
  }

  .tp-runs__dispatch-note,
  .tp-runs__denial,
  .tp-runs__dispatch-result,
  .tp-runs__dispatch-failure {
    margin: 0;
    color: var(--tp-text-muted);
  }

  .tp-runs__dispatch-result,
  .tp-runs__dispatch-failure {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: var(--tp-space-2);
  }
</style>
