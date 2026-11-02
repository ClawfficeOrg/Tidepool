<script lang="ts">
  /**
   * Who is signed in, and the one control that ends the session.
   *
   * The role is always visible in words, and sign-out is a deliberate button
   * rather than a menu item: it asks the pool to end the session and only then
   * reports success.
   */
  import { roleLabel } from './roles';
  import type { AuthSession } from './types';

  interface Props {
    /** The signed-in session. */
    session: AuthSession;
    /** Called when the person asks to sign out. */
    onSignOut?: () => void | Promise<void>;
  }

  let { session, onSignOut }: Props = $props();
  let busy = $state(false);

  async function signOut(): Promise<void> {
    if (busy) return;
    busy = true;
    try {
      await onSignOut?.();
    } finally {
      busy = false;
    }
  }
</script>

<div class="tp-session" data-role={session.user.role} role="group" aria-label="Signed in">
  <span class="tp-session__person" data-session-user={session.user.id}>
    {session.user.display_name}
  </span>
  <span class="tp-session__role">{roleLabel(session.user.role)}</span>
  {#if session.organization}
    <span class="tp-session__org">{session.organization.name}</span>
  {/if}
  <button class="tp-button tp-session__signout" type="button" onclick={signOut} disabled={busy}>
    {busy ? 'Signing out…' : 'Sign out'}
  </button>
</div>

<style>
  .tp-session {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--tp-space-2);
    padding: var(--tp-space-1) var(--tp-space-2);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-control);
    background: var(--tp-terminal-background);
  }

  .tp-session__person {
    font-weight: 600;
    color: var(--tp-text-strong);
  }

  .tp-session__role {
    padding: 0 var(--tp-space-1);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-chip);
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .tp-session__org {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-session__signout {
    justify-self: auto;
  }
</style>
