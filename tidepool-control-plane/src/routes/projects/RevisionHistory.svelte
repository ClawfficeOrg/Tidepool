<script lang="ts">
  /**
   * The revision history: what was stored, what is active, and what to compare.
   *
   * The active revision is named on its own row and in the editor's banner.
   * The list never infers an active row from recency; the pool's `active` flag
   * is carried through, and an ambiguous answer is reported by the page rather
   * than resolved here.
   */
  import StatusBadge from '../../components/StatusBadge.svelte';
  import type { RevisionRow } from '$lib/configuration';

  interface Props {
    /** Revisions, newest version first. */
    rows?: readonly RevisionRow[];
    /** The id the page resolved as active, when unambiguous. */
    activeRevisionId?: string | null;
    /** The revision currently shown in the diff, if any. */
    selectedRevisionId?: string | null;
    /** True while a pool call is in flight. */
    busy?: boolean;
    /** Ask for the diff from the active revision to this one. */
    onCompare?: (revisionId: string) => void;
  }

  let {
    rows = [],
    activeRevisionId = null,
    selectedRevisionId = null,
    busy = false,
    onCompare,
  }: Props = $props();
</script>

<section class="tp-history" aria-labelledby="tp-history-title" data-revision-history="true">
  <h3 class="tp-history__title" id="tp-history-title">Revision history</h3>
  {#if rows.length === 0}
    <p class="tp-page__state" data-history-empty="true">
      No revision has been installed for this project yet.
    </p>
  {:else}
    <ol class="tp-history__list">
      {#each rows as row (row.id)}
        <li
          class="tp-history__item"
          data-revision-id={row.id}
          data-revision-version={row.version}
          data-revision-active={row.active ? 'true' : 'false'}
        >
          <div class="tp-history__head">
            <span class="tp-history__version">v{row.version}</span>
            <StatusBadge kind={row.status === 'valid' ? 'success' : 'danger'} />
            {#if row.active}
              <span class="tp-history__active" data-active-marker="true">
                Active{row.id === activeRevisionId ? '' : ' (not the served active revision)'}
              </span>
            {/if}
            {#if row.errorCount > 0}
              <span class="tp-history__errors">
                {row.errorCount}
                {row.errorCount === 1 ? 'field error' : 'field errors'}
              </span>
            {/if}
          </div>
          <p class="tp-history__meta">
            <time datetime={row.createdAt}>{row.createdAt}</time>
          </p>
          {#if onCompare}
            <button
              class="tp-button"
              type="button"
              disabled={busy}
              data-history-action="compare"
              data-compare-revision={row.id}
              onclick={() => onCompare(row.id)}
            >
              {row.id === selectedRevisionId ? 'Refresh diff' : 'Compare with active'}
            </button>
          {/if}
        </li>
      {/each}
    </ol>
  {/if}
</section>

<style>
  .tp-history {
    display: grid;
    gap: var(--tp-space-2);
  }

  .tp-history__title {
    margin: 0;
    font-family: var(--tp-font-heading);
    font-size: var(--tp-font-body);
  }

  .tp-history__list {
    display: grid;
    gap: var(--tp-space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-history__item {
    display: grid;
    gap: var(--tp-space-1);
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-control);
  }

  .tp-history__item[data-revision-active='true'] {
    border-color: var(--tp-primary);
  }

  .tp-history__head {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-2);
    align-items: baseline;
  }

  .tp-history__version {
    font-family: var(--tp-font-heading);
    color: var(--tp-text-strong);
  }

  .tp-history__active {
    padding: 0 var(--tp-space-1);
    border-radius: var(--tp-radius-chip);
    background: var(--tp-primary);
    color: var(--tp-primary-text);
    font-family: var(--tp-font-caption);
  }

  .tp-history__errors,
  .tp-history__meta {
    margin: 0;
    color: var(--tp-text-muted);
    font-family: var(--tp-font-caption);
  }
</style>
