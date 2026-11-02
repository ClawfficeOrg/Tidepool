/**
 * The connections view, rendered on the server.
 *
 * These are the component-level promises: the exact callback URL is on the
 * card, a credential is shown only as set plus a replace control, a failed
 * verification names its check, and an origin move prints both sides of every
 * callback URL that must change.
 */

import { describe, expect, it } from 'vitest';
import ConnectionsPage from './ConnectionsPage.svelte';
import ConnectionCard from './ConnectionCard.svelte';
import ConnectionForm from './ConnectionForm.svelte';
import SecretField from './SecretField.svelte';
import OriginChangeNotice from './OriginChangeNotice.svelte';
import VerificationReport from './VerificationReport.svelte';
import { renderComponent } from '$lib/testing/render';
import {
  callbackChanges,
  callbackChangeSummary,
  connectionRow,
  emptyConnectionsState,
  verifyConnection,
  type ConnectionRow,
  type ConnectionsTransport,
  type ConnectionsViewState,
} from '$lib/connections';
import type { ConnectionView } from '@clawffice/tidepool-sdk';

const POOL = 'https://pool.example.com';
const OLD_POOL = 'http://localhost:8080';

/** A transport that must not be called by a server-rendered test. */
const UNUSED_TRANSPORT: ConnectionsTransport = {
  list: () => Promise.reject(new Error('the page should render from its initial state')),
  create: () => Promise.reject(new Error('not called')),
  get: () => Promise.reject(new Error('not called')),
  update: () => Promise.reject(new Error('not called')),
  remove: () => Promise.reject(new Error('not called')),
};

/** A contract view with provider-appropriate resources. */
function view(overrides: Partial<ConnectionView> = {}): ConnectionView {
  return {
    id: 'connection-1',
    provider: 'slack',
    slug: 'acme',
    display_name: 'Acme',
    status: 'active',
    managed_by: 'database',
    credential_configured: true,
    resources: [{ resource_kind: 'workspace', external_id: 'T012345', display_name: 'Acme' }],
    created_at: '2026-10-29T09:00:00.000Z',
    ...overrides,
  };
}

/** One row for a transport, with provider-appropriate resources. */
function row(
  transport: 'github_webhook' | 'slack_webhook' | 'slack_socket' | 'discord_gateway',
  overrides: Partial<ConnectionView> = {},
): ConnectionRow {
  const provider =
    transport === 'github_webhook'
      ? 'github'
      : transport === 'discord_gateway'
        ? 'discord'
        : 'slack';
  const resources =
    provider === 'github'
      ? [{ resource_kind: 'repository', external_id: 'acme/app', display_name: 'acme/app' }]
      : provider === 'discord'
        ? [{ resource_kind: 'guild', external_id: '900000000000000001', display_name: 'Acme' }]
        : [{ resource_kind: 'workspace', external_id: 'T012345', display_name: 'Acme' }];
  return connectionRow(view({ provider, resources, ...overrides }), {
    poolUrl: POOL,
    transport,
  });
}

/** A ready state showing the given rows. */
function state(rows: ConnectionRow[], overrides: Partial<ConnectionsViewState> = {}): ConnectionsViewState {
  return { ...emptyConnectionsState(), phase: 'ready', connections: rows, ...overrides };
}

/** Render the page from a supplied state, with a member role. */
function renderPage(
  rows: ConnectionRow[],
  overrides: Partial<ConnectionsViewState> = {},
  props: Record<string, unknown> = {},
) {
  return renderComponent(ConnectionsPage, {
    pool: { baseUrl: POOL },
    token: 'token-1',
    role: 'member',
    transport: UNUSED_TRANSPORT,
    initial: state(rows, overrides),
    ...props,
  });
}

describe('the credential field is write-only', () => {
  it('shows a set fact and a replace control, never a value', () => {
    const rendered = renderComponent(SecretField, { configured: true });
    expect(rendered.html).toContain('data-secret-state="set"');
    expect(rendered.text).toContain('Credential set');
    expect(rendered.text).toContain('Replace credential');
    expect(rendered.text).not.toContain('Show credential');
    expect(rendered.html).not.toContain('data-secret-action="show"');
    expect(rendered.html).not.toContain('value="xoxb');
  });

  it('offers to set one when none exists, not to reveal one', () => {
    const rendered = renderComponent(SecretField, { configured: false });
    expect(rendered.html).toContain('data-secret-state="unset"');
    expect(rendered.text).toContain('No credential set');
    expect(rendered.text).toContain('Set credential');
  });

  it('opens the replacement form empty, with no pre-filled value', () => {
    const rendered = renderComponent(SecretField, { configured: true, initialEditing: true });
    expect(rendered.text).toContain('New credential reference');
    expect(rendered.html).toContain('type="password"');
    expect(rendered.html).toContain('autocomplete="off"');
    expect(rendered.text).toContain('write-only');
    // The only input is empty: no stored value is in the markup.
    expect(rendered.html).not.toMatch(/value="[^"]+"/);
  });
});

describe('the connection card', () => {
  it('prints the exact GitHub callback URL for the current origin', () => {
    const rendered = renderComponent(ConnectionCard, {
      row: row('github_webhook'),
      role: 'member',
    });
    expect(rendered.html).toContain('data-callback-kind="webhook"');
    expect(rendered.html).toContain('data-callback-url="https://pool.example.com/v1/webhooks/github/acme"');
    expect(rendered.text).toContain('https://pool.example.com/v1/webhooks/github/acme');
    expect(rendered.text).toContain('Point the GitHub webhook at this exact URL');
  });

  it('prints the exact Slack callback URL, and no URL for socket mode', () => {
    const webhook = renderComponent(ConnectionCard, {
      row: row('slack_webhook', { id: 'slack-webhook', slug: 'team' }),
      role: 'member',
    });
    expect(webhook.text).toContain('https://pool.example.com/v1/webhooks/slack/team');

    const socket = renderComponent(ConnectionCard, {
      row: row('slack_socket', { id: 'slack-socket', slug: 'team' }),
      role: 'member',
    });
    expect(socket.html).toContain('data-callback-kind="outbound"');
    expect(socket.html).not.toContain('data-callback-url');
    expect(socket.text).toContain('socket mode dials Slack');
    expect(socket.text).toContain('no public address');
  });

  it('explains the Discord bot gateway instead of a callback URL', () => {
    const rendered = renderComponent(ConnectionCard, {
      row: row('discord_gateway'),
      role: 'member',
    });
    expect(rendered.html).toContain('data-connection-transport="discord_gateway"');
    expect(rendered.html).not.toContain('data-callback-url');
    expect(rendered.text).toContain('bot gateway');
    expect(rendered.text).toContain('bot_token');
  });

  it('shows set + replace for a configured credential and no value', () => {
    const rendered = renderComponent(ConnectionCard, {
      row: row('github_webhook'),
      role: 'member',
    });
    expect(rendered.html).toContain('data-credential-state="set"');
    expect(rendered.text).toContain('Credential set');
    expect(rendered.text).toContain('Replace credential');
    expect(rendered.html).not.toMatch(/value="[^"]+"/);
  });

  it('names the failing check and quotes the provider when verification fails', () => {
    const built = row('slack_socket', { credential_configured: false, id: 'slack-socket' });
    const report = verifyConnection(built, { publicOrigin: POOL });
    const rendered = renderComponent(ConnectionCard, {
      row: built,
      role: 'member',
      report,
    });
    expect(rendered.html).toContain('data-verification="failed"');
    expect(rendered.html).toContain('data-check="credential_configured"');
    expect(rendered.html).toContain('data-check-ok="false"');
    expect(rendered.text).toContain('The credential is configured');
    expect(rendered.text).toContain('bot_token, signing_secret, app_token');
    expect(rendered.text).toContain('Verification failed');
  });

  it('names the connection in the delete confirmation', () => {
    const rendered = renderComponent(ConnectionCard, {
      row: row('github_webhook'),
      role: 'member',
      initialConfirmDelete: true,
    });
    expect(rendered.text).toContain('Delete acme');
    expect(rendered.text).toContain('cannot be undone');
    expect(rendered.text).toContain('stops GitHub events');
  });

  it('marks an environment-managed connection read-only', () => {
    const rendered = renderComponent(ConnectionCard, {
      row: row('github_webhook', { managed_by: 'environment' }),
      role: 'owner',
    });
    expect(rendered.html).toContain('data-managed-by="environment"');
    expect(rendered.text).toContain('TIDEPOOL_CONNECTIONS');
    expect(rendered.text).toContain('read-only');
    expect(rendered.html).toMatch(/data-connection-action="delete"[^>]*disabled/);
  });

  it('offers Slack a remembered transport choice and says it is local', () => {
    const rendered = renderComponent(ConnectionCard, {
      row: row('slack_webhook'),
      role: 'member',
    });
    expect(rendered.html).toContain('id="tp-connection-connection-1-transport"');
    expect(rendered.text).toContain('Remembered in this browser');
    expect(rendered.text).toContain('Socket mode');
  });
});

describe('the create form', () => {
  it('offers Slack socket mode for a pool with no public address', () => {
    const rendered = renderComponent(ConnectionForm, {
      poolUrl: POOL,
      initialProvider: 'slack',
      initialTransport: 'slack_socket',
    });
    expect(rendered.html).toContain('data-socket-mode="true"');
    expect(rendered.text).toContain('socket mode');
    expect(rendered.text).toContain('no callback URL');
    expect(rendered.html).toContain('data-credential-write-only="true"');
  });

  it('configures the Discord bot gateway', () => {
    const rendered = renderComponent(ConnectionForm, {
      poolUrl: POOL,
      initialProvider: 'discord',
      initialTransport: 'discord_gateway',
    });
    expect(rendered.html).toContain('data-gateway="true"');
    expect(rendered.text).toContain('Discord configures the bot gateway');
    expect(rendered.text).toContain('bot_token');
  });

  it('previews the GitHub callback URL from the pool origin and slug', () => {
    const rendered = renderComponent(ConnectionForm, {
      poolUrl: POOL,
      initialProvider: 'github',
      initialTransport: 'github_webhook',
    });
    expect(rendered.text).toContain('Callback URL:');
    expect(rendered.html).toContain('data-callback-preview="https://pool.example.com/v1/webhooks/github/{slug}"');
  });
});

describe('the connections page', () => {
  it('renders every connection with its callback and credential state', () => {
    const rendered = renderPage([
      row('github_webhook', { id: 'gh', slug: 'acme' }),
      row('slack_socket', { id: 'sock', slug: 'team' }),
      row('discord_gateway', { id: 'dc', slug: 'guild' }),
    ]);
    expect(rendered.html).toContain('data-connections-page="true"');
    expect(rendered.text).toContain('3 connections');
    expect(rendered.text).toContain('https://pool.example.com/v1/webhooks/github/acme');
    expect(rendered.text).toContain('Credential set');
    expect(rendered.text).toContain('Replace credential');
    expect(rendered.text).toContain('bot gateway');
    expect(rendered.html).toContain('data-credential-field="tp-connection-gh-credential"');
    expect(rendered.html).toContain('data-credential-field="tp-connection-sock-credential"');
    expect(rendered.html).toContain('data-credential-field="tp-connection-dc-credential"');
  });

  it('shows a failed verification report under the connection that failed', () => {
    const built = row('slack_socket', { credential_configured: false });
    const report = verifyConnection(built, { publicOrigin: POOL });
    const rendered = renderPage([built], { verification: report });
    expect(rendered.html).toContain('data-verification="failed"');
    expect(rendered.text).toContain('The credential is configured');
    expect(rendered.text).toContain('Verification failed');
  });

  it('lists both sides of every callback URL that must change with the origin', () => {
    const rows = [
      row('github_webhook', { id: 'gh', slug: 'acme' }),
      row('slack_webhook', { id: 'sl', slug: 'team' }),
      row('slack_socket', { id: 'sock', slug: 'team-socket' }),
    ];
    const rendered = renderPage(rows, {}, { previousPoolUrl: OLD_POOL });
    expect(rendered.html).toContain('data-origin-change="present"');
    expect(rendered.text).toContain('http://localhost:8080/v1/webhooks/github/acme');
    expect(rendered.text).toContain('https://pool.example.com/v1/webhooks/github/acme');
    expect(rendered.text).toContain('http://localhost:8080/v1/webhooks/slack/team');
    expect(rendered.text).toContain('https://pool.example.com/v1/webhooks/slack/team');
    expect(rendered.html).not.toContain('data-origin-change-for="sock"');
    expect(rendered.text).toContain('The pool’s public origin changed');
  });

  it('says nothing needs changing when every connection dials out', () => {
    const rendered = renderPage([row('slack_socket')], {}, { previousPoolUrl: OLD_POOL });
    expect(rendered.html).toContain('data-origin-change="none"');
    expect(rendered.text).toContain('nothing needs changing');
  });

  it('shows the create form to a member and a denial to a viewer', () => {
    const member = renderPage([]);
    expect(member.text).toContain('Add a connection');

    const viewer = renderPage([], {}, { role: 'viewer' });
    expect(viewer.html).toContain('data-connections-denial="create-connection"');
    expect(viewer.html).not.toContain('data-connection-form="true"');
  });

  it('explains the unconfigured and empty states', () => {
    const unconfigured = renderComponent(ConnectionsPage, {
      pool: { baseUrl: null },
      role: 'member',
    });
    expect(unconfigured.text).toContain('No pool is configured');

    const empty = renderPage([]);
    expect(empty.html).toContain('data-connections-state="empty"');
    expect(empty.text).toContain('No connections are configured yet');
  });

  it('renders the origin notice with the callback changes it was given', () => {
    const rows = [row('github_webhook')];
    const rendered = renderComponent(OriginChangeNotice, {
      previousUrl: OLD_POOL,
      currentUrl: POOL,
      changes: callbackChanges(OLD_POOL, POOL, rows),
      summary: callbackChangeSummary(OLD_POOL, POOL),
    });
    expect(rendered.html).toContain('data-origin-change="present"');
    expect(rendered.html).toContain('data-origin-previous-url="http://localhost:8080/v1/webhooks/github/acme"');
    expect(rendered.html).toContain('data-origin-current-url="https://pool.example.com/v1/webhooks/github/acme"');
  });

  it('renders a passing report with every check and no failure', () => {
    const built = row('github_webhook');
    const report = verifyConnection(built, { publicOrigin: POOL });
    const rendered = renderComponent(VerificationReport, { report });
    expect(rendered.html).toContain('data-verification="passed"');
    expect(rendered.text).toContain('Verification passed');
    expect(rendered.html).toContain('data-check-ok="true"');
    expect(rendered.html).not.toContain('data-check-ok="false"');
  });
});
