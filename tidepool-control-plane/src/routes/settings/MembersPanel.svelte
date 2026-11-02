<script lang="ts">
  /**
   * The members panel.
   *
   * The operator list is readable by any signed-in member (`GET /v1/pool/members`
   * allows that); inviting, changing a role and removing are owner-only in the
   * contract. A non-owner therefore sees the list read-only with one sentence
   * explaining why, rather than controls that would come back 403.
   *
   * Every invitation keeps its expiry in view — both as a sentence and as the
   * exact `<time>` the pool reported — and a freshly minted invitation shows
   * its one-time acceptance token with the warning that it cannot be shown
   * again.
   */
  import CreatedInvitationPanel from './CreatedInvitationPanel.svelte';
  import InviteMemberForm from './InviteMemberForm.svelte';
  import MemberRow from './MemberRow.svelte';
  import { canUseControl, controlById, denialReason } from '../../auth/roles';
  import type { Role } from '../../auth/types';
  import type { CreatedInvitation, InvitationRow, MemberRow as MemberRowModel } from '$lib/keys';

  interface Props {
    /** Every operator, owners first. */
    members?: MemberRowModel[];
    /** Pending invitations, soonest expiry first. */
    invitations?: InvitationRow[];
    /** The one-time invitation token just minted, until dismissed. */
    createdInvitation?: CreatedInvitation | null;
    /** The signed-in role, for control gating. */
    role?: Role | null;
    /** True while a request is in flight. */
    busy?: boolean;
    /** The member or invitation whose row is busy, when one is. */
    busyId?: string | null;
    /** One line describing the list, e.g. "3 operators, 1 owner". */
    summary?: string;
    /** Invite an operator. */
    onInvite?: (input: { email: string; role: Role }) => void;
    /** Change one member's role. */
    onChangeRole?: (memberId: string, role: Role) => void;
    /** Remove one member. */
    onRemove?: (memberId: string) => void;
    /** Withdraw one invitation. */
    onRevokeInvitation?: (invitationId: string) => void;
    /** Forget the one-time invitation token. */
    onDismissInvitation?: () => void;
  }

  let {
    members = [],
    invitations = [],
    createdInvitation = null,
    role = null,
    busy = false,
    busyId = null,
    summary = '',
    onInvite,
    onChangeRole,
    onRemove,
    onRevokeInvitation,
    onDismissInvitation,
  }: Props = $props();

  const inviteControl = controlById('invite-member');
  const revokeControl = controlById('revoke-invitation');
  const canInvite = $derived(inviteControl !== null && canUseControl(role, inviteControl));
  const inviteDenial = $derived(
    inviteControl !== null && !canInvite ? denialReason(role, inviteControl) : null,
  );
  const canRevokeInvitation = $derived(
    revokeControl !== null && canUseControl(role, revokeControl),
  );
</script>

<section class="tp-members" aria-labelledby="tp-members-title" data-members-panel="true">
  <header class="tp-members__header">
    <h2 class="tp-members__title" id="tp-members-title">Members</h2>
    <p class="tp-members__lede">
      The people who can sign in to this pool. An owner manages the rest: the
      pool always keeps at least one owner, so the last one cannot be removed or
      demoted.
    </p>
  </header>

  {#if summary}
    <p class="tp-members__summary" data-members-summary="true">{summary}</p>
  {/if}

  {#if !canInvite}
    <p class="tp-members__denial" data-members-denial="owner-only">{inviteDenial}</p>
  {:else}
    <InviteMemberForm {busy} onSubmit={onInvite} />

    {#if createdInvitation}
      <CreatedInvitationPanel
        created={createdInvitation}
        onDismiss={onDismissInvitation}
      />
    {/if}

    <section class="tp-members__invitations" aria-labelledby="tp-invitations-title">
      <h3 class="tp-members__subtitle" id="tp-invitations-title">Pending invitations</h3>
      {#if invitations.length === 0}
        <p class="tp-members__empty" data-invitations-state="empty">
          No invitations are pending. Every invited operator has accepted, or none
          have been sent yet.
        </p>
      {:else}
        <ul class="tp-members__list">
          {#each invitations as invitation (invitation.id)}
            <li
              class="tp-members__invitation"
              data-invitation-id={invitation.id}
              data-invitation-expired={invitation.expiry.expired ? 'true' : 'false'}
            >
              <div class="tp-members__invitation-identity">
                <span class="tp-members__invitation-email"><code>{invitation.email}</code></span>
                <span class="tp-members__invitation-role">{invitation.roleLabel}</span>
              </div>
              <p class="tp-members__expiry" data-invitation-expiry>
                {invitation.expiry.text}
                {#if invitation.expiry.at}
                  <time datetime={invitation.expiry.at}>{invitation.expiry.at}</time>
                {/if}
              </p>
              {#if canRevokeInvitation}
                <button
                  class="tp-button tp-button--quiet"
                  type="button"
                  disabled={busy && busyId === invitation.id}
                  onclick={() => onRevokeInvitation?.(invitation.id)}
                >
                  Revoke invitation
                </button>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}

  <h3 class="tp-members__subtitle">Operators</h3>
  {#if members.length === 0}
    <p class="tp-members__empty" data-members-state="empty">
      No operators were reported. Reload the page; if it stays empty, the pool has
      no accounts yet.
    </p>
  {:else}
    <ul class="tp-members__list">
      {#each members as member (member.id)}
        <li class="tp-members__item">
          <MemberRow
            {member}
            {members}
            {role}
            busy={busyId === member.id}
            onChangeRole={(next) => onChangeRole?.(member.id, next)}
            onRemove={() => onRemove?.(member.id)}
          />
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .tp-members {
    display: grid;
    gap: var(--tp-space-2);
  }

  .tp-members__header {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-members__title,
  .tp-members__subtitle {
    margin: 0;
    color: var(--tp-text-strong);
  }

  .tp-members__title {
    font-size: var(--tp-font-heading);
  }

  .tp-members__subtitle {
    font-size: var(--tp-font-caption);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .tp-members__lede,
  .tp-members__denial,
  .tp-members__summary,
  .tp-members__empty,
  .tp-members__expiry {
    margin: 0;
    max-width: 68ch;
    color: var(--tp-text-muted);
  }

  .tp-members__denial {
    color: var(--tp-text-strong);
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
  }

  .tp-members__list {
    display: grid;
    gap: var(--tp-space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-members__invitation {
    display: grid;
    gap: var(--tp-space-1);
    justify-items: start;
    padding: var(--tp-space-2);
    background: var(--tp-surface);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-members__invitation-identity {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--tp-space-2);
  }

  .tp-members__invitation-role {
    padding: 0 var(--tp-space-1);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-control);
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-members__expiry {
    font-size: var(--tp-font-caption);
  }

  .tp-button--quiet {
    background: transparent;
    color: var(--tp-text-strong);
  }
</style>
