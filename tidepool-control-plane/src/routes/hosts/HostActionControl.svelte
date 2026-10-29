<script lang="ts">
  /**
   * A labelled host action with its consequence, and a second step to confirm.
   *
   * The consequence is not hidden behind the click: it is printed beside the
   * button, so a person reads what granting or revoking execution does before
   * they reach for it. Confirming restates it against the host's slug and names
   * the host in the confirm button, so the irreversible-feeling step is the one
   * that says exactly what will change.
   */
  import type { HostAction } from '$lib/hosts';

  interface Props {
    /** The action to offer. */
    action: HostAction;
    /** Unique key for this action on this page, used for ids. */
    actionKey: string;
    /** True while a request for this host is in flight. */
    busy?: boolean;
    /** Why the role cannot use it; renders the action as unavailable. */
    denial?: string | null;
    /** Start in the confirm step, so the confirm copy can be rendered in tests. */
    initialConfirming?: boolean;
    /** Perform the action, after confirmation. */
    onConfirm?: () => void;
  }

  let {
    action,
    actionKey,
    busy = false,
    denial = null,
    initialConfirming = false,
    onConfirm,
  }: Props = $props();

  const consequenceId = `tp-consequence-${actionKey}`;

  // The confirm step is local state: it starts where the caller asks, then the
  // person drives it. Never re-seeded from the prop.
  // svelte-ignore state_referenced_locally
  let confirming = $state(initialConfirming);

  /** First click: reveal the consequence restated, do not act. */
  function ask(): void {
    if (busy || denial) return;
    confirming = true;
  }

  /** Second click: perform it. */
  function confirm(): void {
    if (busy || denial) return;
    confirming = false;
    onConfirm?.();
  }

  /** Back out without changing anything. */
  function cancel(): void {
    confirming = false;
  }
</script>

<div class="tp-host-action" data-action={action.id} data-confirming={confirming ? 'true' : 'false'}>
  <button
    class="tp-button tp-button--quiet"
    type="button"
    disabled={busy || Boolean(denial)}
    aria-expanded={confirming}
    aria-describedby={consequenceId}
    onclick={ask}
  >
    {action.label}
  </button>

  <p class="tp-host-action__consequence" id={consequenceId}>
    {action.consequence}
  </p>

  {#if denial}
    <p class="tp-host-action__denial">{denial}</p>
  {:else if confirming}
    <div
      class="tp-host-action__confirm"
      role="group"
      aria-label={`Confirm ${action.label.toLowerCase()}`}
    >
      <p class="tp-host-action__confirm-copy">{action.confirmConsequence}</p>
      <div class="tp-host-action__buttons">
        <button class="tp-button" type="button" disabled={busy} onclick={confirm}>
          {action.confirmLabel}
        </button>
        <button class="tp-button tp-button--quiet" type="button" onclick={cancel}>
          Cancel
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .tp-host-action {
    display: grid;
    gap: var(--tp-space-1);
    justify-items: start;
  }

  .tp-host-action__consequence,
  .tp-host-action__denial,
  .tp-host-action__confirm-copy {
    margin: 0;
    max-width: 62ch;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-host-action__denial {
    color: var(--tp-text-strong);
  }

  .tp-host-action__confirm {
    display: grid;
    gap: var(--tp-space-1);
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
  }

  .tp-host-action__buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-1);
  }

  /* A secondary action: the same hit target, without the primary weight. */
  .tp-button--quiet {
    background: transparent;
    color: var(--tp-text-strong);
  }
</style>
