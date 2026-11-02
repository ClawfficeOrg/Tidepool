<script lang="ts">
  /**
   * One principal, shown as a kind word, a distinct glyph and its id.
   *
   * A person is a filled circle and the word "Person"; an agent session is a
   * diamond and "Agent session". The shape, the word and the accessible label
   * all differ, so the two are distinguishable with colour off. The wrapper
   * exposes `data-author-kind` for tests and for styles that need to differ
   * beyond colour.
   */
  import type { Principal } from '@clawffice/tidepool-sdk';
  import { describeAuthor } from '$lib/rooms';

  interface Props {
    /** The principal, or null when the author was deleted. */
    author: Principal | null | undefined;
    /** Render the id in a monospace face after the kind word. */
    showId?: boolean;
  }

  let { author, showId = true }: Props = $props();
  const descriptor = $derived(describeAuthor(author));
</script>

<span
  class="tp-author"
  data-author-kind={descriptor.kind}
  data-author-glyph={descriptor.glyph}
  aria-label={descriptor.label}
  title={descriptor.label}
>
  <span class="tp-author__glyph" aria-hidden="true">{descriptor.glyph}</span>
  <span class="tp-author__kind">{descriptor.kindLabel}</span>
  {#if showId && descriptor.id}
    <span class="tp-author__id">{descriptor.id}</span>
  {/if}
</span>

<style>
  .tp-author {
    display: inline-flex;
    align-items: baseline;
    gap: var(--tp-space-1);
    color: var(--tp-text-strong);
    font-weight: 600;
  }

  .tp-author__glyph {
    font-size: var(--tp-font-body);
    line-height: 1;
  }

  .tp-author__kind {
    font-size: var(--tp-font-caption);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .tp-author[data-author-kind='session'] .tp-author__glyph {
    color: var(--tp-accent);
  }

  .tp-author[data-author-kind='user'] .tp-author__glyph {
    color: var(--tp-primary);
  }

  .tp-author[data-author-kind='unknown'] .tp-author__glyph {
    color: var(--tp-text-muted);
  }

  .tp-author__id {
    font-family: 'ShureTechMono Nerd Font Propo', 'Share Tech Mono', ui-monospace, monospace;
    font-size: var(--tp-font-code);
    color: var(--tp-text-muted);
    overflow-wrap: anywhere;
  }
</style>
