/**
 * The trigger editor across the SDK boundary, without a server.
 *
 * The real TypeScript SDK talks to an in-memory pool through an injected
 * `fetch`, so this test exercises the URLs, bodies, auth header and
 * problem-details parsing the browser uses. It walks the task's two named
 * cases end to end: a connected host whose report is reloaded by host and
 * directory, and an offline host that explains itself and recovers on retry.
 */

import { describe, expect, it } from 'vitest';
import { createClient } from '@clawffice/tidepool-sdk';
import type {
  HostCapabilityReport,
  HostView,
  TriggerDocument,
  TriggerInstallRequest,
} from '@clawffice/tidepool-sdk';
import {
  TriggersController,
  emptyTriggersState,
  triggerYaml,
  triggersTransport,
} from '$lib/triggers';

const BASE_URL = 'https://pool.example.com';
const TOKEN = 'token-1';

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

/** A host the pool can dispatch to. */
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

/** What a host reports in one working directory. */
function report(directory: string): HostCapabilityReport {
  const openai =
    directory === '/srv/wide'
      ? [{ provider: 'openai', model: 'gpt', execution_mode: 'default' }]
      : [];
  return {
    host_id: 'host-1',
    reported_at: '2026-10-29T11:00:00.000Z',
    offered_providers:
      directory === '/srv/wide' ? ['anthropic', 'openai'] : ['anthropic'],
    capabilities: [
      { provider: 'anthropic', model: 'claude', execution_mode: 'default' },
      {
        provider: 'anthropic',
        model: 'claude',
        execution_mode: 'plan',
        thinking_option: 'high',
      },
      ...openai,
    ],
  };
}

/** The fake pool's state. */
interface FakePool {
  hosts: HostView[];
  calls: string[];
  capabilityDirectories: string[];
  validateCalls: TriggerDocument[];
  installCalls: TriggerInstallRequest[];
  offline: boolean;
  refuseConnection: boolean;
}

/** A fresh fake pool. */
function fakePool(): FakePool {
  return {
    hosts: [host(), host({ id: 'host-2', slug: 'build-box', display_name: 'Build box' })],
    calls: [],
    capabilityDirectories: [],
    validateCalls: [],
    installCalls: [],
    offline: false,
    refuseConnection: false,
  };
}

/** A JSON response. */
function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

/** A problem-details response. */
function problem(status: number, code: string, detail: string): Response {
  return new Response(
    JSON.stringify({ status, code, title: detail, detail, requestId: 'req_e2e_triggers' }),
    {
      status,
      headers: {
        'content-type': 'application/problem+json',
        'x-request-id': 'req_e2e_triggers',
      },
    },
  );
}

/** Parse a request body as an object. */
function bodyOf(init: RequestInit | undefined): Record<string, unknown> {
  const raw = init?.body;
  if (typeof raw !== 'string' || raw.trim() === '') return {};
  const parsed: unknown = JSON.parse(raw);
  return parsed !== null && typeof parsed === 'object' && !Array.isArray(parsed)
    ? (parsed as Record<string, unknown>)
    : {};
}

/** The fake pool's `fetch`, mirroring the contract's statuses and shapes. */
function fakeFetch(pool: FakePool): typeof fetch {
  return async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const url =
      typeof input === 'string'
        ? new URL(input)
        : input instanceof URL
          ? input
          : new URL(input.url);
    const method = (init?.method ?? 'GET').toUpperCase();
    const path = url.pathname;
    pool.calls.push(`${method} ${path}`);

    if (new Headers(init?.headers).get('authorization') !== `Bearer ${TOKEN}`) {
      return problem(401, 'unauthorized', 'invalid or missing token');
    }

    if (method === 'GET' && path === '/v1/projects') {
      return json({
        projects: [
          {
            id: 'project-1',
            slug: 'nightly',
            name: 'Nightly',
            created_at: '2026-10-28T20:00:00.000Z',
            updated_at: '2026-10-28T20:00:00.000Z',
          },
        ],
      });
    }

    if (method === 'GET' && path === '/v1/hosts') {
      return json({ hosts: pool.hosts });
    }

    const capability = /^\/v1\/hosts\/([^/]+)\/capabilities$/.exec(path);
    if (method === 'GET' && capability) {
      if (pool.offline) {
        return problem(503, 'daemon_not_connected', 'the host is offline');
      }
      // This contract revision has no directory parameter on the capability
      // route. The editor still reloads on a directory change; the fake pool
      // answers differently on the second call so the test can see the reload.
      const step = pool.capabilityDirectories.length;
      pool.capabilityDirectories.push(url.searchParams.get('directory') ?? '');
      return json(report(step === 0 ? '/srv/narrow' : '/srv/wide'));
    }

    if (method === 'POST' && path === '/v1/triggers/validate') {
      const document = bodyOf(init) as unknown as TriggerDocument;
      pool.validateCalls.push(document);
      if (pool.refuseConnection) {
        return json({
          name: document.name,
          valid: false,
          errors: [
            {
              field: 'filters.connection',
              message: "no active connection with slug 'github-main'",
            },
          ],
        });
      }
      return json({ name: document.name, valid: true, errors: [] });
    }

    if (method === 'POST' && path === '/v1/triggers/install') {
      const request = bodyOf(init) as unknown as TriggerInstallRequest;
      pool.installCalls.push(request);
      return json(
        {
          id: 'trigger-1',
          name: request.name,
          revision: 'revision-1',
          version: 1,
          active: true,
          replaced: false,
        },
        201,
      );
    }

    return problem(404, 'not_found', `no route for ${method} ${path}`);
  };
}

/** Build the page controller over the real SDK and the fake pool. */
function controllerFor(pool: FakePool): TriggersController {
  const client = createClient({
    baseUrl: BASE_URL,
    token: TOKEN,
    fetch: fakeFetch(pool),
  });
  return new TriggersController({
    transport: triggersTransport(client),
    initial: {
      ...emptyTriggersState(),
      document: DOCUMENT,
      yaml: triggerYaml(DOCUMENT),
    },
  });
}

describe('a connected host', () => {
  it('loads the host report, reloads it for a new directory, and installs', async () => {
    const pool = fakePool();
    const controller = controllerFor(pool);

    const loaded = await controller.load();
    expect(loaded.phase).toBe('ready');
    expect(loaded.selectedProjectId).toBe('project-1');
    expect(loaded.choices.status).toBe('ready');
    if (loaded.choices.status === 'ready') {
      expect(loaded.choices.options.providers).toEqual(['anthropic']);
      expect(loaded.choices.options.modes['anthropic\u0000claude']).toEqual([
        'default',
        'plan',
      ]);
    }
    expect(pool.calls).toContain('GET /v1/projects');
    expect(pool.calls).toContain('GET /v1/hosts');
    expect(pool.calls).toContain('GET /v1/hosts/host-1/capabilities');

    const changed = await controller.setDirectory('/srv/wide');
    expect(pool.capabilityDirectories).toHaveLength(2);
    expect(changed.choices.status).toBe('ready');
    if (changed.choices.status === 'ready') {
      expect(changed.choices.options.providers).toEqual(['anthropic', 'openai']);
    }

    const other = await controller.setHost('build-box');
    expect(other.choices.status).toBe('ready');
    expect(pool.calls).toContain('GET /v1/hosts/host-2/capabilities');

    const saved = await controller.save();
    expect(saved.savePhase).toBe('saved');
    expect(saved.saved).toMatchObject({ id: 'trigger-1', version: 1, active: true });
    expect(pool.validateCalls).toHaveLength(1);
    expect(pool.installCalls).toHaveLength(1);
    expect(pool.installCalls[0]).toMatchObject({
      project: 'project-1',
      name: 'auto-fix-issues',
      run: { host: 'build-box' },
    });
    // Validation always runs before install.
    const validateIndex = pool.calls.indexOf('POST /v1/triggers/validate');
    const installIndex = pool.calls.indexOf('POST /v1/triggers/install');
    expect(validateIndex).toBeGreaterThanOrEqual(0);
    expect(installIndex).toBeGreaterThan(validateIndex);
  });

  it('installs nothing when the pool refuses a field', async () => {
    const pool = fakePool();
    pool.refuseConnection = true;
    const controller = controllerFor(pool);
    await controller.load();

    const next = await controller.save();

    expect(next.savePhase).toBe('refused');
    expect(next.issues).toEqual([
      { field: 'filters.connection', message: "no active connection with slug 'github-main'" },
    ]);
    expect(pool.validateCalls).toHaveLength(1);
    expect(pool.installCalls).toHaveLength(0);
  });

  it('refuses an agent value the host does not report before any request', async () => {
    const pool = fakePool();
    const controller = controllerFor(pool);
    await controller.load();
    controller.apply({ field: 'run.agent.model', value: 'ghost-model' });

    const next = await controller.save();

    expect(next.savePhase).toBe('refused');
    expect(next.issues.map((issue) => issue.field)).toContain('run.agent.model');
    expect(pool.validateCalls).toHaveLength(0);
    expect(pool.installCalls).toHaveLength(0);
  });
});

describe('an offline host', () => {
  it('explains itself, keeps the rest editable, and recovers on retry', async () => {
    const pool = fakePool();
    pool.offline = true;
    const controller = controllerFor(pool);

    const loaded = await controller.load();
    expect(loaded.phase).toBe('ready');
    expect(loaded.choices.status).toBe('offline');
    if (loaded.choices.status === 'offline') {
      expect(loaded.choices.message).toContain("The host 'swaygent' is offline");
      expect(loaded.choices.message).toContain('retry');
    }

    controller.apply({ field: 'name', value: 'edited-while-offline' });
    expect(controller.snapshot().document.name).toBe('edited-while-offline');

    pool.offline = false;
    const recovered = await controller.retryChoices();
    expect(recovered.choices.status).toBe('ready');
    if (recovered.choices.status === 'ready') {
      expect(recovered.choices.host).toBe('swaygent');
      expect(recovered.choices.options.providers).toEqual(['anthropic']);
    }
  });

  it('does not ask a host the pool already lists as offline', async () => {
    const pool = fakePool();
    pool.hosts = [host({ status: 'offline' })];
    const controller = controllerFor(pool);

    const loaded = await controller.load();

    expect(loaded.choices.status).toBe('offline');
    expect(pool.calls).not.toContain('GET /v1/hosts/host-1/capabilities');
    if (loaded.choices.status === 'offline') {
      expect(loaded.choices.message).toContain("The host 'swaygent' is offline");
    }
  });
});
