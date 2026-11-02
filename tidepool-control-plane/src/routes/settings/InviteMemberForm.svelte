<script lang="ts">
  /**
   * Invite an operator by email with a role.
   *
   * The form sends the role the inviter chose, and says what each role can do
   * so "admin" is not a guess. Validation is local first; the pool still
   * enforces the owner-only rule and the email rules.
   */
  import { ROLES, type Role } from '../../auth/types';
  import { roleDescription, roleLabel } from '../../auth/roles';
  import { validateInviteInput } from '$lib/keys';

  interface Props {
    /** True while an invite is in flight. */
    busy?: boolean;
    /** Role to start with; member by default, the least surprising choice. */
    initialRole?: Role;
    /** Called with a valid form. */
    onSubmit?: (input: { email: string; role: Role }) => void;
  }

  let { busy = false, initialRole = 'member', onSubmit }: Props = $props();

  let email = $state('');
  // The initial role seeds the form once; the person drives it afterwards.
  // svelte-ignore state_referenced_locally
  let role = $state<Role>(initialRole);
  let localError = $state<string | null>(null);

  /** Validate here first, so a malformed address never leaves the browser. */
  function submit(event: SubmitEvent): void {
    event.preventDefault();
    if (busy) return;
    localError = null;
    const validation = validateInviteInput({ email, role });
    if (!validation.ok) {
      localError = validation.message;
      return;
    }
    onSubmit?.(validation.input);
  }
</script>

<section class="tp-invite" aria-labelledby="tp-invite-title" data-invite-form="true">
  <h3 class="tp-invite__title" id="tp-invite-title">Invite an operator</h3>
  <p class="tp-invite__lede">
    The invitation expires; the panel below shows when. Send the one-time
    acceptance token to the person you invited.
  </p>

  {#if localError}
    <p class="tp-invite__error" role="alert">{localError}</p>
  {/if}

  <form class="tp-invite__form" onsubmit={submit}>
    <label class="tp-invite__field">
      <span class="tp-invite__label">Email address</span>
      <input
        class="tp-invite__input"
        type="email"
        name="email"
        autocomplete="off"
        spellcheck="false"
        required
        bind:value={email}
      />
      <span class="tp-invite__hint">The address the person signs in with.</span>
    </label>

    <label class="tp-invite__field">
      <span class="tp-invite__label">Role</span>
      <select class="tp-invite__input" name="role" bind:value={role}>
        {#each ROLES as option (option)}
          <option value={option}>{roleLabel(option)}</option>
        {/each}
      </select>
      <span class="tp-invite__hint">{roleDescription(role)}</span>
    </label>

    <button class="tp-button" type="submit" disabled={busy}>
      {busy ? 'Sending the invitation…' : 'Send the invitation'}
    </button>
  </form>
</section>

<style>
  .tp-invite {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-3);
    background: var(--tp-surface);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-invite__title {
    margin: 0;
    font-size: var(--tp-font-heading);
    color: var(--tp-text-strong);
  }

  .tp-invite__lede,
  .tp-invite__hint,
  .tp-invite__error {
    margin: 0;
    max-width: 68ch;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-invite__error {
    color: var(--tp-text-strong);
    border-left: 3px solid var(--tp-status-danger);
    padding-left: var(--tp-space-2);
  }

  .tp-invite__form,
  .tp-invite__field {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-invite__label {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .tp-invite__input {
    padding: var(--tp-space-1) var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    background: var(--tp-terminal-background);
    color: var(--tp-text-strong);
    font: inherit;
  }
</style>
