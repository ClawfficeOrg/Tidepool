<script lang="ts">
  /**
   * The activity filters.
   *
   * Project, provider, trigger, status and kind combine: every chosen field
   * narrows the same feed. The project chooser lists the organization's
   * projects by name and sends the project id, which is what the pool's union
   * is scoped by; an empty value means "all".
   */
  import {
    ACTIVITY_KINDS,
    ACTIVITY_PROVIDERS,
    ACTIVITY_STATUSES,
    activityFilterSummary,
    filtersActive,
    kindLabel,
    providerLabel,
    statusWord,
  } from '$lib/runs';
  import type { ActivityFilterField, ActivityFilters } from '$lib/runs';
  import type { ProjectView } from '@clawffice/tidepool-sdk';

  interface Props {
    /** The active filters. */
    filters: ActivityFilters;
    /** Projects the project chooser offers. */
    projects: readonly ProjectView[];
    /** True while a reload is in flight. */
    busy: boolean;
    /** Apply one filter edit. */
    onFilter: (field: ActivityFilterField, value: string) => void;
    /** Drop every filter. */
    onClear: () => void;
  }

  let { filters, projects, busy, onFilter, onClear }: Props = $props();

  const summary = $derived(activityFilterSummary(filters));
  const active = $derived(filtersActive(filters));
</script>

<section class="tp-activity-filters" aria-labelledby="tp-activity-filters-title" data-activity-filters="true">
  <div class="tp-activity-filters__header">
    <h2 class="tp-activity-filters__title" id="tp-activity-filters-title">Filters</h2>
    <button
      class="tp-button"
      type="button"
      data-filter-clear="true"
      disabled={busy || !active}
      onclick={onClear}
    >
      Clear filters
    </button>
  </div>

  <div class="tp-activity-filters__grid">
    <label class="tp-field">
      <span class="tp-field__label">Project</span>
      <select
        class="tp-input"
        value={filters.project}
        data-filter="project"
        disabled={busy}
        onchange={(event) => onFilter('project', (event.currentTarget as HTMLSelectElement).value)}
      >
        <option value="">All projects</option>
        {#each projects as project (project.id)}
          <option value={project.id}>{project.name} ({project.slug})</option>
        {/each}
      </select>
    </label>

    <label class="tp-field">
      <span class="tp-field__label">Provider</span>
      <select
        class="tp-input"
        value={filters.provider}
        data-filter="provider"
        disabled={busy}
        onchange={(event) => onFilter('provider', (event.currentTarget as HTMLSelectElement).value)}
      >
        <option value="">All providers</option>
        {#each ACTIVITY_PROVIDERS as provider (provider)}
          <option value={provider}>{providerLabel(provider)}</option>
        {/each}
      </select>
    </label>

    <label class="tp-field">
      <span class="tp-field__label">Trigger</span>
      <input
        class="tp-input"
        type="search"
        value={filters.trigger}
        placeholder="Trigger name or id"
        data-filter="trigger"
        disabled={busy}
        onchange={(event) => onFilter('trigger', (event.currentTarget as HTMLInputElement).value)}
      />
    </label>

    <label class="tp-field">
      <span class="tp-field__label">Status</span>
      <select
        class="tp-input"
        value={filters.status}
        data-filter="status"
        disabled={busy}
        onchange={(event) => onFilter('status', (event.currentTarget as HTMLSelectElement).value)}
      >
        <option value="">Any status</option>
        {#each ACTIVITY_STATUSES as status (status)}
          <option value={status}>{statusWord(status)}</option>
        {/each}
      </select>
    </label>

    <label class="tp-field">
      <span class="tp-field__label">Kind</span>
      <select
        class="tp-input"
        value={filters.kind}
        data-filter="kind"
        disabled={busy}
        onchange={(event) => onFilter('kind', (event.currentTarget as HTMLSelectElement).value)}
      >
        <option value="">All records</option>
        {#each ACTIVITY_KINDS as kind (kind)}
          <option value={kind}>{kindLabel(kind)}</option>
        {/each}
      </select>
    </label>
  </div>

  <p class="tp-activity-filters__summary" data-filter-summary="true">{summary}</p>
</section>

<style>
  .tp-activity-filters {
    display: grid;
    gap: var(--tp-space-2);
    padding: var(--tp-space-3);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-activity-filters__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--tp-space-2);
  }

  .tp-activity-filters__title {
    margin: 0;
    font-family: var(--tp-font-heading);
    font-size: var(--tp-font-body);
    color: var(--tp-text-strong);
  }

  .tp-activity-filters__grid {
    display: grid;
    gap: var(--tp-space-2);
    grid-template-columns: repeat(auto-fit, minmax(11rem, 1fr));
  }

  .tp-field {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-field__label {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .tp-input {
    padding: var(--tp-space-1) var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    background: var(--tp-surface);
    color: var(--tp-text-strong);
    font: inherit;
  }

  .tp-activity-filters__summary {
    margin: 0;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }
</style>
