/**
 * What a trigger's card says.
 *
 * Criterion: "A trigger's card shows what fires it, what it runs and when it
 * runs next." These tests assert those three answers in plain words, that the
 * card follows the document it is handed, that the state is a word and not
 * colour alone, and that "when" never invents a clock the pool does not have.
 *
 * The model is plain TypeScript, so most of this is asserted without a DOM; the
 * render tests use the server renderer and check what a person would read.
 */

import { describe, expect, it } from 'vitest';
import TriggerCard from './TriggerCard.svelte';
import {
  agentPhrase,
  allowlistSentence,
  eventPhrase,
  filterPhrases,
  firesPhrase,
  nextRunPhrase,
  providerName,
  runsPhrase,
  triggerCard,
  triggerCards,
} from './triggerCards';
import { renderComponent } from '$lib/testing/render';
import type { TriggerDocument, TriggerInstallResponse } from '@clawffice/tidepool-sdk';
import { triggerHostRows, type TriggerHostRow } from '$lib/triggers';
import type { HostView } from '@clawffice/tidepool-sdk';

/** A connected host that may run agents. */
function host(overrides: Partial<HostView> = {}): HostView {
  return {
    id: 'host-1',
    slug: 'swaygent',
    display_name: 'Swaygent',
    hostname: 'swaygent.local',
    status: 'connected',
    execution_enabled: true,
    created_at: '2026-10-29T09:00:00.000Z',
    revoked_at: null,
    ...overrides,
  };
}

const HOSTS: TriggerHostRow[] = triggerHostRows([host()]);

const DOCUMENT: TriggerDocument = {
  name: 'auto-fix-issues',
  event: { provider: 'github', type: 'issue.comment_created' },
  filters: {
    connection: 'github-main',
    resource: { kind: 'repository', id: 'acme/widgets' },
    from_users: ['alice', 'bob'],
    channel: null,
    label: null,
    contains: null,
    pattern: null,
  },
  run: {
    host: 'swaygent',
    agent: {
      provider: 'anthropic',
      model: 'claude',
      execution_mode: 'plan',
      thinking_option: 'high',
    },
  },
};

/** An installed trigger, as the pool reports one. */
function installed(overrides: Partial<TriggerInstallResponse> = {}): TriggerInstallResponse {
  return {
    id: 'trigger-1',
    name: 'auto-fix-issues',
    revision: 'revision-1',
    version: 3,
    active: true,
    replaced: false,
    ...overrides,
  };
}

/** The card model for a document, with the fixture hosts. */
function card(
  document: TriggerDocument = DOCUMENT,
  install: TriggerInstallResponse | null = null,
  hosts: readonly TriggerHostRow[] = HOSTS,
) {
  return triggerCard({ document, hosts, installed: install });
}

describe('what fires a trigger', () => {
  it('names a known provider event in plain words', () => {
    expect(eventPhrase('github', 'issue.comment_created')).toBe(
      'a comment is added to an issue',
    );
    expect(eventPhrase('slack', 'message.created')).toBe('a message is posted');
  });

  it('keeps an unknown event type as its id rather than guessing', () => {
    expect(eventPhrase('github', 'deployment.created')).toBe(
      "a GitHub event of type 'deployment.created'",
    );
  });

  it('says what is missing while the document is still empty', () => {
    expect(eventPhrase('', '')).toBe('a provider event whose provider is not chosen yet');
    expect(eventPhrase('github', '')).toBe(
      'a GitHub event, though the event type is not chosen yet',
    );
  });

  it('reads the provider as a person writes it', () => {
    expect(providerName('github')).toBe('GitHub');
    expect(providerName('bitbucket')).toBe('bitbucket');
  });

  it('states the connection, the resource and the allowlist together', () => {
    const text = firesPhrase(DOCUMENT);
    expect(text).toContain('Fires when a comment is added to an issue');
    expect(text).toContain("through the 'github-main' connection");
    expect(text).toContain("on the repository 'acme/widgets'");
    expect(text).toContain('Only alice and bob may start it');
    expect(text).toContain('an event from anyone else is ignored');
  });

  it('says an empty allowlist stops everything, because the pool refuses it', () => {
    expect(allowlistSentence([])).toContain('Nobody may start it yet');
    expect(allowlistSentence([])).toContain('the pool refuses a trigger without one');
  });

  it('names each extra delivery filter that is set and none that is not', () => {
    const filters = { ...DOCUMENT.filters, channel: 'deploys', label: 'run-me' };
    expect(filterPhrases(filters)).toEqual(["the channel 'deploys'", "the label 'run-me'"]);
    expect(filterPhrases(DOCUMENT.filters)).toEqual([]);
    expect(firesPhrase({ ...DOCUMENT, filters })).toContain(
      "A delivery also has to match the channel 'deploys' and the label 'run-me'.",
    );
  });
});

describe('what a trigger runs', () => {
  it('names the agent, its model, its mode, its thinking and its host', () => {
    expect(agentPhrase(DOCUMENT.run.agent)).toBe(
      "the anthropic agent 'claude' in plan mode with high thinking",
    );
    expect(runsPhrase(DOCUMENT)).toBe(
      "Runs the anthropic agent 'claude' in plan mode with high thinking on the host 'swaygent'.",
    );
  });

  it('names the missing choices instead of printing an empty agent', () => {
    const agent = { ...DOCUMENT.run.agent, model: '', thinking_option: null };
    expect(agentPhrase(agent)).toBe('an agent, once its model is chosen');
    expect(runsPhrase({ ...DOCUMENT, run: { host: '', agent } })).toContain(
      'no host is chosen yet',
    );
  });

  it('drops the thinking clause when there is no thinking option', () => {
    const agent = { ...DOCUMENT.run.agent, thinking_option: null };
    expect(agentPhrase(agent)).not.toContain('thinking');
  });
});

describe('when a trigger runs next', () => {
  it('says the next matching event, and denies having a clock', () => {
    const text = nextRunPhrase(DOCUMENT, HOSTS[0] ?? null);
    expect(text).toContain('It runs on the next matching GitHub event.');
    expect(text).toContain('Nothing is scheduled');
    expect(text).toContain('no clock, cron or queue');
    expect(text).not.toMatch(/\d{1,2}:\d{2}/);
    expect(text).not.toMatch(/\d{4}-\d{2}-\d{2}T/);
  });

  it('says why nothing will run when the host cannot run agents', () => {
    const offline = triggerHostRows([host({ status: 'offline' })])[0] ?? null;
    expect(nextRunPhrase(DOCUMENT, offline)).toContain('the host is offline');

    const ungranted = triggerHostRows([host({ execution_enabled: false })])[0] ?? null;
    expect(nextRunPhrase(DOCUMENT, ungranted)).toContain(
      'the host has not been granted execution',
    );

    expect(nextRunPhrase(DOCUMENT, null)).toContain('no enrolled host has that slug');
  });

  it('says no host is chosen while the document has none', () => {
    const document = { ...DOCUMENT, run: { ...DOCUMENT.run, host: '' } };
    expect(nextRunPhrase(document, null)).toContain('no host is chosen yet');
  });
});

describe('the card state is a word, not a colour', () => {
  it('says a draft is not installed and changes nothing yet', () => {
    const model = card();
    expect(model.statusKind).toBeNull();
    expect(model.statusText).toContain('Not installed yet');
    expect(model.statusText).toContain('the pool runs the installed version');
  });

  it('says which version the pool is running', () => {
    const model = card(DOCUMENT, installed());
    expect(model.statusKind).toBe('success');
    expect(model.statusText).toContain('Installed as version 3');
    expect(model.statusText).toContain('the pool is running this one');
  });

  it('says a replaced install replaced the previous version', () => {
    expect(card(DOCUMENT, installed({ replaced: true })).statusText).toContain(
      'replacing the previous version',
    );
  });

  it('says an inactive version starts nothing', () => {
    const model = card(DOCUMENT, installed({ active: false }));
    expect(model.statusKind).toBe('neutral');
    expect(model.statusText).toContain('not active');
    expect(model.statusText).toContain('no event starts it');
  });
});

describe('the card describes the document it is handed', () => {
  it('uses the installed id as its key and a placeholder before a name', () => {
    expect(card().key).toBe('draft');
    expect(card(DOCUMENT, installed()).key).toBe('trigger-1');
    expect(card({ ...DOCUMENT, name: '  ' }).name).toBe('Untitled trigger');
  });

  it('prints the settings a person matches against the project', () => {
    const facts = card().facts;
    expect(facts).toContainEqual({ label: 'Connection', value: 'github-main' });
    expect(facts).toContainEqual({ label: 'Resource', value: "repository 'acme/widgets'" });
    expect(facts).toContainEqual({ label: 'Allowed users', value: 'alice, bob' });
    expect(facts).toContainEqual({ label: 'Host', value: 'swaygent' });
  });

  it('says none chosen yet rather than printing an empty value', () => {
    const empty = card({
      ...DOCUMENT,
      filters: { ...DOCUMENT.filters, connection: '', from_users: [] },
      run: { ...DOCUMENT.run, host: '' },
    });
    expect(empty.facts).toContainEqual({ label: 'Connection', value: 'none chosen yet' });
    expect(empty.facts).toContainEqual({ label: 'Allowed users', value: 'none chosen yet' });
    expect(empty.facts).toContainEqual({ label: 'Host', value: 'none chosen yet' });
  });

  it('lists the one trigger this page edits', () => {
    expect(triggerCards({ document: DOCUMENT, hosts: HOSTS, installed: null })).toHaveLength(1);
  });
});

describe('the rendered card', () => {
  it('answers fires, runs and next in that order', () => {
    const rendered = renderComponent(TriggerCard, { card: card() });
    expect(rendered.text).toContain('auto-fix-issues');
    expect(rendered.text).toContain('What fires it');
    expect(rendered.text).toContain('What it runs');
    expect(rendered.text).toContain('When it runs next');

    const fires = rendered.html.indexOf('data-trigger-card-answer="fires"');
    const runs = rendered.html.indexOf('data-trigger-card-answer="runs"');
    const next = rendered.html.indexOf('data-trigger-card-answer="next"');
    expect(fires).toBeGreaterThan(-1);
    expect(runs).toBeGreaterThan(fires);
    expect(next).toBeGreaterThan(runs);
  });

  it('shows the install state as a badge word beside the name', () => {
    const rendered = renderComponent(TriggerCard, { card: card(DOCUMENT, installed()) });
    expect(rendered.html).toContain('data-status="success"');
    expect(rendered.text).toContain('OK');
    expect(rendered.text).toContain('Installed as version 3');
  });

  it('shows no badge for a draft, and still says it is a draft', () => {
    const rendered = renderComponent(TriggerCard, { card: card() });
    expect(rendered.html).not.toContain('data-status=');
    expect(rendered.text).toContain('Not installed yet');
  });

  it('prints the settings with their labels', () => {
    const rendered = renderComponent(TriggerCard, { card: card() });
    expect(rendered.text).toContain('Connection');
    expect(rendered.text).toContain('github-main');
    expect(rendered.text).toContain('acme/widgets');
  });
});
