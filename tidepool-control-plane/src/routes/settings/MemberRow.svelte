<script lang="ts">
  /**
   * One pool member, as the settings page lists them.
   *
   * An owner sees a role select and a labelled remove action with its
   * consequence beside it and a second step to confirm. The last-owner rule is
   * checked here as well as in the controller, and when it applies the controls
   * are disabled **and** the reason is printed: the point of the criterion is
   * that a person reads why, not that a button silently refuses them.
   */
  import { canUseControl, controlById } from '../../auth/roles';
  import { ROLES, type Role } from '../../auth/types';
  import { roleLabel } from '../../auth/roles';
  import type { MemberRow } from '$lib/keys';
  import { REMOVE_MEMBER_CONSEQUENCE, removalRule } from '$lib/keys';

  interface Props {
    /** The member to render. */
    member: MemberRow;
    /** Every member, so the last-owner rule can count owners. */
    members: MemberRow[];
    /** The signed-in role, for control gating. */
    role?: Role | null;
    /** True while a request for this member is in flight. */
    busy?: boolean;
    /** Start in the remove-confirm step, for tests and resumed flows. */
    initialConfirming?: boolean;
    /** Change this member's role. */
    onChangeRole?: (role: Role) => void;
    /** Remove this member, after confirmation. */
    onRemove?: () => void;
  }

  let {
    member,
    members,
    role = null,
    busy = false,
    initialConfirming = false,
    onChangeRole,
    onRemove,
  }: Props = $props();

  const changeControl = controlById('change-member-role');
  const removeControl = controlById('remove-member');
  const canChange = $derived(changeControl !== null && canUseControl(role, changeControl));
  const canRemove = $derived(removeControl !== null && canUseControl(role, removeControl));
  const ownerRule = $derived(removalRule(member, members));
  const locked = $derived(ownerRule.isLastOwner);
  const confirmCopy = $derived(
    `Confirm: ${member.displayName} is removed from this pool and signed out everywhere.`,
  );

  // svelte-ignore state_referenced_locally
  let confirming = $state(initialConfirming);

  /** Change the role through the select. */
  function change(event: Event): void {
    const next = (event.currentTarget as HTMLSelectElement).value;
    if (next === member.role) return;
    onChangeRole?.(next as Role);
  }

  /** First click: restate what removal does. */
  function ask(): void {
    if (busy || locked) return;
    confirming = true;
  }

  /** Second click: perform it. */
  function confirm(): void {
    if (busy || locked) return;
    confirming = false;
    onRemove?.();
  }
</script>

<article
  class="tp-member"
  data-member-id={member.id}
  data-member-role={member.role}
  aria-labelledby={`tp-member-${member.id}-name`}
>
  <header class="tp-member__header">
    <div class="tp-member__identity">
      <h3 class="tp-member__name" id={`tp-member-${member.id}-name`}>{member.displayName}</h3>
      <p class="tp-member__email"><code>{member.email}</code></p>
    </div>
    <p class="tp-member__role">
      <span class="tp-member__role-word" data-member-role-word>{member.roleLabel}</span>
      {#if locked}
        <span class="tp-member__lock" data-last-owner-badge="true">Last owner</span>
      {/if}
    </p>
  </header>

  <dl class="tp-member__facts">
    <div class="tp-member__fact">
      <dt>Last sign in</dt>
      <dd title={member.lastLogin.at ?? undefined}>{member.lastLogin.text}</dd>
    </div>
    <div class="tp-member__fact">
      <dt>Password</dt>
      <dd>
        {member.passwordChangeRequired ? 'Change required at next sign in' : 'Set by the operator'}
      </dd>
    </div>
    {#if member.createdAt}
      <div class="tp-member__fact">
        <dt>Added</dt>
        <dd><time datetime={member.createdAt}>{member.createdAt}</time></dd>
      </div>
    {/if}
  </dl>

  {#if canChange || canRemove}
    <div class="tp-member__controls" data-member-controls="true">
      {#if canChange}
        <label class="tp-member__field">
          <span class="tp-member__label">Role</span>
          <select
            class="tp-member__select"
            value={member.role}
            disabled={busy || locked}
            aria-describedby={locked ? `tp-member-${member.id}-last-owner` : undefined}
            onchange={change}
          >
            {#each ROLES as option (option)}
              <option value={option}>{roleLabel(option)}</option>
            {/each}
          </select>
        </label>
      {/if}

      {#if canRemove}
        <div class="tp-member__remove" data-confirming={confirming ? 'true' : 'false'}>
          <button
            class="tp-button tp-button--quiet"
            type="button"
            disabled={busy || locked}
            aria-expanded={confirming}
            aria-describedby={`tp-member-${member.id}-remove-consequence`}
            onclick={ask}
          >
            Remove member
          </button>
          <p class="tp-member__consequence" id={`tp-member-${member.id}-remove-consequence`}>
            {REMOVE_MEMBER_CONSEQUENCE}
          </p>

          {#if confirming && !locked}
            <div class="tp-member__confirm" role="group" aria-label="Confirm remove member">
              <p class="tp-member__confirm-copy">{confirmCopy}</p>
              <div class="tp-member__buttons">
                <button class="tp-button" type="button" disabled={busy} onclick={confirm}>
                  Remove {member.displayName}
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
      {/if}
    </div>

    {#if locked}
      <p
        class="tp-member__last-owner"
        id={`tp-member-${member.id}-last-owner`}
        data-last-owner-reason="true"
        role="note"
      >
        {ownerRule.reason}
      </p>
    {/if}
  {/if}
</article>

<style>
  .tp-member {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-3);
    background: var(--tp-surface);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-member[data-member-role='owner'] {
    border-color: var(--tp-border-strong);
  }

  .tp-member__header {
    display: flex;
    flex-wrap: wrap;
    align-items: start;
    justify-content: space-between;
    gap: var(--tp-space-2);
  }

  .tp-member__name {
    margin: 0;
    font-size: var(--tp-font-heading);
    color: var(--tp-text-strong);
  }

  .tp-member__email {
    margin: 0;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-member__role {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--tp-space-1);
    margin: 0;
  }

  .tp-member__role-word {
    padding: 0 var(--tp-space-1);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-control);
    color: var(--tp-text-strong);
    font-size: var(--tp-font-caption);
  }

  .tp-member__lock {
    padding: 0 var(--tp-space-1);
    border: 1px solid var(--tp-status-warning);
    border-radius: var(--tp-radius-control);
    color: var(--tp-text-strong);
    font-size: var(--tp-font-caption);
  }

  .tp-member__facts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-4);
    margin: 0;
  }

  .tp-member__fact {
    display: grid;
    gap: 2px;
  }

  .tp-member__fact dt {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .tp-member__fact dd {
    margin: 0;
  }

  .tp-member__controls {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: var(--tp-space-3);
  }

  .tp-member__field {
    display: grid;
    gap: 2px;
  }

  .tp-member__label {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .tp-member__select {
    padding: var(--tp-space-1) var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    background: var(--tp-terminal-background);
    color: var(--tp-text-strong);
    font: inherit;
  }

  .tp-member__remove {
    display: grid;
    gap: var(--tp-space-1);
    justify-items: start;
  }

  .tp-member__consequence,
  .tp-member__confirm-copy,
  .tp-member__last-owner {
    margin: 0;
    max-width: 68ch;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-member__last-owner {
    color: var(--tp-text-strong);
    border-left: 3px solid var(--tp-status-warning);
    padding-left: var(--tp-space-2);
  }

  .tp-member__confirm {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-member__buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-1);
  }

  .tp-button--quiet {
    background: transparent;
    color: var(--tp-text-strong);
  }
</style>
