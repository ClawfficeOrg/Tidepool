<script lang="ts">
  /**
   * The one-time key secret.
   *
   * The pool returns `<prefix>.<secret>` exactly once, on creation. This panel
   * is the whole of that moment: the warning comes first, then the secret in a
   * copyable block, then a primary **Copy secret** button — copying is the
   * obvious next action, and the only one that keeps the key usable. The
   * dismiss control is secondary and says plainly that the secret is gone
   * afterwards.
   */
  import type { CreatedApiKey } from '$lib/keys';
  import { copySecret } from '$lib/keys';

  interface Props {
    /** The key that was just created, with its secret. */
    created: CreatedApiKey;
    /** Called when the person has copied the secret and is done. */
    onDismiss?: () => void;
    /** Clipboard override, for tests. */
    clipboard?: { writeText(text: string): Promise<void> } | null;
  }

  let { created, onDismiss, clipboard = null }: Props = $props();

  let copied = $state(false);
  let copyFailed = $state(false);

  /** Copy the secret exactly as the pool returned it. */
  async function copy(): Promise<void> {
    const writer =
      clipboard ??
      (typeof navigator !== 'undefined' && navigator.clipboard
        ? navigator.clipboard
        : null);
    const ok = await copySecret(created.secret, writer);
    copied = ok;
    copyFailed = !ok;
  }
</script>

<section
  class="tp-secret"
  aria-labelledby="tp-key-secret-title"
  data-key-secret-panel="true"
>
  <p class="tp-secret__state">
    <span class="tp-secret__badge">New key</span>
    <span><strong>{created.key.name}</strong> is active. Its secret is below.</span>
  </p>

  <h3 class="tp-secret__title" id="tp-key-secret-title">Copy this secret now</h3>

  <p class="tp-secret__warning" role="note" data-key-secret-warning>{created.warning}</p>
  <p class="tp-secret__next">{created.nextAction}</p>

  <div class="tp-secret__block" data-key-secret-copyable>
    <code class="tp-secret__value" data-key-secret>{created.secret}</code>
  </div>

  <div class="tp-secret__buttons">
    <button class="tp-button" type="button" data-copy-secret onclick={() => void copy()}>
      {copied ? 'Copied' : 'Copy secret'}
    </button>
    <button class="tp-button tp-button--quiet" type="button" onclick={() => onDismiss?.()}>
      I have copied the secret
    </button>
  </div>

  {#if copyFailed}
    <p class="tp-secret__copy-error" role="alert">
      The browser would not let this page write to the clipboard. Select the
      secret above and copy it by hand; it will not be shown again.
    </p>
  {/if}

  <dl class="tp-secret__facts">
    <div class="tp-secret__fact">
      <dt>Prefix</dt>
      <dd><code>{created.key.prefix}</code></dd>
    </div>
    <div class="tp-secret__fact">
      <dt>Scopes</dt>
      <dd>
        {#each created.key.scopes as scope (scope.scope)}
          <span class="tp-secret__scope">{scope.label}</span>
        {/each}
      </dd>
    </div>
    {#if created.key.createdAt}
      <div class="tp-secret__fact">
        <dt>Created</dt>
        <dd><time datetime={created.key.createdAt}>{created.key.createdAt}</time></dd>
      </div>
    {/if}
  </dl>
</section>

<style>
  .tp-secret {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-3);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-panel);
    background: var(--tp-terminal-background);
  }

  .tp-secret__state {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--tp-space-2);
    margin: 0;
  }

  .tp-secret__badge {
    padding: 0 var(--tp-space-1);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .tp-secret__title {
    margin: 0;
    font-size: var(--tp-font-heading);
    color: var(--tp-text-strong);
  }

  .tp-secret__warning,
  .tp-secret__next,
  .tp-secret__copy-error {
    margin: 0;
    max-width: 68ch;
  }

  .tp-secret__warning {
    color: var(--tp-text-strong);
    border-left: 3px solid var(--tp-status-warning);
    padding-left: var(--tp-space-2);
  }

  .tp-secret__next {
    color: var(--tp-text-muted);
  }

  .tp-secret__copy-error {
    color: var(--tp-text-strong);
    border-left: 3px solid var(--tp-status-danger);
    padding-left: var(--tp-space-2);
  }

  .tp-secret__block {
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    background: var(--tp-window-background);
  }

  .tp-secret__value {
    display: block;
    overflow-wrap: anywhere;
    color: var(--tp-text-strong);
    font-family: 'ShureTechMono Nerd Font Propo', 'Share Tech Mono', ui-monospace, monospace;
    font-size: var(--tp-font-code);
    user-select: all;
  }

  .tp-secret__buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-1);
  }

  .tp-secret__facts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-4);
    margin: 0;
    padding-top: var(--tp-space-2);
    border-top: 1px solid var(--tp-border);
  }

  .tp-secret__fact {
    display: grid;
    gap: 2px;
  }

  .tp-secret__fact dt {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .tp-secret__fact dd {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-1);
    margin: 0;
  }

  .tp-secret__scope {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-button--quiet {
    background: transparent;
    color: var(--tp-text-strong);
  }
</style>
