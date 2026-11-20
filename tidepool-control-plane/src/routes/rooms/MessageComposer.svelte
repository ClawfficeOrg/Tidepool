<script lang="ts">
  /**
   * The message composer.
   *
   * A viewer's composer is withdrawn with the reason instead of accepting a
   * message the pool would refuse. A send failure keeps the text in place so it
   * can be retried. The page owns the controller call; this component owns only
   * the draft.
   */
  import ProblemDetails from '../../components/ProblemDetails.svelte';
  import FormField from '../../components/FormField.svelte';
  import { VIEWER_CANNOT_POST, type RoomsViewState } from '$lib/rooms';
  import type { PoolProblem } from '$lib/api/problem';

  interface Props {
    /** True when the pool will accept a message from this member. */
    canPost?: boolean;
    /** Where the last send is. */
    sendPhase?: RoomsViewState['sendPhase'];
    /** A failure from the last send. */
    problem?: PoolProblem | null;
    /** Called with the trimmed body. */
    onSend?: (body: string) => void;
  }

  let { canPost = true, sendPhase = 'idle', problem = null, onSend }: Props = $props();

  let draft = $state('');

  /** Submit the draft, if it is not blank. */
  function submit(event: SubmitEvent): void {
    event.preventDefault();
    const body = draft.trim();
    if (body === '' || !canPost) return;
    onSend?.(body);
    draft = '';
  }
</script>

<form class="tp-composer" data-composer="true" onsubmit={submit} data-form="room_message">
  {#if !canPost}
    <p class="tp-composer__denial" data-composer-denial="viewer">{VIEWER_CANNOT_POST}</p>
  {/if}
  <FormField form="room_message" field="body" label="Message" required>
    {#snippet children(id, described)}
      <textarea
        class="tp-field__control"
        {id}
        rows="3"
        aria-describedby={described}
        bind:value={draft}
        data-composer-field="body"
        placeholder={canPost ? 'Write a message to the room' : 'Read-only room'}
        disabled={!canPost}
      ></textarea>
    {/snippet}
  </FormField>
  <div class="tp-field__actions">
    {#if sendPhase === 'sent'}
      <p class="tp-composer__sent" data-composer-sent="true" role="status">Message sent.</p>
    {/if}
    <button
      class="tp-button"
      type="submit"
      data-rooms-action="send"
      disabled={!canPost || draft.trim() === '' || sendPhase === 'sending'}
    >
      {sendPhase === 'sending' ? 'Sending…' : 'Send message'}
    </button>
  </div>
  {#if problem}
    <ProblemDetails {problem} />
  {/if}
</form>

<style>
  .tp-composer {
    display: grid;
    gap: var(--tp-space-2);
  }

  .tp-composer__denial,
  .tp-composer__sent {
    margin: 0;
    color: var(--tp-text-muted);
  }
</style>
