/**
 * The projects page and the configuration editor, rendered on the server.
 *
 * These are the component-level promises: a pool field error appears beside
 * the authored line it names, the active revision is named once and without
 * guessing, a diff renders the authored files line by line, and the bundle
 * model never rewrites a file the author did not touch.
 */

import { describe, expect, it } from 'vitest';
import ProjectsPage from './ProjectsPage.svelte';
import IssueList from './IssueList.svelte';
import RevisionDiff from './RevisionDiff.svelte';
import RevisionHistory from './RevisionHistory.svelte';
import { renderComponent } from '$lib/testing/render';
import {
  MANIFEST_FILE,
  addBundleFile,
  bundleFromFiles,
  diffBundles,
  editBundleFile,
  emptyProjectsState,
  locateField,
  locateIssues,
  removeBundleFile,
  requestFromBundle,
  resolveActiveRevision,
  revisionRows,
  draftFiles,
  type Bundle,
  type ProjectsTransport,
  type ProjectsViewState,
  type RevisionComparison,
} from '$lib/configuration';
import type {
  FieldIssue,
  ProjectView,
  RevisionMetaView,
  RevisionView,
} from '@clawffice/tidepool-sdk';

const POOL = 'https://pool.example.com';

const MANIFEST = `{
  "hosts": [
    "studio-mac",
    "ghost-host"
  ],
  "environments": [
    {
      "name": "nightly",
      "host": "ghost-host",
      "directory": "/srv/tidepool"
    }
  ],
  "partials": [
    {
      "name": "system",
      "path": "prompts/system.md"
    }
  ],
  "agents": [
    {
      "name": "builder",
      "provider": "anthropic",
      "model": "claude"
    }
  ]
}
`;

const PARTIAL = 'You are the night-shift builder.\n\nKeep the diff small.\n';

/** The authored bundle for the fixture project. */
function bundle(manifest = MANIFEST): Bundle {
  return bundleFromFiles([
    { path: MANIFEST_FILE, content: manifest },
    { path: 'prompts/system.md', content: PARTIAL },
  ]);
}

const PROJECT: ProjectView = {
  id: 'project-1',
  slug: 'nightly',
  name: 'Nightly',
  created_at: '2026-10-28T20:00:00.000Z',
  updated_at: '2026-10-28T20:00:00.000Z',
};

/** A stored revision over the fixture bundle. */
function revision(overrides: Partial<RevisionView> = {}): RevisionView {
  return {
    id: 'revision-1',
    version: 1,
    active: true,
    status: 'valid',
    errors: [],
    files: bundle().files,
    created_at: '2026-10-28T20:05:00.000Z',
    ...overrides,
  };
}

/** Revision metadata for the history. */
function meta(overrides: Partial<RevisionMetaView> = {}): RevisionMetaView {
  return {
    id: 'revision-1',
    version: 1,
    active: true,
    status: 'valid',
    error_count: 0,
    created_at: '2026-10-28T20:05:00.000Z',
    ...overrides,
  };
}

/** A transport that must not be called by a server-rendered test. */
const UNUSED_TRANSPORT: ProjectsTransport = {
  list: () => Promise.reject(new Error('the page renders from its initial state')),
  activeRevision: () => Promise.reject(new Error('not called')),
  revisions: () => Promise.reject(new Error('not called')),
  revision: () => Promise.reject(new Error('not called')),
  validate: () => Promise.reject(new Error('not called')),
  install: () => Promise.reject(new Error('not called')),
};

/** A ready state for the fixture project. */
function state(overrides: Partial<ProjectsViewState> = {}): ProjectsViewState {
  const active = revision();
  return {
    ...emptyProjectsState(),
    phase: 'ready',
    projects: [PROJECT],
    selectedProjectId: PROJECT.id,
    draft: bundle(),
    activeRevision: active,
    activeRevisionId: active.id,
    history: revisionRows([
      meta(),
      meta({ id: 'revision-0', version: 0, active: false, created_at: '2026-10-27T20:00:00.000Z' }),
    ]),
    ...overrides,
  };
}

/** Render the page from a supplied state, with a member role. */
function renderPage(overrides: Partial<ProjectsViewState> = {}, props: Record<string, unknown> = {}) {
  return renderComponent(ProjectsPage, {
    pool: { baseUrl: POOL },
    token: 'token-1',
    role: 'member',
    transport: UNUSED_TRANSPORT,
    initial: state(overrides),
    ...props,
  });
}

/** The named field errors the pool would return for the broken fixture. */
const BROKEN_ISSUES: FieldIssue[] = [
  {
    field: 'hosts.ghost-host',
    revision: 2,
    message: "unknown host slug 'ghost-host'",
  },
  {
    field: 'environments.nightly.host',
    revision: 2,
    message: "unknown host slug 'ghost-host'",
  },
  {
    field: 'environments.nightly.worktree.newBranch',
    revision: 2,
    message: "newBranch is required for mode 'branch-off'",
  },
];

describe('field errors find the authored line', () => {
  it('locates a string-array item by its value', () => {
    const located = locateField(MANIFEST, 'hosts.ghost-host');
    expect(located.located).toBe(true);
    expect(located.line).toBe(4);
    expect(located.snippet).toBe('"ghost-host"');
  });

  it('locates a named array item and its property', () => {
    expect(locateField(MANIFEST, 'environments.nightly.host')).toMatchObject({
      located: true,
      line: 9,
      snippet: '"host": "ghost-host",',
    });
    expect(locateField(MANIFEST, 'agents.builder.model').line).toBe(23);
    expect(locateField(MANIFEST, 'partials.system.path').line).toBe(16);
  });

  it('falls back to the nearest authored object for a missing property', () => {
    const located = locateField(MANIFEST, 'environments.nightly.worktree.newBranch');
    expect(located.located).toBe(true);
    expect(located.line).toBe(7);
  });

  it('marks an unknown field unlocated instead of guessing a line', () => {
    const located = locateField(MANIFEST, 'something.else');
    expect(located.located).toBe(false);
    expect(located.line).toBe(1);
  });

  it('marks every field unlocated when the manifest is malformed', () => {
    const located = locateField('{"hosts": [', 'hosts.ghost-host');
    expect(located.located).toBe(false);
    expect(located.line).toBe(1);
  });

  it('resolves a name that itself contains a dot', () => {
    const manifest = `{
  "environments": [
    {
      "name": "release.nightly",
      "host": "studio-mac",
      "directory": "/srv/tidepool"
    }
  ]
}
`;
    const located = locateField(manifest, 'environments.release.nightly.directory');
    expect(located.located).toBe(true);
    expect(located.line).toBe(6);
  });

  it('locates indexed workflow fields to their step line', () => {
    const manifest = `{
  "workflows": [
    {
      "name": "build",
      "trigger": "manual",
      "steps": [
        {
          "id": "first",
          "agent": "ghost-agent"
        }
      ]
    }
  ]
}
`;
    const located = locateField(manifest, 'workflows.build.steps[0].agent');
    expect(located.located).toBe(true);
    expect(located.line).toBe(9);
    expect(located.snippet).toBe('"agent": "ghost-agent"');
  });

  it('returns issues in the pool order, each with file and line', () => {
    const located = locateIssues(BROKEN_ISSUES, bundle());
    expect(located.map((issue) => issue.line)).toEqual([4, 9, 7]);
    expect(located.every((issue) => issue.file === MANIFEST_FILE)).toBe(true);
    expect(located[0]?.message).toBe("unknown host slug 'ghost-host'");
  });
});

describe('the issue list shows the line beside the message', () => {
  it('renders the file, line, field and the pool’s own words', () => {
    const issues = locateIssues(BROKEN_ISSUES, bundle());
    const rendered = renderComponent(IssueList, { issues });
    expect(rendered.html).toContain('data-issue-line="4"');
    expect(rendered.html).toContain('data-issue-field="hosts.ghost-host"');
    expect(rendered.text).toContain('tidepool.json:4');
    expect(rendered.text).toContain("unknown host slug 'ghost-host'");
    expect(rendered.text).toContain('3 field errors');
  });

  it('says so when a field could not be located', () => {
    const issues = locateIssues([{ field: 'nope', revision: 1, message: 'bad' }], bundle());
    const rendered = renderComponent(IssueList, { issues });
    expect(rendered.html).toContain('data-issue-located="false"');
    expect(rendered.text).toContain('line not located');
  });
});

describe('the editor keeps authored files exact', () => {
  it('edits only the named file and leaves order and bytes untouched', () => {
    const before = bundle();
    const after = editBundleFile(before, MANIFEST_FILE, '{\n  "hosts": []\n}\n');
    expect(after.files.map((file) => file.path)).toEqual(before.files.map((file) => file.path));
    expect(after.files[1]).toBe(before.files[1]);
    expect(after.files[1]?.content).toBe(PARTIAL);
  });

  it('sends untouched files byte-for-byte, including odd whitespace', () => {
    const odd = 'first line  \r\nsecond line\t\r\n\r\nno final newline';
    const before = bundleFromFiles([
      { path: MANIFEST_FILE, content: MANIFEST },
      { path: 'prompts/odd.md', content: odd },
    ]);
    const request = requestFromBundle(editBundleFile(before, MANIFEST_FILE, '{\n}\n'));
    expect(request.files.map((file) => file.path)).toEqual([
      MANIFEST_FILE,
      'prompts/odd.md',
    ]);
    expect(request.files[1]?.content).toBe(odd);
  });

  it('adds at the end and removes only the named path', () => {
    const added = addBundleFile(bundle(), 'prompts/extra.md', 'extra\n');
    expect(added.files.at(-1)?.path).toBe('prompts/extra.md');
    expect(added.files.slice(0, 2)).toEqual(bundle().files);
    const removed = removeBundleFile(added, 'prompts/system.md');
    expect(removed.files.map((file) => file.path)).toEqual([MANIFEST_FILE, 'prompts/extra.md']);
    expect(removed.files[0]?.content).toBe(MANIFEST);
  });

  it('marks only edited files dirty against the active revision', () => {
    const active = revision();
    const draft = editBundleFile(bundle(), 'prompts/system.md', 'changed\n');
    expect(draftFiles(draft, active).map((file) => file.dirty)).toEqual([false, true]);
  });
});

describe('a refused save stays on the active revision', () => {
  const refused = () =>
    state({
      savePhase: 'refused',
      refusedVersion: 2,
      issues: locateIssues(BROKEN_ISSUES, bundle()),
      draft: editBundleFile(bundle(), MANIFEST_FILE, MANIFEST.replace('"studio-mac"', '"ghost-host"')),
    });

  it('shows the pool’s field errors at their lines', () => {
    const rendered = renderPage(refused());
    expect(rendered.text).toContain('3 field errors');
    expect(rendered.text).toContain('tidepool.json:4');
    expect(rendered.text).toContain("unknown host slug 'ghost-host'");
  });

  it('names v1 as still active and says the bundle was refused', () => {
    const rendered = renderPage(refused());
    expect(rendered.text).toContain('Active revision v1');
    expect(rendered.text).toContain('Revision v1 remains active.');
    expect(rendered.html).toContain('data-save-phase="refused"');
  });

  it('keeps the author’s broken draft in the editor', () => {
    const rendered = renderPage(refused());
    expect(rendered.html).toContain('data-editor-path="tidepool.json"');
    expect(rendered.text).toContain('ghost-host');
  });
});

describe('the active revision is unambiguous', () => {
  it('marks exactly one history row active', () => {
    const rendered = renderPage();
    const markers = rendered.html.match(/data-active-marker="true"/g) ?? [];
    expect(markers).toHaveLength(1);
    expect(rendered.text).toContain('Active revision v1');
  });

  it('reports multiple active revisions instead of choosing one', () => {
    const resolution = resolveActiveRevision([
      meta(),
      meta({ id: 'revision-2', version: 2, active: true }),
    ]);
    expect(resolution.active).toBeNull();
    expect(resolution.problem).toContain('marks 2 revisions active');
  });

  it('reports an empty active set when a history exists', () => {
    const resolution = resolveActiveRevision([meta({ active: false })]);
    expect(resolution.active).toBeNull();
    expect(resolution.problem).toContain('No revision is marked active');
  });

  it('disables saving when the served active and the history disagree', () => {
    const ambiguous = state({
      activeProblem:
        'The pool serves revision v2 as active, but its history marks v1 active. The editor will not guess which is in force.',
    });
    const rendered = renderPage(ambiguous);
    expect(rendered.html).toContain('data-active-problem="true"');
    expect(rendered.text).toContain('The active revision is ambiguous');
    expect(rendered.html).toMatch(/<button[^>]*disabled=""[^>]*data-save-action="save"/);
  });
});

describe('the diff renders the authored files', () => {
  const updatedManifest = MANIFEST.replace(
    '    "studio-mac",\n    "ghost-host"',
    '    "studio-mac",\n    "build-box"',
  );

  const comparison: RevisionComparison = {
    from: { id: 'revision-1', version: 1 },
    to: { id: 'revision-2', version: 2 },
    identical: false,
    files: diffBundles(
      bundle(),
      bundleFromFiles([
        { path: MANIFEST_FILE, content: updatedManifest },
        { path: 'prompts/system.md', content: PARTIAL },
        { path: 'prompts/extra.md', content: 'extra instructions\n' },
      ]),
    ),
  };

  it('names both revisions and each authored file that changed', () => {
    const rendered = renderComponent(RevisionDiff, { comparison });
    expect(rendered.text).toContain('Changes from v1 to v2');
    expect(rendered.html).toContain('data-diff-file="tidepool.json"');
    expect(rendered.html).toContain('data-diff-file="prompts/extra.md"');
    expect(rendered.html).not.toContain('data-diff-file="prompts/system.md"');
  });

  it('shows the removed and added authored lines', () => {
    const rendered = renderComponent(RevisionDiff, { comparison });
    expect(rendered.html).toContain('data-diff-line="removed"');
    expect(rendered.html).toContain('data-diff-line="added"');
    expect(rendered.text).toContain('"ghost-host"');
    expect(rendered.text).toContain('"build-box"');
    expect(rendered.text).toContain('extra instructions');
  });

  it('reports the two revisions as identical when they are the same revision', () => {
    const same: RevisionComparison = {
      from: { id: 'revision-1', version: 1 },
      to: { id: 'revision-1', version: 1 },
      identical: true,
      files: diffBundles(bundle(), bundle()),
    };
    const rendered = renderComponent(RevisionDiff, { comparison: same });
    expect(rendered.text).toContain('This is the active revision');
    expect(rendered.html).toContain('data-diff-empty="true"');
  });
});

describe('the history says which revision is in force', () => {
  it('lists newest first and marks the active row', () => {
    const rendered = renderComponent(RevisionHistory, {
      rows: revisionRows([
        meta({ id: 'revision-1', version: 1, active: false }),
        meta({ id: 'revision-2', version: 2, active: true }),
      ]),
      activeRevisionId: 'revision-2',
    });
    const versions = [...rendered.html.matchAll(/data-revision-version="(\d+)"/g)].map(
      (match) => match[1],
    );
    expect(versions).toEqual(['2', '1']);
    expect(rendered.html).toContain('data-revision-active="true"');
    expect(rendered.text).toContain('Active');
  });
});

describe('the page shows the editor over real files', () => {
  it('renders the authored manifest text in the textarea', () => {
    const rendered = renderPage();
    expect(rendered.html).toContain('data-file-content-path="tidepool.json"');
    expect(rendered.text).toContain('"studio-mac"');
    expect(rendered.html).toContain('data-file-path="prompts/system.md"');
  });

  it('says what to do when a project has no files yet', () => {
    const rendered = renderPage({
      draft: { files: [] },
      activeRevision: null,
      activeRevisionId: null,
      history: [],
      activeProblem: null,
    });
    expect(rendered.html).toContain('data-editor-empty="true"');
    expect(rendered.text).toContain('No revision is active yet');
  });
});
