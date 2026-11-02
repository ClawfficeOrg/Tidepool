<script lang="ts">
  /**
   * Where events arrive, and the exact URL to give the provider.
   *
   * GitHub and Slack webhooks print the callback URL for the pool's current
   * origin, character for character: an operator pastes it into the provider,
   * so an approximate origin is a broken webhook. Socket mode and the Discord
   * gateway have no inbound URL at all, and say so instead of showing an empty
   * field.
   */
  import type { CallbackView } from '$lib/connections';

  interface Props {
    /** The callback block to render. */
    callback: CallbackView;
  }

  let { callback }: Props = $props();
</script>

<section class="tp-callback" data-callback-kind={callback.kind} aria-label="Event transport">
  <h3 class="tp-callback__title">{callback.kind === 'webhook' ? 'Callback URL' : 'Event transport'}</h3>

  {#if callback.url}
    <code class="tp-callback__url" data-callback-url={callback.url}>{callback.url}</code>
    <p class="tp-callback__note">{callback.note}</p>
  {:else}
    <p class="tp-callback__note" data-callback-note="true">{callback.note}</p>
  {/if}
</section>

<style>
  .tp-callback {
    display: grid;
    gap: var(--tp-space-1);
    padding: var(--tp-space-2);
    background: var(--tp-window-background);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-control);
  }

  .tp-callback__title {
    margin: 0;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .tp-callback__url {
    display: block;
    overflow-wrap: anywhere;
    color: var(--tp-text-strong);
    font-size: var(--tp-font-code);
  }

  .tp-callback__note {
    margin: 0;
    max-width: 68ch;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }
</style>
