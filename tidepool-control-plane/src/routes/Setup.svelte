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

  {#if localError}
    <p class="tp-auth__notice" role="alert">{localError}</p>
  {/if}

  {#if problem}
    <ProblemDetails {problem} />
  {/if}

  <form class="tp-auth__form" onsubmit={submit}>
    <label class="tp-auth__field">
      <span class="tp-auth__label">Organization name</span>
      <input
        class="tp-auth__input"
        type="text"
        name="organization_name"
        autocomplete="organization"
        required
        bind:value={organizationName}
      />
    </label>

    <label class="tp-auth__field">
      <span class="tp-auth__label">Your name</span>
      <input
        class="tp-auth__input"
        type="text"
        name="display_name"
        autocomplete="name"
        required
        bind:value={displayName}
      />
    </label>

    <label class="tp-auth__field">
      <span class="tp-auth__label">Email</span>
      <input
        class="tp-auth__input"
        type="email"
        name="email"
        autocomplete="username"
        required
        bind:value={email}
      />
    </label>

    <label class="tp-auth__field">
      <span class="tp-auth__label">Password</span>
      <input
        class="tp-auth__input"
        type="password"
        name="password"
        autocomplete="new-password"
        minlength={MIN_SETUP_PASSWORD_LENGTH}
        required
        bind:value={password}
      />
      <span class="tp-auth__hint">At least {MIN_SETUP_PASSWORD_LENGTH} characters.</span>
    </label>

    <label class="tp-auth__field">
      <span class="tp-auth__label">Confirm password</span>
      <input
        class="tp-auth__input"
        type="password"
        name="password_confirmation"
        autocomplete="new-password"
        minlength={MIN_SETUP_PASSWORD_LENGTH}
        required
        bind:value={confirmation}
      />
    </label>

    <button class="tp-button" type="submit" disabled={busy}>
      {busy ? 'Creating the pool…' : 'Create the pool and sign in'}
    </button>
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

  .tp-auth__field {
    display: grid;
    gap: 2px;
  }

  .tp-auth__label {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .tp-auth__hint {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-auth__input {
    padding: var(--tp-space-1) var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    background: var(--tp-terminal-background);
    color: var(--tp-text-strong);
    font: inherit;
  }
</style>
