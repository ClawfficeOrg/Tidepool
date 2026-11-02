/**
 * The trigger editor's availability rules, form/YAML agreement and rendered
 * states.
 *
 * These are server-rendered component tests plus direct controller tests: no
 * DOM, no browser and no live pool. The end-to-end file beside this one walks
 * the same flows through the real SDK against a fake HTTP pool.
 */

import { describe, expect, it } from 'vitest';
import TriggersPage from './TriggersPage.svelte';
import { renderComponent } from '$lib/testing/render';
import type {
  HostCapabilityReport,
  HostView,
  ProjectView,
  TriggerDocument,
} from '@clawffice/tidepool-sdk';
import { ProblemError } from '@clawffice/tidepool-sdk';
import {
  TriggersController,
  agentOptionsFromReport,
  documentFromYaml,
  emptyTriggersState,
  preflightIssues,
  selectionAvailability,
  triggerHostRows,
  triggerYaml,
  type AgentChoicesState,
  type TriggersTransport,
  type TriggersViewState,
} from '$lib/triggers';

const POOL = { baseUrl: 'https://pool.example.com' };

const PROJECT: ProjectView = {
  id: 'project-1',
  slug: 'nightly',
  name: 'Nightly',
  created_at: '2026-10-28T20:00:00.000Z',
  updated_at: '2026-10-28T20:00:00.000Z',
};

/** One connected host that may run agents. */
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

const HOSTS: HostView[] = [
  host(),
  host({
    id: 'host-2',
    slug: 'build-box',
    display_name: 'Build box',
    hostname: 'build-box.local',
  }),
  host({
    id: 'host-3',
    slug: 'night-host',
    display_name: 'Night host',
    hostname: 'night-host.local',
    status: 'offline',
  }),
  host({
    id: 'host-4',
    slug: 'gone-host',
    display_name: 'Gone host',
    hostname: 'gone-host.local',
    status: 'revoked',
    execution_enabled: false,
  }),
];

/**
 * What the connected host reports.
 *
 * `github`/`copilot` is reported but outside the pool's offered providers, so
 * it must never appear in the cascade.
 */
const REPORT: HostCapabilityReport = {
  host_id: 'host-1',
  reported_at: '2026-10-29T11:00:00.000Z',
  offered_providers: ['anthropic', 'openai'],
  capabilities: [
    { provider: 'anthropic', model: 'claude', execution_mode: 'default' },
    {
      provider: 'anthropic',
      model: 'claude',
      execution_mode: 'plan',
      thinking_option: 'high',
    },
    {
      provider: 'anthropic',
      model: 'claude',
      execution_mode: 'plan',
      thinking_option: 'low',
    },
    { provider: 'openai', model: 'gpt', execution_mode: 'default' },
    { provider: 'github', model: 'copilot', execution_mode: 'default' },
  ],
};

const DOCUMENT: TriggerDocument = {
  name: 'auto-fix-issues',
  event: { provider: 'github', type: 'issue.comment_created' },
  filters: {
    connection: 'github-main',
    resource: { kind: 'repository', id: 'acme/widgets' },
    from_users: ['alice'],
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

/** The ready agent-choice state for the fixture report. */
function readyChoices(): AgentChoicesState {
  return {
    status: 'ready',
    host: 'swaygent',
    directory: '',
    report: REPORT,
    options: agentOptionsFromReport(REPORT),
  };
}

/** A ready editor state. */
function state(overrides: Partial<TriggersViewState> = {}): TriggersViewState {
  const document = overrides.document ?? DOCUMENT;
  return {
    ...emptyTriggersState(),
    phase: 'ready',
    projects: [PROJECT],
    selectedProjectId: PROJECT.id,
    hosts: triggerHostRows(HOSTS),
    document,
    yaml: triggerYaml(document),
    choices: readyChoices(),
    ...overrides,
  };
}

/** A transport that a server-rendered test must not call. */
const UNUSED_TRANSPORT: TriggersTransport = {
  listProjects: () => Promise.reject(new Error('the page renders from its initial state')),
  listHosts: () => Promise.reject(new Error('not called')),
  capabilities: () => Promise.reject(new Error('not called')),
  validate: () => Promise.reject(new Error('not called')),
  install: () => Promise.reject(new Error('not called')),
};

/** A recording transport for controller tests. */
function fakeTransport(options: {
  report?: HostCapabilityReport;
  reportFor?: (directory: string) => HostCapabilityReport;
  validationValid?: boolean;
  validationErrors?: { field: string; message: string }[];
} = {}): TriggersTransport & {
  capabilityCalls: { hostId: string; directory: string }[];
  validateCalls: TriggerDocument[];
  installCalls: unknown[];
} {
  const calls = {
    capabilityCalls: [] as { hostId: string; directory: string }[],
    validateCalls: [] as TriggerDocument[],
    installCalls: [] as unknown[],
  };
  const report = options.report ?? REPORT;
  return {
    ...calls,
    listProjects: () => Promise.resolve({ projects: [PROJECT] }),
    listHosts: () => Promise.resolve({ hosts: HOSTS }),
    capabilities: (hostId, directory) => {
      calls.capabilityCalls.push({ hostId, directory });
      return Promise.resolve(options.reportFor ? options.reportFor(directory) : report);
    },
    validate: (document) => {
      calls.validateCalls.push(document);
      return Promise.resolve({
        name: document.name,
        valid: options.validationValid ?? true,
        errors: options.validationErrors ?? [],
      });
    },
    install: (request) => {
      calls.installCalls.push(request);
      return Promise.resolve({
        id: 'trigger-1',
        name: request.name,
        revision: 'revision-1',
        version: 1,
        active: true,
        replaced: false,
      });
    },
  };
}

/** Render the page from a supplied state, with a member role. */
function renderPage(overrides: Partial<TriggersViewState> = {}, props: Record<string, unknown> = {}) {
  return renderComponent(TriggersPage, {
    pool: POOL,
    token: 'token-1',
    role: 'member',
    transport: UNUSED_TRANSPORT,
    initial: state(overrides),
    ...props,
  });
}

describe('the agent cascade offers only what the host reports', () => {
  it('keeps the offered providers and drops a reported but non-grantable one', () => {
    const options = agentOptionsFromReport(REPORT);
    expect(options.providers).toEqual(['anthropic', 'openai']);
    expect(options.models['anthropic']).toEqual(['claude']);
    expect(options.models['github']).toBeUndefined();
    expect(options.modes['anthropic\u0000claude']).toEqual(['default', 'plan']);
    expect(options.thinking['anthropic\u0000claude\u0000plan']).toEqual(['high', 'low']);
  });

  it('renders the offered model and never the non-offered provider in the cascade', () => {
    const rendered = renderPage();
    expect(rendered.html).toContain('value="anthropic"');
    expect(rendered.html).toContain('value="openai"');
    expect(rendered.html).toContain('value="claude"');
    expect(rendered.text).not.toContain('copilot');
  });

  it('marks an authored value the host no longer reports without replacing it', () => {
    const document: TriggerDocument = {
      ...DOCUMENT,
      run: { ...DOCUMENT.run, agent: { ...DOCUMENT.run.agent, model: 'claude-2' } },
    };
    const rendered = renderPage({ document, yaml: triggerYaml(document) });

    expect(rendered.text).toContain('claude-2 (unavailable)');
    expect(rendered.text).toContain('no longer reports');
    expect(rendered.html).toContain('data-agent-unavailable-fields="true"');

    const controller = new TriggersController({
      transport: fakeTransport(),
      initial: state({ document }),
    });
    expect(controller.snapshot().document.run.agent.model).toBe('claude-2');
    expect(
      selectionAvailability(agentOptionsFromReport(REPORT), controller.snapshot().document.run.agent)
        .unavailable.map((entry) => entry.field),
    ).toContain('model');
  });
});

describe('changing the host or directory reloads the choices', () => {
  it('asks the pool again for a new host', async () => {
    const transport = fakeTransport();
    const controller = new TriggersController({ transport, initial: state() });

    const next = await controller.setHost('build-box');

    expect(transport.capabilityCalls).toEqual([{ hostId: 'host-2', directory: '' }]);
    expect(next.choices.status).toBe('ready');
    if (next.choices.status === 'ready') {
      expect(next.choices.host).toBe('build-box');
    }
  });

  it('asks the pool again for a new working directory and offers what it answers', async () => {
    const narrow: HostCapabilityReport = {
      ...REPORT,
      offered_providers: ['anthropic'],
    };
    const wide: HostCapabilityReport = {
      ...REPORT,
      offered_providers: ['anthropic', 'openai', 'google'],
      capabilities: [
        ...REPORT.capabilities,
        { provider: 'google', model: 'gemini', execution_mode: 'default' },
      ],
    };
    const transport = fakeTransport({
      reportFor: (directory) => (directory === '/srv/wide' ? wide : narrow),
    });
    const controller = new TriggersController({ transport, initial: state() });

    const first = await controller.setDirectory('/srv/narrow');
    expect(transport.capabilityCalls).toEqual([{ hostId: 'host-1', directory: '/srv/narrow' }]);
    expect(first.choices.status).toBe('ready');
    if (first.choices.status === 'ready') {
      expect(first.choices.options.providers).toEqual(['anthropic']);
    }

    const second = await controller.setDirectory('/srv/wide');
    expect(transport.capabilityCalls).toEqual([
      { hostId: 'host-1', directory: '/srv/narrow' },
      { hostId: 'host-1', directory: '/srv/wide' },
    ]);
    expect(second.choices.status).toBe('ready');
    if (second.choices.status === 'ready') {
      expect(second.choices.options.providers).toEqual(['anthropic', 'google', 'openai']);
    }
  });
});

describe('a host that cannot run the agent is a save-time refusal', () => {
  it('names run.agent.model and never calls the pool', async () => {
    const transport = fakeTransport();
    const document: TriggerDocument = {
      ...DOCUMENT,
      run: { ...DOCUMENT.run, agent: { ...DOCUMENT.run.agent, model: 'claude-2' } },
    };
    const controller = new TriggersController({
      transport,
      initial: state({ document }),
    });

    const next = await controller.save();

    expect(next.savePhase).toBe('refused');
    expect(next.issues.map((issue) => issue.field)).toContain('run.agent.model');
    expect(next.issues.map((issue) => issue.message)).toContain(
      "host 'swaygent' cannot run 'claude-2'",
    );
    expect(transport.validateCalls).toHaveLength(0);
    expect(transport.installCalls).toHaveLength(0);
    expect(next.saved).toBeNull();
  });

  it('refuses a host without the execution grant at run.host', () => {
    const connectedOnly = host({ execution_enabled: false });
    const issues = preflightIssues({
      document: DOCUMENT,
      hosts: triggerHostRows([connectedOnly]),
      choices: readyChoices(),
    });
    expect(issues).toContainEqual({
      field: 'run.host',
      message: "host 'swaygent' has not been granted execution",
    });
  });

  it('surfaces the pool’s own named refusal when preflight passes', async () => {
    const transport = fakeTransport({
      validationValid: false,
      validationErrors: [
        { field: 'filters.connection', message: "no active connection with slug 'github-main'" },
      ],
    });
    const controller = new TriggersController({ transport, initial: state() });

    const next = await controller.save();

    expect(next.savePhase).toBe('refused');
    expect(next.issues).toEqual([
      { field: 'filters.connection', message: "no active connection with slug 'github-main'" },
    ]);
    expect(transport.validateCalls).toHaveLength(1);
    expect(transport.installCalls).toHaveLength(0);
  });

  it('installs only after validation says the document is valid', async () => {
    const transport = fakeTransport();
    const controller = new TriggersController({ transport, initial: state() });

    const next = await controller.save();

    expect(next.savePhase).toBe('saved');
    expect(next.saved?.version).toBe(1);
    expect(transport.validateCalls).toHaveLength(1);
    expect(transport.installCalls).toHaveLength(1);
    expect(transport.installCalls[0]).toMatchObject({
      project: 'project-1',
      name: 'auto-fix-issues',
    });
  });
});

describe('the form and the YAML describe one document', () => {
  it('round-trips the canonical YAML through the parser', () => {
    const parsed = documentFromYaml(triggerYaml(DOCUMENT));
    expect(parsed.ok).toBe(true);
    if (parsed.ok) expect(parsed.document).toEqual(DOCUMENT);
  });

  it('rewrites the YAML when a form field changes', () => {
    const controller = new TriggersController({
      transport: fakeTransport(),
      initial: state(),
    });

    controller.apply({ field: 'name', value: 'renamed-trigger' });

    const next = controller.snapshot();
    const parsed = documentFromYaml(next.yaml);
    expect(parsed.ok).toBe(true);
    if (parsed.ok) {
      expect(parsed.document.name).toBe('renamed-trigger');
      expect(parsed.document).toEqual(next.document);
    }
  });

  it('adopts YAML into the form when it parses', () => {
    const controller = new TriggersController({
      transport: fakeTransport(),
      initial: state(),
    });
    controller.setMode('yaml');
    const typed = triggerYaml(DOCUMENT).replace('name: auto-fix-issues', 'name: typed-in-yaml');

    controller.editYaml(typed);

    const next = controller.snapshot();
    expect(next.yamlError).toBeNull();
    expect(next.document.name).toBe('typed-in-yaml');
    expect(next.yaml).toBe(typed);
  });

  it('keeps the last parsed document while YAML is broken and refuses the form until fixed', () => {
    const controller = new TriggersController({
      transport: fakeTransport(),
      initial: state(),
    });
    controller.setMode('yaml');

    controller.editYaml('name: [unterminated\n');

    const broken = controller.snapshot();
    expect(broken.yamlError).toContain('YAML line');
    expect(broken.document.name).toBe(DOCUMENT.name);
    expect(broken.mode).toBe('yaml');
    expect(controller.setMode('form').mode).toBe('yaml');

    const fixed = triggerYaml({ ...DOCUMENT, name: 'fixed' });
    controller.editYaml(fixed);
    expect(controller.snapshot().yamlError).toBeNull();
    expect(controller.setMode('form').mode).toBe('form');
    expect(controller.snapshot().document.name).toBe('fixed');
  });

  it('refuses YAML with an unknown key rather than silently dropping it', () => {
    const parsed = documentFromYaml(`${triggerYaml(DOCUMENT)}runDirectory: /srv/tidepool\n`);
    expect(parsed.ok).toBe(false);
    if (!parsed.ok) {
      expect(parsed.issues.map((issue) => issue.message).join(' ')).toContain(
        'unknown key "runDirectory"',
      );
    }
  });
});

describe('an offline host explains itself and leaves the rest editable', () => {
  it('renders the explanation and a retry, with the form still mounted', () => {
    const offline: AgentChoicesState = {
      status: 'offline',
      host: 'night-host',
      directory: '/srv/tidepool',
      message:
        "The host 'night-host' is offline, so the pool cannot ask it what it can run in '/srv/tidepool'. Start the host, then retry. The rest of the trigger stays editable.",
      problem: null,
    };
    const document: TriggerDocument = {
      ...DOCUMENT,
      run: { ...DOCUMENT.run, host: 'night-host' },
    };
    const rendered = renderPage({
      document,
      yaml: triggerYaml(document),
      directory: '/srv/tidepool',
      choices: offline,
    });

    expect(rendered.text).toContain("The host 'night-host' is offline");
    expect(rendered.html).toContain('data-agent-retry="true"');
    expect(rendered.html).toContain('data-trigger-form="true"');
    expect(rendered.html).toContain('data-save-action="save"');
    expect(rendered.html).not.toMatch(/data-trigger-form="true"[^>]*disabled/);
  });

  it('changes another field while the host is offline', async () => {
    const controller = new TriggersController({
      transport: fakeTransport(),
      initial: state({
        document: {
          ...DOCUMENT,
          run: { ...DOCUMENT.run, host: 'night-host' },
        },
      }),
    });

    const next = await controller.setHost('night-host');
    expect(next.choices.status).toBe('offline');

    controller.apply({ field: 'name', value: 'still-editable' });
    expect(controller.snapshot().document.name).toBe('still-editable');
    expect(controller.snapshot().choices.status).toBe('offline');
  });

  it('retries the capability call and recovers when the host answers', async () => {
    let offline = true;
    const transport: TriggersTransport = {
      ...fakeTransport(),
      capabilities: () =>
        offline
          ? Promise.reject(
              new ProblemError('host is offline', {
                status: 503,
                code: 'daemon_not_connected',
                title: 'The host is offline',
                detail: 'daemon_not_connected',
                requestId: 'req_offline',
              }),
            )
          : Promise.resolve(REPORT),
    };
    const controller = new TriggersController({ transport, initial: state() });

    const failed = await controller.reloadChoices();
    expect(failed.choices.status).toBe('offline');
    expect(failed.choices.status === 'offline' ? failed.choices.message : '').toContain(
      'is offline',
    );

    offline = false;
    const recovered = await controller.retryChoices();
    expect(recovered.choices.status).toBe('ready');
  });
});

describe('the rendered page', () => {
  it('shows the project, the host source and the save state', () => {
    const rendered = renderPage();
    expect(rendered.text).toContain('Nightly');
    expect(rendered.text).toContain('swaygent');
    expect(rendered.html).toContain('data-agent-source="ready"');
    expect(rendered.text).toContain('Not saved yet.');
  });

  it('renders named issues with the field the author must fix', () => {
    const rendered = renderPage({
      savePhase: 'refused',
      issues: [{ field: 'run.agent.model', message: "host 'swaygent' cannot run 'ghost'" }],
    });
    expect(rendered.html).toContain('data-issue-field="run.agent.model"');
    expect(rendered.text).toContain('run.agent.model');
    expect(rendered.text).toContain("cannot run 'ghost'");
  });

  it('shows a YAML parse failure and the view that still describes the document', () => {
    const rendered = renderPage({
      mode: 'yaml',
      yamlError: 'YAML line 1: the document is empty',
      issues: [{ field: 'yaml', message: 'YAML line 1: the document is empty' }],
    });
    expect(rendered.text).toContain('YAML line 1');
    expect(rendered.html).toContain('data-yaml-error="true"');
    expect(rendered.text).toContain('last YAML that parsed');
  });
});
