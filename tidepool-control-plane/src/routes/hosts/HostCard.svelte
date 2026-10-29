<script lang="ts">
  /**
   * One host, as the hosts page lists it.
   *
   * The row answers the three questions an operator has: what is this machine
   * (slug and display name), what is it doing (status), and may it run agents
   * (the execution grant). The grant is deliberately a labelled action with
   * its consequence beside it, never a bare toggle: revoking it changes what
   * the pool will dispatch.
   */
  import HostStatusBadge from './HostStatusBadge.svelte';
  import ExecutionChip from './ExecutionChip.svelte';
  import HostActionControl from './HostActionControl.svelte';
  import { canUseControl, controlById, denialReason } from '../../auth/roles';
  import type { Role } from '../../auth/types';
  import { executionAction, type HostAction, type HostRow } from '$lib/hosts';

  interface Props {
    /** The host to render. */
    row: HostRow;
    /** The signed-in role, for control gating. */
    role?: Role | null;
    /** True while a request for this host is in flight. */
    busy?: boolean;
    /** Start the action in its confirm step, for tests and resumed flows. */
    confirmingAction?: HostAction['id'] | null;
    /** Grant `host.execute`. */
    onGrant?: () => void;
    /** Revoke `host.execute`. */
    onRevoke?: () => void;
  }

  let {
    row,
    role = null,
    busy = false,
    confirmingAction = null,
    onGrant,
    onRevoke,
  }: Props = $props();

  const action = $derived(executionAction(row));
  const denial = $derived.by((): string | null => {
    if (!action) return null;
    const control = controlById(action.controlId);
    if (!control || canUseControl(role, control)) return null;
    return denialReason(role, control);
  });
</script>

<article
  class="tp-host"
  data-host-id={row.id}
  data-host-status={row.status.status ?? 'unknown'}
  data-execution={row.execution.enabled ? 'enabled' : 'disabled'}
  aria-labelledby={`tp-host-${row.id}-slug`}
>
  <header class="tp-host__header">
    <div class="tp-host__identity">
      <h2 class="tp-host__slug" id={`tp-host-${row.id}-slug`}>{row.slug}</h2>
      <p class="tp-host__display">{row.displayName}</p>
    </div>
    <div class="tp-host__badges">
      <HostStatusBadge status={row.status.status} />
      <ExecutionChip execution={row.execution} />
    </div>
  </header>

  <dl class="tp-host__facts">
    <div class="tp-host__fact">
      <dt>Slug</dt>
      <dd><code>{row.slug}</code></dd>
    </div>
    <div class="tp-host__fact">
      <dt>Hostname</dt>
      <dd><code>{row.hostname}</code></dd>
    </div>
    <div class="tp-host__fact">
      <dt>Last seen</dt>
      <dd title={row.lastSeen.at ?? undefined}>{row.lastSeen.text}</dd>
    </div>
    <div class="tp-host__fact">
      <dt>Execution</dt>
      <dd>{row.execution.word}</dd>
    </div>
    {#if row.revokedAt}
      <div class="tp-host__fact">
        <dt>Revoked at</dt>
        <dd>{row.revokedAt}</dd>
      </div>
    {/if}
  </dl>

  <div class="tp-host__action">
    {#if action}
      <HostActionControl
        {action}
        actionKey={`${row.id}-${action.id}`}
        {busy}
        {denial}
        initialConfirming={confirmingAction === action.id}
        onConfirm={action.id === 'grant-execution' ? onGrant : onRevoke}
      />
    {:else}
      <p class="tp-host__note">
        This host's relationship with the pool has ended, so there is nothing to
        grant or revoke. Enrol the machine again if it should come back.
      </p>
    {/if}
  </div>
</article>

<style>
  .tp-host {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-3);
    background: var(--tp-surface);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-host[data-host-status='revoked'] {
    border-style: dashed;
  }

  .tp-host__header {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--tp-space-2);
  }

  .tp-host__identity {
    display: grid;
    gap: 2px;
  }

  .tp-host__slug {
    margin: 0;
    font-size: var(--tp-font-heading);
    color: var(--tp-text-strong);
  }

  .tp-host__display {
    margin: 0;
    color: var(--tp-text-muted);
  }

  .tp-host__badges {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--tp-space-1);
  }

  .tp-host__facts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-3);
    margin: 0;
    padding-top: var(--tp-space-2);
    border-top: 1px solid var(--tp-border);
  }

  .tp-host__fact {
    display: grid;
    gap: 2px;
  }

  .tp-host__fact dt {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .tp-host__fact dd {
    margin: 0;
  }

  .tp-host__note {
    margin: 0;
    max-width: 62ch;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }
</style>
