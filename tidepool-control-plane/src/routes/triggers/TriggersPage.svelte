<script lang="ts">
  /**
   * The trigger editor.
   *
   * Choose a project, a host and a working directory; the pool then asks that
   * host what it can run and the form offers only those choices. The same
   * document is editable as a form or as YAML, and saving runs the pool's own
   * validate endpoint before installing anything.
   *
   * The page renders from a `TriggersViewState`. It loads itself on mount when
   * it has a pool and a session token (or an injected transport), and it can be
   * rendered from an initial state in tests, which is how the connected,
   * offline, unavailable-value and YAML-error states are asserted without a
   * browser.
   */
  import ProblemDetails from '../../components/ProblemDetails.svelte';
  import StatusBadge from '../../components/StatusBadge.svelte';
  import AgentSourcePanel from './AgentSourcePanel.svelte';
  import TriggerForm from './TriggerForm.svelte';
  import YamlTriggerEditor from './YamlTriggerEditor.svelte';
  import { canUseControl, controlById, denialReason } from '../../auth/roles';
  import type { Role } from '../../auth/types';
  import { poolClient, isPoolConfigured, type PoolConfig } from '$lib/api/pool';
  import {
    TriggersController,
    canLoadTriggers,
    emptyTriggersState,
    saveSummary,
    selectionAvailability,
    triggersTransport,
    type TriggerEdit,
    type TriggersTransport,
    type TriggersViewState,
  } from '$lib/triggers';

  interface Props {
    /** The pool this control plane is pointed at. */
    pool?: PoolConfig;
    /** The signed-in operator's token, used to load projects and hosts. */
    token?: string | null;
    /** The signed-in operator's role, for control gating. */
    role?: Role | null;
    /** The state to render before loading; also how tests render any state. */
    initial?: TriggersViewState;
    /** Transport override, for tests; otherwise built from the pool and token. */
    transport?: TriggersTransport | null;
  }

  let {
    pool = { baseUrl: null },
    token = null,
    role = null,
    initial = emptyTriggersState(),
    transport = null,
  }: Props = $props();

  let view = $state<TriggersViewState>(initial);

  /** Build the controller once: the page keeps one view of one pool. */
  const controller: TriggersController | null = (() => {
    if (transport) return new TriggersController({ transport });
    if (!canLoadTriggers(pool, token)) return null;
    try {
      return new TriggersController({ transport: triggersTransport(poolClient(pool, { token })) });
    } catch {
      return null;
    }
  })();

  const configured = transport !== null || isPoolConfigured(pool);
  const installControl = controlById('install-trigger');
  const canInstall = $derived(
    installControl !== null && canUseControl(role, installControl) && view.phase === 'ready',
  );
  const installDenial = $derived(
    installControl !== null && !canUseControl(role, installControl)
      ? denialReason(role, installControl)
      : null,
  );
  const selectedProject = $derived(
    view.projects.find((project) => project.id === view.selectedProjectId) ?? null,
  );
  const options = $derived(view.choices.status === 'ready' ? view.choices.options : null);
  const availability = $derived(
    selectionAvailability(options, view.document.run.agent),
  );
  const summary = $derived(saveSummary(view));

  /** Adopt a controller result. */
  function adopt(next: TriggersViewState): void {
    view = next;
  }

  /** Load projects and hosts, then the choices for the chosen host. */
  async function refresh(): Promise<void> {
    if (!controller) return;
    adopt(await controller.load());
  }

  $effect(() => {
    // A page that has a pool and no initial state loads itself once; a page
    // rendered from a supplied state keeps that state.
    if (controller && initial.phase === 'idle' && controller.snapshot().phase === 'idle') {
      void refresh();
    }
  });

  /** Choose the project the trigger belongs to. */
  function selectProject(projectId: string): void {
    if (!controller) {
      view = { ...view, selectedProjectId: projectId };
      return;
    }
    adopt(controller.selectProject(projectId));
  }

  /** Switch between form and YAML. */
  function setMode(mode: 'form' | 'yaml'): void {
    if (!controller) {
      if (mode === 'form' && view.yamlError !== null) return;
      view = { ...view, mode };
      return;
    }
    adopt(controller.setMode(mode));
  }

  /** Apply one form edit. */
  function edit(change: TriggerEdit): void {
    if (!controller) return;
    adopt(controller.apply(change));
  }

  /** Choose a host and reload its agent choices. */
  async function setHost(slug: string): Promise<void> {
    if (!controller) return;
    adopt(await controller.setHost(slug));
  }

  /** Change the working directory and reload the agent choices. */
  async function setDirectory(directory: string): Promise<void> {
    if (!controller) return;
    adopt(await controller.setDirectory(directory));
  }

  /** Adopt YAML only when it parses. */
  function editYaml(text: string): void {
    if (!controller) return;
    adopt(controller.editYaml(text));
  }

  /** Ask the host again after an offline or failed answer. */
  async function retryChoices(): Promise<void> {
    if (!controller) return;
    adopt(await controller.retryChoices());
  }

  /** Validate and install through the pool. */
  async function save(): Promise<void> {
    if (!controller) return;
    adopt(await controller.save());
  }
</script>

<section class="tp-triggers tp-page" aria-labelledby="tp-triggers-title" data-triggers-page="true">
  <h1 class="tp-page__title" id="tp-triggers-title">Triggers</h1>
  <p class="tp-page__lede">
    A trigger says which provider event may start an agent and on which host. Pick
    the host and working directory first; the pool asks that host what it can run,
    and only those providers, models, execution modes and thinking options are
    offered. The form and the YAML are one document.
  </p>

  {#if view.problem}
    <ProblemDetails problem={view.problem} onRetry={() => void refresh()} />
  {/if}

  {#if !configured}
    <p class="tp-page__state" data-triggers-state="unconfigured">
      <StatusBadge kind="pending" />
      <span>
        No pool is configured, so there is no host to ask and no trigger to
        install. Point the control plane at a pool and sign in.
      </span>
    </p>
  {:else}
    {#if view.phase === 'loading' && view.projects.length === 0}
      <p class="tp-page__state" data-triggers-state="loading">
        <StatusBadge kind="pending" />
        <span>Loading the projects and hosts in this organization…</span>
      </p>
    {:else if view.hosts.length === 0 && view.phase === 'ready'}
      <p class="tp-page__state" data-triggers-state="no-hosts">
        <StatusBadge kind="warning" />
        <span>
          No hosts are enrolled, so no agent choices can be loaded. Enrol a host
          on the Hosts page first.
        </span>
      </p>
    {:else}
      <div class="tp-triggers__toolbar">
        <label class="tp-field">
          <span class="tp-field__label">Project</span>
          <select
            class="tp-input"
            value={view.selectedProjectId ?? ''}
            data-project-select="true"
            onchange={(event) =>
              selectProject((event.currentTarget as HTMLSelectElement).value)}
          >
            <option value="">Choose a project…</option>
            {#each view.projects as project (project.id)}
              <option value={project.id}>{project.name} ({project.slug})</option>
            {/each}
          </select>
        </label>
        <p class="tp-triggers__project-note">
          {#if selectedProject}
            The trigger is installed into <strong>{selectedProject.name}</strong>.
          {:else}
            A trigger belongs to one project; choose the project it installs into.
          {/if}
        </p>
      </div>

      <AgentSourcePanel choices={view.choices} onRetry={() => void retryChoices()} />

      <div class="tp-triggers__modes" role="group" aria-label="Editor view">
        <button
          class="tp-button"
          type="button"
          aria-pressed={view.mode === 'form'}
          disabled={view.yamlError !== null}
          data-trigger-mode="form"
          onclick={() => setMode('form')}
        >
          Form
        </button>
        <button
          class="tp-button"
          type="button"
          aria-pressed={view.mode === 'yaml'}
          data-trigger-mode="yaml"
          onclick={() => setMode('yaml')}
        >
          YAML
        </button>
      </div>

      {#if view.mode === 'form'}
        <TriggerForm
          document={view.document}
          hosts={view.hosts}
          directory={view.directory}
          options={options}
          availability={availability}
          issues={view.issues}
          disabled={!canInstall}
          onEdit={edit}
          onHost={(slug) => void setHost(slug)}
          onDirectory={(directory) => void setDirectory(directory)}
        />
      {:else}
        <YamlTriggerEditor
          value={view.yaml}
          error={view.yamlError}
          disabled={!canInstall}
          onInput={editYaml}
        />
      {/if}

      {#if view.issues.length > 0}
        <section class="tp-triggers__issues" aria-labelledby="tp-trigger-issues-title">
          <h2 class="tp-triggers__issues-title" id="tp-trigger-issues-title">
            Fields that need attention
          </h2>
          <ul class="tp-triggers__issue-list" data-trigger-issues="true">
            {#each view.issues as issue, index (index)}
              <li class="tp-triggers__issue" data-issue-field={issue.field}>
                <code>{issue.field}</code>
                <span>{issue.message}</span>
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      <div class="tp-triggers__save">
        <button
          class="tp-button tp-triggers__save-button"
          type="button"
          disabled={view.busy || !canInstall || view.yamlError !== null}
          data-save-action="save"
          onclick={() => void save()}
        >
          {view.savePhase === 'validating' || view.savePhase === 'installing'
            ? 'Saving…'
            : 'Validate and install'}
        </button>
        <p class="tp-triggers__save-summary" data-save-phase={view.savePhase}>
          {summary}
        </p>
      </div>

      {#if installDenial}
        <p class="tp-triggers__denial" data-triggers-denial="install-trigger">
          {installDenial}
        </p>
      {/if}

      {#if view.saved}
        <p class="tp-triggers__saved" data-trigger-saved="true">
          <StatusBadge kind="success" />
          <span>
            Installed <strong>{view.saved.name}</strong> revision
            {view.saved.version} ({view.saved.revision}); it is
            {view.saved.active ? 'active' : 'not active'}.
          </span>
        </p>
      {/if}

      {#if view.saveProblem}
        <ProblemDetails problem={view.saveProblem} />
      {/if}
    {/if}
  {/if}
</section>

<style>
  .tp-triggers {
    display: grid;
    gap: var(--tp-space-3);
  }

  .tp-triggers__toolbar {
    display: grid;
    gap: var(--tp-space-1);
    grid-template-columns: minmax(14rem, 24rem) 1fr;
    align-items: end;
  }

  .tp-triggers__project-note {
    margin: 0;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-triggers__modes {
    display: flex;
    gap: var(--tp-space-1);
  }

  .tp-triggers__modes .tp-button[aria-pressed='true'] {
    border-color: var(--tp-primary);
  }

  .tp-triggers__issues {
    display: grid;
    gap: var(--tp-space-1);
    padding: var(--tp-space-2);
    border: 1px solid var(--tp-status-warning);
    border-radius: var(--tp-radius-panel);
  }

  .tp-triggers__issues-title {
    margin: 0;
    font-family: var(--tp-font-heading);
    font-size: var(--tp-font-body);
  }

  .tp-triggers__issue-list {
    display: grid;
    gap: var(--tp-space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tp-triggers__issue {
    display: flex;
    gap: var(--tp-space-2);
    align-items: baseline;
    flex-wrap: wrap;
  }

  .tp-triggers__save {
    display: flex;
    gap: var(--tp-space-2);
    align-items: center;
    flex-wrap: wrap;
  }

  .tp-triggers__save-summary {
    margin: 0;
    color: var(--tp-text-muted);
    font-family: var(--tp-font-caption);
  }

  .tp-triggers__denial {
    margin: 0;
    color: var(--tp-text-muted);
  }

  .tp-triggers__saved {
    display: flex;
    gap: var(--tp-space-2);
    align-items: baseline;
    margin: 0;
  }
</style>
