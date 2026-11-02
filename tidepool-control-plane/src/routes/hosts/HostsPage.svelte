<script lang="ts">
  /**
   * The hosts page.
   *
   * Every host the pool has enrolled, in attention order, with its slug,
   * display name, status, last seen and whether it may run agents. Enrolling
   * mints a single-use token and prints the exact command for the machine; the
   * execution grant is a labelled action whose consequence is printed beside
   * it, with a confirm step before anything changes.
   *
   * The page renders from a `HostsViewState`. It loads itself on mount when it
   * has a pool and a session token (or an injected transport), and it can be
   * rendered from an initial state in tests, which is how every status and
   * both action states are asserted without a browser.
   */
  import ProblemDetails from '../../components/ProblemDetails.svelte';
  import StatusBadge from '../../components/StatusBadge.svelte';
  import EnrollHostPanel from './EnrollHostPanel.svelte';
  import HostCard from './HostCard.svelte';
  import { canUseControl, controlById, denialReason } from '../../auth/roles';
  import type { Role } from '../../auth/types';
  import { poolClient, isPoolConfigured, type PoolConfig } from '$lib/api/pool';
  import {
    HostsController,
    canLoadHosts,
    emptyHostsState,
    hostCounts,
    hostSummary,
    hostsTransport,
    type HostsTransport,
    type HostsViewState,
  } from '$lib/hosts';

  interface Props {
    /** The pool this control plane is pointed at. */
    pool?: PoolConfig;
    /** The signed-in operator's token, used to load the hosts. */
    token?: string | null;
    /** The signed-in operator's role, for control gating. */
    role?: Role | null;
    /** The state to render before loading; also how tests render any state. */
    initial?: HostsViewState;
    /** Transport override, for tests; otherwise built from the pool and token. */
    transport?: HostsTransport | null;
  }

  let {
    pool = { baseUrl: null },
    token = null,
    role = null,
    initial = emptyHostsState(),
    transport = null,
  }: Props = $props();

  let view = $state<HostsViewState>(initial);

  /** Build the controller once: the page keeps one view of one pool. */
  const controller: HostsController | null = (() => {
    if (transport) {
      return new HostsController({
        transport,
        poolUrl: pool.baseUrl ?? '',
      });
    }
    if (!canLoadHosts(pool, token)) return null;
    try {
      return new HostsController({
        transport: hostsTransport(poolClient(pool, { token })),
        poolUrl: pool.baseUrl ?? '',
      });
    } catch {
      return null;
    }
  })();

  const configured = transport !== null || isPoolConfigured(pool);
  const enrolControl = controlById('enrol-host');
  const canEnrol = $derived(enrolControl !== null && canUseControl(role, enrolControl));
  const enrolDenial = $derived(
    enrolControl !== null && !canEnrol ? denialReason(role, enrolControl) : null,
  );
  const summary = $derived(hostSummary(hostCounts(view.hosts)));

  /** Adopt a controller result. */
  function adopt(next: HostsViewState): void {
    view = next;
  }

  /** Load the list, on mount and on retry. */
  async function refresh(): Promise<void> {
    if (!controller) return;
    adopt(await controller.load());
  }

  $effect(() => {
    // A page that has a pool and no initial state loads itself once; a page
    // rendered from a supplied state keeps that state.
    if (controller && initial.phase === 'idle' && controller.snapshot().phase === 'idle') {
      void refresh();
    }
  });

  /** Enrol a host and show the token once. */
  async function enroll(input: { hostname: string; display_name: string | null }): Promise<void> {
    if (!controller) return;
    adopt(await controller.enroll(input));
  }

  /** Grant or revoke the execution grant for one host. */
  async function setExecution(hostId: string, enabled: boolean): Promise<void> {
    if (!controller) return;
    adopt(await controller.setExecution(hostId, enabled));
  }

  /** Forget the one-time token, which the contract cannot show again. */
  function dismissEnrollment(): void {
    if (!controller) {
      view = { ...view, enrollment: null };
      return;
    }
    adopt(controller.dismissEnrollment());
  }
</script>

<section class="tp-hosts tp-page" aria-labelledby="tp-hosts-title" data-hosts-page="true">
  <h1 class="tp-page__title" id="tp-hosts-title">Hosts</h1>
  <p class="tp-page__lede">
    Every machine enrolled with this pool, with its slug, display name, last
    seen, and whether it may run agents. Approval, connection and execution are
    three different things: a host can be connected and still not be allowed to
    run anything.
  </p>

  {#if view.problem}
    <ProblemDetails problem={view.problem} onRetry={() => void refresh()} />
  {/if}

  {#if !configured}
    <p class="tp-page__state" data-hosts-state="unconfigured">
      <StatusBadge kind="pending" />
      <span>
        No pool is configured, so there are no hosts to show. Point the control
        plane at a pool and sign in.
      </span>
    </p>
  {:else}
    {#if enrolControl}
      {#if canEnrol}
        <EnrollHostPanel
          poolUrl={pool.baseUrl}
          busy={view.busy}
          enrollment={view.enrollment}
          onSubmit={(input) => void enroll(input)}
          onDismiss={dismissEnrollment}
        />
      {:else}
        <p class="tp-hosts__denial" data-hosts-denial="enrol-host">{enrolDenial}</p>
      {/if}
    {/if}

    {#if view.hosts.length === 0 && (view.phase === 'idle' || view.phase === 'loading')}
      <p class="tp-page__state" data-hosts-state="loading">
        <StatusBadge kind="pending" />
        <span>Loading the hosts enrolled with this pool…</span>
      </p>
    {:else if view.hosts.length === 0 && view.phase !== 'error'}
      <p class="tp-page__state" data-hosts-state="empty">
        <StatusBadge kind="neutral" />
        <span>
          No hosts are enrolled yet. Enrol one above; it appears here as approval
          required until the machine exchanges its token and connects.
        </span>
      </p>
    {:else if view.hosts.length > 0}
      <p class="tp-hosts__summary" data-host-summary="true">{summary}</p>
      <ul class="tp-hosts__list">
        {#each view.hosts as row (row.id)}
          <li class="tp-hosts__item">
            <HostCard
              {row}
              {role}
              busy={view.busyHostId === row.id}
              onGrant={() => void setExecution(row.id, true)}
              onRevoke={() => void setExecution(row.id, false)}
            />
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  .tp-hosts {
    display: grid;
    gap: var(--tp-space-3);
  }

  .tp-hosts__summary {
    margin: 0;
    color: var(--tp-text-muted);
  }

  .tp-hosts__denial {
    margin: 0;
    max-width: 68ch;
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    color: var(--tp-text-muted);
  }

  .tp-hosts__list {
    display: grid;
    gap: var(--tp-space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-hosts__item {
    display: block;
  }
</style>
