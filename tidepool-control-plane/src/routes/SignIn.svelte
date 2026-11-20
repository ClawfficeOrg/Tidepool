<script lang="ts">
  /**
   * Operator sign-in.
   *
   * The form is the only way into the control plane: the pool returns a token,
   * the controller remembers it, and a reload restores it. A failure is shown
   * as problem details (with its request id) above the form so the credential
   * fields stay usable.
   */
  import ProblemDetails from '../components/ProblemDetails.svelte';
  import FormField from '../components/FormField.svelte';
  import type { PoolProblem } from '$lib/api/problem';
  import type { LoginInput } from '../auth/types';

  interface Props {
    /** The pool this form signs in to. */
    poolUrl?: string | null;
    /** Why the person is here, when it was not a fresh visit. */
    notice?: string | null;
    /** A failure from the last attempt. */
    problem?: PoolProblem | null;
    /** Called with the credentials when the form is submitted. */
    onSubmit?: (input: LoginInput) => void | Promise<void>;
  }

  let { poolUrl = null, notice = null, problem = null, onSubmit }: Props = $props();

  let email = $state('');
  let password = $state('');
  let busy = $state(false);

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (busy) return;
    busy = true;
    try {
      await onSubmit?.({ email, password });
    } finally {
      busy = false;
    }
  }
</script>

<section class="tp-auth" aria-labelledby="tp-signin-title" data-auth-page="signin">
  <h1 class="tp-auth__title" id="tp-signin-title">Sign in</h1>
  <p class="tp-auth__lede">
    {#if poolUrl}
      Sign in to the pool at <code>{poolUrl}</code>. Your session survives a reload
      and ends when you sign out.
    {:else}
      Sign in to the pool. Your session survives a reload and ends when you sign out.
    {/if}
  </p>

  {#if notice}
    <p class="tp-auth__notice" role="status">{notice}</p>
  {/if}

  {#if problem}
    <ProblemDetails {problem} />
  {/if}

  <form class="tp-auth__form" onsubmit={submit} data-form="signin">
    <FormField form="signin" field="email" label="Email" required help="The address you sign in with.">
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
      form="signin"
      field="password"
      label="Password"
      required
      help="The password set for this account."
    >
      {#snippet children(id, described)}
        <input
          class="tp-field__control"
          {id}
          type="password"
          name="password"
          autocomplete="current-password"
          required
          aria-describedby={described}
          bind:value={password}
        />
      {/snippet}
    </FormField>

    <div class="tp-field__actions">
      <button class="tp-button" type="submit" disabled={busy}>
        {busy ? 'Signing in…' : 'Sign in'}
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
