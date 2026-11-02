<script lang="ts">
  /**
   * One API key, as the settings page lists it.
   *
   * The row shows the prefix (never a secret), the scopes with their labels,
   * when the key was created, when it was last used, and its status. An active
   * key offers revocation with the consequence printed beside the button and a
   * second step to confirm; a revoked key stays in place, keeps its revocation
   * time, and offers no action — it is marked, not removed.
   */
  import ApiKeyStatusBadge from './ApiKeyStatusBadge.svelte';
  import type { ApiKeyRow } from '$lib/keys';
  import { REVOKE_KEY_CONSEQUENCE } from '$lib/keys';

  interface Props {
    /** The key to render. */
    row: ApiKeyRow;
    /** True while a request for this key is in flight. */
    busy?: boolean;
    /** Why the role cannot revoke it; renders the action as unavailable. */
    denial?: string | null;
    /** Start in the confirm step, for tests and resumed flows. */
    initialConfirming?: boolean;
    /** Revoke the key, after confirmation. */
    onRevoke?: () => void;
  }

  let {
    row,
    busy = false,
    denial = null,
    initialConfirming = false,
    onRevoke,
  }: Props = $props();

  const consequenceId = $derived(`tp-key-consequence-${row.id}`);
  const active = $derived(row.status.status === 'active');
  const confirmCopy = $derived(
    `Confirm: ${row.name} stops authenticating immediately. This cannot be undone.`,
  );

  // svelte-ignore state_referenced_locally
  let confirming = $state(initialConfirming);

  /** First click: restate what revoking does. */
  function ask(): void {
    if (busy || denial) return;
    confirming = true;
  }

  /** Second click: perform it. */
  function confirm(): void {
    if (busy || denial) return;
    confirming = false;
    onRevoke?.();
  }
</script>

<article
  class="tp-key"
  data-api-key-id={row.id}
  data-key-status={row.status.status ?? 'unknown'}
  aria-labelledby={`tp-key-${row.id}-name`}
>
  <header class="tp-key__header">
    <div class="tp-key__identity">
      <h3 class="tp-key__name" id={`tp-key-${row.id}-name`}>{row.name}</h3>
      <p class="tp-key__prefix">
        Prefix <code data-key-prefix>{row.prefix}</code>
      </p>
    </div>
    <ApiKeyStatusBadge status={row.status.status} />
  </header>

  <dl class="tp-key__facts">
    <div class="tp-key__fact">
      <dt>Scopes</dt>
      <dd>
        {#if row.scopes.length === 0}
          <span class="tp-key__note">No scopes: this key can do nothing.</span>
        {:else}
          <span class="tp-key__scopes">
            {#each row.scopes as scope (scope.scope)}
              <span class="tp-key__scope" data-key-scope={scope.scope}>{scope.label}</span>
            {/each}
          </span>
        {/if}
      </dd>
    </div>
    <div class="tp-key__fact">
      <dt>Created</dt>
      <dd>
        {#if row.createdAt}
          <time datetime={row.createdAt}>{row.createdAt}</time>
        {:else}
          Not reported
        {/if}
      </dd>
    </div>
    <div class="tp-key__fact">
      <dt>Last used</dt>
      <dd title={row.lastUsed.at ?? undefined} data-key-last-used>{row.lastUsed.text}</dd>
    </div>
    {#if row.revokedAt}
      <div class="tp-key__fact">
        <dt>Revoked at</dt>
        <dd data-key-revoked-at>
          <time datetime={row.revokedAt}>{row.revokedAt}</time>
        </dd>
      </div>
    {/if}
  </dl>

  {#if active}
    <div class="tp-key__action" data-confirming={confirming ? 'true' : 'false'}>
      <button
        class="tp-button tp-button--quiet"
        type="button"
        disabled={busy || Boolean(denial)}
        aria-expanded={confirming}
        aria-describedby={consequenceId}
        onclick={ask}
      >
        Revoke key
      </button>
      <p class="tp-key__consequence" id={consequenceId}>{REVOKE_KEY_CONSEQUENCE}</p>

      {#if denial}
        <p class="tp-key__denial">{denial}</p>
      {:else if confirming}
        <div class="tp-key__confirm" role="group" aria-label="Confirm revoke key">
          <p class="tp-key__confirm-copy">
            {confirmCopy}
          </p>
          <div class="tp-key__buttons">
            <button class="tp-button" type="button" disabled={busy} onclick={confirm}>
              Revoke key {row.name}
            </button>
            <button
              class="tp-button tp-button--quiet"
              type="button"
              onclick={() => {
                confirming = false;
              }}
            >
              Cancel
            </button>
          </div>
        </div>
      {/if}
    </div>
  {:else}
    <p class="tp-key__note" data-key-revoked-note="true">
      This key is revoked: it stays here for the audit trail and authenticates
      nothing. Create a new key if the automation still needs access.
    </p>
  {/if}
</article>

<style>
  .tp-key {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-3);
    background: var(--tp-surface);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-key[data-key-status='revoked'] {
    border-color: var(--tp-border-strong);
  }

  .tp-key__header {
    display: flex;
    flex-wrap: wrap;
    align-items: start;
    justify-content: space-between;
    gap: var(--tp-space-2);
  }

  .tp-key__name {
    margin: 0;
    font-size: var(--tp-font-heading);
    color: var(--tp-text-strong);
  }

  .tp-key__prefix {
    margin: 0;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-key__facts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-4);
    margin: 0;
  }

  .tp-key__fact {
    display: grid;
    gap: 2px;
  }

  .tp-key__fact dt {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .tp-key__fact dd {
    margin: 0;
  }

  .tp-key__scopes {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-1);
  }

  .tp-key__scope {
    padding: 0 var(--tp-space-1);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-control);
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-key__action {
    display: grid;
    gap: var(--tp-space-1);
    justify-items: start;
  }

  .tp-key__consequence,
  .tp-key__denial,
  .tp-key__confirm-copy,
  .tp-key__note {
    margin: 0;
    max-width: 68ch;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-key__denial {
    color: var(--tp-text-strong);
  }

  .tp-key__confirm {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-key__buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-1);
  }

  .tp-button--quiet {
    background: transparent;
    color: var(--tp-text-strong);
  }
</style>
