<script lang="ts">
  /**
   * The API keys panel.
   *
   * Keys are owner-only in the contract, so a non-owner is told why the panel
   * is unavailable instead of meeting a 403 after a click. An owner sees the
   * create form (or, right after creating, the one-time secret), then every
   * key — active first, revoked kept in place.
   */
  import ApiKeyCard from './ApiKeyCard.svelte';
  import ApiKeySecretPanel from './ApiKeySecretPanel.svelte';
  import CreateKeyForm from './CreateKeyForm.svelte';
  import { canUseControl, controlById, denialReason } from '../../auth/roles';
  import type { Role } from '../../auth/types';
  import type { ApiKeyRow, CreatedApiKey } from '$lib/keys';

  interface Props {
    /** Every key, active first and revoked kept. */
    keys?: ApiKeyRow[];
    /** The one-time secret of a key just created, until dismissed. */
    createdKey?: CreatedApiKey | null;
    /** The signed-in role, for control gating. */
    role?: Role | null;
    /** True while a request is in flight. */
    busy?: boolean;
    /** The key whose row is busy, when one is. */
    busyId?: string | null;
    /** One line describing the list, e.g. "2 active, 1 revoked key". */
    summary?: string;
    /** Create a key with the chosen scopes. */
    onCreate?: (input: { name: string; scopes: string[] }) => void;
    /** Revoke one key. */
    onRevoke?: (keyId: string) => void;
    /** Forget the one-time secret. */
    onDismissSecret?: () => void;
  }

  let {
    keys = [],
    createdKey = null,
    role = null,
    busy = false,
    busyId = null,
    summary = '',
    onCreate,
    onRevoke,
    onDismissSecret,
  }: Props = $props();

  const createControl = controlById('create-api-key');
  const revokeControl = controlById('revoke-api-key');
  const canCreate = $derived(createControl !== null && canUseControl(role, createControl));
  const denied = $derived(
    createControl !== null && !canCreate ? denialReason(role, createControl) : null,
  );
  const revokeDenial = $derived(
    revokeControl !== null && !canUseControl(role, revokeControl)
      ? denialReason(role, revokeControl)
      : null,
  );
</script>

<section class="tp-keys" aria-labelledby="tp-keys-title" data-keys-panel="true">
  <header class="tp-keys__header">
    <h2 class="tp-keys__title" id="tp-keys-title">API keys</h2>
    <p class="tp-keys__lede">
      Keys authenticate automation against this pool. The secret is shown once,
      when the key is created; the pool stores only its hash, so it can never be
      read back here.
    </p>
  </header>

  {#if !canCreate}
    <p class="tp-keys__denial" data-keys-denial="owner-only">{denied}</p>
  {:else}
    {#if summary}
      <p class="tp-keys__summary" data-keys-summary="true">{summary}</p>
    {/if}

    {#if createdKey}
      <ApiKeySecretPanel created={createdKey} onDismiss={onDismissSecret} />
    {:else}
      <CreateKeyForm {busy} onSubmit={onCreate} />
    {/if}

    {#if keys.length === 0}
      <p class="tp-page__state" data-keys-state="empty">
        <span>No API keys yet. Create one above when an automation needs access.</span>
      </p>
    {:else}
      <ul class="tp-keys__list">
        {#each keys as key (key.id)}
          <li class="tp-keys__item">
            <ApiKeyCard
              row={key}
              busy={busyId === key.id}
              denial={revokeDenial}
              onRevoke={() => onRevoke?.(key.id)}
            />
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  .tp-keys {
    display: grid;
    gap: var(--tp-space-2);
  }

  .tp-keys__header {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-keys__title {
    margin: 0;
    font-size: var(--tp-font-heading);
    color: var(--tp-text-strong);
  }

  .tp-keys__lede,
  .tp-keys__denial,
  .tp-keys__summary {
    margin: 0;
    max-width: 68ch;
    color: var(--tp-text-muted);
  }

  .tp-keys__denial {
    color: var(--tp-text-strong);
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
  }

  .tp-keys__list {
    display: grid;
    gap: var(--tp-space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-keys__item {
    display: block;
  }
</style>
