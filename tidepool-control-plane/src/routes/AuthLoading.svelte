<script lang="ts">
  /**
   * The short state between mounting and knowing who is signed in.
   *
   * It is a status, not a spinner with no words: a screen reader hears that the
   * control plane is checking the session, and the page does not flash a
   * sign-in form that is about to be replaced.
   */
</script>

<section class="tp-auth" role="status" aria-live="polite" data-auth-page="loading">
  <h1 class="tp-auth__title">Checking your session</h1>
  <p class="tp-auth__lede">Asking the pool whether this browser is still signed in.</p>
</section>

<style>
  .tp-auth {
    display: grid;
    gap: var(--tp-space-2);
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
  }
</style>
