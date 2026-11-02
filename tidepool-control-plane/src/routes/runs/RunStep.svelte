<script lang="ts">
  /**
   * One step of a run, as a person reads it.
   *
   * The status is colour + icon + word. A failure is a sentence first and a code
   * second: the sentence says what happened (a host that went offline says so),
   * and the pool's stable code stays available underneath for a bug report.
   *
   * The output is the agent's own text. It is rendered in a `<pre>` without
   * trimming, parsing or markdown, and the copy control writes exactly those
   * characters, so what a person copies is what the agent wrote.
   */
  import StatusBadge from '../../components/StatusBadge.svelte';
  import {
    NO_STEP_OUTPUT,
    copyOutput,
    failureExplanation,
    formatTimestamp,
    statusTone,
    statusWord,
    stepHasOutput,
    stepOutputText,
  } from '$lib/runs';
  import type { RunStepView } from '@clawffice/tidepool-sdk';

  interface Props {
    /** The step to render. */
    step: RunStepView;
  }

  let { step }: Props = $props();

  let copied = $state(false);
  let copyFailed = $state(false);

  const failure = $derived(failureExplanation(step.error ?? null, step.reason ?? null));
  const output = $derived(stepOutputText(step));

  /** Copy the step's output verbatim. */
  async function copy(): Promise<void> {
    const ok = await copyOutput(output);
    copied = ok;
    copyFailed = !ok;
  }
</script>

<li class="tp-run-step" data-step-id={step.id} data-step-name={step.step} data-step-status={step.status}>
  <header class="tp-run-step__header">
    <StatusBadge kind={statusTone(step.status)} />
    <h3 class="tp-run-step__title">{step.position + 1}. {step.step}</h3>
    <span class="tp-run-step__word">{statusWord(step.status)}</span>
  </header>

  <dl class="tp-run-step__facts">
    {#if step.host}<div><dt>Host</dt><dd>{step.host}</dd></div>{/if}
    {#if step.environment}<div><dt>Environment</dt><dd>{step.environment}</dd></div>{/if}
    {#if step.directory}<div><dt>Directory</dt><dd><code>{step.directory}</code></dd></div>{/if}
    {#if step.branch}<div><dt>Branch</dt><dd><code>{step.branch}</code></dd></div>{/if}
    {#if step.executionId}<div><dt>Execution</dt><dd><code>{step.executionId}</code></dd></div>{/if}
    {#if step.startedAt}<div><dt>Started</dt><dd>{formatTimestamp(step.startedAt)}</dd></div>{/if}
    {#if step.completedAt}<div><dt>Finished</dt><dd>{formatTimestamp(step.completedAt)}</dd></div>{/if}
  </dl>

  {#if failure}
    <section class="tp-run-step__failure" aria-labelledby={`tp-step-failure-${step.id}`} data-step-failure="true">
      <h4 class="tp-run-step__failure-title" id={`tp-step-failure-${step.id}`}>What happened</h4>
      <p class="tp-run-step__failure-sentence" data-failure-sentence="true">{failure.sentence}</p>
      {#if failure.detail}
        <p class="tp-run-step__failure-detail">{failure.detail}</p>
      {/if}
      {#if failure.code}
        <p class="tp-run-step__failure-code">
          Code <code data-failure-code={failure.code}>{failure.code}</code>
        </p>
      {/if}
    </section>
  {/if}

  <section class="tp-run-step__output-block" aria-labelledby={`tp-step-output-${step.id}`}>
    <div class="tp-run-step__output-header">
      <h4 class="tp-run-step__output-title" id={`tp-step-output-${step.id}`}>Output</h4>
      <button
        class="tp-button tp-run-step__copy"
        type="button"
        data-copy-step={step.id}
        disabled={!stepHasOutput(step)}
        onclick={() => void copy()}
      >
        {copied ? 'Copied' : 'Copy output'}
      </button>
    </div>
    {#if stepHasOutput(step)}
      <pre class="tp-run-step__output" data-step-output={step.step}>{output}</pre>
    {:else}
      <p class="tp-run-step__output-empty" data-step-output-empty="true">{NO_STEP_OUTPUT}</p>
    {/if}
    {#if copyFailed}
      <p class="tp-run-step__copy-failed" role="status">
        The browser did not allow the copy. Select the text above instead.
      </p>
    {/if}
  </section>
</li>

<style>
  .tp-run-step {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-3);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-run-step__header {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: var(--tp-space-2);
  }

  .tp-run-step__title {
    margin: 0;
    font-family: var(--tp-font-heading);
    font-size: var(--tp-font-body);
    color: var(--tp-text-strong);
  }

  .tp-run-step__word {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-run-step__facts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-3);
    margin: 0;
  }

  .tp-run-step__facts dt {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .tp-run-step__facts dd {
    margin: 0;
  }

  .tp-run-step__failure {
    display: grid;
    gap: var(--tp-space-1);
    padding: var(--tp-space-2);
    border-left: 3px solid var(--tp-status-danger);
    background: var(--tp-surface-raised, transparent);
  }

  .tp-run-step__failure-title,
  .tp-run-step__output-title {
    margin: 0;
    font-size: var(--tp-font-caption);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--tp-text-muted);
  }

  .tp-run-step__failure-sentence,
  .tp-run-step__failure-detail,
  .tp-run-step__failure-code,
  .tp-run-step__output-empty,
  .tp-run-step__copy-failed {
    margin: 0;
  }

  .tp-run-step__failure-sentence {
    color: var(--tp-text-strong);
  }

  .tp-run-step__output-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--tp-space-2);
  }

  .tp-run-step__output {
    margin: 0;
    padding: var(--tp-space-2);
    max-height: 28rem;
    overflow: auto;
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-control);
    background: var(--tp-surface);
    font-family: 'ShureTechMono Nerd Font Propo', 'Share Tech Mono', ui-monospace, monospace;
    font-size: var(--tp-font-code);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
