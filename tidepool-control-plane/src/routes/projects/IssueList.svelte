<script lang="ts">
  /**
   * The pool's field errors, each pinned to the authored line it belongs to.
   *
   * The pool names a field (`hosts.studio-mac`); this list says that field's
   * file and line, quotes the line, and repeats the pool's own message. An
   * issue the parser could not place says so instead of pretending a line.
   */
  import type { LocatedIssue } from '$lib/configuration';
  import { sortLocatedIssues } from '$lib/configuration';

  interface Props {
    /** Issues located against the draft manifest. */
    issues?: readonly LocatedIssue[];
    /** The file the issues were located in, for the heading. */
    fileName?: string;
  }

  let { issues = [], fileName = 'tidepool.json' }: Props = $props();

  const ordered = $derived(sortLocatedIssues(issues));
</script>

{#if ordered.length > 0}
  <section class="tp-issues" aria-labelledby="tp-issues-title" data-issues="true">
    <h3 class="tp-issues__title" id="tp-issues-title">
      {ordered.length}
      {ordered.length === 1 ? 'field error' : 'field errors'} in {fileName}
    </h3>
    <ol class="tp-issues__list">
      {#each ordered as issue (`${issue.field}:${issue.message}`)}
        <li
          class="tp-issues__item"
          data-issue-field={issue.field}
          data-issue-line={issue.line}
          data-issue-column={issue.column}
          data-issue-located={issue.located ? 'true' : 'false'}
        >
          <p class="tp-issues__where">
            <span class="tp-issues__line">
              {issue.located ? `${issue.file}:${issue.line}` : `${issue.file} (line not located)`}
            </span>
            <code class="tp-issues__field">{issue.field}</code>
          </p>
          <p class="tp-issues__message">{issue.message}</p>
          {#if issue.snippet !== ''}
            <pre class="tp-issues__snippet"><code>{issue.snippet}</code></pre>
          {/if}
        </li>
      {/each}
    </ol>
  </section>
{/if}

<style>
  .tp-issues {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
  }

  .tp-issues__title {
    margin: 0;
    font-size: var(--tp-font-caption);
  }

  .tp-issues__list {
    display: grid;
    gap: var(--tp-space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-issues__item {
    display: grid;
    gap: 0.25rem;
    padding: var(--tp-space-1) var(--tp-space-2);
    border-left: 3px solid var(--tp-status-danger);
  }

  .tp-issues__where {
    display: flex;
    gap: var(--tp-space-2);
    align-items: baseline;
    margin: 0;
  }

  .tp-issues__line {
    font-variant-numeric: tabular-nums;
    color: var(--tp-text-muted);
  }

  .tp-issues__field {
    font-size: var(--tp-font-caption);
  }

  .tp-issues__message {
    margin: 0;
  }

  .tp-issues__snippet {
    margin: 0;
    padding: var(--tp-space-1);
    overflow-x: auto;
    background: var(--tp-terminal-background);
    border-radius: var(--tp-radius-control);
  }
</style>
