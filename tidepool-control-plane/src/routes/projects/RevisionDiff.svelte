<script lang="ts">
  /**
   * The diff between two revisions, over the authored files.
   *
   * Each file is listed by its authored path and status; a changed file shows
   * its aligned lines with both line numbers, so the author reads exactly the
   * text they wrote and exactly what moved. There is no normalisation step
   * between the stored revision and this view.
   */
  import type { FileDiff, RevisionComparison } from '$lib/configuration';
  import { changedFileDiffs, diffSummary } from '$lib/configuration';

  interface Props {
    /** The comparison to render. */
    comparison: RevisionComparison;
    /** Called when the diff is dismissed. */
    onClose?: () => void;
  }

  let { comparison, onClose }: Props = $props();

  const files = $derived(changedFileDiffs(comparison.files));
  const summary = $derived(diffSummary(comparison.files));

  /** The word shown for a file's status. */
  function statusWord(status: FileDiff['status']): string {
    switch (status) {
      case 'added':
        return 'Added';
      case 'removed':
        return 'Removed';
      case 'changed':
        return 'Changed';
      case 'unchanged':
        return 'Unchanged';
    }
  }

  /** The marker printed before a diff line. */
  function marker(kind: FileDiff['lines'][number]['kind']): string {
    if (kind === 'added') return '+';
    if (kind === 'removed') return '-';
    return ' ';
  }
</script>

<section class="tp-diff" aria-labelledby="tp-diff-title" data-revision-diff="true">
  <header class="tp-diff__header">
    <h3 class="tp-diff__title" id="tp-diff-title">
      {#if comparison.from}
        Changes from v{comparison.from.version} to v{comparison.to.version}
      {:else}
        Authored files in v{comparison.to.version}
      {/if}
    </h3>
    {#if onClose}
      <button class="tp-button" type="button" onclick={onClose} data-diff-action="close">
        Close diff
      </button>
    {/if}
  </header>

  <p class="tp-diff__summary" data-diff-summary="true">
    {comparison.identical
      ? 'This is the active revision, so the two sides are identical.'
      : summary}
  </p>

  {#if files.length === 0}
    <p class="tp-page__state" data-diff-empty="true">
      The two revisions hold the same authored files.
    </p>
  {:else}
    <ul class="tp-diff__files">
      {#each files as file (file.path)}
        <li
          class="tp-diff__file"
          data-diff-file={file.path}
          data-diff-status={file.status}
        >
          <h4 class="tp-diff__path">
            <code>{file.path}</code>
            <span class="tp-diff__status" data-diff-word={file.status}>
              {statusWord(file.status)}
            </span>
          </h4>
          <p class="tp-diff__counts">
            {file.oldLines} {file.oldLines === 1 ? 'line' : 'lines'} before,
            {file.newLines} after
          </p>
          {#if file.truncated}
            <p class="tp-diff__truncated" data-diff-truncated="true">
              This file is too large to align in the page; the line counts above
              describe the change.
            </p>
          {:else if file.lines.length > 0}
            <div class="tp-diff__body" role="table" aria-label="Diff of {file.path}">
              {#each file.lines as line, index (`${line.kind}:${index}`)}
                <div class="tp-diff__line" data-diff-line={line.kind} role="row">
                  <span class="tp-diff__number" aria-hidden="true">
                    {line.oldLine ?? ''}
                  </span>
                  <span class="tp-diff__number" aria-hidden="true">
                    {line.newLine ?? ''}
                  </span>
                  <span class="tp-diff__marker" aria-hidden="true">{marker(line.kind)}</span>
                  <code class="tp-diff__text">{line.text}</code>
                </div>
              {/each}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .tp-diff {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-diff__header {
    display: flex;
    gap: var(--tp-space-2);
    align-items: baseline;
    justify-content: space-between;
  }

  .tp-diff__title {
    margin: 0;
    font-family: var(--tp-font-heading);
  }

  .tp-diff__summary,
  .tp-diff__counts {
    margin: 0;
    color: var(--tp-text-muted);
    font-family: var(--tp-font-caption);
  }

  .tp-diff__files {
    display: grid;
    gap: var(--tp-space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-diff__file {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-diff__path {
    display: flex;
    gap: var(--tp-space-2);
    align-items: baseline;
    margin: 0;
    font-size: var(--tp-font-body);
  }

  .tp-diff__status {
    font-family: var(--tp-font-caption);
    color: var(--tp-text-muted);
  }

  .tp-diff__body {
    overflow-x: auto;
    padding: var(--tp-space-1);
    background: var(--tp-terminal-background);
    border-radius: var(--tp-radius-control);
    font-family: var(--tp-font-code);
    font-size: var(--tp-font-caption);
  }

  .tp-diff__line {
    display: grid;
    grid-template-columns: 4ch 4ch 2ch 1fr;
    gap: var(--tp-space-1);
    white-space: pre;
  }

  .tp-diff__line[data-diff-line='added'] .tp-diff__text {
    color: var(--tp-status-success);
  }

  .tp-diff__line[data-diff-line='removed'] .tp-diff__text {
    color: var(--tp-status-danger);
  }

  .tp-diff__number {
    text-align: right;
    color: var(--tp-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .tp-diff__marker {
    color: var(--tp-text-muted);
  }

  .tp-diff__text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .tp-diff__truncated {
    margin: 0;
    color: var(--tp-status-warning);
  }
</style>
