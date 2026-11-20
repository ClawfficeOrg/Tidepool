<script lang="ts">
  /**
   * Invite an operator by email with a role.
   *
   * The form sends the role the inviter chose, and says what each role can do
   * so "admin" is not a guess. Validation is local first; the pool still
   * enforces the owner-only rule and the email rules. The error appears under
   * the field that caused it, like every other form here.
   */
  import { ROLES, type Role } from '../../auth/types';
  import { roleDescription, roleLabel } from '../../auth/roles';
  import { validateInviteInput } from '$lib/keys';
  import FormField from '../../components/FormField.svelte';

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
  /** The one validation failure, tied to the field it names. */
  let failure = $state<{ field: 'email' | 'role'; message: string } | null>(null);

  /** Validate here first, so a malformed address never leaves the browser. */
  function submit(event: SubmitEvent): void {
    event.preventDefault();
    if (busy) return;
    failure = null;
    const validation = validateInviteInput({ email, role });
    if (!validation.ok) {
      failure = { field: validation.field, message: validation.message };
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

  <form class="tp-invite__form" onsubmit={submit} data-form="invite">
    <FormField
      form="invite"
      field="email"
      label="Email address"
      required
      help="The address the person signs in with."
      error={failure?.field === 'email' ? failure.message : null}
    >
      {#snippet children(id, described)}
        <input
          class="tp-field__control"
          {id}
          type="email"
          name="email"
          autocomplete="off"
          spellcheck="false"
          required
          aria-invalid={failure?.field === 'email'}
          aria-describedby={described}
          bind:value={email}
        />
      {/snippet}
    </FormField>

    <FormField
      form="invite"
      field="role"
      label="Role"
      required
      help={roleDescription(role)}
      error={failure?.field === 'role' ? failure.message : null}
    >
      {#snippet children(id, described)}
        <select
          class="tp-field__control"
          {id}
          name="role"
          aria-invalid={failure?.field === 'role'}
          aria-describedby={described}
          bind:value={role}
        >
          {#each ROLES as option (option)}
            <option value={option}>{roleLabel(option)}</option>
          {/each}
        </select>
      {/snippet}
    </FormField>

    <div class="tp-field__actions">
      <button class="tp-button" type="submit" disabled={busy}>
        {busy ? 'Sending the invitation…' : 'Send the invitation'}
      </button>
    </div>
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

  .tp-invite__lede {
    margin: 0;
    max-width: 68ch;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-invite__form {
    display: grid;
    gap: var(--tp-space-2);
  }
</style>
