/**
 * TideTable runs from the control plane: validate a start request, build the
 * body the pool expects, and read a job back.
 *
 * The pool dispatches a `tkrun` job to a connected host; the host runs
 * `tidetable tkrun` locally. This module only shapes and checks the request
 * and wraps the SDK calls, so the page and its tests share one definition of a
 * valid run.
 */

/** A TideTable task id: three dot-separated numbers, e.g. `1.22.6`. */
export const TASK_ID = /^\d+\.\d+\.\d+$/;

/** A checkout name the host maps to a directory: lowercase, digits, dashes. */
export const CHECKOUT_NAME = /^[a-z0-9][a-z0-9_-]{0,62}$/;

/** The most tasks one run may name. */
export const MAX_TASKS = 16;

/** What the start form holds, as the user typed it. */
export interface TidetableRunInput {
  /** The control plane project id (a UUID from the project list). */
  projectId: string;
  /** The host's public id. */
  host: string;
  /** The checkout name on that host. */
  checkout: string;
  /** Task ids, separated by spaces or commas. */
  tasks: string;
  /** Hours, as typed; an integer from 1 to 12. */
  hours: string;
  /** Parallel workers, as typed; an integer from 1 to 4. */
  jobs: string;
  /** Show routing and commands without running them. */
  dryRun: boolean;
}

/** The body `POST /v1/projects/{id}/jobs` takes. */
export interface TidetableRunRequest {
  host: string;
  checkout: string;
  options: {
    hours: number;
    jobs: number;
    tasks: string[];
    dryRun: boolean;
  };
}

/** Split a task list into ids and say what is wrong with it. */
export function parseTaskIds(text: string): { ids: string[]; problems: string[] } {
  const ids = text
    .split(/[\s,]+/)
    .map((id) => id.trim())
    .filter((id) => id !== '');
  const problems: string[] = [];
  if (ids.length === 0) {
    problems.push('Name at least one task id, such as 1.22.6.');
  }
  if (ids.length > MAX_TASKS) {
    problems.push(`A run takes at most ${MAX_TASKS} tasks; this has ${ids.length}.`);
  }
  for (const id of ids) {
    if (!TASK_ID.test(id)) {
      problems.push(`"${id}" is not a task id. Task ids look like 1.22.6.`);
    }
  }
  return { ids, problems };
}

/** Whole number within bounds, or null. */
function wholeNumber(text: string, min: number, max: number): number | null {
  const trimmed = text.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const value = Number(trimmed);
  return value >= min && value <= max ? value : null;
}

/**
 * Check the start form and build the request.
 *
 * Returns the request when every field is valid, otherwise the list of plain
 * sentences to show next to the form. Nothing is sent until this passes.
 */
export function buildRunRequest(input: TidetableRunInput): {
  request: TidetableRunRequest | null;
  problems: string[];
} {
  const problems: string[] = [];
  const host = input.host.trim();
  const checkout = input.checkout.trim();
  if (host === '') problems.push('Choose the host that should run the job.');
  if (!CHECKOUT_NAME.test(checkout)) {
    problems.push('The checkout name is lowercase letters, digits, dashes or underscores, as the host names it.');
  }
  const tasks = parseTaskIds(input.tasks);
  problems.push(...tasks.problems);
  const hours = wholeNumber(input.hours, 1, 12);
  if (hours === null) problems.push('Hours is a whole number from 1 to 12.');
  const jobs = wholeNumber(input.jobs, 1, 4);
  if (jobs === null) problems.push('Parallel jobs is a whole number from 1 to 4.');
  if (problems.length > 0 || hours === null || jobs === null) {
    return { request: null, problems };
  }
  return {
    request: {
      host,
      checkout,
      options: { hours, jobs, tasks: tasks.ids, dryRun: input.dryRun },
    },
    problems: [],
  };
}

/** A job is still going: the pool will update it. */
export function jobIsRunning(status: string): boolean {
  return status === 'queued' || status === 'running';
}

/** A line for the job's status, readable without knowing the status names. */
export function jobStatusLine(job: { status: string; refusalCode: string | null }): string {
  switch (job.status) {
    case 'queued':
      return 'Queued: waiting for the host to start it.';
    case 'running':
      return 'Running on the host.';
    case 'succeeded':
      return 'Finished.';
    case 'refused':
      return job.refusalCode
        ? `Refused (${job.refusalCode}). The host did not start the run.`
        : 'Refused. The host did not start the run.';
    default:
      return `Status: ${job.status}.`;
  }
}
