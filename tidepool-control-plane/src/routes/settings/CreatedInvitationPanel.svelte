<script lang="ts">
  /**
   * The one-time invitation acceptance token.
   *
   * The hub returns the token exactly once and stores only its hash; v1 has no
   * mail transport, so the page shows the token in a copyable block with the
   * invitation's expiry. Copying it and sending it to the invitee is the next
   * action, and the panel says that in those words.
   */
  import type { CreatedInvitation } from '$lib/keys';
  import { copySecret } from '$lib/keys';

  interface Props {
    /** The invitation just created, with its one-time token. */
    created: CreatedInvitation;
    /** Called when the token has been copied and sent. */
    onDismiss?: () => void;
    /** Clipboard override, for tests. */
    clipboard?: { writeText(text: string): Promise<void> } | null;
  }

  let { created, onDismiss, clipboard = null }: Props = $props();

  let copied = $state(false);
  let copyFailed = $state(false);

  /** Copy the token exactly as the pool returned it. */
  async function copy(): Promise<void> {
    const writer =
      clipboard ??
      (typeof navigator !== 'undefined' && navigator.clipboard
        ? navigator.clipboard
        : null);
    const ok = await copySecret(created.token, writer);
    copied = ok;
    copyFailed = !ok;
  }
</script>

<section
  class="tp-invitation-result"
  aria-labelledby="tp-invitation-result-title"
  data-invitation-result="true"
>
  <h3 class="tp-invitation-result__title" id="tp-invitation-result-title">
    Invitation created: copy the acceptance token
  </h3>

  <p class="tp-invitation-result__state">
    <strong>{created.invitation.email}</strong> was invited as
    <strong>{created.invitation.roleLabel}</strong>.
    <span data-invitation-expiry>{created.invitation.expiry.text}</span>
  </p>

  <p class="tp-invitation-result__warning" role="note" data-invitation-token-warning>
    {created.warning}
  </p>

  <div class="tp-invitation-result__block">
    <code class="tp-invitation-result__value" data-invitation-token>{created.token}</code>
  </div>

  <div class="tp-invitation-result__buttons">
    <button class="tp-button" type="button" data-copy-invitation onclick={() => void copy()}>
      {copied ? 'Copied' : 'Copy acceptance token'}
    </button>
    <button class="tp-button tp-button--quiet" type="button" onclick={() => onDismiss?.()}>
      I have sent the token
    </button>
  </div>

  {#if copyFailed}
    <p class="tp-invitation-result__copy-error" role="alert">
      The browser would not let this page write to the clipboard. Select the
      token above and copy it by hand; it will not be shown again.
    </p>
  {/if}

  {#if created.invitation.expiresAt}
    <p class="tp-invitation-result__expiry">
      Expires at
      <time datetime={created.invitation.expiresAt}>{created.invitation.expiresAt}</time>.
    </p>
  {/if}
</section>

<style>
  .tp-invitation-result {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-3);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-panel);
    background: var(--tp-terminal-background);
  }

  .tp-invitation-result__title {
    margin: 0;
    font-size: var(--tp-font-heading);
    color: var(--tp-text-strong);
  }

  .tp-invitation-result__state,
  .tp-invitation-result__warning,
  .tp-invitation-result__expiry,
  .tp-invitation-result__copy-error {
    margin: 0;
    max-width: 68ch;
  }

  .tp-invitation-result__warning {
    color: var(--tp-text-strong);
    border-left: 3px solid var(--tp-status-warning);
    padding-left: var(--tp-space-2);
  }

  .tp-invitation-result__copy-error {
    color: var(--tp-text-strong);
    border-left: 3px solid var(--tp-status-danger);
    padding-left: var(--tp-space-2);
  }

  .tp-invitation-result__block {
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    background: var(--tp-window-background);
  }

  .tp-invitation-result__value {
    display: block;
    overflow-wrap: anywhere;
    color: var(--tp-text-strong);
    font-family: 'ShureTechMono Nerd Font Propo', 'Share Tech Mono', ui-monospace, monospace;
    font-size: var(--tp-font-code);
    user-select: all;
  }

  .tp-invitation-result__buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-1);
  }

  .tp-button--quiet {
    background: transparent;
    color: var(--tp-text-strong);
  }
</style>
