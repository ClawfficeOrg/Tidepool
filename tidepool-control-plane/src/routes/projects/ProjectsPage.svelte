<script lang="ts">
  /**
   * Projects and the configuration editor.
   *
   * Pick a project, edit its authored file bundle as text, and save through the
   * pool's own validate/install endpoint. The page shows the active revision,
   * the history, and a diff between revisions over the authored files. A save
   * that the pool refuses keeps the active revision where it was and pins each
   * named field error to the offending line of the manifest.
   *
   * The page renders from a `ProjectsViewState`. It loads itself on mount when
   * it has a pool and a session token (or an injected transport), and it can be
   * rendered from an initial state in tests, which is how the refusal, the
   * ambiguous-active and the diff states are asserted without a browser.
   */
  import ProblemDetails from '../../components/ProblemDetails.svelte';
  import StatusBadge from '../../components/StatusBadge.svelte';
  import FileEditor from './FileEditor.svelte';
  import RevisionDiff from './RevisionDiff.svelte';
  import RevisionHistory from './RevisionHistory.svelte';
  import { canUseControl, controlById } from '../../auth/roles';
  import type { Role } from '../../auth/types';
  import { poolClient, isPoolConfigured, type PoolConfig } from '$lib/api/pool';
  import {
    activeRevisionLabel,
    canLoadProjects,
    draftFiles,
    emptyProjectsState,
    ProjectsController,
    projectsTransport,
    saveSummary,
    type ProjectsTransport,
    type ProjectsViewState,
  } from '$lib/configuration';

  interface Props {
    /** The pool this control plane is pointed at. */
    pool?: PoolConfig;
    /** The signed-in operator's token, used to load projects. */
    token?: string | null;
    /** The signed-in operator's role, for control gating. */
    role?: Role | null;
    /** The state to render before loading; also how tests render any state. */
    initial?: ProjectsViewState;
    /** Transport override, for tests; otherwise built from the pool and token. */
    transport?: ProjectsTransport | null;
  }

  let {
    pool = { baseUrl: null },
    token = null,
    role = null,
    initial = emptyProjectsState(),
    transport = null,
  }: Props = $props();

  let view = $state<ProjectsViewState>(initial);
  let selectedPath = $state<string | null>(null);

  /** Build the controller once: the page keeps one view of one pool. */
  const controller: ProjectsController | null = (() => {
    if (transport) return new ProjectsController({ transport });
    if (!canLoadProjects(pool, token)) return null;
    try {
      return new ProjectsController({ transport: projectsTransport(poolClient(pool, { token })) });
    } catch {
      return null;
    }
  })();

  const configured = transport !== null || isPoolConfigured(pool);
  const saveControl = controlById('install-configuration');
  const canSave = $derived(
    saveControl !== null && canUseControl(role, saveControl) && view.activeProblem === null,
  );
  const selectedProject = $derived(
    view.projects.find((project) => project.id === view.selectedProjectId) ?? null,
  );
  const files = $derived(draftFiles(view.draft, view.activeRevision));
  const openPath = $derived.by(() => {
    if (selectedPath !== null && files.some((file) => file.path === selectedPath)) {
      return selectedPath;
    }
    return files[0]?.path ?? null;
  });
  const activeLabel = $derived(activeRevisionLabel(view));
  const summary = $derived(saveSummary(view));

  /** Adopt a controller result and keep the open file if it still exists. */
  function adopt(next: ProjectsViewState): void {
    view = next;
  }

  /** Load the project list, on mount and on retry. */
  async function refresh(): Promise<void> {
    if (!controller) return;
    adopt(await controller.load());
  }

  $effect(() => {
    if (controller && initial.phase === 'idle' && controller.snapshot().phase === 'idle') {
      void refresh();
    }
  });

  /** Open one project. */
  async function selectProject(projectId: string): Promise<void> {
    if (!controller) return;
    selectedPath = null;
    adopt(await controller.selectProject(projectId));
  }

  /** Replace one file's exact content. */
  function editFile(path: string, content: string): void {
    if (!controller) return;
    adopt(controller.editFile(path, content));
  }

  /** Add a path at the end of the bundle. */
  function addFile(path: string): void {
    if (!controller) return;
    adopt(controller.addFile(path));
    selectedPath = path;
  }

  /** Remove one path. */
  function removeFile(path: string): void {
    if (!controller) return;
    adopt(controller.removeFile(path));
    if (selectedPath === path) selectedPath = null;
  }

  /** Validate and install the draft through the pool. */
  async function save(): Promise<void> {
    if (!controller) return;
    adopt(await controller.save());
  }

  /** Diff one revision against the active one. */
  async function compare(revisionId: string): Promise<void> {
    if (!controller) return;
    adopt(await controller.compare(revisionId));
  }

  /** Close the diff view. */
  function clearComparison(): void {
    if (!controller) return;
    adopt(controller.clearComparison());
  }
</script>

<section class="tp-projects tp-page" aria-labelledby="tp-projects-title" data-projects-page="true">
  <h1 class="tp-page__title" id="tp-projects-title">Projects</h1>
  <p class="tp-page__lede">
    A project's configuration is the bundle of files its author wrote. Edit the
    files as text and save through the pool's own validation; the pool stores
    the exact authored bytes as an immutable revision. The editor never
    reorders, normalises or reformats a file.
  </p>

  {#if view.problem}
    <ProblemDetails problem={view.problem} onRetry={() => void refresh()} />
  {/if}

  {#if !configured}
    <p class="tp-page__state" data-projects-state="unconfigured">
      <StatusBadge kind="pending" />
      <span>
        No pool is configured, so there are no projects to show. Point the
        control plane at a pool and sign in.
      </span>
    </p>
  {:else}
    {#if view.projects.length === 0 && (view.phase === 'idle' || view.phase === 'loading')}
      <p class="tp-page__state" data-projects-state="loading">
        <StatusBadge kind="pending" />
        <span>Loading the projects in this organization…</span>
      </p>
    {:else if view.projects.length === 0 && view.phase !== 'error'}
      <p class="tp-page__state" data-projects-state="empty">
        <StatusBadge kind="neutral" />
        <span>
          No projects exist in this organization yet. Create one with the pool
          API and its configuration can be authored here.
        </span>
      </p>
    {:else if view.projects.length > 0}
      <nav class="tp-projects__list" aria-label="Projects" data-project-list="true">
        <ul class="tp-projects__items">
          {#each view.projects as project (project.id)}
            <li>
              <button
                class="tp-projects__item"
                type="button"
                aria-current={project.id === view.selectedProjectId ? 'true' : undefined}
                data-project-id={project.id}
                onclick={() => void selectProject(project.id)}
              >
                <span class="tp-projects__name">{project.name}</span>
                <code class="tp-projects__slug">{project.slug}</code>
              </button>
            </li>
          {/each}
        </ul>
      </nav>

      {#if selectedProject === null}
        <p class="tp-page__state" data-projects-state="no-selection">
          <StatusBadge kind="neutral" />
          <span>Choose a project to edit its configuration.</span>
        </p>
      {:else}
        <section class="tp-projects__panel" aria-labelledby="tp-project-title">
          <header class="tp-projects__header">
            <h2 class="tp-projects__title" id="tp-project-title">{selectedProject.name}</h2>
            <p class="tp-projects__active" data-active-revision="true">
              {activeLabel}
              {#if view.activeRevision}
                <span class="tp-projects__active-meta">
                  — {view.activeRevision.status} · stored {view.activeRevision.created_at}
                </span>
              {/if}
            </p>
          </header>

          {#if view.activeProblem}
            <p class="tp-projects__problem" data-active-problem="true" role="status">
              <StatusBadge kind="warning" />
              <span>{view.activeProblem}</span>
            </p>
          {/if}

          <FileEditor
            {files}
            selectedPath={openPath}
            issues={view.issues}
            busy={view.busy}
            canEdit={canSave}
            onSelect={(path) => (selectedPath = path)}
            onEdit={editFile}
            onAdd={canSave ? addFile : undefined}
            onRemove={canSave ? removeFile : undefined}
          />

          <div class="tp-projects__save">
            <button
              class="tp-button tp-projects__save-button"
              type="button"
              disabled={view.busy || !canSave}
              data-save-action="save"
              onclick={() => void save()}
            >
              {view.savePhase === 'validating' || view.savePhase === 'installing'
                ? 'Saving…'
                : 'Save and activate'}
            </button>
            <p class="tp-projects__save-summary" data-save-phase={view.savePhase}>
              {summary}
            </p>
          </div>

          {#if view.saveProblem}
            <ProblemDetails problem={view.saveProblem} />
          {/if}

          {#if view.comparison}
            <RevisionDiff comparison={view.comparison} onClose={clearComparison} />
          {/if}

          <RevisionHistory
            rows={view.history}
            activeRevisionId={view.activeRevisionId}
            selectedRevisionId={view.selectedRevisionId}
            busy={view.busy}
            onCompare={(revisionId) => void compare(revisionId)}
          />
        </section>
      {/if}
    {/if}
  {/if}
</section>

<style>
  .tp-projects {
    display: grid;
    gap: var(--tp-space-3);
  }

  .tp-projects__items {
    display: flex;
    flex-wrap: wrap;
    gap: var(--tp-space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-projects__item {
    display: inline-flex;
    gap: var(--tp-space-2);
    align-items: baseline;
    padding: var(--tp-space-1) var(--tp-space-2);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-chip);
    background: transparent;
    color: var(--tp-text);
    cursor: pointer;
  }

  .tp-projects__item[aria-current='true'] {
    border-color: var(--tp-primary);
  }

  .tp-projects__name {
    font-weight: 600;
  }

  .tp-projects__slug {
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-projects__panel {
    display: grid;
    gap: var(--tp-space-3);
    padding: var(--tp-space-3);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-panel);
  }

  .tp-projects__header {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-projects__title {
    margin: 0;
    font-family: var(--tp-font-heading);
    font-size: var(--tp-font-title);
  }

  .tp-projects__active {
    margin: 0;
    color: var(--tp-text-strong);
    font-family: var(--tp-font-heading);
  }

  .tp-projects__active-meta {
    color: var(--tp-text-muted);
    font-family: var(--tp-font-caption);
  }

  .tp-projects__problem {
    display: flex;
    gap: var(--tp-space-2);
    align-items: baseline;
    margin: 0;
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-status-warning);
    border-radius: var(--tp-radius-control);
  }

  .tp-projects__save {
    display: flex;
    gap: var(--tp-space-2);
    align-items: center;
    flex-wrap: wrap;
  }

  .tp-projects__save-summary {
    margin: 0;
    color: var(--tp-text-muted);
    font-family: var(--tp-font-caption);
  }
</style>
