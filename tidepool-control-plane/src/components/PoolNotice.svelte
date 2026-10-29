<script lang="ts">
  /**
   * What the shell says about the pool: configured, or not.
   *
   * "No pool configured" is the first-run state and is stated plainly, with the
   * one action that resolves it. It is a notice, not an error: nothing failed.
   */
  import StatusBadge from './StatusBadge.svelte';
  import { NO_POOL_MESSAGE, isPoolConfigured, type PoolConfig } from '$lib/api/pool';

  interface Props {
    /** What the app knows about the pool. */
    config: PoolConfig;
    /** Called when a person asks to set the pool URL. */
    onConfigure?: () => void;
  }

  let { config, onConfigure }: Props = $props();
  const configured = $derived(isPoolConfigured(config));
</script>

<section class="tp-pool-notice" data-configured={configured} aria-labelledby="tp-pool-notice-title">
  <h2 class="tp-pool-notice__title" id="tp-pool-notice-title">Pool</h2>
  {#if configured}
    <p class="tp-pool-notice__body">
      <StatusBadge kind="info" />
      Connected to <code>{config.baseUrl}</code>. Requests carry this origin's
      credentials when you sign in.
    </p>
  {:else}
    <p class="tp-pool-notice__body" role="status">
      <StatusBadge kind="warning" />
      <span>{NO_POOL_MESSAGE}</span>
    </p>
    <button class="tp-button" type="button" onclick={() => onConfigure?.()}>Set the pool URL</button>
  {/if}
</section>
