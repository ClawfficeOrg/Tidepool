<script lang="ts">
  /**
   * The connections page.
   *
   * Every provider connection the pool will receive events from, with the
   * exact callback URL for this pool, the credential shown only as set/not set,
   * and a verification action whose failure names the failing check. The page
   * renders from a `ConnectionsViewState`, so every state can be asserted
   * without a browser, and it loads itself from the shell's pool and token when
   * no state is supplied.
   */
  import ProblemDetails from '../../components/ProblemDetails.svelte';
  import StatusBadge from '../../components/StatusBadge.svelte';
  import ConnectionCard from './ConnectionCard.svelte';
  import ConnectionForm from './ConnectionForm.svelte';
  import OriginChangeNotice from './OriginChangeNotice.svelte';
  import { canUseControl, controlById, denialReason } from '../../auth/roles';
  import type { Role } from '../../auth/types';
  import { isPoolConfigured, poolClient, type PoolConfig } from '$lib/api/pool';
  import {
    ConnectionsController,
    callbackChangeSummary,
    callbackChanges,
    canLoadConnections,
    connectionCounts,
    connectionSummary,
    connectionsTransport,
    emptyConnectionsState,
    poolOrigin,
    previousPoolOrigin,
    rememberPoolOrigin,
    type ConnectionStorage,
    type ConnectionTransport,
    type ConnectionsTransport,
    type ConnectionsViewState,
    type NewConnectionInput,
  } from '$lib/connections';

  interface Props {
    /** The pool this control plane is pointed at. */
    pool?: PoolConfig;
    /** The signed-in operator's token, used to load connections. */
    token?: string | null;
    /** The signed-in operator's role, for control gating. */
    role?: Role | null;
    /** The state to render before loading; also how tests render any state. */
    initial?: ConnectionsViewState;
    /** Transport override, for tests; otherwise built from the pool and token. */
    transport?: ConnectionsTransport | null;
    /** The origin the page last showed, for the change banner. */
    previousPoolUrl?: string | null;
  }

  let {
    pool = { baseUrl: null },
    token = null,
    role = null,
    initial = emptyConnectionsState(),
    transport = null,
    previousPoolUrl = null,
  }: Props = $props();

  /** localStorage when the page is in a browser; null during server render. */
  function browserStorage(): ConnectionStorage | null {
    if (typeof window === 'undefined') return null;
    try {
      return window.localStorage;
    } catch {
      return null;
    }
  }

  let view = $state<ConnectionsViewState>(initial);

  /** Build the controller once: the page keeps one view of one pool. */
  const controller: ConnectionsController | null = (() => {
    if (transport) {
      return new ConnectionsController({
        transport,
        poolUrl: pool.baseUrl ?? '',
      });
    }
    if (!canLoadConnections(pool, token)) return null;
    try {
      return new ConnectionsController({
        transport: connectionsTransport(poolClient(pool, { token })),
        poolUrl: pool.baseUrl ?? '',
        storage: browserStorage(),
      });
    } catch {
      return null;
    }
  })();

  const configured = transport !== null || isPoolConfigured(pool);
  const createControl = controlById('create-connection');
  const canCreate = $derived(createControl !== null && canUseControl(role, createControl));
  const createDenial = $derived(
    createControl !== null && !canCreate ? denialReason(role, createControl) : null,
  );
  const summary = $derived(connectionSummary(connectionCounts(view.connections)));

  let previousOrigin = $state<string | null>(previousPoolUrl ?? null);

  $effect(() => {
    // In a browser, remember the origin shown and compare it with the previous
    // one, so moving the pool's public address lists the URLs that must change.
    if (previousPoolUrl !== null) return;
    const storage = browserStorage();
    if (!storage) return;
    previousOrigin = previousPoolOrigin(storage, pool.baseUrl);
    rememberPoolOrigin(storage, pool.baseUrl);
  });

  const changes = $derived(
    previousOrigin ? callbackChanges(previousOrigin, pool.baseUrl, view.connections) : [],
  );
  const changeSummary = $derived(callbackChangeSummary(previousOrigin, pool.baseUrl));

  /** Adopt a controller result. */
  function adopt(next: ConnectionsViewState): void {
    view = next;
  }

  /** Load the list, on mount and on retry. */
  async function refresh(): Promise<void> {
    if (!controller) return;
    adopt(await controller.load());
  }

  $effect(() => {
    if (controller && initial.phase === 'idle' && controller.snapshot().phase === 'idle') {
      void refresh();
    }
  });

  /** Create a connection and verify it immediately. */
  async function create(input: NewConnectionInput): Promise<void> {
    if (!controller) return;
    adopt(await controller.create(input));
  }

  /** Verify one connection. */
  async function verify(id: string): Promise<void> {
    if (!controller) return;
    adopt(await controller.verify(id));
  }

  /** Rename one connection. */
  async function rename(id: string, displayName: string): Promise<void> {
    if (!controller) return;
    adopt(await controller.rename(id, displayName));
  }

  /** Enable or disable one connection. */
  async function toggle(id: string, enabled: boolean): Promise<void> {
    if (!controller) return;
    adopt(await controller.setStatus(id, enabled ? 'active' : 'disabled'));
  }

  /** Delete one connection after the card's confirm step. */
  async function remove(id: string): Promise<void> {
    if (!controller) return;
    adopt(await controller.remove(id));
  }

  /** Replace a write-only credential. */
  async function replace(id: string, credentialRef: string): Promise<void> {
    if (!controller) return;
    adopt(await controller.replaceCredential(id, credentialRef));
  }

  /** Remember a transport preference for one connection. */
  function rememberTransport(id: string, next: ConnectionTransport): void {
    if (!controller) return;
    adopt(controller.rememberTransport(id, next));
  }

  /** The verification report for a row, when it is that row's. */
  function reportFor(id: string) {
    return view.verification?.connectionId === id ? view.verification : null;
  }
</script>

<section class="tp-connections tp-page" aria-labelledby="tp-connections-title" data-connections-page="true">
  <h1 class="tp-page__title" id="tp-connections-title">Connections</h1>
  <p class="tp-page__lede">
    The provider accounts this pool receives events from. GitHub and Slack
    webhooks print the exact callback URL for the pool’s current origin; Slack
    socket mode and the Discord bot gateway dial out instead. Credentials are
    write-only: the page can show that one is set, never what it is.
  </p>

  {#if view.problem}
    <ProblemDetails problem={view.problem} onRetry={() => void refresh()} />
  {/if}

  {#if !configured}
    <p class="tp-page__state" data-connections-state="unconfigured">
      <StatusBadge kind="pending" />
      <span>
        No pool is configured, so there are no connections to show. Point the
        control plane at a pool and sign in.
      </span>
    </p>
  {:else}
    {#if previousOrigin && previousOrigin !== poolOrigin(pool.baseUrl)}
      <OriginChangeNotice
        previousUrl={previousOrigin}
        currentUrl={poolOrigin(pool.baseUrl) ?? ''}
        {changes}
        summary={changeSummary}
      />
    {/if}

    {#if createControl}
      {#if canCreate}
        <ConnectionForm poolUrl={pool.baseUrl} busy={view.busy} onSubmit={(input) => void create(input)} />
      {:else}
        <p class="tp-connections__denial" data-connections-denial="create-connection">{createDenial}</p>
      {/if}
    {/if}

    {#if view.connections.length === 0 && (view.phase === 'idle' || view.phase === 'loading')}
      <p class="tp-page__state" data-connections-state="loading">
        <StatusBadge kind="pending" />
        <span>Loading the connections configured on this pool…</span>
      </p>
    {:else if view.connections.length === 0 && view.phase !== 'error'}
      <p class="tp-page__state" data-connections-state="empty">
        <StatusBadge kind="neutral" />
        <span>
          No connections are configured yet. Add GitHub, Slack or Discord above;
          each one appears with its callback URL and credential state.
        </span>
      </p>
    {:else if view.connections.length > 0}
      <p class="tp-connections__summary" data-connection-summary="true">{summary}</p>
      <ul class="tp-connections__list">
        {#each view.connections as row (row.id)}
          <li class="tp-connections__item">
            <ConnectionCard
              {row}
              {role}
              busy={view.busyConnectionId === row.id}
              verifying={view.verifyingId === row.id}
              report={reportFor(row.id)}
              onVerify={() => void verify(row.id)}
              onRename={(name) => void rename(row.id, name)}
              onToggleStatus={(enabled) => void toggle(row.id, enabled)}
              onDelete={() => void remove(row.id)}
              onReplace={(reference) => void replace(row.id, reference)}
              onTransport={(next) => rememberTransport(row.id, next)}
            />
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  .tp-connections {
    display: grid;
    gap: var(--tp-space-3);
  }

  .tp-connections__summary {
    margin: 0;
    color: var(--tp-text-muted);
  }

  .tp-connections__denial {
    margin: 0;
    max-width: 68ch;
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    color: var(--tp-text-muted);
  }

  .tp-connections__list {
    display: grid;
    gap: var(--tp-space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-connections__item {
    display: block;
  }
</style>
