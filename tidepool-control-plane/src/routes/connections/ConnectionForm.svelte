<script lang="ts">
  /**
   * The create form.
   *
   * Provider first, because it decides the transport choices, the credential
   * fields and the resource identity. Slack gets both webhook and socket mode;
   * socket mode is offered explicitly for a pool with no public address, and
   * says that no callback URL will be needed. Discord configures the bot
   * gateway and has no inbound URL at all.
   *
   * The credential input is write-only: the form submits a reference to the
   * pool's secret store, never a value the page can read back.
   */
  import {
    CONNECTION_PROVIDERS,
    CONNECTION_PROVIDERS_CATALOG,
    defaultTransport,
    describeProvider,
    describeTransport,
    isConnectionProvider,
    transportsFor,
    type ConnectionProvider,
    type ConnectionTransport,
    type NewConnectionInput,
    type ResourceInput,
  } from '$lib/connections';

  interface Props {
    /** True while a create is in flight. */
    busy?: boolean;
    /** Absolute pool URL, shown so the callback can be previewed. */
    poolUrl?: string | null;
    /** Initial provider, for tests and resumed flows. */
    initialProvider?: ConnectionProvider;
    /** Initial transport, for tests and resumed flows. */
    initialTransport?: ConnectionTransport;
    /** Called with the completed form. */
    onSubmit?: (input: NewConnectionInput) => void;
  }

  let {
    busy = false,
    poolUrl = null,
    initialProvider = 'github',
    initialTransport,
    onSubmit,
  }: Props = $props();

  let provider = $state<ConnectionProvider>(initialProvider);
  let transport = $state<ConnectionTransport>(
    initialTransport && describeTransport(initialTransport)
      ? initialTransport
      : defaultTransport(initialProvider),
  );
  let slug = $state('');
  let displayName = $state('');
  let credentialRef = $state('');
  let resources = $state<ResourceInput[]>([]);

  const descriptor = $derived(CONNECTION_PROVIDERS_CATALOG[provider]);
  const transportChoices = $derived(transportsFor(provider));
  const chosenTransport = $derived(describeTransport(transport));

  /** The default resource kind for the current provider. */
  function defaultKind(current: ConnectionProvider): string {
    const kinds = CONNECTION_PROVIDERS_CATALOG[current].resourceKinds;
    return kinds.find((kind) => kind !== 'channel') ?? kinds[0] ?? '';
  }

  /** Reset the form's provider-dependent choices. */
  function selectProvider(next: string): void {
    if (!isConnectionProvider(next)) return;
    provider = next;
    transport = defaultTransport(next);
    resources = [];
  }

  /** Keep the transport consistent with its provider. */
  function selectTransport(next: string): void {
    if (describeTransport(next)) transport = next as ConnectionTransport;
  }

  /** Add one empty resource row. */
  function addResource(): void {
    resources = [...resources, { kind: defaultKind(provider), externalId: '', displayName: '' }];
  }

  /** Remove one resource row. */
  function removeResource(index: number): void {
    resources = resources.filter((_, current) => current !== index);
  }

  /** Submit the form. Validation lives in `$lib/connections`. */
  function submit(event: SubmitEvent): void {
    event.preventDefault();
    onSubmit?.({
      provider,
      transport,
      slug,
      displayName,
      credentialRef,
      resources,
    });
  }
</script>

<section class="tp-connection-form" data-connection-form="true" aria-labelledby="tp-connection-form-title">
  <h2 class="tp-connection-form__title" id="tp-connection-form-title">Add a connection</h2>
  <p class="tp-connection-form__lede">
    A connection names a provider account the pool may receive events from.
    Credentials live in the pool’s secret store; this form only names the
    reference, and the value is never shown again.
  </p>

  <form class="tp-connection-form__body" onsubmit={submit}>
    <div class="tp-connection-form__grid">
      <div class="tp-field">
        <label class="tp-field__label" for="tp-connection-provider">Provider</label>
        <select
          class="tp-field__input"
          id="tp-connection-provider"
          name="provider"
          onchange={(event) => selectProvider(event.currentTarget.value)}
          disabled={busy}
        >
          {#each CONNECTION_PROVIDERS as choice (choice)}
            <option value={choice} selected={choice === provider}>
              {CONNECTION_PROVIDERS_CATALOG[choice].label}
            </option>
          {/each}
        </select>
        <p class="tp-field__help">{descriptor.summary}</p>
      </div>

      <div class="tp-field">
        <label class="tp-field__label" for="tp-connection-transport">Transport</label>
        <select
          class="tp-field__input"
          id="tp-connection-transport"
          name="transport"
          onchange={(event) => selectTransport(event.currentTarget.value)}
          disabled={busy}
        >
          {#each transportChoices as choice (choice.id)}
            <option value={choice.id} selected={choice.id === transport}>{choice.label}</option>
          {/each}
        </select>
        {#if chosenTransport}
          <p class="tp-field__help" data-transport-help={chosenTransport.id}>
            {chosenTransport.description}
          </p>
          {#if chosenTransport.id === 'slack_socket'}
            <p class="tp-field__help" data-socket-mode="true">
              Choose socket mode when this pool has no public address: the pool
              dials Slack, so no callback URL is needed.
            </p>
          {:else if chosenTransport.id === 'discord_gateway'}
            <p class="tp-field__help" data-gateway="true">
              Discord configures the bot gateway: the pool dials out with
              bot_token, so there is no callback URL to paste.
            </p>
          {:else if chosenTransport.callbackTemplate}
            <p class="tp-field__help" data-callback-preview={`${poolUrl ?? ''}${chosenTransport.callbackTemplate.replace('{slug}', slug.trim() || '{slug}')}`}>
              Callback URL: {poolUrl ?? 'the pool origin'}
              {chosenTransport.callbackTemplate.replace('{slug}', slug.trim() || '{slug}')}
            </p>
          {/if}
        {/if}
      </div>

      <div class="tp-field">
        <label class="tp-field__label" for="tp-connection-slug">Slug</label>
        <input
          class="tp-field__input"
          id="tp-connection-slug"
          name="slug"
          type="text"
          autocomplete="off"
          placeholder="acme"
          bind:value={slug}
          disabled={busy}
        />
        <p class="tp-field__help">
          The callback path segment; lowercased, with non-alphanumerics collapsed.
        </p>
      </div>

      <div class="tp-field">
        <label class="tp-field__label" for="tp-connection-display">Display name</label>
        <input
          class="tp-field__input"
          id="tp-connection-display"
          name="display_name"
          type="text"
          autocomplete="off"
          placeholder="Acme"
          bind:value={displayName}
          disabled={busy}
        />
      </div>

      <div class="tp-field tp-field--wide">
        <label class="tp-field__label" for="tp-connection-credential">
          Credential reference
        </label>
        <input
          class="tp-field__input"
          id="tp-connection-credential"
          name="credential_ref"
          type="password"
          autocomplete="off"
          spellcheck="false"
          placeholder="e.g. GITHUB_ACME_CREDS"
          bind:value={credentialRef}
          disabled={busy}
        />
        <p class="tp-field__help" data-credential-write-only="true">
          The name of the secret holding {descriptor.label}’s
          {descriptor.credentialFields.map((field) => field.key).join(', ')}.
          Write-only: the pool never sends it back.
        </p>
      </div>
    </div>

    <fieldset class="tp-connection-form__resources">
      <legend>Resources</legend>
      <p class="tp-field__help">
        The {descriptor.label} identities this connection delivers for
        ({descriptor.resourceKinds.join(', ')}). Leave empty to add them later.
      </p>
      {#each resources as resource, index (index)}
        <div class="tp-connection-form__resource">
          <label class="tp-field">
            <span class="tp-field__label">Resource kind</span>
            <select
              class="tp-field__input"
              value={resource.kind}
              onchange={(event) => {
                const kind = event.currentTarget.value;
                resource.kind = kind;
              }}
              disabled={busy}
            >
              {#each descriptor.resourceKinds as kind (kind)}
                <option value={kind} selected={kind === resource.kind}>{kind}</option>
              {/each}
            </select>
          </label>
          <label class="tp-field">
            <span class="tp-field__label">External id</span>
            <input
              class="tp-field__input"
              type="text"
              autocomplete="off"
              placeholder={resource.kind === 'repository' ? 'acme/app' : 'provider id'}
              bind:value={resource.externalId}
              disabled={busy}
            />
          </label>
          <button
            class="tp-button tp-button--quiet"
            type="button"
            onclick={() => removeResource(index)}
            disabled={busy}
          >
            Remove resource
          </button>
        </div>
      {/each}
      <button class="tp-button tp-button--quiet" type="button" onclick={addResource} disabled={busy}>
        Add resource
      </button>
    </fieldset>

    <button class="tp-button" type="submit" disabled={busy}>Add connection</button>
  </form>
</section>

<style>
  .tp-connection-form {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-3);
    background: var(--tp-surface);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-connection-form__title {
    margin: 0;
    color: var(--tp-text-strong);
    font-size: var(--tp-font-heading);
  }

  .tp-connection-form__lede {
    margin: 0;
    max-width: 78ch;
    color: var(--tp-text-muted);
  }

  .tp-connection-form__body {
    display: grid;
    gap: var(--tp-space-2);
  }

  .tp-connection-form__grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(15rem, 1fr));
    gap: var(--tp-space-2);
  }

  .tp-field {
    display: grid;
    gap: 2px;
  }

  .tp-field--wide {
    grid-column: 1 / -1;
  }

  .tp-field__label {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .tp-field__input {
    padding: var(--tp-space-1) var(--tp-space-2);
    color: var(--tp-text-strong);
    background: var(--tp-window-background);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
  }

  .tp-field__help {
    margin: 0;
    max-width: 68ch;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-connection-form__resources {
    display: grid;
    gap: var(--tp-space-1);
    margin: 0;
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-control);
  }

  .tp-connection-form__resource {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-1);
    align-items: flex-end;
  }
</style>
