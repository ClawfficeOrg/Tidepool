<script lang="ts">
  /**
   * The open run: the arrival that started it, how it reached an agent, and
   * every step in order.
   *
   * The arrival sentence is the goal's "created, continued or restored" in
   * words. A run-level failure is shown once, above the steps, from whichever
   * step carries it, so a host that went offline reads as that sentence before
   * a person opens the step. A live run says so and shows when it was last
   * read; the page keeps reading it until it stops.
   */
  import ProblemDetails from '../../components/ProblemDetails.svelte';
  import StatusBadge from '../../components/StatusBadge.svelte';
  import RunStep from './RunStep.svelte';
  import {
    arrivalModeLabel,
    arrivalSentence,
    formatTimestamp,
    projectLabel,
    runFailureExplanation,
    statusTone,
    statusWord,
  } from '$lib/runs';
  import type { PoolProblem } from '$lib/api/problem';
  import type { ActivityRunDetail, ProjectView } from '@clawffice/tidepool-sdk';

  interface Props {
    /** The detail's load state. */
    phase: 'idle' | 'loading' | 'ready' | 'error';
    /** The run and its arrival record. */
    detail: ActivityRunDetail | null;
    /** A failure loading the detail. */
    problem: PoolProblem | null;
    /** True while the page is following a live run. */
    live: boolean;
    /** When the detail was last read. */
    updatedAt: string | null;
    /** The organization's projects, for a readable project name. */
    projects: readonly ProjectView[];
    /** Ask the pool for the detail again. */
    onRetry: () => void;
    /** Close the detail. */
    onClose: () => void;
  }

  let {
    phase,
    detail,
    problem,
    live,
    updatedAt,
    projects,
    onRetry,
    onClose,
  }: Props = $props();

  const run = $derived(detail?.run ?? null);
  const failure = $derived(run ? runFailureExplanation(run) : null);
</script>

<section
  class="tp-run-detail"
  aria-labelledby="tp-run-detail-title"
  data-run-detail={run?.id ?? ''}
  data-run-status={run?.status ?? ''}
  data-run-live={live ? 'true' : 'false'}
>
  <header class="tp-run-detail__header">
    <div>
      <h2 class="tp-run-detail__title" id="tp-run-detail-title">
        {#if run}Run of '{run.triggerName}'{:else}Run detail{/if}
      </h2>
      {#if live}
        <p class="tp-run-detail__live" data-runs-live="true">
          Following this run live. Last read {formatTimestamp(updatedAt)}.
        </p>
      {:else if updatedAt}
        <p class="tp-run-detail__updated" data-detail-updated={updatedAt}>
          Last read {formatTimestamp(updatedAt)}.
        </p>
      {/if}
    </div>
    <div class="tp-run-detail__actions">
      <button class="tp-button" type="button" data-detail-action="refresh" onclick={onRetry}>
        Read again
      </button>
      <button class="tp-button" type="button" data-detail-action="close" onclick={onClose}>
        Close
      </button>
    </div>
  </header>

  {#if phase === 'loading'}
    <p class="tp-page__state" data-run-detail-state="loading">
      <StatusBadge kind="pending" />
      <span>Reading the run and the arrival that started it…</span>
    </p>
  {:else if phase === 'error'}
    {#if problem}
      <ProblemDetails {problem} onRetry={onRetry} />
    {:else}
      <p class="tp-page__state" data-run-detail-state="error">
        <StatusBadge kind="danger" />
        <span>The run could not be read.</span>
      </p>
    {/if}
  {:else if run && detail}
    <div class="tp-run-detail__summary">
      <p class="tp-run-detail__status">
        <StatusBadge kind={statusTone(run.status)} />
        <strong>{statusWord(run.status)}</strong>
      </p>
      <dl class="tp-run-detail__facts">
        <div><dt>Project</dt><dd>{projectLabel(run.project, projects)}</dd></div>
        <div><dt>Workflow</dt><dd>{run.workflow}</dd></div>
        <div><dt>Trigger</dt><dd>{run.triggerName}</dd></div>
        <div><dt>Origin</dt><dd>{run.origin === 'manual' ? 'Manual request' : 'Provider event'}</dd></div>
        {#if run.actor}<div><dt>Actor</dt><dd>{run.actor}</dd></div>{/if}
        <div><dt>Created</dt><dd>{formatTimestamp(run.createdAt)}</dd></div>
        <div><dt>Started</dt><dd>{formatTimestamp(run.startedAt)}</dd></div>
        <div><dt>Finished</dt><dd>{formatTimestamp(run.completedAt)}</dd></div>
        {#if run.deadlineAt}<div><dt>Deadline</dt><dd>{formatTimestamp(run.deadlineAt)}</dd></div>{/if}
        <div><dt>Delivery key</dt><dd><code>{run.deliveryKey}</code></dd></div>
      </dl>
    </div>

    <section class="tp-run-detail__arrival" aria-labelledby="tp-run-arrival-title">
      <h3 class="tp-run-detail__section-title" id="tp-run-arrival-title">How the agent was reached</h3>
      <p class="tp-run-detail__arrival-mode">
        <StatusBadge kind="info" />
        <strong data-arrival-mode={detail.arrival.mode}>{arrivalModeLabel(detail.arrival.mode)}</strong>
      </p>
      <p class="tp-run-detail__arrival-sentence" data-arrival-sentence="true">
        {arrivalSentence(detail.arrival)}
      </p>
      {#if detail.arrival.provider}
        <p class="tp-run-detail__arrival-provider">Arrived from {detail.arrival.provider}.</p>
      {/if}
      {#if detail.arrival.conversationKey}
        <p class="tp-run-detail__arrival-conversation">
          Conversation <code>{detail.arrival.conversationKey}</code>
        </p>
      {/if}
      {#if detail.arrival.outputLimits.length > 0}
        <p class="tp-run-detail__arrival-limits">
          This arrival alone declared {detail.arrival.outputLimits.length}
          step{detail.arrival.outputLimits.length === 1 ? '' : 's'} with output limits.
        </p>
      {/if}
    </section>

    {#if failure}
      <section
        class="tp-run-detail__failure"
        aria-labelledby="tp-run-failure-title"
        data-run-failure={failure.code ?? 'unnamed'}
      >
        <h3 class="tp-run-detail__section-title" id="tp-run-failure-title">Why the run stopped</h3>
        <p class="tp-run-detail__failure-sentence" data-failure-sentence="true">{failure.sentence}</p>
        {#if failure.detail}<p class="tp-run-detail__failure-detail">{failure.detail}</p>{/if}
        {#if failure.code}
          <p class="tp-run-detail__failure-code">
            Code <code data-failure-code={failure.code}>{failure.code}</code>
          </p>
        {/if}
      </section>
    {/if}

    <section class="tp-run-detail__request" aria-labelledby="tp-run-request-title">
      <h3 class="tp-run-detail__section-title" id="tp-run-request-title">What was asked</h3>
      <pre class="tp-run-detail__request-text" data-run-request="true">{run.requestText}</pre>
    </section>

    <section class="tp-run-detail__steps" aria-labelledby="tp-run-steps-title">
      <h3 class="tp-run-detail__section-title" id="tp-run-steps-title">
        Steps ({run.steps.length})
      </h3>
      {#if run.steps.length === 0}
        <p class="tp-page__state" data-run-steps-empty="true">
          <StatusBadge kind="pending" />
          <span>No step has been recorded for this run yet.</span>
        </p>
      {:else}
        <ol class="tp-run-detail__step-list">
          {#each run.steps as step (step.id)}
            <RunStep {step} />
          {/each}
        </ol>
      {/if}
    </section>
  {:else}
    <p class="tp-page__state" data-run-detail-state="empty">
      <StatusBadge kind="pending" />
      <span>Choose a run to see its steps and output.</span>
    </p>
  {/if}
</section>

<style>
  .tp-run-detail {
    display: grid;
    gap: var(--tp-space-3);
    align-content: start;
  }

  .tp-run-detail__header {
    display: flex;
    justify-content: space-between;
    align-items: start;
    flex-wrap: wrap;
    gap: var(--tp-space-2);
  }

  .tp-run-detail__title {
    margin: 0;
    font-family: var(--tp-font-heading);
    font-size: var(--tp-font-heading);
    color: var(--tp-text-strong);
  }

  .tp-run-detail__live,
  .tp-run-detail__updated {
    margin: var(--tp-space-1) 0 0;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-run-detail__live {
    color: var(--tp-status-info, var(--tp-text-strong));
  }

  .tp-run-detail__actions {
    display: flex;
    gap: var(--tp-space-1);
  }

  .tp-run-detail__summary,
  .tp-run-detail__arrival,
  .tp-run-detail__failure,
  .tp-run-detail__request,
  .tp-run-detail__steps {
    display: grid;
    gap: var(--tp-space-2);
  }

  .tp-run-detail__status,
  .tp-run-detail__arrival-mode {
    display: flex;
    align-items: center;
    gap: var(--tp-space-2);
    margin: 0;
  }

  .tp-run-detail__facts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-3);
    margin: 0;
  }

  .tp-run-detail__facts dt {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .tp-run-detail__facts dd {
    margin: 0;
  }

  .tp-run-detail__section-title {
    margin: 0;
    font-family: var(--tp-font-heading);
    font-size: var(--tp-font-body);
    color: var(--tp-text-strong);
  }

  .tp-run-detail__arrival-sentence,
  .tp-run-detail__arrival-provider,
  .tp-run-detail__arrival-conversation,
  .tp-run-detail__arrival-limits,
  .tp-run-detail__failure-sentence,
  .tp-run-detail__failure-detail,
  .tp-run-detail__failure-code {
    margin: 0;
    max-width: 68ch;
  }

  .tp-run-detail__failure {
    padding: var(--tp-space-2);
    border-left: 3px solid var(--tp-status-danger);
  }

  .tp-run-detail__failure-sentence {
    color: var(--tp-text-strong);
  }

  .tp-run-detail__request-text {
    margin: 0;
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-control);
    background: var(--tp-surface);
    font-family: 'ShureTechMono Nerd Font Propo', 'Share Tech Mono', ui-monospace, monospace;
    font-size: var(--tp-font-code);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .tp-run-detail__step-list {
    display: grid;
    gap: var(--tp-space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }
</style>
