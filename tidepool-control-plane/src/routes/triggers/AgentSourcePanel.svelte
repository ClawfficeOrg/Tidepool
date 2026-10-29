<script lang="ts">
  /**
   * Where the agent choices came from.
   *
   * The editor never guesses a host's options. This panel says whether it is
   * asking, what the host reported, or why it could not answer — and when the
   * host is offline it offers the retry the author needs, without taking the
   * rest of the trigger away.
   */
  import type { AgentChoicesState } from '$lib/triggers';

  interface Props {
    /** The last agent-choice state. */
    choices: AgentChoicesState;
    /** Ask again; absent while a request is already running. */
    onRetry?: () => void;
  }

  let { choices, onRetry }: Props = $props();

  const variantCount = $derived(
    choices.status === 'ready' ? choices.report.capabilities.length : 0,
  );
  const providerCount = $derived(
    choices.status === 'ready' ? choices.options.providers.length : 0,
  );
</script>

<section
  class="tp-agent-source"
  data-agent-source={choices.status}
  aria-live="polite"
  aria-labelledby="tp-agent-source-title"
>
  <h3 class="tp-agent-source__title" id="tp-agent-source-title">Agent choices</h3>

  {#if choices.status === 'idle'}
    <p class="tp-agent-source__message">
      Choose a host first. The pool will then ask that host which providers,
      models, execution modes and thinking options it offers.
    </p>
  {:else if choices.status === 'loading'}
    <p class="tp-agent-source__message">
      Asking <code>{choices.host}</code> what it can run… The rest of the trigger
      stays editable while the answer arrives.
    </p>
  {:else if choices.status === 'ready'}
    <p class="tp-agent-source__message">
      <code>{choices.host}</code> reports {variantCount} variant{variantCount === 1 ? '' : 's'}
      across {providerCount} offered provider{providerCount === 1 ? '' : 's'}{choices.directory.trim() === ''
        ? ''
        : ` for ${choices.directory.trim()}`}. Only these choices are offered below.
    </p>
    {#if choices.report.reported_at}
      <p class="tp-agent-source__meta">
        Last reported {choices.report.reported_at}. A host that withdraws a model
        leaves the authored value in place and marks it unavailable.
      </p>
    {:else}
      <p class="tp-agent-source__meta">
        The pool did not report when this list was last refreshed.
      </p>
    {/if}
  {:else if choices.status === 'offline'}
    <p class="tp-agent-source__message" data-agent-offline="true">
      {choices.message}
    </p>
    {#if choices.problem}
      <p class="tp-agent-source__meta">
        The pool said: {choices.problem.message}
      </p>
    {/if}
    {#if onRetry}
      <button
        class="tp-button"
        type="button"
        data-agent-retry="true"
        onclick={() => onRetry?.()}
      >
        Retry asking {choices.host}
      </button>
    {/if}
  {:else}
    <p class="tp-agent-source__message" data-agent-unavailable="true">
      {choices.message}
    </p>
    {#if onRetry}
      <button
        class="tp-button"
        type="button"
        data-agent-retry="true"
        onclick={() => onRetry?.()}
      >
        Try again
      </button>
    {/if}
  {/if}
</section>

<style>
  .tp-agent-source {
    display: grid;
    gap: var(--tp-space-1);
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-agent-source__title {
    margin: 0;
    font-family: var(--tp-font-heading);
    font-size: var(--tp-font-body);
  }

  .tp-agent-source__message,
  .tp-agent-source__meta {
    margin: 0;
  }

  .tp-agent-source__meta {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-agent-source[data-agent-source='offline'] {
    border-color: var(--tp-status-warning);
  }

  .tp-agent-source[data-agent-source='unavailable'],
  .tp-agent-source[data-agent-source='unknown-host'] {
    border-color: var(--tp-status-danger);
  }
</style>
