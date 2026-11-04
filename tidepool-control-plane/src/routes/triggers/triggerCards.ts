/**
 * What a trigger is, said in words a person can read without the contract.
 *
 * The page has one job beyond editing: a card has to answer three questions
 * with no jargon — what fires this trigger, what it runs, and when it runs
 * next. All three are derived here, from the same `TriggerDocument` the form
 * and the YAML edit, so the card cannot drift from what would be installed.
 *
 * The capital fact about "next" is that a trigger has no clock. The pool starts
 * a run when a provider event arrives; nothing is scheduled, queued or timed.
 * There is no trigger-list route either, so the page cannot show other
 * triggers' next runs. This module therefore says the honest thing instead of
 * inventing a time: the next run is the next matching event, or nothing at all
 * while the host cannot run agents.
 *
 * The module is plain TypeScript so the wording is asserted without a DOM.
 */

import type {
  AgentSelection,
  TriggerDocument,
  TriggerFilters,
  TriggerInstallResponse,
} from '@clawffice/tidepool-sdk';
import type { StatusKind } from '$lib/status';
import type { TriggerHostRow } from '$lib/triggers';

/** Providers as a person reads them; an unknown provider keeps its own id. */
const PROVIDER_NAMES: Readonly<Record<string, string>> = {
  github: 'GitHub',
  slack: 'Slack',
  discord: 'Discord',
};

/**
 * Events that have a plain-words name.
 *
 * Anything else is described by its type id rather than guessed at: an invented
 * sentence about an event nobody recognises would be worse than the id.
 */
const EVENT_PHRASES: Readonly<Record<string, string>> = {
  'github/issue.comment_created': 'a comment is added to an issue',
  'github/issue.comment': 'a comment is added to an issue',
  'github/issues.opened': 'an issue is opened',
  'github/issue.opened': 'an issue is opened',
  'github/pull_request.opened': 'a pull request is opened',
  'github/pull_request_review.submitted': 'a pull request review is submitted',
  'slack/message.created': 'a message is posted',
  'slack/message.posted': 'a message is posted',
  'discord/message.created': 'a message is posted',
};

/** One named fact the card prints under its three answers. */
export interface TriggerFact {
  /** The small heading, e.g. `Connection`. */
  label: string;
  /** The value as the pool stores it, or a plain "none yet". */
  value: string;
}

/** One trigger, described the way the page shows it. */
export interface TriggerCardModel {
  /** Stable key for the list; the installed id, or `draft`. */
  key: string;
  /** The trigger's name, or a plain placeholder before one is typed. */
  name: string;
  /** Badge kind for the install state, or null while it is only a draft. */
  statusKind: StatusKind | null;
  /** The install state in words; always present, so the state is never colour. */
  statusText: string;
  /** What event lets this trigger run, and who may cause it. */
  fires: string;
  /** Which agent, which host, and in which mode. */
  runs: string;
  /** When the next run happens — which is to say, on the next matching event. */
  nextRun: string;
  /** The exact ids a person can match against the project's configuration. */
  facts: TriggerFact[];
}

/** What the card is derived from. */
export interface TriggerCardInput {
  /** The document the form and the YAML edit. */
  document: TriggerDocument;
  /** Enrolled hosts, so the card can say whether the chosen one can run. */
  hosts: readonly TriggerHostRow[];
  /** The install the pool reported, or null while this is only a draft. */
  installed: TriggerInstallResponse | null;
}

/** `a`, `a and b`, `a, b and c`. */
function joinWords(words: readonly string[]): string {
  if (words.length === 0) return '';
  if (words.length === 1) return words[0] ?? '';
  const head = words.slice(0, -1).join(', ');
  return `${head} and ${words[words.length - 1] ?? ''}`;
}

/** The trimmed, non-empty handles of an allowlist. */
function handlesOf(users: readonly string[]): string[] {
  return users.map((user) => user.trim()).filter((user) => user !== '');
}

/** The provider as a person reads it. */
export function providerName(provider: string): string {
  const trimmed = provider.trim();
  return PROVIDER_NAMES[trimmed] ?? trimmed;
}

/** What has to happen for the trigger to be considered, in one phrase. */
export function eventPhrase(provider: string, type: string): string {
  const named = providerName(provider);
  const trimmed = type.trim();
  if (named === '') return 'a provider event whose provider is not chosen yet';
  if (trimmed === '') return `a ${named} event, though the event type is not chosen yet`;
  return EVENT_PHRASES[`${provider.trim()}/${trimmed}`] ?? `a ${named} event of type '${trimmed}'`;
}

/** The extra delivery conditions, each in one phrase. */
export function filterPhrases(filters: TriggerFilters): string[] {
  const phrases: string[] = [];
  const channel = filters.channel?.trim() ?? '';
  const label = filters.label?.trim() ?? '';
  const contains = filters.contains?.trim() ?? '';
  const pattern = filters.pattern?.trim() ?? '';
  if (channel !== '') phrases.push(`the channel '${channel}'`);
  if (label !== '') phrases.push(`the label '${label}'`);
  if (contains !== '') phrases.push(`text that contains '${contains}'`);
  if (pattern !== '') phrases.push(`text that matches the pattern '${pattern}'`);
  return phrases;
}

/** The allowlist as a sentence, including what an empty one means. */
export function allowlistSentence(users: readonly string[]): string {
  const handles = handlesOf(users);
  if (handles.length === 0) {
    return 'Nobody may start it yet: the allowlist is empty, and the pool refuses a trigger without one.';
  }
  return `Only ${joinWords(handles)} may start it; an event from anyone else is ignored.`;
}

/** The agent, in one phrase, with the missing choices named. */
export function agentPhrase(agent: AgentSelection): string {
  const provider = agent.provider.trim();
  const model = agent.model.trim();
  const mode = agent.execution_mode.trim();
  const thinking = agent.thinking_option?.trim() ?? '';
  if (provider === '' || model === '' || mode === '') {
    const missing: string[] = [];
    if (provider === '') missing.push('provider');
    if (model === '') missing.push('model');
    if (mode === '') missing.push('mode');
    return `an agent, once its ${joinWords(missing)} ${missing.length === 1 ? 'is' : 'are'} chosen`;
  }
  const base = `the ${provider} agent '${model}' in ${mode} mode`;
  return thinking === '' ? base : `${base} with ${thinking} thinking`;
}

/** Whether the card can promise that a matching event would start a run. */
function hostBlocker(host: TriggerHostRow | null): string | null {
  if (host === null) return 'no enrolled host has that slug';
  if (host.status === 'offline') return 'the host is offline';
  if (host.status === 'revoked') return 'the host was revoked';
  if (host.status === 'pending_approval') return 'the host is waiting for approval';
  if (!host.executionEnabled) return 'the host has not been granted execution';
  return null;
}

/** The resource, in one phrase, naming only the parts that are set. */
function resourcePhrase(kind: string, id: string): string | null {
  const trimmedKind = kind.trim();
  const trimmedId = id.trim();
  if (trimmedKind === '' && trimmedId === '') return null;
  if (trimmedKind === '') return `'${trimmedId}'`;
  if (trimmedId === '') return `every ${trimmedKind}`;
  return `${trimmedKind} '${trimmedId}'`;
}

/** What fires the trigger, in one or two sentences. */
export function firesPhrase(document: TriggerDocument): string {
  const { filters } = document;
  const event = eventPhrase(document.event.provider, document.event.type);
  const connection = filters.connection.trim();
  const resource = resourcePhrase(filters.resource.kind, filters.resource.id);
  const guards = filterPhrases(filters);

  let text = `Fires when ${event}`;
  if (connection !== '') text += ` through the '${connection}' connection`;
  if (resource !== null) text += ` on the ${resource}`;
  text += '.';
  text += ` ${allowlistSentence(filters.from_users)}`;
  if (guards.length > 0) {
    text += ` A delivery also has to match ${joinWords(guards)}.`;
  }
  return text;
}

/** What the trigger runs, in one sentence. */
export function runsPhrase(document: TriggerDocument): string {
  const host = document.run.host.trim();
  const agent = agentPhrase(document.run.agent);
  if (host === '') {
    return `Runs ${agent}, but no host is chosen yet: a run needs a host that can do the work.`;
  }
  return `Runs ${agent} on the host '${host}'.`;
}

/** When the trigger runs next, in one sentence, with no invented time. */
export function nextRunPhrase(document: TriggerDocument, host: TriggerHostRow | null): string {
  const slug = document.run.host.trim();
  const blocker = slug === '' ? 'no host is chosen yet' : hostBlocker(host);
  if (blocker !== null) {
    return `Nothing will run until ${blocker}; until then a matching event is recorded and no agent starts.`;
  }
  const provider = providerName(document.event.provider);
  return `It runs on the next matching ${provider} event. Nothing is scheduled: a trigger has no clock, cron or queue, so it waits for an event and does nothing in between.`;
}

/** The install state of the card, in words and a badge kind. */
function statusOf(installed: TriggerInstallResponse | null): {
  statusKind: StatusKind | null;
  statusText: string;
} {
  if (installed === null) {
    return {
      statusKind: null,
      statusText:
        'Not installed yet. Editing below changes the draft only; the pool runs the installed version.',
    };
  }
  if (!installed.active) {
    return {
      statusKind: 'neutral',
      statusText: `Version ${installed.version} is installed but not active, so no event starts it.`,
    };
  }
  const replaced = installed.replaced ? ', replacing the previous version' : '';
  return {
    statusKind: 'success',
    statusText: `Installed as version ${installed.version}${replaced}; the pool is running this one.`,
  };
}

/** Describe one trigger. */
export function triggerCard(input: TriggerCardInput): TriggerCardModel {
  const { document, hosts, installed } = input;
  const slug = document.run.host.trim();
  const host = slug === '' ? null : (hosts.find((row) => row.slug === slug) ?? null);
  const handles = handlesOf(document.filters.from_users);
  const resource = resourcePhrase(
    document.filters.resource.kind,
    document.filters.resource.id,
  );
  const connection = document.filters.connection.trim();
  const status = statusOf(installed);

  return {
    key: installed?.id ?? 'draft',
    name: document.name.trim() === '' ? 'Untitled trigger' : document.name.trim(),
    statusKind: status.statusKind,
    statusText: status.statusText,
    fires: firesPhrase(document),
    runs: runsPhrase(document),
    nextRun: nextRunPhrase(document, host),
    facts: [
      { label: 'Connection', value: connection === '' ? 'none chosen yet' : connection },
      { label: 'Resource', value: resource ?? 'any resource' },
      {
        label: 'Allowed users',
        value: handles.length === 0 ? 'none chosen yet' : handles.join(', '),
      },
      { label: 'Host', value: slug === '' ? 'none chosen yet' : slug },
    ],
  };
}

/** The list the page shows: the trigger being edited, installed or not. */
export function triggerCards(input: TriggerCardInput): TriggerCardModel[] {
  return [triggerCard(input)];
}
