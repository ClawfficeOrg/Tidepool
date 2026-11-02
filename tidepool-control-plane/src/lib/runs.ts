/**
 * Runs and activity: the model behind the activity page and the run detail.
 *
 * The hub records four kinds of thing for every delivery — the receipt that
 * says what arrived and whether it matched a trigger, the run it started, each
 * step of that run, and each agent execution behind a step. `GET /v1/activity`
 * returns them as one cursor-paged feed; this module owns what the page does
 * with that feed:
 *
 * - **filters combine**. Project, provider, trigger, status and kind narrow one
 *   union, and every predicate must match. The page also filters the page it
 *   receives before rendering it, so a server that ignored a filter — or a
 *   stale page that arrived after the filter changed — can never show another
 *   project's runs.
 * - **a running execution follows itself**. A detail that is still pending or
 *   running is read again on a timer until it reaches a terminal state, so the
 *   page updates without a refresh. A closed or failed detail stops polling.
 * - **failure codes are sentences**. The pool stores a stable code with a step
 *   failure (`daemon_not_connected: host 'swaygent' is offline`). The page
 *   shows a sentence a person understands and keeps the code available beside
 *   it for a bug report.
 * - **output is kept verbatim**. A step's output is the agent's own text. It is
 *   never trimmed, parsed or reflowed; the page puts it in a `<pre>` and copies
 *   exactly those characters.
 *
 * Everything that can be decided without a network is a pure function here, so
 * the filter combination, the live detail, the code mapping and the copy path
 * are all tested without a DOM or a pool. The controller is the only part that
 * talks to the hub, and it takes a transport by injection so an end-to-end
 * manual dispatch can be walked through the real SDK against a fake pool.
 */

import type {
  ActivityEntry,
  ActivityPage,
  ActivityRunDetail,
  ArrivalDetail,
  ManualDispatchRequest,
  ManualDispatchResponse,
  ProjectView,
  ProjectsResponse,
  RunStepView,
  RunView,
  TidepoolClient,
} from '@clawffice/tidepool-sdk';
import { presentProblem, type PoolProblem } from './api/problem';
import { isPoolConfigured, type PoolConfig } from './api/pool';
import type { StatusKind } from './status';

// ── filters ────────────────────────────────────────────────────────────────

/** The four statuses an arrival receipt can carry. */
export const RECEIPT_STATUSES = ['received', 'matched', 'unmatched', 'rejected'] as const;

/** Every status an activity entry can carry, across all four kinds. */
export const ACTIVITY_STATUSES = [
  'pending',
  'running',
  'succeeded',
  'failed',
  'skipped',
  'interrupted',
  'received',
  'matched',
  'unmatched',
  'rejected',
  'queued',
] as const;

/** The record kinds the activity feed combines. */
export const ACTIVITY_KINDS = ['receipt', 'run', 'step', 'execution'] as const;

/** The providers a receipt or run can name. */
export const ACTIVITY_PROVIDERS = ['github', 'slack', 'discord'] as const;

/** One record kind. */
export type ActivityKind = (typeof ACTIVITY_KINDS)[number];

/** One provider. */
export type ActivityProvider = (typeof ACTIVITY_PROVIDERS)[number];

/** The filter fields the page exposes. */
export type ActivityFilterField = 'project' | 'provider' | 'trigger' | 'status' | 'kind';

/**
 * The combined filters.
 *
 * An empty string means "any"; the page never sends an empty filter to the
 * pool, because an absent filter and an empty one are not the same query.
 */
export interface ActivityFilters {
  /** Project public id. */
  project: string;
  /** Provider kind, lowercase. */
  provider: string;
  /** Trigger public id or trigger name. */
  trigger: string;
  /** Record status, lowercase. */
  status: string;
  /** Record kind, lowercase. */
  kind: string;
}

/** No filters at all. */
export function emptyActivityFilters(): ActivityFilters {
  return { project: '', provider: '', trigger: '', status: '', kind: '' };
}

/** Lowercase a provider, status or kind and leave an id or name alone. */
function normaliseLower(value: string): string {
  return value.trim().toLowerCase();
}

/**
 * Normalise one filter edit.
 *
 * Provider, status and kind are contract enums, so they are lowercased; project
 * ids and trigger names are identifiers and keep their case.
 */
export function normaliseActivityFilters(filters: ActivityFilters): ActivityFilters {
  return {
    project: filters.project.trim(),
    provider: normaliseLower(filters.provider),
    trigger: filters.trigger.trim(),
    status: normaliseLower(filters.status),
    kind: normaliseLower(filters.kind),
  };
}

/** True when at least one filter narrows the feed. */
export function filtersActive(filters: ActivityFilters): boolean {
  const normalised = normaliseActivityFilters(filters);
  return Object.values(normalised).some((value) => value !== '');
}

/** Apply one filter edit, returning a new filter set. */
export function applyActivityFilter(
  filters: ActivityFilters,
  field: ActivityFilterField,
  value: string,
): ActivityFilters {
  return normaliseActivityFilters({ ...filters, [field]: value });
}

/** The query-string shape the activity resource accepts. */
export interface ActivityQueryValues {
  /** Project public id. */
  project?: string;
  /** Provider kind. */
  provider?: string;
  /** Trigger public id or name. */
  trigger?: string;
  /** Record status. */
  status?: string;
  /** Record kind. */
  kind?: string;
  /** Opaque page cursor. */
  cursor?: string;
  /** Page size. */
  limit?: number;
}

/**
 * Build the pool query for a filter set.
 *
 * Only non-empty filters are sent: the pool treats an absent filter as "all",
 * and sending `project=` would be a different request. The cursor and limit are
 * added only when the caller names them.
 */
export function activityQuery(
  filters: ActivityFilters,
  options: { cursor?: string | null; limit?: number } = {},
): ActivityQueryValues {
  const normalised = normaliseActivityFilters(filters);
  const query: ActivityQueryValues = {};
  if (normalised.project !== '') query.project = normalised.project;
  if (normalised.provider !== '') query.provider = normalised.provider;
  if (normalised.trigger !== '') query.trigger = normalised.trigger;
  if (normalised.status !== '') query.status = normalised.status;
  if (normalised.kind !== '') query.kind = normalised.kind;
  if (options.cursor !== undefined && options.cursor !== null && options.cursor !== '') {
    query.cursor = options.cursor;
  }
  if (options.limit !== undefined) query.limit = options.limit;
  return query;
}

/** True when one entry satisfies every active filter. */
export function entryMatchesFilters(entry: ActivityEntry, filters: ActivityFilters): boolean {
  const normalised = normaliseActivityFilters(filters);
  if (normalised.project !== '' && entry.project !== normalised.project) return false;
  if (normalised.provider !== '' && entry.provider !== normalised.provider) return false;
  if (normalised.trigger !== '') {
    const namesTrigger =
      entry.trigger === normalised.trigger || entry.triggerName === normalised.trigger;
    if (!namesTrigger) return false;
  }
  if (normalised.status !== '' && entry.status !== normalised.status) return false;
  if (normalised.kind !== '' && entry.kind !== normalised.kind) return false;
  return true;
}

/**
 * Keep only the entries every active filter accepts.
 *
 * This is the page's own guard, applied to each page the pool returns. It is
 * deliberately strict about a missing field: an entry with no `project` never
 * matches a project filter, and an entry with no `provider` never matches a
 * provider filter. A union row that cannot be attributed to the chosen project
 * must not appear under it.
 */
export function filterActivityEntries(
  entries: readonly ActivityEntry[],
  filters: ActivityFilters,
): ActivityEntry[] {
  return entries.filter((entry) => entryMatchesFilters(entry, filters));
}

/** One line describing the active filter combination. */
export function activityFilterSummary(filters: ActivityFilters): string {
  const normalised = normaliseActivityFilters(filters);
  const parts = [
    normalised.project === '' ? 'all projects' : `project ${normalised.project}`,
    normalised.provider === '' ? 'all providers' : providerLabel(normalised.provider),
    normalised.trigger === '' ? 'all triggers' : `trigger '${normalised.trigger}'`,
    normalised.status === '' ? 'any status' : statusWord(normalised.status),
    normalised.kind === '' ? 'all records' : `${kindLabel(normalised.kind)} records`,
  ];
  return `Showing ${parts.join(', ')}.`;
}

// ── reading the feed ───────────────────────────────────────────────────────

/** The provider's own name. */
export function providerLabel(provider: string | null | undefined): string {
  switch (provider) {
    case 'github':
      return 'GitHub';
    case 'slack':
      return 'Slack';
    case 'discord':
      return 'Discord';
    default:
      return provider && provider.trim() !== '' ? provider : 'Unknown provider';
  }
}

/** The word a status is rendered with. */
export function statusWord(status: string | null | undefined): string {
  switch (status) {
    case 'pending':
      return 'Waiting to start';
    case 'queued':
      return 'Queued';
    case 'running':
      return 'Running';
    case 'succeeded':
      return 'Succeeded';
    case 'failed':
      return 'Failed';
    case 'skipped':
      return 'Skipped';
    case 'interrupted':
      return 'Interrupted';
    case 'received':
      return 'Received';
    case 'matched':
      return 'Matched';
    case 'unmatched':
      return 'Matched nothing';
    case 'rejected':
      return 'Rejected';
    default:
      return status && status.trim() !== '' ? status : 'Unknown';
  }
}

/** The status tone for a run, step or execution state. */
export function statusTone(status: string | null | undefined): StatusKind {
  switch (status) {
    case 'succeeded':
    case 'matched':
      return 'success';
    case 'running':
      return 'info';
    case 'pending':
    case 'queued':
    case 'received':
      return 'pending';
    case 'failed':
    case 'rejected':
      return 'danger';
    case 'interrupted':
      return 'warning';
    case 'skipped':
    case 'unmatched':
      return 'neutral';
    default:
      return 'unknown';
  }
}

/** The label for a record kind. */
export function kindLabel(kind: string | null | undefined): string {
  switch (kind) {
    case 'receipt':
      return 'Arrival';
    case 'run':
      return 'Run';
    case 'step':
      return 'Step';
    case 'execution':
      return 'Execution';
    default:
      return kind && kind.trim() !== '' ? kind : 'Record';
  }
}

/** A short heading for one activity entry. */
export function activityEntryTitle(entry: ActivityEntry): string {
  switch (entry.kind) {
    case 'receipt': {
      const event = entry.eventType?.trim();
      return `${providerLabel(entry.provider)} ${event !== undefined && event !== '' ? event : 'delivery'}`;
    }
    case 'run':
      return entry.triggerName?.trim() !== undefined && entry.triggerName.trim() !== ''
        ? `Run of '${entry.triggerName}'`
        : 'A run';
    case 'step':
      return entry.step?.trim() !== undefined && entry.step.trim() !== ''
        ? `Step '${entry.step}'`
        : 'A step';
    case 'execution':
      return 'Agent execution';
    default:
      return kindLabel(entry.kind);
  }
}

/**
 * One readable sentence for an activity entry.
 *
 * The status badge carries the state; this sentence says what the record is, so
 * a receipt reads as an arrival that matched or did not, and a run reads as
 * either an event or a manual request.
 */
export function activityEntrySentence(entry: ActivityEntry): string {
  switch (entry.kind) {
    case 'receipt': {
      const who = entry.actor?.trim() ? ` from ${entry.actor.trim()}` : '';
      const what = entry.eventType?.trim() ? entry.eventType.trim() : 'a delivery';
      const action =
        entry.matched === true
          ? 'matched a trigger'
          : entry.matched === false
            ? 'matched no trigger'
            : `is ${statusWord(entry.status).toLowerCase()}`;
      const duplicates =
        typeof entry.duplicates === 'number' && entry.duplicates > 0
          ? ` The provider delivered it ${entry.duplicates} more time${entry.duplicates === 1 ? '' : 's'}.`
          : '';
      return `${providerLabel(entry.provider)} delivered ${what}${who}; it ${action}.${duplicates}`;
    }
    case 'run': {
      const origin =
        entry.actor?.trim() !== undefined && entry.actor.trim() !== ''
          ? ` by ${entry.actor.trim()}`
          : '';
      const trigger = entry.triggerName?.trim() ? `'${entry.triggerName.trim()}'` : 'a workflow';
      return `A ${entry.status === 'pending' || entry.status === 'running' ? 'run' : 'finished run'} of ${trigger}${origin}.`;
    }
    case 'step': {
      const step = entry.step?.trim() ? `'${entry.step.trim()}'` : 'A step';
      const failure = failureExplanation(entry.error ?? null, null);
      return failure
        ? `${step} ${failure.sentence}`
        : `${step} is ${statusWord(entry.status).toLowerCase()}.`;
    }
    case 'execution':
      return `The agent execution is ${statusWord(entry.status).toLowerCase()}.`;
    default:
      return `${kindLabel(entry.kind)} with status ${statusWord(entry.status).toLowerCase()}.`;
  }
}

/** The run an activity entry can open, or null when it names none. */
export function activityEntryRunId(entry: ActivityEntry): string | null {
  if (entry.kind === 'run') return entry.run ?? entry.id;
  return entry.run ?? null;
}

/** The project's name when the page has it, otherwise its id. */
export function projectLabel(projectId: string | null | undefined, projects: readonly ProjectView[]): string {
  if (projectId === null || projectId === undefined || projectId.trim() === '') return 'No project';
  const project = projects.find((candidate) => candidate.id === projectId);
  return project ? project.name : projectId;
}

/** A stable, readable timestamp for any pool value, or an em dash. */
export function formatTimestamp(value: string | null | undefined): string {
  if (value === null || value === undefined || value.trim() === '') return '—';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return `${date.toISOString().slice(0, 19).replace('T', ' ')} UTC`;
}

// ── failure codes ──────────────────────────────────────────────────────────

/** One named failure, the sentence a person reads and the code they can quote. */
export interface FailureExplanation {
  /** The stable pool code, when one was recorded. */
  code: string | null;
  /** A sentence that says what happened, in a person's words. */
  sentence: string;
  /** The pool's own remaining detail, kept underneath. */
  detail: string | null;
}

/**
 * The code-to-sentence table.
 *
 * Every entry starts with the code the pool stores, so the pool and the page
 * cannot drift: `daemon_not_connected` is the code dispatch and the host
 * protocol both use, and the sentence names the host going offline in those
 * words rather than showing "daemon_not_connected" to a person.
 */
export const FAILURE_SENTENCES: Readonly<Record<string, string>> = {
  daemon_not_connected:
    'The host went offline before it could run this step. Start the host, then dispatch the run again.',
  host_not_found:
    'The host this step names is no longer enrolled with the pool, so the step could not start.',
  execution_failed: 'The agent on the host reported that this step failed.',
  interrupted: 'This step was interrupted before it finished, so its work is incomplete.',
  invalid_output:
    'The step finished, but its output was not the JSON the workflow declares for it.',
  step_deadline: 'The step ran past its deadline and was stopped.',
};

/** The codes the table explains. */
export const NAMED_FAILURE_CODES: readonly string[] = Object.keys(FAILURE_SENTENCES);

/** A leading `code: detail` in a stored error string. */
const FAILURE_PREFIX = /^([a-z][a-z0-9_]*):\s*([\s\S]*)$/;

/** Split a stored error string into a leading code and the rest. */
export function splitFailureCode(errorText: string | null | undefined): {
  code: string | null;
  detail: string;
} {
  const text = errorText?.trim() ?? '';
  const match = FAILURE_PREFIX.exec(text);
  if (!match) return { code: null, detail: text };
  return { code: match[1] ?? null, detail: (match[2] ?? '').trim() };
}

/** True when a value is a code the page has a sentence for. */
export function isNamedFailureCode(code: string | null | undefined): boolean {
  return typeof code === 'string' && Object.hasOwn(FAILURE_SENTENCES, code);
}

/**
 * Explain one stored step failure.
 *
 * The error text wins because it is the pool's record of the failure itself;
 * `reason` is the step's own skip/stop reason and is used when the error has no
 * leading code. An unknown code is never hidden: the page still shows the
 * pool's text, and the code stays available beside it.
 */
export function failureExplanation(
  errorText: string | null | undefined,
  reason: string | null | undefined,
): FailureExplanation | null {
  const { code: errorCode, detail: errorDetail } = splitFailureCode(errorText);
  const rawText = errorText?.trim() ?? '';
  const trimmedReason = reason?.trim() ?? '';

  if (errorCode !== null) {
    const sentence = FAILURE_SENTENCES[errorCode];
    if (sentence !== undefined) {
      return {
        code: errorCode,
        sentence,
        detail: errorDetail === '' ? null : errorDetail,
      };
    }
    return {
      code: errorCode,
      sentence:
        errorDetail === ''
          ? `The step failed with the pool's '${errorCode}' code.`
          : `The step failed with the pool's '${errorCode}' code: ${errorDetail}`,
      detail: null,
    };
  }

  const reasonSentence = trimmedReason === '' ? undefined : FAILURE_SENTENCES[trimmedReason];
  if (reasonSentence !== undefined) {
    return {
      code: trimmedReason,
      sentence: reasonSentence,
      detail: rawText === trimmedReason || rawText === '' ? null : rawText,
    };
  }

  // A stored error can be the bare code rather than `code: detail`.
  const bareSentence = rawText === '' ? undefined : FAILURE_SENTENCES[rawText];
  if (bareSentence !== undefined) {
    return { code: rawText, sentence: bareSentence, detail: null };
  }

  if (rawText !== '') {
    return { code: null, sentence: `The step failed: ${rawText}`, detail: null };
  }
  if (trimmedReason !== '') {
    return {
      code: trimmedReason,
      sentence: `The step stopped with the pool's '${trimmedReason}' code.`,
      detail: null,
    };
  }
  return null;
}

/** The failure a person should read first for a run, if any. */
export function runFailureExplanation(run: RunView): FailureExplanation | null {
  for (const step of run.steps) {
    if (step.status !== 'failed' && step.status !== 'interrupted') continue;
    const failure = failureExplanation(step.error ?? null, step.reason ?? null);
    if (failure) return failure;
  }
  if (run.status === 'failed' || run.status === 'interrupted') {
    return {
      code: null,
      sentence:
        run.status === 'interrupted'
          ? 'This run was interrupted before it finished.'
          : 'This run failed, but the pool recorded no step failure detail.',
      detail: null,
    };
  }
  return null;
}

// ── live detail ────────────────────────────────────────────────────────────

/** The states that mean a record is still working. */
const LIVE_STATUSES: ReadonlySet<string> = new Set(['pending', 'running']);

/** True when a step is still pending or running. */
export function stepIsLive(step: RunStepView): boolean {
  return LIVE_STATUSES.has(step.status);
}

/**
 * True when a run detail still needs following.
 *
 * A run that says `succeeded` but still has a running step is live too: the
 * run status is written when the last step finishes, and a page that stopped on
 * the run word would miss the step's output.
 */
export function runIsLive(run: RunView): boolean {
  return LIVE_STATUSES.has(run.status) || run.steps.some(stepIsLive);
}

/** The sentence for how an arrival related to its conversation's agent. */
export function arrivalSentence(arrival: ArrivalDetail): string {
  switch (arrival.mode) {
    case 'created':
      return 'This arrival created a new agent for the conversation.';
    case 'continued':
      return arrival.steered
        ? 'This arrival continued the agent already serving the conversation and steered it while it was busy.'
        : 'This arrival continued the agent already serving the conversation.';
    case 'restored':
      return 'The conversation’s agent was gone, so this arrival restored it with a new agent.';
    default:
      return 'The pool did not record how this arrival reached an agent.';
  }
}

/** The label for the arrival mode. */
export function arrivalModeLabel(mode: ArrivalDetail['mode']): string {
  switch (mode) {
    case 'created':
      return 'Created an agent';
    case 'continued':
      return 'Continued an agent';
    case 'restored':
      return 'Restored an agent';
    default:
      return 'Arrival';
  }
}

/** The step's output exactly as the agent wrote it. */
export function stepOutputText(step: RunStepView): string {
  return step.output ?? '';
}

/** True when a step has output to show. */
export function stepHasOutput(step: RunStepView): boolean {
  return (step.output ?? '') !== '';
}

/** The message shown instead of an empty output block. */
export const NO_STEP_OUTPUT = 'This step has not written any output yet.';

/** Write text to the clipboard; the writer is injectable for tests. */
export type ClipboardWriter = (text: string) => Promise<void> | void;

/** The browser clipboard writer, used by the copy control. */
async function writeToBrowserClipboard(text: string): Promise<void> {
  if (typeof navigator === 'undefined' || navigator.clipboard === undefined) {
    throw new Error('this browser does not offer a clipboard');
  }
  await navigator.clipboard.writeText(text);
}

/**
 * Copy exactly `text`, with no trimming or normalisation.
 *
 * Returns false rather than throwing when the clipboard is unavailable, so a
 * button can say so without turning a copy into a page failure.
 */
export async function copyOutput(
  text: string,
  write: ClipboardWriter = writeToBrowserClipboard,
): Promise<boolean> {
  try {
    await write(text);
    return true;
  } catch {
    return false;
  }
}

// ── manual dispatch ────────────────────────────────────────────────────────

/** The fields of one manual run request. */
export interface ManualRunInput {
  /** Project public id. */
  project: string;
  /** Installed trigger name. */
  trigger: string;
  /** Actor the trigger's filters must allow. */
  actor: string;
  /** Leading `key=value` tokens plus the request text. */
  input: string;
  /** Stable delivery key; a repeat returns the first run. */
  deliveryKey: string;
  /** Optional expected active revision. */
  revision?: string | null;
}

/** A client-supplied delivery key that is stable for one button press. */
export function manualDeliveryKey(actor: string, nowMs: number, sequence: number): string {
  const who =
    actor
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '') || 'operator';
  return `control-plane-${who}-${nowMs}-${sequence}`;
}

/** What a manual dispatch needs before it can be sent. */
export function manualRunIssues(input: ManualRunInput): string[] {
  const issues: string[] = [];
  if (input.project.trim() === '') issues.push('Choose the project the run belongs to.');
  if (input.trigger.trim() === '') issues.push('Name the installed trigger to run.');
  if (input.actor.trim() === '') issues.push('Name the actor the trigger must allow.');
  if (input.deliveryKey.trim() === '') {
    issues.push('A delivery key is required, so a repeated press cannot start a second run.');
  }
  return issues;
}

/** Build the contract request from the page's input. */
export function manualRunRequest(input: ManualRunInput): ManualDispatchRequest {
  const request: ManualDispatchRequest = {
    project: input.project.trim(),
    trigger: input.trigger.trim(),
    actor: input.actor.trim(),
    deliveryKey: input.deliveryKey.trim(),
    input: input.input,
  };
  const revision = input.revision?.trim() ?? '';
  if (revision !== '') request.revision = revision;
  return request;
}

/** The result kept after a successful dispatch. */
export interface DispatchReceipt {
  /** The run that was created or matched. */
  runId: string;
  /** True when the delivery key already named this run. */
  deduplicated: boolean;
}

/** What the page says after a dispatch. */
export function dispatchSentence(receipt: DispatchReceipt): string {
  return receipt.deduplicated
    ? `This request had already run, so the pool returned the existing run ${receipt.runId}.`
    : `The pool started run ${receipt.runId}. The detail below follows it until it finishes.`;
}

// ── the controller ─────────────────────────────────────────────────────────

/** The pool calls the activity page makes, injectable for tests. */
export interface RunsTransport {
  /** List the organization's projects, for the project filter and chooser. */
  listProjects(): Promise<ProjectsResponse>;
  /** Read one page of the combined activity feed. */
  list(query?: ActivityQueryValues): Promise<ActivityPage>;
  /** Read one run plus the arrival that started it. */
  run(run: string): Promise<ActivityRunDetail>;
  /** Start a workflow run from a manual request. */
  dispatch(request: ManualDispatchRequest): Promise<ManualDispatchResponse>;
}

/** Build the transport from the typed SDK client. */
export function runsTransport(client: TidepoolClient): RunsTransport {
  return {
    listProjects: () => client.api.projects.list(),
    list: (query) => client.api.activity.list({ query }),
    run: (run) => client.api.activity.run(run),
    dispatch: (request) => client.api.runs.dispatch(request),
  };
}

/** One timer the controller can schedule; injectable so tests are deterministic. */
export interface RunsScheduler {
  /** Run `callback` after `delayMs`, returning a handle for `cancel`. */
  schedule(callback: () => void | Promise<unknown>, delayMs: number): unknown;
  /** Cancel a handle returned by `schedule`. */
  cancel(handle: unknown): void;
}

/** The default scheduler: the browser's or Node's `setTimeout`. */
export const DEFAULT_RUNS_SCHEDULER: RunsScheduler = {
  schedule: (callback, delayMs) => setTimeout(() => void callback(), delayMs),
  cancel: (handle) => {
    if (handle !== null && handle !== undefined) {
      clearTimeout(handle as ReturnType<typeof setTimeout>);
    }
  },
};

/** How often a live run detail is read again. */
export const DEFAULT_POLL_INTERVAL_MS = 1_500;

/** How many entries one activity page asks for. */
export const ACTIVITY_PAGE_SIZE = 25;

/** Everything the activity page renders. */
export interface RunsViewState {
  /** The feed's load state. */
  phase: 'idle' | 'loading' | 'ready' | 'error';
  /** Projects in this organization. */
  projects: ProjectView[];
  /** The current page of entries, already narrowed by the filters. */
  items: ActivityEntry[];
  /** The active filter combination. */
  filters: ActivityFilters;
  /** True when the pool has another page. */
  hasMore: boolean;
  /** Cursor for the next page. */
  nextCursor: string | null;
  /** True while a later page is loading. */
  loadingMore: boolean;
  /** A failure loading the feed. */
  problem: PoolProblem | null;
  /** The run whose detail is open. */
  selectedRunId: string | null;
  /** The detail's load state. */
  detailPhase: 'idle' | 'loading' | 'ready' | 'error';
  /** The open run and its arrival record. */
  detail: ActivityRunDetail | null;
  /** A failure loading the detail. */
  detailProblem: PoolProblem | null;
  /** When the detail was last read, for the live indicator. */
  detailUpdatedAt: string | null;
  /** True while the open run still needs following. */
  live: boolean;
  /** Where a manual dispatch is. */
  dispatchPhase: 'idle' | 'dispatching' | 'dispatched' | 'failed';
  /** A failure from a manual dispatch. */
  dispatchProblem: PoolProblem | null;
  /** The run a dispatch created or matched. */
  lastDispatch: DispatchReceipt | null;
  /** True while any pool call is in flight. */
  busy: boolean;
}

/** The state before anything has been loaded. */
export function emptyRunsState(filters: ActivityFilters = emptyActivityFilters()): RunsViewState {
  return {
    phase: 'idle',
    projects: [],
    items: [],
    filters,
    hasMore: false,
    nextCursor: null,
    loadingMore: false,
    problem: null,
    selectedRunId: null,
    detailPhase: 'idle',
    detail: null,
    detailProblem: null,
    detailUpdatedAt: null,
    live: false,
    dispatchPhase: 'idle',
    dispatchProblem: null,
    lastDispatch: null,
    busy: false,
  };
}

/** True when the page can load from this pool config. */
export function canLoadRuns(
  config: PoolConfig,
  token: string | null | undefined,
): boolean {
  return isPoolConfigured(config) && typeof token === 'string' && token.trim() !== '';
}

/** A synthetic problem for a dispatch the page refuses before sending it. */
function dispatchInputProblem(issue: string): PoolProblem {
  return {
    kind: 'app',
    title: 'The run was not dispatched',
    message: issue,
    requestId: null,
    status: null,
    code: 'validation_failed',
    type: null,
    retryAfterMs: null,
  };
}

/** How the controller is built. */
export interface RunsControllerOptions {
  /** The pool calls. */
  transport: RunsTransport;
  /** A resumed or preloaded state; tests use this to start at a fixture. */
  initial?: RunsViewState;
  /** Where the live timer comes from. */
  scheduler?: RunsScheduler;
  /** The clock, so `detailUpdatedAt` is deterministic in tests. */
  now?: () => number;
  /** How often a live detail is read again. */
  pollIntervalMs?: number;
}

/** The activity page's controller. */
export class RunsController {
  private readonly transport: RunsTransport;
  private readonly scheduler: RunsScheduler;
  private readonly now: () => number;
  private readonly pollIntervalMs: number;
  private state: RunsViewState;
  /** Guards an older feed page from replacing a newer one. */
  private listToken = 0;
  /** Guards an older detail answer from replacing a newer one. */
  private detailToken = 0;
  /** The pending live timer, when one is scheduled. */
  private pollHandle: unknown = null;

  constructor(options: RunsControllerOptions) {
    this.transport = options.transport;
    this.scheduler = options.scheduler ?? DEFAULT_RUNS_SCHEDULER;
    this.now = options.now ?? Date.now;
    this.pollIntervalMs = options.pollIntervalMs ?? DEFAULT_POLL_INTERVAL_MS;
    this.state = options.initial ?? emptyRunsState();
  }

  /** The current state, without touching the pool. */
  snapshot(): RunsViewState {
    return this.state;
  }

  /** Load projects and the first activity page. */
  async load(): Promise<RunsViewState> {
    const token = (this.listToken += 1);
    this.state = {
      ...this.state,
      phase: this.state.phase === 'ready' ? 'ready' : 'loading',
      problem: null,
      busy: true,
    };
    try {
      const [projects, page] = await Promise.all([
        this.transport.listProjects(),
        this.transport.list(activityQuery(this.state.filters, { limit: ACTIVITY_PAGE_SIZE })),
      ]);
      if (token !== this.listToken) return this.state;
      this.state = {
        ...this.state,
        phase: 'ready',
        projects: [...projects.projects],
        items: filterActivityEntries(page.items, this.state.filters),
        hasMore: page.hasMore,
        nextCursor: page.nextCursor ?? null,
        loadingMore: false,
        problem: null,
        busy: false,
      };
    } catch (error) {
      if (token !== this.listToken) return this.state;
      this.state = {
        ...this.state,
        phase: 'error',
        problem: presentProblem(error),
        busy: false,
      };
    }
    return this.state;
  }

  /** Read the first page again, keeping the loaded projects. */
  async refresh(): Promise<RunsViewState> {
    if (this.state.projects.length === 0) return this.load();
    const token = (this.listToken += 1);
    this.state = { ...this.state, problem: null, busy: true };
    try {
      const page = await this.transport.list(
        activityQuery(this.state.filters, { limit: ACTIVITY_PAGE_SIZE }),
      );
      if (token !== this.listToken) return this.state;
      this.state = {
        ...this.state,
        phase: 'ready',
        items: filterActivityEntries(page.items, this.state.filters),
        hasMore: page.hasMore,
        nextCursor: page.nextCursor ?? null,
        loadingMore: false,
        problem: null,
        busy: false,
      };
    } catch (error) {
      if (token !== this.listToken) return this.state;
      this.state = { ...this.state, phase: 'error', problem: presentProblem(error), busy: false };
    }
    return this.state;
  }

  /** Apply one filter and reload the first page from the pool. */
  async setFilter(field: ActivityFilterField, value: string): Promise<RunsViewState> {
    const filters = applyActivityFilter(this.state.filters, field, value);
    if (filters[field] === this.state.filters[field]) return this.state;
    this.state = {
      ...this.state,
      filters,
      items: [],
      hasMore: false,
      nextCursor: null,
      problem: null,
    };
    return this.refresh();
  }

  /** Drop every filter and reload. */
  async clearFilters(): Promise<RunsViewState> {
    if (!filtersActive(this.state.filters)) return this.state;
    this.state = {
      ...this.state,
      filters: emptyActivityFilters(),
      items: [],
      hasMore: false,
      nextCursor: null,
      problem: null,
    };
    return this.refresh();
  }

  /** Append the next page, if there is one. */
  async loadMore(): Promise<RunsViewState> {
    if (!this.state.hasMore || this.state.loadingMore) return this.state;
    const cursor = this.state.nextCursor;
    if (cursor === null || cursor === '') return this.state;
    const token = (this.listToken += 1);
    this.state = { ...this.state, loadingMore: true };
    try {
      const page = await this.transport.list(
        activityQuery(this.state.filters, { cursor, limit: ACTIVITY_PAGE_SIZE }),
      );
      if (token !== this.listToken) return this.state;
      this.state = {
        ...this.state,
        items: [
          ...this.state.items,
          ...filterActivityEntries(page.items, this.state.filters),
        ],
        hasMore: page.hasMore,
        nextCursor: page.nextCursor ?? null,
        loadingMore: false,
      };
    } catch (error) {
      if (token !== this.listToken) return this.state;
      this.state = { ...this.state, loadingMore: false, problem: presentProblem(error) };
    }
    return this.state;
  }

  /** Open a run's detail and start following it when it is still running. */
  async selectRun(runId: string): Promise<RunsViewState> {
    const id = runId.trim();
    if (id === '') return this.state;
    this.stopPolling();
    const token = (this.detailToken += 1);
    this.state = {
      ...this.state,
      selectedRunId: id,
      detailPhase: 'loading',
      detail: null,
      detailProblem: null,
      detailUpdatedAt: null,
      live: false,
    };
    try {
      const detail = await this.transport.run(id);
      if (token !== this.detailToken) return this.state;
      const live = runIsLive(detail.run);
      this.state = {
        ...this.state,
        detailPhase: 'ready',
        detail,
        detailProblem: null,
        detailUpdatedAt: new Date(this.now()).toISOString(),
        live,
      };
      if (live) this.schedulePoll();
    } catch (error) {
      if (token !== this.detailToken) return this.state;
      this.state = {
        ...this.state,
        detailPhase: 'error',
        detailProblem: presentProblem(error),
        live: false,
      };
    }
    return this.state;
  }

  /** Read the open detail again; the timer calls this, and so may a person. */
  async pollDetailOnce(): Promise<RunsViewState> {
    const runId = this.state.selectedRunId;
    if (runId === null) return this.state;
    const token = (this.detailToken += 1);
    try {
      const detail = await this.transport.run(runId);
      if (token !== this.detailToken) return this.state;
      const live = runIsLive(detail.run);
      this.state = {
        ...this.state,
        detailPhase: 'ready',
        detail,
        detailProblem: null,
        detailUpdatedAt: new Date(this.now()).toISOString(),
        live,
      };
      if (live) this.schedulePoll();
    } catch (error) {
      if (token !== this.detailToken) return this.state;
      const problem = presentProblem(error);
      this.state = { ...this.state, detailProblem: problem };
      // A missing run will not come back; a network blip might, so keep the
      // timer only while the last known detail was still live.
      if (problem.status === 404) {
        this.state = { ...this.state, live: false, detailPhase: 'error' };
      } else if (this.state.live) {
        this.schedulePoll();
      }
    }
    return this.state;
  }

  /** Close the detail and stop following it. */
  closeRun(): RunsViewState {
    this.stopPolling();
    this.detailToken += 1;
    this.state = {
      ...this.state,
      selectedRunId: null,
      detailPhase: 'idle',
      detail: null,
      detailProblem: null,
      detailUpdatedAt: null,
      live: false,
    };
    return this.state;
  }

  /** Start a workflow run from a manual request and follow it. */
  async dispatchManual(input: ManualRunInput): Promise<RunsViewState> {
    const issues = manualRunIssues(input);
    if (issues.length > 0) {
      this.state = {
        ...this.state,
        dispatchPhase: 'failed',
        dispatchProblem: dispatchInputProblem(issues[0] ?? 'The request is incomplete.'),
        lastDispatch: null,
      };
      return this.state;
    }

    this.state = {
      ...this.state,
      dispatchPhase: 'dispatching',
      dispatchProblem: null,
      lastDispatch: null,
      busy: true,
    };
    try {
      const response = await this.transport.dispatch(manualRunRequest(input));
      const receipt: DispatchReceipt = {
        runId: response.run.id,
        deduplicated: response.deduplicated,
      };
      this.state = {
        ...this.state,
        dispatchPhase: 'dispatched',
        dispatchProblem: null,
        lastDispatch: receipt,
        busy: false,
      };
      await this.selectRun(receipt.runId);
    } catch (error) {
      this.state = {
        ...this.state,
        dispatchPhase: 'failed',
        dispatchProblem: presentProblem(error),
        busy: false,
      };
    }
    return this.state;
  }

  /** Stop the live timer; safe to call more than once. */
  dispose(): void {
    this.stopPolling();
  }

  /** Schedule the next live read, unless one is already pending. */
  private schedulePoll(): void {
    if (this.pollHandle !== null) return;
    if (!this.state.live) return;
    this.pollHandle = this.scheduler.schedule(() => {
      this.pollHandle = null;
      return this.pollDetailOnce();
    }, this.pollIntervalMs);
  }

  /** Clear the pending live read. */
  private stopPolling(): void {
    if (this.pollHandle === null) return;
    this.scheduler.cancel(this.pollHandle);
    this.pollHandle = null;
  }
}
