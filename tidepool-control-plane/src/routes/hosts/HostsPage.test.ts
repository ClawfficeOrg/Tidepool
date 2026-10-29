import { describe, expect, it } from 'vitest';
import App from '../../App.svelte';
import HostsPage from './HostsPage.svelte';
import HostCard from './HostCard.svelte';
import { renderComponent } from '$lib/testing/render';
import {
  ENROLLMENT_TOKEN_WARNING,
  GRANT_CONSEQUENCE,
  HOST_STATUSES,
  HOST_STATUS_DESCRIPTORS,
  LAST_SEEN_UNREPORTED,
  REVOKE_CONSEQUENCE,
  emptyHostsState,
  executionState,
  hostRow,
  type HostRow,
  type HostsViewState,
} from '$lib/hosts';
import type { HostsTransport } from '$lib/hosts';
import { presentProblem } from '$lib/api/problem';
import { ProblemError } from '@clawffice/tidepool-sdk';

const POOL = { baseUrl: 'https://pool.example.com' };
const NOW = new Date('2026-10-29T12:00:00.000Z');

/** A transport that must not be called by a server-rendered test. */
const UNUSED_TRANSPORT: HostsTransport = {
  list: () => Promise.reject(new Error('the page should render from its initial state')),
  create: () => Promise.reject(new Error('not called')),
  mintToken: () => Promise.reject(new Error('not called')),
  grantExecution: () => Promise.reject(new Error('not called')),
  revokeExecution: () => Promise.reject(new Error('not called')),
};

/** One host row, overridable per test. */
function row(overrides: Partial<HostRow> = {}): HostRow {
  return {
    ...hostRow(
      {
        id: 'host-1',
        slug: 'swaygent',
        display_name: 'Swaygent',
        hostname: 'swaygent.local',
        status: 'connected',
        execution_enabled: false,
        created_at: '2026-10-29T09:00:00.000Z',
        revoked_at: null,
      },
      NOW,
    ),
    ...overrides,
  };
}

/** A ready state showing the given rows. */
function state(rows: HostRow[], overrides: Partial<HostsViewState> = {}): HostsViewState {
  return { ...emptyHostsState(), phase: 'ready', hosts: rows, ...overrides };
}

/** Render the page from a supplied state, with a member role. */
function renderPage(
  rows: HostRow[],
  overrides: Partial<HostsViewState> = {},
  props: Record<string, unknown> = {},
) {
  return renderComponent(HostsPage, {
    pool: POOL,
    token: 'token-1',
    role: 'member',
    transport: UNUSED_TRANSPORT,
    initial: state(rows, overrides),
    ...props,
  });
}

describe('the hosts page: statuses', () => {
  it('renders all four statuses with an icon and a word', () => {
    for (const status of HOST_STATUSES) {
      const rendered = renderPage([row({ status: { ...HOST_STATUS_DESCRIPTORS[status] } })]);
      const descriptor = HOST_STATUS_DESCRIPTORS[status];
      expect(rendered.html).toContain(`data-host-status="${status}"`);
      expect(rendered.html).toContain(`d="${descriptor.icon}"`);
      expect(rendered.text).toContain(descriptor.word);
      expect(rendered.html).toContain(`title="${descriptor.description}"`);
    }
  });

  it('never makes a status colour alone: the icon is decorative, the word carries it', () => {
    const rendered = renderPage([row({ status: HOST_STATUS_DESCRIPTORS.offline })]);
    expect(rendered.html).toMatch(/<svg[^>]*aria-hidden="true"/);
    expect(rendered.html).toContain('role="presentation"');
    expect(rendered.text).toContain('Offline');
  });

  it('shows the slug, display name, hostname and last seen for a host', () => {
    const rendered = renderPage([row()]);
    expect(rendered.text).toContain('swaygent');
    expect(rendered.text).toContain('Swaygent');
    expect(rendered.text).toContain('swaygent.local');
    expect(rendered.text).toContain(LAST_SEEN_UNREPORTED);
  });
});

describe('the hosts page: connection is not execution', () => {
  it('labels a connected host without the grant as connected only and offers the grant', () => {
    const rendered = renderPage([row()]);
    expect(rendered.text).toContain('Connected only');
    expect(rendered.text).toContain('Grant execution');
    expect(rendered.text).not.toContain('Revoke execution');
    expect(rendered.text).toContain(GRANT_CONSEQUENCE);
    expect(rendered.html).toContain('data-action="grant-execution"');
  });

  it('shows a granted host as one that may run agents, with the revoke action', () => {
    const rendered = renderPage([row({ execution: executionState('connected', true) })]);
    expect(rendered.text).toContain('May run agents');
    expect(rendered.text).toContain('Revoke execution');
    expect(rendered.text).not.toContain('Grant execution');
    expect(rendered.text).toContain(REVOKE_CONSEQUENCE);
    expect(rendered.html).toContain('data-execution="enabled"');
  });

  it('counts connected-only hosts separately from connected hosts', () => {
    const rendered = renderPage([
      row({ id: 'a', slug: 'alpha' }),
      row({
        id: 'b',
        slug: 'beta',
        execution: executionState('connected', true),
      }),
    ]);
    expect(rendered.text).toContain('2 connected');
    expect(rendered.text).toContain('1 connected only');
  });

  it('offers no execution action for a revoked host, and says why', () => {
    const rendered = renderPage([
      row({
        status: HOST_STATUS_DESCRIPTORS.revoked,
        execution: executionState('revoked', false),
      }),
    ]);
    expect(rendered.text).toContain('Revoked');
    expect(rendered.text).not.toContain('Grant execution');
    expect(rendered.text).not.toContain('Revoke execution');
    expect(rendered.text).toContain('nothing to grant or revoke');
  });
});

describe('the hosts page: action consequences and confirmation', () => {
  it('prints the consequence beside the action before it is confirmed', () => {
    const rendered = renderPage([row()]);
    expect(rendered.text).toContain(GRANT_CONSEQUENCE);
    expect(rendered.html).toContain('id="tp-consequence-host-1-grant-execution"');
    expect(rendered.html).toContain('aria-describedby="tp-consequence-host-1-grant-execution"');
    // Not confirmed yet: no second-step button.
    expect(rendered.text).not.toContain('Grant execution to swaygent');
  });

  it('gives each host its own ids, so a list never duplicates one', () => {
    const rendered = renderPage([
      row({ id: 'host-a', slug: 'alpha' }),
      row({ id: 'host-b', slug: 'beta' }),
    ]);
    expect(rendered.html).toContain('id="tp-consequence-host-a-grant-execution"');
    expect(rendered.html).toContain('id="tp-consequence-host-b-grant-execution"');
  });

  it('restates the consequence and names the host at the confirm step', () => {
    const granted = renderPage([
      row({ execution: executionState('connected', true) }),
    ]);
    expect(granted.text).not.toContain('Revoke execution for swaygent');

    const confirming = renderComponent(HostCard, {
      row: row(),
      role: 'member',
      confirmingAction: 'grant-execution',
    });
    expect(confirming.text).toContain(
      'Confirm: the pool may start agents on swaygent from now on.',
    );
    expect(confirming.text).toContain('Grant execution to swaygent');
    expect(confirming.text).toContain('Cancel');
    expect(confirming.html).toContain('data-confirming="true"');
  });

  it('shows the revoke consequence at its own confirm step', () => {
    const confirming = renderComponent(HostCard, {
      row: row({ execution: executionState('connected', true) }),
      role: 'member',
      confirmingAction: 'revoke-execution',
    });
    expect(confirming.text).toContain(
      'Confirm: swaygent stays connected but can no longer run agents.',
    );
    expect(confirming.text).toContain('Revoke execution for swaygent');
  });

  it('hides the mutating controls from a viewer and says why', () => {
    const rendered = renderPage([row()], {}, { role: 'viewer' });
    expect(rendered.html).toContain('data-hosts-denial="enrol-host"');
    expect(rendered.text).toContain('viewer role is read-only');
    expect(rendered.text).not.toContain('Enrol and mint the token');
    expect(rendered.text).not.toContain('Grant execution to swaygent');
    expect(rendered.html).toMatch(/<button[^>]*disabled[^>]*>/);
    expect(rendered.html).toContain('data-action="grant-execution"');
  });
});

describe('the hosts page: enrolment', () => {
  it('shows the single-use token and the exact command after enrolling', () => {
    const enrollment = {
      host: row({ status: HOST_STATUS_DESCRIPTORS.pending_approval }),
      token: 'pub.secret-token',
      expiresAt: '2026-10-29T12:10:00.000Z',
      command:
        "export POOL_HTTP_URL='https://pool.example.com'\nENROLLMENT_TOKEN='pub.secret-token' cargo run -p tidepool-host-agent",
      poolUrl: POOL.baseUrl,
      warning: ENROLLMENT_TOKEN_WARNING,
    };
    const rendered = renderPage([enrollment.host], { enrollment });

    expect(rendered.html).toContain('data-enrol-result="true"');
    expect(rendered.html).toContain('data-enrollment-token');
    expect(rendered.text).toContain('pub.secret-token');
    expect(rendered.html).toContain('data-enrollment-command');
    expect(rendered.text).toContain("ENROLLMENT_TOKEN='pub.secret-token'");
    expect(rendered.text).toContain('cargo run -p tidepool-host-agent');
    expect(rendered.text).toContain('only time it can be shown');
    expect(rendered.text).toContain('Expires at');
    expect(rendered.text).toContain('Approval required');
    // The form is replaced by the result, so the token cannot be minted twice by accident.
    expect(rendered.text).not.toContain('Enrol and mint the token');
  });

  it('offers the enrolment form when there is no token to show', () => {
    const rendered = renderPage([]);
    expect(rendered.html).toContain('data-enrol-panel="true"');
    expect(rendered.text).toContain('Enrol and mint the token');
    expect(rendered.text).not.toContain('data-enrol-result');
  });
});

describe('the hosts page: load, empty and failure states', () => {
  it('says no hosts are enrolled yet', () => {
    const rendered = renderPage([]);
    expect(rendered.html).toContain('data-hosts-state="empty"');
    expect(rendered.text).toContain('No hosts are enrolled yet');
  });

  it('says it is loading before the first answer', () => {
    const rendered = renderPage([], { phase: 'loading' });
    expect(rendered.html).toContain('data-hosts-state="loading"');
    expect(rendered.text).toContain('Loading the hosts');
  });

  it('renders a pool failure with its request id', () => {
    const problem = presentProblem(
      new ProblemError('request failed', {
        status: 503,
        code: 'database_unavailable',
        title: 'Database unavailable',
        detail: 'The pool has no database connection.',
        requestId: 'req_hosts_1',
      }),
    );
    const rendered = renderPage([], { phase: 'error', problem });
    expect(rendered.text).toContain('Database unavailable');
    expect(rendered.text).toContain('Request id req_hosts_1');
  });

  it('says there is no pool rather than showing an empty list', () => {
    const rendered = renderComponent(HostsPage, {});
    expect(rendered.html).toContain('data-hosts-state="unconfigured"');
    expect(rendered.text).toContain('No pool is configured');
  });
});

describe('the hosts page in the app shell', () => {
  it('is wired to the signed-in session and renders its own heading', () => {
    const rendered = renderComponent(App, {
      pool: POOL,
      path: '/hosts',
      auth: {
        status: 'signed-in',
        session: {
          token: 'token-1',
          expiresAt: 9_000_000,
          user: {
            id: 'user-1',
            email: 'owner@example.com',
            display_name: 'Ada',
            role: 'owner',
          },
          organization: { id: 'org-1', name: 'Rock Pool', created_at: NOW.toISOString() },
          mustChangePassword: false,
        },
      },
    });
    expect(rendered.html).toContain('data-hosts-page="true"');
    expect(rendered.text).toContain('Machines enrolled with this pool');
    expect(rendered.text).toContain('Enrol a host');
    expect(rendered.html).toContain('data-enrol-panel="true"');
    expect(rendered.text).toContain('Loading the hosts enrolled with this pool');
  });
});
