<script lang="ts">
  /**
   * Start a TideTable run on a host, then follow it.
   *
   * The form picks a project, a host, a checkout and tasks. Dry run is on by
   * default. A started job is polled every few seconds while the pool says it
   * is queued or running, and its output is shown as it grows.
   */
  import { onDestroy } from 'svelte';
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
      job = await client.api.projects.dispatchJob(form.projectId.trim(), built.request);
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

<section class="tp-tidetable-run" aria-labelledby="tp-tidetable-run-title" data-tidetable-run="true">
  <h2 class="tp-tidetable-run__title" id="tp-tidetable-run-title">Start a TideTable run</h2>
  <p class="tp-tidetable-run__help">
    The host runs the tasks in its own checkout. Dry run shows what would happen without changing anything.
  </p>

  {#if denial}
    <p class="tp-tidetable-run__denial" data-run-denial="dispatch-run">{denial}</p>
  {/if}

  <form class="tp-tidetable-run__form" onsubmit={start} data-tidetable-run-form="true">
    <label>
      Project id
      <input type="text" bind:value={form.projectId} autocomplete="off" required />
    </label>
    <label>
      Host id
      <input type="text" bind:value={form.host} autocomplete="off" required />
    </label>
    <label>
      Checkout name
      <input type="text" bind:value={form.checkout} autocomplete="off" required />
    </label>
    <label>
      Tasks
      <textarea bind:value={form.tasks} rows="2" placeholder="1.22.6 1.22.7"></textarea>
    </label>
    <div class="tp-tidetable-run__row">
      <label>
        Hours
        <input type="number" min="1" max="12" bind:value={form.hours} />
      </label>
      <label>
        Parallel jobs
        <input type="number" min="1" max="4" bind:value={form.jobs} />
      </label>
      <label class="tp-tidetable-run__check">
        <input type="checkbox" bind:checked={form.dryRun} />
        Dry run
      </label>
    </div>
    <button type="submit" disabled={denial !== null || busy || client === null}>
      {busy ? 'Starting…' : form.dryRun ? 'Check the run' : 'Start the run'}
    </button>
  </form>

  {#if problems.length > 0}
    <ul class="tp-tidetable-run__problems" data-run-problems={problems.length}>
      {#each problems as problem}
        <li>{problem}</li>
      {/each}
    </ul>
  {/if}

  {#if failure}
    <p class="tp-tidetable-run__failure" role="alert">{failure}</p>
  {/if}

  {#if job}
    <article class="tp-tidetable-run__job" data-job-id={job.id} data-job-status={job.status}>
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
        <pre class="tp-tidetable-run__output" data-job-output="true">{job.output}</pre>
      {/if}
      {#if running}
        <div class="tp-tidetable-run__actions">
          <button type="button" onclick={() => void stop('stop')} disabled={denial !== null}>
            Stop after this step
          </button>
          <button type="button" onclick={() => void stop('kill')} disabled={denial !== null}>
            Kill now
          </button>
        </div>
      {/if}
    </article>
  {/if}
</section>

<style>
  .tp-tidetable-run {
    display: grid;
    gap: 0.75rem;
    padding: 1rem 0;
  }
  .tp-tidetable-run__title {
    font-size: 1.1rem;
    margin: 0;
  }
  .tp-tidetable-run__help {
    margin: 0;
    opacity: 0.8;
  }
  .tp-tidetable-run__form {
    display: grid;
    gap: 0.5rem;
    max-width: 36rem;
  }
  .tp-tidetable-run__form label {
    display: grid;
    gap: 0.25rem;
  }
  .tp-tidetable-run__row {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
    align-items: end;
  }
  .tp-tidetable-run__check {
    display: flex !important;
    align-items: center;
    gap: 0.4rem;
  }
  .tp-tidetable-run__output {
    max-height: 24rem;
    overflow: auto;
    white-space: pre-wrap;
    font-size: 0.85rem;
    padding: 0.5rem;
  }
  .tp-tidetable-run__actions {
    display: flex;
    gap: 0.5rem;
  }
  @media (max-width: 480px) {
    .tp-tidetable-run__form {
      max-width: none;
    }
  }
</style>
