<script lang="ts">
  /** Landing for a path the route table does not know. */
  import StatusBadge from '../components/StatusBadge.svelte';

  interface Props {
    /** The path that was asked for. */
    path: string;
  }

  let { path }: Props = $props();
</script>

<section class="tp-page" aria-labelledby="tp-not-found-title">
  <h1 class="tp-page__title" id="tp-not-found-title">No such page</h1>
  <p class="tp-page__state">
    <StatusBadge kind="warning" />
    <span>There is no page at <code>{path}</code>.</span>
  </p>
  <p class="tp-page__lede">Use the navigation to reach a page that exists.</p>
</section>
