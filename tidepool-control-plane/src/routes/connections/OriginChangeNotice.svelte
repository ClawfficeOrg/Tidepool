<script lang="ts">
  /**
   * The callback URLs that must change when the pool's public origin moves.
   *
   * The old URL and the new URL are printed side by side, per connection, so
   * the operator can update each provider without guessing which path belongs
   * to which slug. Connections that dial out have no callback URL and are
   * correctly absent.
   */
  import type { CallbackChange } from '$lib/connections';

  interface Props {
    /** The origin the page last showed. */
    previousUrl: string;
    /** The origin the pool has now. */
    currentUrl: string;
    /** One entry per webhook connection whose URL moved. */
    changes: readonly CallbackChange[];
    /** The sentence above the list. */
    summary: string;
  }

  let { previousUrl, currentUrl, changes, summary }: Props = $props();
</script>

<section
  class="tp-origin-change"
  data-origin-change={changes.length === 0 ? 'none' : 'present'}
  aria-labelledby="tp-origin-change-title"
  role="status"
>
  <h2 class="tp-origin-change__title" id="tp-origin-change-title">
    The pool’s public origin changed
  </h2>
  <p class="tp-origin-change__summary" data-origin-summary="true">{summary}</p>

  {#if changes.length === 0}
    <p class="tp-origin-change__none">
      No connection uses a callback URL, so nothing needs changing at the
      provider.
    </p>
  {:else}
    <ul class="tp-origin-change__list">
      {#each changes as change (change.connectionId)}
        <li
          class="tp-origin-change__item"
          data-origin-change-for={change.connectionId}
        >
          <p class="tp-origin-change__identity">
            {change.provider} · <code>{change.slug}</code>
            <span class="tp-origin-change__name">{change.displayName}</span>
          </p>
          <dl class="tp-origin-change__urls">
            <div>
              <dt>Was</dt>
              <dd><code data-origin-previous-url={change.previousUrl}>{change.previousUrl}</code></dd>
            </div>
            <div>
              <dt>Now</dt>
              <dd><code data-origin-current-url={change.currentUrl}>{change.currentUrl}</code></dd>
            </div>
          </dl>
        </li>
      {/each}
    </ul>
  {/if}
  <p class="tp-origin-change__note">
    Previous origin: <code data-origin-previous={previousUrl}>{previousUrl}</code>.
    Current origin: <code data-origin-current={currentUrl}>{currentUrl}</code>.
  </p>
</section>

<style>
  .tp-origin-change {
    display: grid;
    gap: var(--tp-space-1);
    padding: var(--tp-space-3);
    background: var(--tp-surface);
    border: 1px solid var(--tp-status-warning);
    border-radius: var(--tp-radius-panel);
  }

  .tp-origin-change__title {
    margin: 0;
    color: var(--tp-text-strong);
    font-size: var(--tp-font-heading);
  }

  .tp-origin-change__summary,
  .tp-origin-change__none,
  .tp-origin-change__note {
    margin: 0;
    max-width: 78ch;
    color: var(--tp-text-muted);
  }

  .tp-origin-change__list {
    display: grid;
    gap: var(--tp-space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-origin-change__item {
    display: grid;
    gap: 2px;
    padding-top: var(--tp-space-2);
    border-top: 1px solid var(--tp-border);
  }

  .tp-origin-change__identity {
    margin: 0;
    color: var(--tp-text-strong);
  }

  .tp-origin-change__name {
    color: var(--tp-text-muted);
    font-weight: 400;
  }

  .tp-origin-change__urls {
    display: grid;
    gap: var(--tp-space-1);
    margin: 0;
  }

  .tp-origin-change__urls dt {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .tp-origin-change__urls dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
