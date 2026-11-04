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
    validateConnectionInput,
    type ConnectionProvider,
    type ConnectionTransport,
    type NewConnectionInput,
    type ResourceInput,
  } from '$lib/connections';
  import FormField from '../../components/FormField.svelte';

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
  /** The one validation failure, tied to the field it names. */
  let failure = $state<{ field: string; message: string } | null>(null);

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
    if (busy) return;
    failure = null;
    const validation = validateConnectionInput({
      provider,
      transport,
      slug,
      displayName,
      credentialRef,
      resources,
    });
    if (!validation.ok) {
      failure = { field: validation.field, message: validation.message };
      return;
    }
    onSubmit?.(validation.input);
  }
</script>

<section class="tp-connection-form" data-connection-form="true" aria-labelledby="tp-connection-form-title">
  <h2 class="tp-connection-form__title" id="tp-connection-form-title">Add a connection</h2>
  <p class="tp-connection-form__lede">
    A connection names a provider account the pool may receive events from.
    Credentials live in the pool’s secret store; this form only names the
    reference, and the value is never shown again.
  </p>

  <form class="tp-connection-form__body" onsubmit={submit} data-form="connection">
    <div class="tp-connection-form__grid">
      <FormField
        form="connection"
        field="provider"
        label="Provider"
        required
        id="tp-connection-provider"
        help={descriptor.summary}
        error={failure?.field === 'provider' ? failure.message : null}
      >
        {#snippet children(id, described)}
          <select
            class="tp-field__control"
            {id}
            name="provider"
            aria-invalid={failure?.field === 'provider'}
            aria-describedby={described}
            onchange={(event) => selectProvider(event.currentTarget.value)}
            disabled={busy}
          >
            {#each CONNECTION_PROVIDERS as choice (choice)}
              <option value={choice} selected={choice === provider}>
                {CONNECTION_PROVIDERS_CATALOG[choice].label}
              </option>
            {/each}
          </select>
        {/snippet}
      </FormField>

      <FormField
        form="connection"
        field="transport"
        label="Transport"
        required
        id="tp-connection-transport"
        help={null}
        error={failure?.field === 'transport' ? failure.message : null}
      >
        {#snippet children(id, described)}
          <select
            class="tp-field__control"
            {id}
            name="transport"
            aria-invalid={failure?.field === 'transport'}
            aria-describedby={described}
            onchange={(event) => selectTransport(event.currentTarget.value)}
            disabled={busy}
          >
            {#each transportChoices as choice (choice.id)}
              <option value={choice.id} selected={choice.id === transport}>{choice.label}</option>
            {/each}
          </select>
        {/snippet}
        {#snippet extra()}
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
              <p
                class="tp-field__help"
                data-callback-preview={`${poolUrl ?? ''}${chosenTransport.callbackTemplate.replace('{slug}', slug.trim() || '{slug}')}`}
              >
                Callback URL: {poolUrl ?? 'the pool origin'}
                {chosenTransport.callbackTemplate.replace('{slug}', slug.trim() || '{slug}')}
              </p>
            {/if}
          {/if}
        {/snippet}
      </FormField>

      <FormField
        form="connection"
        field="slug"
        label="Slug"
        required
        id="tp-connection-slug"
        help="The callback path segment; lowercased, with non-alphanumerics collapsed."
        error={failure?.field === 'slug' ? failure.message : null}
      >
        {#snippet children(id, described)}
          <input
            class="tp-field__control"
            {id}
            name="slug"
            type="text"
            autocomplete="off"
            placeholder="acme"
            aria-invalid={failure?.field === 'slug'}
            aria-describedby={described}
            bind:value={slug}
            disabled={busy}
          />
        {/snippet}
      </FormField>

      <FormField
        form="connection"
        field="display_name"
        label="Display name"
        required
        id="tp-connection-display"
        error={failure?.field === 'display_name' ? failure.message : null}
      >
        {#snippet children(id, described)}
          <input
            class="tp-field__control"
            {id}
            name="display_name"
            type="text"
            autocomplete="off"
            placeholder="Acme"
            aria-invalid={failure?.field === 'display_name'}
            aria-describedby={described}
            bind:value={displayName}
            disabled={busy}
          />
        {/snippet}
      </FormField>

      <FormField
        form="connection"
        field="credential_ref"
        label="Credential reference"
        required
        wide
        id="tp-connection-credential"
        help={`The name of the secret holding ${descriptor.label}'s ${descriptor.credentialFields
          .map((field) => field.key)
          .join(', ')}. Write-only: the pool never sends it back.`}
        error={failure?.field === 'credential_ref' ? failure.message : null}
      >
        {#snippet children(id, described)}
          <input
            class="tp-field__control"
            {id}
            name="credential_ref"
            type="password"
            autocomplete="off"
            spellcheck="false"
            placeholder="e.g. GITHUB_ACME_CREDS"
            aria-invalid={failure?.field === 'credential_ref'}
            aria-describedby={described}
            data-credential-write-only="true"
            bind:value={credentialRef}
            disabled={busy}
          />
        {/snippet}
      </FormField>
    </div>

    <FormField
      form="connection"
      field="resources"
      label="Resources"
      wide
      group
      help={`The ${descriptor.label} identities this connection delivers for
        (${descriptor.resourceKinds.join(', ')}). Leave empty to add them later.`}
      error={failure?.field === 'resources' ? failure.message : null}
    >
      {#snippet children()}
        <div class="tp-connection-form__resources">
          {#each resources as resource, index (index)}
            <div class="tp-connection-form__resource">
              <FormField form="connection" field={`resource-${index}-kind`} label="Resource kind">
                {#snippet children(id, described)}
                  <select
                    class="tp-field__control"
                    {id}
                    value={resource.kind}
                    aria-describedby={described}
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
                {/snippet}
              </FormField>
              <FormField form="connection" field={`resource-${index}-external`} label="External id">
                {#snippet children(id, described)}
                  <input
                    class="tp-field__control"
                    {id}
                    type="text"
                    autocomplete="off"
                    placeholder={resource.kind === 'repository' ? 'acme/app' : 'provider id'}
                    aria-describedby={described}
                    bind:value={resource.externalId}
                    disabled={busy}
                  />
                {/snippet}
              </FormField>
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
        </div>
      {/snippet}
    </FormField>

    <div class="tp-field__actions">
      <button class="tp-button" type="submit" disabled={busy}>Add connection</button>
    </div>
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

  .tp-connection-form__resources {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-connection-form__resource {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-1);
    align-items: flex-end;
  }
</style>
