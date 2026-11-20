<script lang="ts">
  /**
   * First-run setup.
   *
   * Shown only while the pool reports no organization. The form creates the
   * organization and the first owner, then the controller signs that owner in
   * with the same credentials, so setup ends at the desk rather than at a
   * second sign-in form.
   */
  import ProblemDetails from '../components/ProblemDetails.svelte';
  import FormField from '../components/FormField.svelte';
  import type { PoolProblem } from '$lib/api/problem';
  import { MIN_SETUP_PASSWORD_LENGTH, type SetupInput } from '../auth/types';

  interface Props {
    /** The pool this setup creates the first operator for. */
    poolUrl?: string | null;
    /** Why setup could not proceed, when the controller said so. */
    notice?: string | null;
    /** A failure from the last attempt. */
    problem?: PoolProblem | null;
    /** Called with the completed form. */
    onSubmit?: (input: SetupInput) => void | Promise<void>;
  }

  let { poolUrl = null, notice = null, problem = null, onSubmit }: Props = $props();

  let organizationName = $state('');
  let displayName = $state('');
  let email = $state('');
  let password = $state('');
  let confirmation = $state('');
  let localError = $state<string | null>(null);
  let busy = $state(false);

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (busy) return;
    localError = null;
    if (password !== confirmation) {
      localError = 'The two passwords do not match.';
      return;
    }
    busy = true;
    try {
      await onSubmit?.({
        organization_name: organizationName,
        display_name: displayName,
        email,
        password,
      });
    } finally {
      busy = false;
    }
  }
</script>

<section class="tp-auth" aria-labelledby="tp-setup-title" data-auth-page="setup">
  <h1 class="tp-auth__title" id="tp-setup-title">Set up this pool</h1>
  <p class="tp-auth__lede">
    {#if poolUrl}
      <code>{poolUrl}</code> has no organization yet. Create it and the first
      owner here; you will be signed in when setup finishes.
    {:else}
      This pool has no organization yet. Create it and the first owner here; you
      will be signed in when setup finishes.
    {/if}
  </p>

  {#if notice}
    <p class="tp-auth__notice" role="status">{notice}</p>
  {/if}

  {#if problem}
    <ProblemDetails {problem} />
  {/if}

  <form class="tp-auth__form" onsubmit={submit} data-form="setup">
    <FormField
      form="setup"
      field="organization_name"
      label="Organization name"
      required
      help="The name of the organization this pool serves."
    >
      {#snippet children(id, described)}
        <input
          class="tp-field__control"
          {id}
          type="text"
          name="organization_name"
          autocomplete="organization"
          required
          aria-describedby={described}
          bind:value={organizationName}
        />
      {/snippet}
    </FormField>

    <FormField
      form="setup"
      field="display_name"
      label="Your name"
      required
      help="What the members list calls you."
    >
      {#snippet children(id, described)}
        <input
          class="tp-field__control"
          {id}
          type="text"
          name="display_name"
          autocomplete="name"
          required
          aria-describedby={described}
          bind:value={displayName}
        />
      {/snippet}
    </FormField>

    <FormField
      form="setup"
      field="email"
      label="Email"
      required
      help="The address you sign in with."
    >
      {#snippet children(id, described)}
        <input
          class="tp-field__control"
          {id}
          type="email"
          name="email"
          autocomplete="username"
          required
          aria-describedby={described}
          bind:value={email}
        />
      {/snippet}
    </FormField>

    <FormField
      form="setup"
      field="password"
      label="Password"
      required
      help="At least {MIN_SETUP_PASSWORD_LENGTH} characters."
    >
      {#snippet children(id, described)}
        <input
          class="tp-field__control"
          {id}
          type="password"
          name="password"
          autocomplete="new-password"
          minlength={MIN_SETUP_PASSWORD_LENGTH}
          required
          aria-describedby={described}
          bind:value={password}
        />
      {/snippet}
    </FormField>

    <FormField
      form="setup"
      field="password_confirmation"
      label="Confirm password"
      required
      help="Type the same password again."
      error={localError}
    >
      {#snippet children(id, described)}
        <input
          class="tp-field__control"
          {id}
          type="password"
          name="password_confirmation"
          autocomplete="new-password"
          minlength={MIN_SETUP_PASSWORD_LENGTH}
          required
          aria-invalid={localError !== null}
          aria-describedby={described}
          bind:value={confirmation}
        />
      {/snippet}
    </FormField>

    <div class="tp-field__actions">
      <button class="tp-button" type="submit" disabled={busy}>
        {busy ? 'Creating the pool…' : 'Create the pool and sign in'}
      </button>
    </div>
  </form>
</section>

<style>
  .tp-auth {
    display: grid;
    gap: var(--tp-space-3);
    max-width: 34rem;
    padding: var(--tp-space-5);
    background: var(--tp-surface);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-auth__title {
    margin: 0;
    font-size: var(--tp-font-heading);
    color: var(--tp-text-strong);
  }

  .tp-auth__lede {
    margin: 0;
    color: var(--tp-text-muted);
    max-width: 68ch;
  }

  .tp-auth__notice {
    margin: 0;
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    color: var(--tp-text-strong);
  }

  .tp-auth__form {
    display: grid;
    gap: var(--tp-space-2);
  }
</style>
