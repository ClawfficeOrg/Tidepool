<script lang="ts">
  /**
   * One connection, as the connections page lists it.
   *
   * The card answers: what provider account is this, how do its events arrive,
   * which exact URL goes at the provider, and is the credential set. The
   * credential is only ever shown as a boolean with a *replace* control. Rename,
   * enable/disable and delete are labelled actions; delete has a second,
   * connection-named confirm step because it stops deliveries immediately.
   */
  import StatusBadge from '../../components/StatusBadge.svelte';
  import VerificationReport from './VerificationReport.svelte';
  import SecretField from './SecretField.svelte';
  import ConnectionCallback from './ConnectionCallback.svelte';
  import { canUseControl, controlById, denialReason } from '../../auth/roles';
  import type { Role } from '../../auth/types';
  import {
    describeTransport,
    transportsFor,
    type ConnectionRow,
    type ConnectionTransport,
    type VerificationReport as VerificationReportType,
  } from '$lib/connections';

  interface Props {
    /** The connection to render. */
    row: ConnectionRow;
    /** The signed-in role, for control gating. */
    role?: Role | null;
    /** True while a request for this connection is in flight. */
    busy?: boolean;
    /** True while this connection is being verified. */
    verifying?: boolean;
    /** The last verification report for this connection, if any. */
    report?: VerificationReportType | null;
    /** Start in the rename form, for tests and resumed flows. */
    initialRenaming?: boolean;
    /** Start at the delete confirmation, for tests and resumed flows. */
    initialConfirmDelete?: boolean;
    /** Verify against the pool. */
    onVerify?: () => void;
    /** Rename the connection. */
    onRename?: (displayName: string) => void;
    /** Enable or disable the connection. */
    onToggleStatus?: (enabled: boolean) => void;
    /** Delete the connection after confirmation. */
    onDelete?: () => void;
    /** Replace the write-only credential. */
    onReplace?: (credentialRef: string) => void;
    /** Remember a different transport for this connection. */
    onTransport?: (transport: ConnectionTransport) => void;
  }

  let {
    row,
    role = null,
    busy = false,
    verifying = false,
    report = null,
    initialRenaming = false,
    initialConfirmDelete = false,
    onVerify,
    onRename,
    onToggleStatus,
    onDelete,
    onReplace,
    onTransport,
  }: Props = $props();

  const updateControl = controlById('update-connection');
  const deleteControl = controlById('delete-connection');
  const canUpdate = $derived(updateControl !== null && canUseControl(role, updateControl));
  const canDelete = $derived(deleteControl !== null && canUseControl(role, deleteControl));
  const updateDenial = $derived(
    updateControl !== null && !canUpdate ? denialReason(role, updateControl) : null,
  );
  const deleteDenial = $derived(
    deleteControl !== null && !canDelete ? denialReason(role, deleteControl) : null,
  );
  const transportChoices = $derived(row.provider ? transportsFor(row.provider.provider) : []);
  const transportDescriptor = $derived(describeTransport(row.transport));

  let renaming = $state(initialRenaming);
  let name = $state(row.displayName);
  let confirmingDelete = $state(initialConfirmDelete);

  /** Submit the rename form. */
  function submitRename(event: SubmitEvent): void {
    event.preventDefault();
    const next = name;
    renaming = false;
    onRename?.(next);
  }

  /** Open the rename form at the current name. */
  function startRename(): void {
    name = row.displayName;
    renaming = true;
  }

  /** Delete after the second step. */
  function confirmDelete(): void {
    confirmingDelete = false;
    onDelete?.();
  }

  /** Remember the chosen transport. */
  function selectTransport(next: string): void {
    const descriptor = describeTransport(next);
    if (descriptor) onTransport?.(descriptor.id);
  }
</script>

<article
  class="tp-connection"
  data-connection-id={row.id}
  data-connection-provider={row.providerId}
  data-connection-status={row.status.value ?? 'unknown'}
  data-connection-transport={row.transport}
  data-credential-state={row.credential.configured ? 'set' : 'unset'}
  aria-labelledby={`tp-connection-${row.id}-slug`}
>
  <header class="tp-connection__header">
    <div class="tp-connection__identity">
      <h2 class="tp-connection__slug" id={`tp-connection-${row.id}-slug`}>{row.slug}</h2>
      <p class="tp-connection__display">{row.displayName}</p>
    </div>
    <div class="tp-connection__badges">
      <StatusBadge kind={row.status.tone} />
      <span class="tp-connection__provider">{row.provider?.label ?? row.providerId}</span>
    </div>
  </header>

  <dl class="tp-connection__facts">
    <div class="tp-connection__fact">
      <dt>Provider</dt>
      <dd>{row.provider?.label ?? row.providerId}</dd>
    </div>
    <div class="tp-connection__fact">
      <dt>Slug</dt>
      <dd><code>{row.slug}</code></dd>
    </div>
    <div class="tp-connection__fact">
      <dt>Transport</dt>
      <dd data-transport-label={row.transport}>{transportDescriptor?.label ?? row.transport}</dd>
    </div>
    <div class="tp-connection__fact">
      <dt>Managed by</dt>
      <dd>
        {#if row.managedBy === 'database'}
          The pool database
        {:else if row.managedBy === 'environment'}
          TIDEPOOL_CONNECTIONS (read-only here)
        {:else}
          Unknown
        {/if}
      </dd>
    </div>
  </dl>

  {#if row.managedBy === 'environment'}
    <p class="tp-connection__note" data-managed-by="environment">
      This connection is supplied by the environment, so it is read-only in this
      page. Change <code>TIDEPOOL_CONNECTIONS</code> and restart the pool to
      edit or remove it.
    </p>
  {/if}

  <div class="tp-connection__credential">
    <h3 class="tp-connection__section-title">Credential</h3>
    <SecretField
      configured={row.credential.configured}
      busy={busy || !canUpdate || !row.editable}
      fieldId={`tp-connection-${row.id}-credential`}
      onReplace={(reference) => onReplace?.(reference)}
    />
  </div>

  <ConnectionCallback callback={row.callback} />

  {#if transportChoices.length > 1}
    <div class="tp-connection__transport">
      <label class="tp-connection__section-title" for={`tp-connection-${row.id}-transport`}>
        Transport
      </label>
      <select
        class="tp-connection__select"
        id={`tp-connection-${row.id}-transport`}
        value={row.transport}
        onchange={(event) => selectTransport(event.currentTarget.value)}
        disabled={busy}
      >
        {#each transportChoices as choice (choice.id)}
          <option value={choice.id} selected={choice.id === row.transport}>{choice.label}</option>
        {/each}
      </select>
      <p class="tp-connection__note">
        Remembered in this browser: this contract revision does not carry the
        transport on the connection, so the page cannot read it back from the
        pool.
      </p>
    </div>
  {/if}

  {#if row.resources.length > 0}
    <div class="tp-connection__resources">
      <h3 class="tp-connection__section-title">Resources</h3>
      <ul class="tp-connection__resource-list">
        {#each row.resources as resource (resource.kind + ':' + resource.externalId)}
          <li class="tp-connection__resource">
            <code>{resource.kind}</code>
            <span>{resource.externalId}</span>
            {#if resource.displayName}
              <span class="tp-connection__resource-name">{resource.displayName}</span>
            {/if}
          </li>
        {/each}
      </ul>
    </div>
  {:else}
    <p class="tp-connection__note" data-resources="none">
      No resources are registered, so no delivery can match this connection yet.
    </p>
  {/if}

  <div class="tp-connection__verify">
    <button class="tp-button" type="button" onclick={() => onVerify?.()} disabled={busy || verifying}>
      {verifying ? 'Verifying…' : 'Verify connection'}
    </button>
    {#if report}
      <VerificationReport report={report} />
    {/if}
  </div>

  <div class="tp-connection__actions">
    {#if renaming}
      <form class="tp-connection__rename" onsubmit={submitRename}>
        <label class="tp-connection__section-title" for={`tp-connection-${row.id}-name`}>
          New display name
        </label>
        <input
          class="tp-connection__input"
          id={`tp-connection-${row.id}-name`}
          type="text"
          autocomplete="off"
          bind:value={name}
          disabled={busy}
        />
        <button class="tp-button" type="submit" disabled={busy || name.trim() === ''}>
          Save name
        </button>
        <button
          class="tp-button tp-button--quiet"
          type="button"
          onclick={() => (renaming = false)}
          disabled={busy}
        >
          Cancel
        </button>
      </form>
    {:else}
      <button
        class="tp-button tp-button--quiet"
        type="button"
        data-connection-action="rename"
        onclick={startRename}
        disabled={busy || !canUpdate || !row.editable}
      >
        Rename
      </button>
    {/if}

    <button
      class="tp-button tp-button--quiet"
      type="button"
      data-connection-action={row.status.value === 'active' ? 'disable' : 'enable'}
      onclick={() => onToggleStatus?.(row.status.value !== 'active')}
      disabled={busy || !canUpdate || !row.editable}
    >
      {row.status.value === 'active' ? 'Disable' : 'Enable'}
    </button>

    {#if confirmingDelete}
      <div class="tp-connection__delete-confirm" role="alert">
        <p>
          Confirm: deleting <code>{row.slug}</code> stops {row.provider?.label ?? row.providerId}
          events for this connection immediately. This cannot be undone.
        </p>
        <button class="tp-button tp-button--danger" type="button" onclick={confirmDelete} disabled={busy}>
          Delete {row.slug}
        </button>
        <button
          class="tp-button tp-button--quiet"
          type="button"
          onclick={() => (confirmingDelete = false)}
          disabled={busy}
        >
          Keep connection
        </button>
      </div>
    {:else}
      <button
        class="tp-button tp-button--danger"
        type="button"
        data-connection-action="delete"
        onclick={() => (confirmingDelete = true)}
        disabled={busy || !canDelete || !row.editable}
      >
        Delete
      </button>
    {/if}
  </div>

  {#if (!canUpdate && updateDenial) || (!canDelete && deleteDenial)}
    <p class="tp-connection__note" data-connection-denial="true">
      {updateDenial ?? deleteDenial}
    </p>
  {/if}
</article>

<style>
  .tp-connection {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-3);
    background: var(--tp-surface);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-connection[data-connection-status='disabled'] {
    border-style: dashed;
  }

  .tp-connection__header {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--tp-space-2);
  }

  .tp-connection__identity {
    display: grid;
    gap: 2px;
  }

  .tp-connection__slug {
    margin: 0;
    color: var(--tp-text-strong);
    font-size: var(--tp-font-heading);
  }

  .tp-connection__display {
    margin: 0;
    color: var(--tp-text-muted);
  }

  .tp-connection__badges {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--tp-space-1);
  }

  .tp-connection__provider {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-connection__facts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-3);
    margin: 0;
    padding-top: var(--tp-space-2);
    border-top: 1px solid var(--tp-border);
  }

  .tp-connection__fact {
    display: grid;
    gap: 2px;
  }

  .tp-connection__fact dt,
  .tp-connection__section-title {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    font-weight: 400;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .tp-connection__fact dd {
    margin: 0;
  }

  .tp-connection__credential,
  .tp-connection__transport,
  .tp-connection__resources,
  .tp-connection__verify {
    display: grid;
    gap: var(--tp-space-1);
    padding-top: var(--tp-space-2);
    border-top: 1px solid var(--tp-border);
  }

  .tp-connection__section-title {
    margin: 0;
  }

  .tp-connection__select,
  .tp-connection__input {
    max-width: 42ch;
    padding: var(--tp-space-1) var(--tp-space-2);
    color: var(--tp-text-strong);
    background: var(--tp-window-background);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
  }

  .tp-connection__resource-list {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-connection__resource {
    display: flex;
    gap: var(--tp-space-1);
    padding: 2px var(--tp-space-1);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-chip);
  }

  .tp-connection__resource-name {
    color: var(--tp-text-muted);
  }

  .tp-connection__actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-1);
    align-items: flex-start;
    padding-top: var(--tp-space-2);
    border-top: 1px solid var(--tp-border);
  }

  .tp-connection__rename {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-connection__delete-confirm {
    display: grid;
    gap: var(--tp-space-1);
    max-width: 68ch;
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-status-danger);
    border-radius: var(--tp-radius-control);
  }

  .tp-connection__delete-confirm p {
    margin: 0;
  }

  .tp-connection__note {
    margin: 0;
    max-width: 68ch;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }
</style>
