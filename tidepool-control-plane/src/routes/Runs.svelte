<script lang="ts">
  /** The runs and activity page. Arrives with task 2.1.7. */
  import StatusBadge from '../components/StatusBadge.svelte';
</script>

<section class="tp-page" aria-labelledby="tp-runs-title">
  <h1 class="tp-page__title" id="tp-runs-title">Runs</h1>
  <p class="tp-page__lede">
    What arrived, whether it matched a trigger, what ran, and what each step
    wrote.
  </p>
  <p class="tp-page__state">
    <StatusBadge kind="pending" />
    <span>Not built yet. Task 2.1.7 adds activity, filters and the live run detail.</span>
  </p>
</section>
