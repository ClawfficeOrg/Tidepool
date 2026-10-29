<script lang="ts">
  /**
   * The room history.
   *
   * Messages render oldest first, keyed by the pool's id, so a live frame that
   * arrives twice never becomes two rows. The scroll container asks for an
   * older page when the reader reaches the top; the button does the same thing
   * for a keyboard or screen-reader reader, and both go through the controller's
   * cursor walk.
   */
  import type { MessageView } from '@clawffice/tidepool-sdk';
  import AuthorBadge from './AuthorBadge.svelte';
  import { shouldLoadOlder } from '$lib/rooms';

  interface Props {
    /** The messages on screen, ordered by sequence. */
    messages: readonly MessageView[];
    /** True when a further older page may exist. */
    hasOlder?: boolean;
    /** True while an older page is being read. */
    loadingOlder?: boolean;
    /** True while the page is behind the room. */
    catchingUp?: boolean;
    /** Called when the reader reaches the top, or presses the older button. */
    onLoadOlder?: () => void;
  }

  let {
    messages,
    hasOlder = false,
    loadingOlder = false,
    catchingUp = false,
    onLoadOlder,
  }: Props = $props();

  /** Ask for older messages when a scroll reaches the top. */
  function onscroll(event: Event): void {
    const target = event.currentTarget as HTMLElement | null;
    if (!target) return;
    if (shouldLoadOlder(target.scrollTop, hasOlder, loadingOlder)) {
      onLoadOlder?.();
    }
  }
</script>

<div
  class="tp-messages"
  data-message-list="true"
  data-message-count={messages.length}
  data-catching-up={catchingUp ? 'true' : 'false'}
  role="log"
  aria-label="Room history"
  tabindex="-1"
  {onscroll}
>
  {#if hasOlder}
    <button
      class="tp-button tp-messages__older"
      type="button"
      data-rooms-action="older"
      disabled={loadingOlder}
      onclick={() => onLoadOlder?.()}
    >
      {loadingOlder ? 'Loading older messages…' : 'Load older messages'}
    </button>
  {:else if messages.length > 0}
    <p class="tp-messages__start" data-message-start="true">
      This is the beginning of the room’s history.
    </p>
  {/if}

  {#if messages.length === 0}
    <p class="tp-messages__empty" data-message-empty="true">
      No messages yet. The first message will appear here.
    </p>
  {:else}
    <ol class="tp-messages__list">
      {#each messages as message (message.id)}
        <li
          class="tp-messages__item"
          data-message-id={message.id}
          data-message-sequence={message.sequence}
          data-author-kind={message.author?.kind ?? 'unknown'}
        >
          <AuthorBadge author={message.author} />
          <time class="tp-messages__time" datetime={message.created_at}>{message.created_at}</time>
          {#if message.reply_to}
            <span class="tp-messages__reply" data-message-reply-to={message.reply_to}>
              Replying to {message.reply_to}
            </span>
          {/if}
          <p class="tp-messages__body">{message.body}</p>
          <span class="tp-messages__id" aria-label={`Message id ${message.id}`}>{message.id}</span>
        </li>
      {/each}
    </ol>
  {/if}
</div>

<style>
  .tp-messages {
    display: grid;
    gap: var(--tp-space-2);
    max-height: 32rem;
    overflow-y: auto;
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
    background: var(--tp-surface);
    align-content: start;
  }

  .tp-messages__older {
    justify-self: center;
  }

  .tp-messages__start,
  .tp-messages__empty {
    margin: 0;
    color: var(--tp-text-muted);
    text-align: center;
  }

  .tp-messages__list {
    display: grid;
    gap: var(--tp-space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-messages__item {
    display: grid;
    gap: 2px;
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-control);
    background: var(--tp-surface-raised, var(--tp-surface));
  }

  .tp-messages__time,
  .tp-messages__id {
    font-family: 'ShureTechMono Nerd Font Propo', 'Share Tech Mono', ui-monospace, monospace;
    font-size: var(--tp-font-code);
    color: var(--tp-text-muted);
  }

  .tp-messages__reply {
    font-size: var(--tp-font-caption);
    color: var(--tp-text-muted);
  }

  .tp-messages__body {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .tp-messages__id {
    opacity: 0.75;
  }
</style>
