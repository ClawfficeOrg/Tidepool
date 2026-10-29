<script lang="ts">
  /** The hosts page. Arrives with task 2.1.3. */
  import StatusBadge from '../components/StatusBadge.svelte';
</script>

<section class="tp-page" aria-labelledby="tp-hosts-title">
  <h1 class="tp-page__title" id="tp-hosts-title">Hosts</h1>
  <p class="tp-page__lede">
    Every machine enrolled with this pool, with its slug, display name, last
    seen, and whether it may run agents.
  </p>
  <p class="tp-page__state">
    <StatusBadge kind="pending" />
    <span>Not built yet. Task 2.1.3 adds enrolment and the execution grant.</span>
  </p>
</section>
