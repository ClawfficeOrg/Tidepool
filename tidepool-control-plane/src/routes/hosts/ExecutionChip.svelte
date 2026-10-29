<script lang="ts">
  /**
   * Whether a host may run agents.
   *
   * Not a status badge, because it is not a lifecycle state: it is the separate
   * `host.execute` grant. Rendering it as its own labelled chip keeps a
   * connected-only host from reading as fully ready.
   */
  import type { ExecutionState } from '$lib/hosts';

  interface Props {
    /** The execution state of one host. */
    execution: ExecutionState;
  }

  let { execution }: Props = $props();
</script>

<span
  class="tp-execution"
  data-execution={execution.enabled ? 'enabled' : 'disabled'}
  title={execution.description}
>
  <svg
    class="tp-execution__icon"
    viewBox="0 0 24 24"
    aria-hidden="true"
    focusable="false"
    role="presentation"
  >
    {#if execution.enabled}
      <path
        d="M5 12l5 5L20 7M5 4v4M3 6h4M19 16v4M17 18h4"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    {:else}
      <path
        d="M12 3a9 9 0 100 18 9 9 0 000-18zM6 6l12 12"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    {/if}
  </svg>
  <span class="tp-execution__word">{execution.word}</span>
</span>

<style>
  .tp-execution {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 8px 2px 6px;
    border: 1px dashed var(--tp-border-strong);
    border-radius: var(--tp-radius-chip);
    font-size: var(--tp-font-caption);
    font-weight: 600;
    white-space: nowrap;
  }

  .tp-execution__icon {
    width: 14px;
    height: 14px;
    flex: none;
  }

  .tp-execution[data-execution='enabled'] .tp-execution__icon {
    color: var(--tp-status-success);
  }

  .tp-execution[data-execution='disabled'] .tp-execution__icon {
    color: var(--tp-status-warning);
  }

  .tp-execution__word {
    color: var(--tp-text-strong);
  }
</style>
