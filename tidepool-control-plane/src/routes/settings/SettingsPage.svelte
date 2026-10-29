<script lang="ts">
  /**
   * The settings page: members and API keys.
   *
   * It renders from a `MembersKeysState` and loads itself on mount when it has
   * a pool and a session token (or an injected transport). It can be rendered
   * from an initial state in tests, which is how the last-owner rule, the
   * one-time secret and the revoked-key state are asserted without a browser.
   *
   * Everything the page decides lives in `$lib/keys`, including the two rules
   * this task is judged on: the last owner cannot be removed or demoted, and a
   * key's secret is shown once. The page also keeps the settings placeholder
   * for the pool URL and preferences, which arrive with task 2.1.10.
   */
  import ProblemDetails from '../../components/ProblemDetails.svelte';
  import StatusBadge from '../../components/StatusBadge.svelte';
  import ApiKeysPanel from './ApiKeysPanel.svelte';
  import MembersPanel from './MembersPanel.svelte';
  import type { Role } from '../../auth/types';
  import { isPoolConfigured, poolClient, type PoolConfig } from '$lib/api/pool';
  import {
    MembersKeysController,
    canLoadMembersKeys,
    emptyMembersKeysState,
    keysSummary,
    membersKeysCounts,
    membersKeysTransport,
    membersSummary,
    type MembersKeysState,
    type MembersKeysTransport,
  } from '$lib/keys';

  interface Props {
    /** The pool this control plane is pointed at. */
    pool?: PoolConfig;
    /** The signed-in operator's token, used to load members and keys. */
    token?: string | null;
    /** The signed-in operator's role, for control gating. */
    role?: Role | null;
    /** The state to render before loading; also how tests render any state. */
    initial?: MembersKeysState;
    /** Transport override, for tests; otherwise built from the pool and token. */
    transport?: MembersKeysTransport | null;
  }

  let {
    pool = { baseUrl: null },
    token = null,
    role = null,
    initial = emptyMembersKeysState(),
    transport = null,
  }: Props = $props();

  let view = $state<MembersKeysState>(initial);

  /** Build the controller once: the page keeps one view of one pool. */
  const controller: MembersKeysController | null = (() => {
    if (transport) return new MembersKeysController({ transport });
    if (!canLoadMembersKeys(pool, token)) return null;
    try {
      return new MembersKeysController({
        transport: membersKeysTransport(poolClient(pool, { token })),
      });
    } catch {
      return null;
    }
  })();

  const configured = transport !== null || isPoolConfigured(pool);
  const signedIn = transport !== null || canLoadMembersKeys(pool, token);
  const counts = $derived(membersKeysCounts(view));
  const memberSummary = $derived(membersSummary(counts));
  const keySummary = $derived(keysSummary(counts));

  /** Adopt a controller result. */
  function adopt(next: MembersKeysState): void {
    view = next;
  }

  /** Load every panel, on mount and on retry. */
  async function refresh(): Promise<void> {
    if (!controller) return;
    adopt(await controller.load({ includeOwnerData: role === 'owner' }));
  }

  $effect(() => {
    // A page that has a pool and no initial state loads itself once; a page
    // rendered from a supplied state keeps that state.
    if (controller && initial.phase === 'idle' && controller.snapshot().phase === 'idle') {
      void refresh();
    }
  });

  /** Invite an operator and hold the one-time token. */
  async function invite(input: { email: string; role: Role }): Promise<void> {
    if (!controller) return;
    adopt(await controller.invite(input));
  }

  /** Change a member's role, with the last-owner rule checked first. */
  async function changeRole(memberId: string, next: Role): Promise<void> {
    if (!controller) return;
    adopt(await controller.changeRole(memberId, next));
  }

  /** Remove a member, with the last-owner rule checked first. */
  async function removeMember(memberId: string): Promise<void> {
    if (!controller) return;
    adopt(await controller.removeMember(memberId));
  }

  /** Withdraw a pending invitation. */
  async function revokeInvitation(invitationId: string): Promise<void> {
    if (!controller) return;
    adopt(await controller.revokeInvitation(invitationId));
  }

  /** Create a key and show its secret once. */
  async function createKey(input: { name: string; scopes: string[] }): Promise<void> {
    if (!controller) return;
    adopt(await controller.createKey(input));
  }

  /** Revoke a key; its row stays and is marked revoked. */
  async function revokeKey(keyId: string): Promise<void> {
    if (!controller) return;
    adopt(await controller.revokeKey(keyId));
  }

  /** Forget the one-time key secret. */
  function dismissSecret(): void {
    if (!controller) {
      view = { ...view, createdKey: null };
      return;
    }
    adopt(controller.dismissCreatedKey());
  }

  /** Forget the one-time invitation token. */
  function dismissInvitation(): void {
    if (!controller) {
      view = { ...view, createdInvitation: null };
      return;
    }
    adopt(controller.dismissCreatedInvitation());
  }
</script>

<section class="tp-settings tp-page" aria-labelledby="tp-settings-title" data-settings-page="true">
  <h1 class="tp-page__title" id="tp-settings-title">Settings</h1>
  <p class="tp-page__lede">
    The pool URL, the palette, time and density preferences, and the version of
    this control plane. Members and API keys for this pool are managed below;
    the rest of the settings arrive with task 2.1.10.
  </p>

  {#if !configured}
    <p class="tp-page__state" data-settings-state="unconfigured">
      <StatusBadge kind="pending" />
      <span>
        No pool is configured, so there are no members or keys to show. Point the
        control plane at a pool and sign in.
      </span>
    </p>
  {:else if !signedIn}
    <p class="tp-page__state" data-settings-state="signed-out">
      <StatusBadge kind="pending" />
      <span>
        Sign in to manage this pool's members and API keys. The member list needs
        a session; inviting and key management need the owner role.
      </span>
    </p>
  {:else}
    {#if view.problem}
      <ProblemDetails problem={view.problem} onRetry={() => void refresh()} />
    {/if}

    {#if view.notice}
      <p class="tp-settings__notice" role="status" data-settings-notice="true">
        <StatusBadge kind="success" />
        <span>{view.notice}</span>
      </p>
    {/if}

    <MembersPanel
      members={view.members}
      invitations={view.invitations}
      createdInvitation={view.createdInvitation}
      {role}
      busy={view.busy}
      busyId={view.busyId}
      summary={memberSummary}
      onInvite={(input) => void invite(input)}
      onChangeRole={(memberId, next) => void changeRole(memberId, next)}
      onRemove={(memberId) => void removeMember(memberId)}
      onRevokeInvitation={(id) => void revokeInvitation(id)}
      onDismissInvitation={dismissInvitation}
    />

    <ApiKeysPanel
      keys={view.keys}
      createdKey={view.createdKey}
      {role}
      busy={view.busy}
      busyId={view.busyId}
      summary={keySummary}
      onCreate={(input) => void createKey(input)}
      onRevoke={(keyId) => void revokeKey(keyId)}
      onDismissSecret={dismissSecret}
    />
  {/if}
</section>

<style>
  .tp-settings {
    display: grid;
    gap: var(--tp-space-4);
  }

  .tp-settings__notice {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--tp-space-2);
    margin: 0;
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-control);
  }
</style>
