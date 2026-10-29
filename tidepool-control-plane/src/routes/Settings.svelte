<script lang="ts">
  /** The settings page. Arrives with task 2.1.10. */
  import StatusBadge from '../components/StatusBadge.svelte';
</script>

<section class="tp-page" aria-labelledby="tp-settings-title">
  <h1 class="tp-page__title" id="tp-settings-title">Settings</h1>
  <p class="tp-page__lede">
    The pool URL, the palette, time and density preferences, and the version of
    this control plane.
  </p>
  <p class="tp-page__state">
    <StatusBadge kind="pending" />
    <span>Not built yet. Task 2.1.10 adds the pool URL and settings that survive a reload.</span>
  </p>
</section>
