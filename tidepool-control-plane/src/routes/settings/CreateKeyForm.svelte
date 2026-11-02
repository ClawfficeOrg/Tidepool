<script lang="ts">
  /**
   * Create an API key with an explicit scope set.
   *
   * Every scope is a checkbox with a sentence saying what it permits, because a
   * key is exactly as powerful as the ticks here. Nothing is ticked in advance:
   * a default would hand out power nobody chose. The form validates locally so
   * an empty name or an empty scope set never becomes a round trip, and the
   * pool still validates the body.
   */
  import { API_KEY_SCOPES, validateKeyInput } from '$lib/keys';

  interface Props {
    /** True while a create is in flight. */
    busy?: boolean;
    /** Scopes to start with, for tests and resumed flows. */
    initialScopes?: readonly string[];
    /** Called with a valid form. */
    onSubmit?: (input: { name: string; scopes: string[] }) => void;
  }

  let { busy = false, initialScopes = [], onSubmit }: Props = $props();

  let name = $state('');
  // The initial scopes seed the form once; the person drives them afterwards.
  // svelte-ignore state_referenced_locally
  let scopes = $state<string[]>([...initialScopes]);
  let localError = $state<string | null>(null);

  /** Validate here first, so an empty form never becomes a request. */
  function submit(event: SubmitEvent): void {
    event.preventDefault();
    if (busy) return;
    localError = null;
    const validation = validateKeyInput({ name, scopes });
    if (!validation.ok) {
      localError = validation.message;
      return;
    }
    onSubmit?.(validation.input);
  }
</script>

<section class="tp-create-key" aria-labelledby="tp-create-key-title" data-create-key-form="true">
  <h3 class="tp-create-key__title" id="tp-create-key-title">Create a key</h3>
  <p class="tp-create-key__lede">
    A key acts for this organization with exactly the scopes you tick. The
    secret is shown once, immediately after it is created.
  </p>

  {#if localError}
    <p class="tp-create-key__error" role="alert">{localError}</p>
  {/if}

  <form class="tp-create-key__form" onsubmit={submit}>
    <label class="tp-create-key__field">
      <span class="tp-create-key__label">Key name</span>
      <input
        class="tp-create-key__input"
        type="text"
        name="key_name"
        autocomplete="off"
        spellcheck="false"
        required
        bind:value={name}
      />
      <span class="tp-create-key__hint">
        What this key is for, e.g. <code>nightly-config-sync</code>. Names are
        for people; the key itself is the prefix.
      </span>
    </label>

    <fieldset class="tp-create-key__scopes">
      <legend class="tp-create-key__label">Scopes</legend>
      {#each API_KEY_SCOPES as scope (scope.scope)}
        <label class="tp-create-key__scope">
          <input type="checkbox" value={scope.scope} bind:group={scopes} />
          <span>
            <span class="tp-create-key__scope-label">{scope.label}</span>
            <code class="tp-create-key__scope-name">{scope.scope}</code>
            <span class="tp-create-key__hint">{scope.description}</span>
          </span>
        </label>
      {/each}
    </fieldset>

    <button class="tp-button" type="submit" disabled={busy}>
      {busy ? 'Creating the key…' : 'Create the key'}
    </button>
  </form>
</section>

<style>
  .tp-create-key {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-3);
    background: var(--tp-surface);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-create-key__title {
    margin: 0;
    font-size: var(--tp-font-heading);
    color: var(--tp-text-strong);
  }

  .tp-create-key__lede,
  .tp-create-key__hint,
  .tp-create-key__error {
    margin: 0;
    max-width: 68ch;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-create-key__error {
    color: var(--tp-text-strong);
    border-left: 3px solid var(--tp-status-danger);
    padding-left: var(--tp-space-2);
  }

  .tp-create-key__form,
  .tp-create-key__field,
  .tp-create-key__scopes {
    display: grid;
    gap: var(--tp-space-1);
    margin: 0;
    padding: 0;
    border: 0;
  }

  .tp-create-key__label {
    padding: 0;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .tp-create-key__input {
    padding: var(--tp-space-1) var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    background: var(--tp-terminal-background);
    color: var(--tp-text-strong);
    font: inherit;
  }

  .tp-create-key__scope {
    display: flex;
    align-items: start;
    gap: var(--tp-space-1);
    padding: var(--tp-space-1);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-control);
  }

  .tp-create-key__scope-label {
    display: block;
    color: var(--tp-text-strong);
  }

  .tp-create-key__scope-name {
    display: block;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-code);
  }
</style>
