<script lang="ts">
  /**
   * The currentness banner.
   *
   * It reads only from `catchingUp`, `current` and the live presence, through
   * `currentnessBanner`, which is the one place that decides the words. A page
   * that has cached history but has not confirmed it renders "Catching up";
   * only a confirmed current fetch renders "Current".
   */
  import StatusBadge from '../../components/StatusBadge.svelte';
  import { currentnessBanner, type RoomsViewState } from '$lib/rooms';

  interface Props {
    /** The part of the page state that decides currentness. */
    state: Pick<RoomsViewState, 'catchingUp' | 'current' | 'presence' | 'lastSyncedAt'>;
  }

  let { state }: Props = $props();
  const banner = $derived(currentnessBanner(state));
</script>

<p
  class="tp-catch-up"
  data-catch-up-status={banner.status}
  role="status"
  aria-live="polite"
>
  <StatusBadge kind={banner.status === 'current' ? 'success' : 'pending'} />
  <span class="tp-catch-up__text">{banner.text}</span>
  {#if banner.status === 'current' && state.lastSyncedAt}
    <span class="tp-catch-up__time">Last read {state.lastSyncedAt}</span>
  {/if}
</p>

<style>
  .tp-catch-up {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--tp-space-2);
    margin: 0;
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-control);
    background: var(--tp-surface-raised, var(--tp-surface));
    color: var(--tp-text-muted);
  }

  .tp-catch-up[data-catch-up-status='current'] {
    border-color: var(--tp-status-success);
  }

  .tp-catch-up__time {
    font-family: 'ShureTechMono Nerd Font Propo', 'Share Tech Mono', ui-monospace, monospace;
    font-size: var(--tp-font-code);
  }
</style>
