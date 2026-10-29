<script lang="ts">
  /**
   * The write-only credential field.
   *
   * The component is never given a stored value, only whether one is set. When
   * a credential exists the only control is *replace*; the input is empty and
   * hidden until replacement is chosen, and it is cleared the moment the form
   * is submitted. There is no "show" control and no prop that could render the
   * pool's secret back to the screen.
   */
  interface Props {
    /** Whether the pool reports a credential for this connection. */
    configured: boolean;
    /** True while a request for this connection is in flight. */
    busy?: boolean;
    /** Start in the replace form, for tests and resumed flows. */
    initialEditing?: boolean;
    /** Unique input id, so a list of cards never duplicates one. */
    fieldId?: string;
    /** Called with the new credential reference. */
    onReplace?: (credentialRef: string) => void;
  }

  let {
    configured,
    busy = false,
    initialEditing = false,
    fieldId = 'tp-secret-input',
    onReplace,
  }: Props = $props();

  let editing = $state(initialEditing);
  let value = $state('');

  /** Open the replacement form, always empty. */
  function startReplace(): void {
    value = '';
    editing = true;
  }

  /** Close without sending anything. */
  function cancel(): void {
    value = '';
    editing = false;
  }

  /** Hand the new reference to the page, then forget it locally. */
  function submit(event: SubmitEvent): void {
    event.preventDefault();
    const next = value;
    value = '';
    editing = false;
    onReplace?.(next);
  }
</script>

<div class="tp-secret" data-secret-state={configured ? 'set' : 'unset'} data-credential-field={fieldId}>
  <div class="tp-secret__status">
    <span class="tp-secret__word">
      {configured ? 'Credential set' : 'No credential set'}
    </span>
    <span class="tp-secret__description">
      {configured
        ? 'The pool holds a credential for this connection and never sends it back.'
        : 'The pool will not accept events until a credential is configured.'}
    </span>
  </div>

  {#if editing}
    <form class="tp-secret__form" onsubmit={submit}>
      <label class="tp-secret__label" for={fieldId}>
        {configured ? 'New credential reference' : 'Credential reference'}
      </label>
      <input
        class="tp-secret__input"
        id={fieldId}
        name="credential_ref"
        type="password"
        autocomplete="off"
        spellcheck="false"
        placeholder={configured ? 'Enter a replacement reference' : 'e.g. SLACK_ACME_CREDS'}
        bind:value
        disabled={busy}
      />
      <p class="tp-secret__help">
        The value is write-only: it is sent to the pool and never read back.
      </p>
      <div class="tp-secret__actions">
        <button class="tp-button" type="submit" disabled={busy || value.trim() === ''}>
          {configured ? 'Save replacement' : 'Set credential'}
        </button>
        <button class="tp-button tp-button--quiet" type="button" onclick={cancel} disabled={busy}>
          Cancel
        </button>
      </div>
    </form>
  {:else}
    <button
      class="tp-button tp-button--quiet"
      type="button"
      data-secret-action={configured ? 'replace' : 'set'}
      onclick={startReplace}
      disabled={busy}
    >
      {configured ? 'Replace credential' : 'Set credential'}
    </button>
  {/if}
</div>

<style>
  .tp-secret {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-secret__status {
    display: grid;
    gap: 2px;
  }

  .tp-secret__word {
    color: var(--tp-text-strong);
    font-weight: 600;
  }

  .tp-secret__description {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    max-width: 62ch;
  }

  .tp-secret__form {
    display: grid;
    gap: var(--tp-space-1);
    max-width: 42ch;
  }

  .tp-secret__label {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .tp-secret__input {
    padding: var(--tp-space-1) var(--tp-space-2);
    color: var(--tp-text-strong);
    background: var(--tp-window-background);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
  }

  .tp-secret__help {
    margin: 0;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-secret__actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-1);
  }
</style>
