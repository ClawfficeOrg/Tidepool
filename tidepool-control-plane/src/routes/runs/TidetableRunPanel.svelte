<script lang="ts">
  /**
   * Start a TideTable run on a host, then follow it.
   *
   * The form picks a project and a host from lists, names a checkout, and
   * takes tasks. Dry run is on by default. A started job is polled every few
   * seconds while the pool says it is queued or running.
   */
  import { onDestroy, onMount } from 'svelte';
  import { canUseControl, controlById, denialReason } from '../../auth/roles';
  import type { Role } from '../../auth/types';
  import { isPoolConfigured, poolClient, type PoolConfig } from '$lib/api/pool';
  import {
    buildRunRequest,
    jobIsRunning,
    jobStatusLine,
    type TidetableRunInput,
  } from '$lib/tidetableRuns';

  interface Props {
    /** The configured pool, from the shell. */
    pool?: PoolConfig;
    /** The signed-in session's token. */
    token?: string | null;
    /** The signed-in role, for control gating. */
    role?: Role | null;
    /** How often a running job is re-read, in milliseconds. */
    pollMs?: number;
  }

  let { pool = { baseUrl: null }, token = null, role = null, pollMs = 3000 }: Props = $props();

  const dispatchControl = controlById('dispatch-run');
  const denial = $derived(
    dispatchControl !== null && !canUseControl(role, dispatchControl)
      ? denialReason(role, dispatchControl)
      : null,
  );

  // svelte-ignore state_referenced_locally
  const client = (() => {
    if (!isPoolConfigured(pool) || !token) return null;
    try {
      return poolClient(pool, { token });
    } catch {
      return null;
    }
  })();

  let projects = $state<{ id: string; name: string; slug: string }[]>([]);
  let hosts = $state<{ id: string; slug: string; display_name: string; status: string }[]>([]);
  let loadFailure = $state<string | null>(null);

  let form = $state<TidetableRunInput>({
    projectId: '',
    host: '',
    checkout: '',
    tasks: '',
    hours: '1',
    jobs: '1',
    dryRun: true,
  });
  let problems = $state<string[]>([]);
  let failure = $state<string | null>(null);
  let busy = $state(false);
  let job = $state<{
    id: string;
    status: string;
    refusalCode: string | null;
    owner: string | null;
    pid: number | null;
    exitCode: number | null;
    summary: string | null;
    output: string | null;
  } | null>(null);
  let timer: ReturnType<typeof setInterval> | null = null;

  const running = $derived(job !== null && jobIsRunning(job.status));
  const selectedProject = $derived(projects.find((project) => project.id === form.projectId) ?? null);
  const usableHosts = $derived(hosts.filter((host) => host.status !== 'revoked'));

  // Until hosts report their checkout names, the project's slug is the default.
  const checkoutSuggestions = $derived(selectedProject ? [selectedProject.slug] : []);

  onMount(async () => {
    if (!client) return;
    try {
      const [projectList, hostList] = await Promise.all([
        client.api.projects.list(),
        client.api.hosts.list(),
      ]);
      projects = projectList.projects;
      hosts = hostList.hosts;
      if (projects.length === 1) form.projectId = projects[0].id;
      if (usableHosts.length === 1) form.host = usableHosts[0].id;
    } catch (error) {
      loadFailure = `Could not load projects and hosts: ${error instanceof Error ? error.message : String(error)}`;
    }
  });

  function stopPolling(): void {
    if (timer !== null) {
      clearInterval(timer);
      timer = null;
    }
  }

  async function refresh(): Promise<void> {
    if (!client || job === null) return;
    try {
      const next = await client.api.jobs.get(job.id);
      job = next;
      if (!jobIsRunning(next.status)) stopPolling();
    } catch (error) {
      failure = `Could not read the job: ${error instanceof Error ? error.message : String(error)}`;
      stopPolling();
    }
  }

  function follow(): void {
    stopPolling();
    if (job !== null && jobIsRunning(job.status)) {
      timer = setInterval(() => void refresh(), pollMs);
    }
  }

  async function start(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (denial || !client || busy) return;
    const built = buildRunRequest(form);
    problems = built.problems;
    failure = null;
    if (built.request === null) return;
    busy = true;
    try {
      job = await client.api.projects.dispatchJob(form.projectId, built.request);
      follow();
    } catch (error) {
      failure = `The run was not started: ${error instanceof Error ? error.message : String(error)}`;
    } finally {
      busy = false;
    }
  }

  async function stop(mode: 'stop' | 'kill'): Promise<void> {
    if (!client || job === null) return;
    try {
      job = await client.api.jobs.stop(job.id, { mode });
      follow();
    } catch (error) {
      failure = `Could not ${mode} the run: ${error instanceof Error ? error.message : String(error)}`;
    }
  }

  onDestroy(stopPolling);
</script>

<section class="run-panel" aria-labelledby="run-panel-title" data-tidetable-run="true">
  <header class="run-panel__header">
    <h2 id="run-panel-title">Start a TideTable run</h2>
    <p>The host runs the tasks in its own checkout. Dry run shows what would happen and changes nothing.</p>
  </header>

  {#if denial}
    <p class="run-panel__notice" data-run-denial="dispatch-run">{denial}</p>
  {/if}
  {#if loadFailure}
    <p class="run-panel__notice" role="alert">{loadFailure}</p>
  {/if}

  <form class="run-panel__form" onsubmit={start} data-tidetable-run-form="true">
    <div class="run-panel__grid">
      <div class="run-field">
        <label for="run-project">Project</label>
        <select id="run-project" bind:value={form.projectId} required>
          <option value="" disabled>{projects.length ? 'Choose a project' : 'No projects yet'}</option>
          {#each projects as project (project.id)}
            <option value={project.id}>{project.name} ({project.slug})</option>
          {/each}
        </select>
      </div>

      <div class="run-field">
        <label for="run-host">Host</label>
        <select id="run-host" bind:value={form.host} required>
          <option value="" disabled>{usableHosts.length ? 'Choose a host' : 'No hosts enrolled'}</option>
          {#each usableHosts as host (host.id)}
            <option value={host.id}>{host.slug} · {host.status}</option>
          {/each}
        </select>
        <p class="run-field__help">The host that will run the job. Only enrolled hosts are listed.</p>
      </div>

      <div class="run-field">
        <label for="run-checkout">Checkout name</label>
        <input
          id="run-checkout"
          type="text"
          list="run-checkout-options"
          bind:value={form.checkout}
          autocomplete="off"
          required
        />
        <datalist id="run-checkout-options">
          {#each checkoutSuggestions as name (name)}
            <option value={name}></option>
          {/each}
        </datalist>
        <p class="run-field__help">The name the host uses for its checkout, such as the project slug.</p>
      </div>
    </div>

    <div class="run-field">
      <label for="run-tasks">Tasks</label>
      <textarea id="run-tasks" bind:value={form.tasks} rows="2" placeholder="1.22.6 1.22.7"></textarea>
      <p class="run-field__help">Task ids separated by spaces or commas, such as 1.22.6. Up to 16.</p>
    </div>

    <div class="run-panel__grid">
      <div class="run-field">
        <label for="run-hours">Hours</label>
        <input id="run-hours" type="number" min="1" max="12" step="1" bind:value={form.hours} />
      </div>
      <div class="run-field">
        <label for="run-jobs">Parallel jobs</label>
        <input id="run-jobs" type="number" min="1" max="4" step="1" bind:value={form.jobs} />
      </div>
      <div class="run-field run-field--check">
        <label>
          <input type="checkbox" bind:checked={form.dryRun} />
          <span>Dry run: show what would happen, change nothing</span>
        </label>
      </div>
    </div>

    {#if problems.length > 0}
      <ul class="run-panel__problems" data-run-problems={problems.length}>
        {#each problems as problem}
          <li>{problem}</li>
        {/each}
      </ul>
    {/if}

    <div class="run-panel__actions">
      <button type="submit" class="run-panel__submit" disabled={denial !== null || busy || client === null}>
        {busy ? 'Starting…' : form.dryRun ? 'Check the run' : 'Start the run'}
      </button>
    </div>
  </form>

  {#if failure}
    <p class="run-panel__notice" role="alert">{failure}</p>
  {/if}

  {#if job}
    <article class="run-panel__job" data-job-id={job.id} data-job-status={job.status}>
      <header>
        <h3>Job {job.id}</h3>
        <p data-job-line="true">{jobStatusLine(job)}</p>
      </header>
      <dl>
        {#if job.owner}
          <dt>Owner</dt>
          <dd>{job.owner}</dd>
        {/if}
        {#if job.pid !== null}
          <dt>Process</dt>
          <dd>{job.pid}</dd>
        {/if}
        {#if job.exitCode !== null}
          <dt>Exit code</dt>
          <dd>{job.exitCode}</dd>
        {/if}
        {#if job.summary}
          <dt>Summary</dt>
          <dd>{job.summary}</dd>
        {/if}
      </dl>
      {#if job.output}
        <pre class="run-panel__output" data-job-output="true">{job.output}</pre>
      {/if}
      {#if running}
        <div class="run-panel__actions">
          <button type="button" class="run-panel__secondary" onclick={() => void stop('stop')} disabled={denial !== null}>
            Stop after this step
          </button>
          <button type="button" class="run-panel__danger" onclick={() => void stop('kill')} disabled={denial !== null}>
            Kill now
          </button>
        </div>
      {/if}
    </article>
  {/if}
</section>

<style>
  .run-panel {
    display: grid;
    gap: 1rem;
    padding: 1rem 0;
    max-width: 52rem;
  }
  .run-panel__header h2 {
    font-size: 1.15rem;
    margin: 0 0 0.25rem;
  }
  .run-panel__header p {
    margin: 0;
  }
  .run-panel__notice {
    margin: 0;
    padding: 0.6rem 0.8rem;
    border: 1px solid currentColor;
    border-radius: 6px;
  }
  .run-panel__form {
    display: grid;
    gap: 1rem;
  }
  .run-panel__grid {
    display: grid;
    gap: 1rem;
    grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
  }
  .run-field {
    display: grid;
    gap: 0.35rem;
    align-content: start;
  }
  .run-field label {
    font-weight: 600;
  }
  .run-field input,
  .run-field select,
  .run-field textarea {
    font: inherit;
    padding: 0.45rem 0.6rem;
    border: 1px solid currentColor;
    border-radius: 6px;
    background: transparent;
    color: inherit;
    width: 100%;
    box-sizing: border-box;
  }
  .run-field__help {
    margin: 0;
    font-size: 0.85rem;
  }
  .run-field--check label {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    font-weight: 400;
  }
  .run-panel__problems {
    margin: 0;
    padding-left: 1.2rem;
  }
  .run-panel__actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .run-panel__submit,
  .run-panel__secondary,
  .run-panel__danger {
    font: inherit;
    padding: 0.5rem 1rem;
    border-radius: 6px;
    border: 1px solid currentColor;
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .run-panel__submit {
    font-weight: 600;
  }
  .run-panel__submit:disabled,
  .run-panel__secondary:disabled,
  .run-panel__danger:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .run-panel__job {
    display: grid;
    gap: 0.6rem;
    padding: 0.8rem;
    border: 1px solid currentColor;
    border-radius: 8px;
  }
  .run-panel__job h3 {
    margin: 0;
    font-size: 1rem;
  }
  .run-panel__job p {
    margin: 0;
  }
  .run-panel__output {
    max-height: 24rem;
    overflow: auto;
    white-space: pre-wrap;
    font-size: 0.85rem;
    padding: 0.5rem;
    border: 1px solid currentColor;
    border-radius: 6px;
    margin: 0;
  }
</style>
