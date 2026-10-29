<script lang="ts">
  /**
   * The activity feed.
   *
   * One row per receipt, run, step or execution, newest first. The row says what
   * the record is in a sentence, marks its status with colour + icon + word, and
   * offers the run it belongs to. The project is named from the loaded project
   * list, never guessed from an id.
   */
  import StatusBadge from '../../components/StatusBadge.svelte';
  import {
    activityEntryRunId,
    activityEntrySentence,
    activityEntryTitle,
    formatTimestamp,
    kindLabel,
    projectLabel,
    statusTone,
    statusWord,
  } from '$lib/runs';
  import type { ActivityEntry, ProjectView } from '@clawffice/tidepool-sdk';

  interface Props {
    /** Entries already narrowed by the filters. */
    items: readonly ActivityEntry[];
    /** Projects, for readable project names. */
    projects: readonly ProjectView[];
    /** The run whose detail is open. */
    selectedRunId: string | null;
    /** Open a run. */
    onOpen: (runId: string) => void;
  }

  let { items, projects, selectedRunId, onOpen }: Props = $props();
</script>

{#if items.length === 0}
  <p class="tp-activity-list__empty" data-activity-empty="true">
    <StatusBadge kind="neutral" />
    <span>No records match these filters.</span>
  </p>
{:else}
  <ol class="tp-activity-list" data-activity-list="true">
    {#each items as entry (entry.id)}
      {@const runId = activityEntryRunId(entry)}
      <li
        class="tp-activity-list__row"
        data-activity-kind={entry.kind}
        data-activity-id={entry.id}
        data-activity-status={entry.status}
        data-activity-project={entry.project ?? ''}
      >
        <div class="tp-activity-list__heading">
          <StatusBadge kind={statusTone(entry.status)} />
          <span class="tp-activity-list__kind">{kindLabel(entry.kind)}</span>
          <h3 class="tp-activity-list__title">{activityEntryTitle(entry)}</h3>
        </div>
        <p class="tp-activity-list__sentence">{activityEntrySentence(entry)}</p>
        <div class="tp-activity-list__meta">
          <span>{statusWord(entry.status)}</span>
          <span>{formatTimestamp(entry.at)}</span>
          <span>{projectLabel(entry.project, projects)}</span>
          {#if entry.triggerName}<span>Trigger {entry.triggerName}</span>{/if}
          {#if entry.step}<span>Step {entry.step}</span>{/if}
        </div>
        {#if runId}
          <button
            class="tp-button"
            type="button"
            data-open-run={runId}
            aria-pressed={selectedRunId === runId}
            onclick={() => onOpen(runId)}
          >
            {selectedRunId === runId ? 'Showing this run' : 'Open run'}
          </button>
        {/if}
      </li>
    {/each}
  </ol>
{/if}

<style>
  .tp-activity-list {
    display: grid;
    gap: var(--tp-space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-activity-list__row {
    display: grid;
    gap: var(--tp-space-1);
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-activity-list__heading {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: var(--tp-space-2);
  }

  .tp-activity-list__kind {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .tp-activity-list__title {
    margin: 0;
    font-family: var(--tp-font-heading);
    font-size: var(--tp-font-body);
    color: var(--tp-text-strong);
  }

  .tp-activity-list__sentence {
    margin: 0;
    max-width: 68ch;
  }

  .tp-activity-list__meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-2);
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-activity-list__empty {
    display: flex;
    align-items: center;
    gap: var(--tp-space-2);
    margin: 0;
    color: var(--tp-text-muted);
  }
</style>
