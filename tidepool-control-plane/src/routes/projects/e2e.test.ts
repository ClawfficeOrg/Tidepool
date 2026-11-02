/**
 * End to end across the client boundary, without a server.
 *
 * The real TypeScript SDK talks to an in-memory pool through an injected
 * `fetch`, so this walks the task's story exactly as the browser does: install
 * a bundle, break it, see the pool refuse the save with the error pinned to the
 * authored line, fix it, and see the fixed bundle become the new active
 * revision. It also proves the editor sent every other authored file
 * byte-for-byte and in order, and that a refused save never moves the active
 * revision.
 */

import { describe, expect, it } from 'vitest';
import { createClient } from '@clawffice/tidepool-sdk';
import type {
  ConfigurationRequest,
  FieldIssue,
  ProjectView,
  RevisionMetaView,
  RevisionView,
} from '@clawffice/tidepool-sdk';
import {
  MANIFEST_FILE,
  ProjectsController,
  bundleFromRevision,
  projectsTransport,
  saveSummary,
  type ProjectsViewState,
} from '$lib/configuration';

const BASE_URL = 'https://pool.example.com';
const TOKEN = 'token-1';
const PROJECT_ID = 'project-1';

/** A revision as the fake pool stores it, with its authored files. */
interface StoredRevision {
  id: string;
  version: number;
  active: boolean;
  status: 'valid' | 'invalid';
  errors: FieldIssue[];
  files: { path: string; content: string }[];
  created_at: string;
}

/** The fake pool's state and the requests it saw. */
interface FakePool {
  project: ProjectView;
  revisions: StoredRevision[];
  hosts: Set<string>;
  connections: Set<string>;
  validateCalls: ConfigurationRequest[];
  installCalls: ConfigurationRequest[];
  /** When set, install stores the revision but refuses to activate it. */
  forceInactive: boolean;
  /** When set, validate answers 500 instead of a result. */
  failValidation: boolean;
  /** When set, the active-configuration read answers 404 despite the flag. */
  hideActive: boolean;
  responses: string[];
}

/** A valid authored manifest for the fixture project. */
const VALID_MANIFEST = `{
  "hosts": [
    "studio-mac"
  ],
  "partials": [
    {
      "name": "system",
      "path": "prompts/system.md"
    }
  ]
}
`;

/** The partial's exact bytes, including trailing spaces and the final newline. */
const PARTIAL_TEXT = 'You are the night-shift builder.  \n\nKeep the diff small.\n';

/** A pool seeded with one valid, active revision. */
function seededPool(): FakePool {
  return {
    project: {
      id: PROJECT_ID,
      slug: 'nightly',
      name: 'Nightly',
      created_at: '2026-10-28T20:00:00.000Z',
      updated_at: '2026-10-28T20:00:00.000Z',
    },
    revisions: [
      {
        id: 'revision-1',
        version: 1,
        active: true,
        status: 'valid',
        errors: [],
        files: [
          { path: MANIFEST_FILE, content: VALID_MANIFEST },
          { path: 'prompts/system.md', content: PARTIAL_TEXT },
        ],
        created_at: '2026-10-28T20:05:00.000Z',
      },
    ],
    hosts: new Set(['studio-mac', 'build-box']),
    connections: new Set(['acme']),
    validateCalls: [],
    installCalls: [],
    forceInactive: false,
    failValidation: false,
    hideActive: false,
    responses: [],
  };
}

/** The next revision version. */
function nextVersion(pool: FakePool): number {
  return Math.max(0, ...pool.revisions.map((revision) => revision.version)) + 1;
}

/** The authored files as a revision view. */
function view(revision: StoredRevision): RevisionView {
  return {
    id: revision.id,
    version: revision.version,
    active: revision.active,
    status: revision.status,
    errors: revision.errors,
    files: revision.files.map((file) => ({ path: file.path, content: file.content })),
    created_at: revision.created_at,
  };
}

/** Revision metadata for the history response. */
function meta(revision: StoredRevision): RevisionMetaView {
  return {
    id: revision.id,
    version: revision.version,
    active: revision.active,
    status: revision.status,
    error_count: revision.errors.length,
    created_at: revision.created_at,
  };
}

/** The manifest file in a request, if it has one. */
function manifestOf(files: readonly { path: string; content: string }[]): string | null {
  return files.find((file) => file.path === MANIFEST_FILE)?.content ?? null;
}

/**
 * The pool's reference resolution over the bundle.
 *
 * Mirrors the real service's shape: a slug that does not resolve produces a
 * `FieldIssue` naming the manifest path, which is what the editor maps back to
 * a line.
 */
function issuesFor(
  files: readonly { path: string; content: string }[],
  version: number,
  pool: FakePool,
): FieldIssue[] {
  const manifest = manifestOf(files);
  if (manifest === null) {
    return [
      { field: 'files', revision: version, message: "configuration bundle is missing 'tidepool.json'" },
    ];
  }
  let parsed: Record<string, unknown>;
  try {
    const value: unknown = JSON.parse(manifest);
    if (value === null || typeof value !== 'object' || Array.isArray(value)) {
      return [{ field: 'files', revision: version, message: 'manifest must be a JSON object' }];
    }
    parsed = value as Record<string, unknown>;
  } catch {
    return [{ field: 'files', revision: version, message: 'manifest is not valid JSON' }];
  }

  const paths = new Set(files.map((file) => file.path));
  const issues: FieldIssue[] = [];
  const strings = (value: unknown): string[] =>
    Array.isArray(value) ? value.filter((entry): entry is string => typeof entry === 'string') : [];
  const normalise = (value: string): string => value.trim().toLowerCase();
  const hosts = new Set([...pool.hosts].map(normalise));
  const connections = new Set([...pool.connections].map(normalise));

  for (const slug of strings(parsed['hosts'])) {
    if (!hosts.has(normalise(slug))) {
      issues.push({
        field: `hosts.${slug}`,
        revision: version,
        message: `unknown host slug '${slug}'`,
      });
    }
  }
  for (const slug of strings(parsed['connections'])) {
    if (!connections.has(normalise(slug))) {
      issues.push({
        field: `connections.${slug}`,
        revision: version,
        message: `no active connection with slug '${slug}'`,
      });
    }
  }
  const environments = Array.isArray(parsed['environments']) ? parsed['environments'] : [];
  for (const environment of environments) {
    if (environment === null || typeof environment !== 'object') continue;
    const record = environment as Record<string, unknown>;
    const name = typeof record['name'] === 'string' ? record['name'] : '';
    const host = typeof record['host'] === 'string' ? record['host'] : '';
    if (!hosts.has(normalise(host))) {
      issues.push({
        field: `environments.${name}.host`,
        revision: version,
        message: `unknown host slug '${host}'`,
      });
    }
  }
  const partials = Array.isArray(parsed['partials']) ? parsed['partials'] : [];
  for (const partial of partials) {
    if (partial === null || typeof partial !== 'object') continue;
    const record = partial as Record<string, unknown>;
    const name = typeof record['name'] === 'string' ? record['name'] : '';
    const path = typeof record['path'] === 'string' ? record['path'] : '';
    if (!paths.has(path)) {
      issues.push({
        field: `partials.${name}.path`,
        revision: version,
        message: `path '${path}' is not in the bundle`,
      });
    }
  }
  return issues;
}

/** A JSON response, recorded so the test can prove what the pool answered. */
function json(pool: FakePool, body: unknown, status = 200): Response {
  const raw = JSON.stringify(body);
  pool.responses.push(raw);
  return new Response(raw, {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

/** A problem-details response, in the pool's words. */
function problem(pool: FakePool, status: number, code: string, detail: string): Response {
  return json(
    pool,
    { status, code, detail, title: 'The pool rejected that request', requestId: 'req_e2e_projects' },
    status,
  );
}

/** The request body, parsed as an object. */
function bodyOf(init: RequestInit | undefined): Record<string, unknown> {
  const raw = init?.body;
  if (typeof raw !== 'string' || raw.trim() === '') return {};
  const parsed: unknown = JSON.parse(raw);
  return parsed !== null && typeof parsed === 'object' && !Array.isArray(parsed)
    ? (parsed as Record<string, unknown>)
    : {};
}

/** The typed request a validate or install call carried. */
function requestOf(init: RequestInit | undefined): ConfigurationRequest {
  const body = bodyOf(init);
  const files = Array.isArray(body['files']) ? body['files'] : [];
  return {
    files: files.map((entry) => {
      const record = (entry ?? {}) as Record<string, unknown>;
      return {
        path: typeof record['path'] === 'string' ? record['path'] : '',
        content: typeof record['content'] === 'string' ? record['content'] : '',
      };
    }),
  };
}

/** The fake pool's `fetch`, mirroring the project/configuration contract. */
function fakeFetch(pool: FakePool): typeof fetch {
  return async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const url =
      typeof input === 'string' ? new URL(input) : input instanceof URL ? input : new URL(input.url);
    const method = (init?.method ?? 'GET').toUpperCase();
    const path = url.pathname;

    const authorization =
      init?.headers instanceof Headers
        ? init.headers.get('authorization')
        : (init?.headers as Record<string, string> | undefined)?.['authorization'] ??
          (init?.headers as Record<string, string> | undefined)?.['Authorization'];
    if (authorization !== `Bearer ${TOKEN}`) {
      return problem(pool, 401, 'unauthorized', 'a bearer token is required');
    }

    if (method === 'GET' && path === '/v1/projects') {
      return json(pool, { projects: [pool.project] });
    }

    const revisionMatch = new RegExp(`^/v1/projects/${PROJECT_ID}/revisions/([^/]+)$`).exec(path);
    if (method === 'GET' && revisionMatch) {
      const id = revisionMatch[1];
      const revision = pool.revisions.find((candidate) => candidate.id === id);
      return revision
        ? json(pool, view(revision))
        : problem(pool, 404, 'not_found', 'revision not found');
    }

    if (method === 'GET' && path === `/v1/projects/${PROJECT_ID}/revisions`) {
      return json(pool, {
        revisions: [...pool.revisions]
          .sort((left, right) => right.version - left.version)
          .map(meta),
      });
    }

    if (method === 'GET' && path === `/v1/projects/${PROJECT_ID}/configuration`) {
      const active = pool.hideActive
        ? undefined
        : pool.revisions.find((revision) => revision.active);
      return active
        ? json(pool, view(active))
        : problem(pool, 404, 'not_found', 'no active configuration');
    }

    if (method === 'POST' && path === `/v1/projects/${PROJECT_ID}/configuration/validate`) {
      const request = requestOf(init);
      pool.validateCalls.push(request);
      if (pool.failValidation) {
        return problem(pool, 500, 'internal_error', 'the pool failed to validate that bundle');
      }
      const version = nextVersion(pool);
      const errors = issuesFor(request.files, version, pool);
      return json(pool, { valid: errors.length === 0, version, errors });
    }

    if (method === 'POST' && path === `/v1/projects/${PROJECT_ID}/configuration/install`) {
      const request = requestOf(init);
      pool.installCalls.push(request);
      const version = nextVersion(pool);
      const errors = issuesFor(request.files, version, pool);
      const valid = errors.length === 0;
      const active = valid && !pool.forceInactive;
      if (active) {
        for (const revision of pool.revisions) revision.active = false;
      }
      const stored: StoredRevision = {
        id: `revision-${pool.revisions.length + 1}`,
        version,
        active,
        status: valid ? 'valid' : 'invalid',
        errors,
        files: request.files.map((file) => ({ path: file.path, content: file.content })),
        created_at: `2026-10-29T09:0${version}:00.000Z`,
      };
      pool.revisions.push(stored);
      return json(pool, { revision: view(stored), activated: active }, 201);
    }

    return problem(pool, 404, 'not_found', `no route for ${method} ${path}`);
  };
}

/** A controller over the real SDK and the fake pool. */
function controllerFor(pool: FakePool): ProjectsController {
  const client = createClient({ baseUrl: BASE_URL, token: TOKEN, fetch: fakeFetch(pool) });
  return new ProjectsController({ transport: projectsTransport(client) });
}

/** The active revision's id, for terse assertions. */
function activeId(state: ProjectsViewState): string | null {
  return state.activeRevisionId;
}

describe('installing, breaking, refusing and fixing a bundle', () => {
  it('walks the whole flow without moving the active revision on refusal', async () => {
    const pool = seededPool();
    pool.revisions = [];
    const controller = controllerFor(pool);

    const loaded = await controller.load();
    expect(loaded.projects.map((project) => project.id)).toEqual([PROJECT_ID]);
    expect(loaded.activeRevision).toBeNull();
    expect(loaded.draft.files).toEqual([]);

    // Install the first bundle: the manifest, then its partial, in that order.
    controller.addFile(MANIFEST_FILE, VALID_MANIFEST);
    controller.addFile('prompts/system.md', PARTIAL_TEXT);
    const installed = await controller.save();
    expect(installed.savePhase).toBe('saved');
    expect(activeId(installed)).toBe('revision-1');
    expect(installed.activeRevision?.version).toBe(1);
    expect(installed.draft.files.map((file) => file.path)).toEqual([
      MANIFEST_FILE,
      'prompts/system.md',
    ]);
    expect(pool.installCalls[0]?.files[1]?.content).toBe(PARTIAL_TEXT);

    const goodManifest = installed.draft.files[0]?.content ?? '';
    const brokenManifest = goodManifest.replace('"studio-mac"', '"ghost-host"');
    controller.editFile(MANIFEST_FILE, brokenManifest);
    const refused = await controller.save();

    expect(refused.savePhase).toBe('refused');
    expect(refused.issues).toHaveLength(1);
    expect(refused.issues[0]).toMatchObject({
      field: 'hosts.ghost-host',
      file: MANIFEST_FILE,
      line: 3,
      located: true,
      message: "unknown host slug 'ghost-host'",
    });
    expect(refused.issues[0]?.snippet).toBe('"ghost-host"');
    expect(refused.activeRevisionId).toBe('revision-1');
    expect(refused.activeRevision?.version).toBe(1);
    expect(saveSummary(refused)).toContain('Revision v1 remains active');
    expect(pool.validateCalls).toHaveLength(2);
    expect(pool.installCalls).toHaveLength(1);
    expect(pool.revisions).toHaveLength(1);
    // The author's broken draft is still on screen, not replaced by the store.
    expect(refused.draft.files[0]?.content).toContain('"ghost-host"');

    const fixedManifest = brokenManifest.replace('"ghost-host"', '"studio-mac",\n    "build-box"');
    controller.editFile(MANIFEST_FILE, fixedManifest);
    const saved = await controller.save();

    expect(saved.savePhase).toBe('saved');
    expect(saved.issues).toEqual([]);
    expect(activeId(saved)).toBe('revision-2');
    expect(saved.activeRevision?.version).toBe(2);
    expect(saved.history.map((row) => [row.version, row.active])).toEqual([
      [2, true],
      [1, false],
    ]);

    // The second install carried the authored files byte-for-byte, in order.
    const payload = pool.installCalls[1];
    expect(payload?.files.map((file) => file.path)).toEqual([
      MANIFEST_FILE,
      'prompts/system.md',
    ]);
    expect(payload?.files[0]?.content).toBe(fixedManifest);
    expect(payload?.files[1]?.content).toBe(PARTIAL_TEXT);
    expect(pool.revisions[1]?.files[1]?.content).toBe(PARTIAL_TEXT);
  });

  it('renders the authored diff between the two revisions', async () => {
    const pool = seededPool();
    pool.revisions = [];
    const controller = controllerFor(pool);
    await controller.load();
    controller.addFile(MANIFEST_FILE, VALID_MANIFEST);
    controller.addFile('prompts/system.md', PARTIAL_TEXT);
    await controller.save();
    const changed = VALID_MANIFEST.replace(
      '    "studio-mac"',
      '    "studio-mac",\n    "build-box"',
    );
    controller.editFile(MANIFEST_FILE, changed);
    await controller.save();

    const compared = await controller.compare('revision-2', 'revision-1');
    expect(compared.comparison).not.toBeNull();
    expect(compared.comparison?.from).toEqual({ id: 'revision-1', version: 1 });
    expect(compared.comparison?.to).toEqual({ id: 'revision-2', version: 2 });
    const file = compared.comparison?.files.find((entry) => entry.path === MANIFEST_FILE);
    expect(file?.status).toBe('changed');
    expect(file?.lines.filter((line) => line.kind === 'added').map((line) => line.text)).toContain(
      '    "build-box"',
    );
    expect(
      file?.lines.filter((line) => line.kind === 'removed').map((line) => line.text),
    ).toContain('    "studio-mac"');
    expect(
      compared.comparison?.files.find((entry) => entry.path === 'prompts/system.md')?.status,
    ).toBe('unchanged');
  });
});

describe('failures leave the active revision where it was', () => {
  it('refuses to treat a stored-but-inactive revision as a save', async () => {
    const pool = seededPool();
    pool.forceInactive = true;
    const controller = controllerFor(pool);
    await controller.load();
    controller.editFile(
      MANIFEST_FILE,
      (await controller.snapshot()).draft.files[0]?.content ?? '',
    );
    const next = await controller.save();

    expect(next.savePhase).toBe('refused');
    expect(next.activeRevisionId).toBe('revision-1');
    expect(next.activeRevision?.version).toBe(1);
    expect(next.saveProblem?.message).toContain('did not activate it');
    expect(next.history.map((row) => [row.version, row.active])).toEqual([
      [2, false],
      [1, true],
    ]);
  });

  it('reports a call failure without moving the active revision', async () => {
    const pool = seededPool();
    pool.failValidation = true;
    const controller = controllerFor(pool);
    await controller.load();
    const next = await controller.save();

    expect(next.savePhase).toBe('failed');
    expect(next.activeRevisionId).toBe('revision-1');
    expect(next.saveProblem?.status).toBe(500);
    expect(pool.installCalls).toHaveLength(0);
  });

  it('reports a history-active revision the pool does not serve', async () => {
    const pool = seededPool();
    pool.hideActive = true;
    const controller = controllerFor(pool);
    const loaded = await controller.load();

    expect(loaded.activeRevision).toBeNull();
    expect(loaded.activeProblem).toContain('serves no active configuration');
    const next = await controller.save();
    expect(next.savePhase).toBe('refused');
    expect(pool.validateCalls).toHaveLength(0);
  });

  it('refuses to save while the pool reports two active revisions', async () => {
    const pool = seededPool();
    const first = pool.revisions[0];
    if (!first) throw new Error('fixture has no revision');
    pool.revisions.push({
      ...first,
      id: 'revision-2',
      version: 2,
      active: true,
      files: first.files.map((file) => ({ ...file })),
    });
    const controller = controllerFor(pool);
    const loaded = await controller.load();

    expect(loaded.activeProblem).toContain('marks 2 revisions active');
    const next = await controller.save();
    expect(next.savePhase).toBe('refused');
    expect(pool.validateCalls).toHaveLength(0);
  });
});

describe('a project with no active revision', () => {
  it('treats the missing active configuration as a state, not a failure', async () => {
    const pool = seededPool();
    pool.revisions = [];
    const controller = controllerFor(pool);
    const loaded = await controller.load();

    expect(loaded.problem).toBeNull();
    expect(loaded.activeRevision).toBeNull();
    expect(loaded.activeRevisionId).toBeNull();
    expect(loaded.activeProblem).toBeNull();
    expect(loaded.draft.files).toEqual([]);
    expect(loaded.history).toEqual([]);
  });
});

describe('the bundle the editor keeps is the authored bundle', () => {
  it('round-trips a stored revision without normalising it', () => {
    const pool = seededPool();
    const stored = pool.revisions[0];
    if (!stored) throw new Error('fixture has no revision');
    const bundle = bundleFromRevision(view(stored));
    expect(bundle.files).toEqual(stored.files);
    expect(bundle.files.map((file) => file.content)).toEqual([
      stored.files[0]?.content,
      stored.files[1]?.content,
    ]);
  });
});
