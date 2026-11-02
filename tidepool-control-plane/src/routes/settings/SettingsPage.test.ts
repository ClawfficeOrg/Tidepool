/**
 * The settings page: members and API keys.
 *
 * Server-rendered component tests, no DOM and no network. They assert what a
 * person sees: the last owner's disabled controls *and* the sentence that
 * explains them, an invitation's expiry, the one-time secret with copying as
 * the next action, and a revoked key that stays in the list marked revoked.
 */

import { describe, expect, it } from 'vitest';
import App from '../../App.svelte';
import ApiKeyCard from './ApiKeyCard.svelte';
import MemberRow from './MemberRow.svelte';
import SettingsPage from './SettingsPage.svelte';
import { renderComponent } from '$lib/testing/render';
import {
  API_KEY_STATUS_DESCRIPTORS,
  API_KEY_SECRET_WARNING,
  INVITATION_TOKEN_WARNING,
  LAST_OWNER_REASON,
  REMOVE_MEMBER_CONSEQUENCE,
  REVOKE_KEY_CONSEQUENCE,
  apiKeyRow,
  createdApiKey,
  createdInvitation,
  emptyMembersKeysState,
  invitationRow,
  memberRow,
  type ApiKeyRow,
  type MemberRow as MemberRowModel,
  type MembersKeysState,
  type MembersKeysTransport,
} from '$lib/keys';

const POOL = { baseUrl: 'https://pool.example.com' };
const NOW = new Date('2026-10-29T12:00:00.000Z');

/** A transport that must not be called by a server-rendered test. */
const UNUSED_TRANSPORT: MembersKeysTransport = {
  listMembers: () => Promise.reject(new Error('the page should render from its initial state')),
  inviteMember: () => Promise.reject(new Error('not called')),
  changeMemberRole: () => Promise.reject(new Error('not called')),
  removeMember: () => Promise.reject(new Error('not called')),
  listInvitations: () => Promise.reject(new Error('not called')),
  revokeInvitation: () => Promise.reject(new Error('not called')),
  listKeys: () => Promise.reject(new Error('not called')),
  createKey: () => Promise.reject(new Error('not called')),
  revokeKey: () => Promise.reject(new Error('not called')),
};

/** One member row, overridable per test. */
function member(overrides: Partial<MemberRowModel> = {}): MemberRowModel {
  return {
    ...memberRow(
      {
        id: 'user-1',
        email: 'ada@example.com',
        display_name: 'Ada',
        role: 'owner',
        password_change_required: false,
        last_login_at: '2026-10-29T09:00:00.000Z',
        created_at: '2026-10-01T09:00:00.000Z',
      },
      NOW,
    ),
    ...overrides,
  };
}

/** One key row, overridable per test. */
function key(overrides: Partial<ApiKeyRow> = {}): ApiKeyRow {
  return {
    ...apiKeyRow(
      {
        id: 'key-1',
        name: 'nightly',
        prefix: 'tp_live_abc',
        scopes: ['projects:read', 'runs:dispatch'],
        status: 'active',
        created_at: '2026-10-28T09:00:00.000Z',
        last_used_at: null,
        revoked_at: null,
      },
      NOW,
    ),
    ...overrides,
  };
}

/** A ready state with the given content. */
function state(overrides: Partial<MembersKeysState> = {}): MembersKeysState {
  return { ...emptyMembersKeysState(), phase: 'ready', ...overrides };
}

/** Render the page from a supplied state. */
function renderPage(
  overrides: Partial<MembersKeysState> = {},
  props: Record<string, unknown> = {},
) {
  return renderComponent(SettingsPage, {
    pool: POOL,
    token: 'token-1',
    role: 'owner',
    transport: UNUSED_TRANSPORT,
    initial: state(overrides),
    ...props,
  });
}

describe('the last-owner rule', () => {
  it('cannot remove or demote the only owner, and shows the reason', () => {
    const rendered = renderPage({ members: [member()] });

    expect(rendered.text).toContain(LAST_OWNER_REASON);
    expect(rendered.html).toContain('data-last-owner-reason="true"');
    expect(rendered.html).toContain('data-last-owner-badge="true"');
    // The role select and the remove control are present but unavailable.
    expect(rendered.html).toMatch(/<select[^>]*disabled/);
    expect(rendered.html).toMatch(/<button[^>]*disabled[^>]*>Remove member/);
    // Nothing is in a confirm step, so no action can be completed.
    expect(rendered.text).not.toContain('Remove Ada');
  });

  it('allows demoting and removing an owner once another owner exists', () => {
    const second = member({ id: 'user-2', displayName: 'Grace', email: 'grace@example.com' });
    const rendered = renderPage({ members: [member(), second] });

    expect(rendered.text).not.toContain(LAST_OWNER_REASON);
    expect(rendered.html).not.toContain('data-last-owner-reason="true"');
    expect(rendered.html).not.toMatch(/<select[^>]*disabled/);
    expect(rendered.html).toContain(REMOVE_MEMBER_CONSEQUENCE);
  });

  it('restates the consequence and names the member at the remove confirm step', () => {
    const second = member({ id: 'user-2', displayName: 'Grace', email: 'grace@example.com' });
    const rendered = renderComponent(MemberRow, {
      member: second,
      members: [member(), second],
      role: 'owner',
      initialConfirming: true,
    });

    expect(rendered.text).toContain(
      'Confirm: Grace is removed from this pool and signed out everywhere.',
    );
    expect(rendered.text).toContain('Remove Grace');
    expect(rendered.text).toContain('Cancel');
    expect(rendered.html).toContain('data-confirming="true"');
  });

  it('hides mutating controls from a non-owner and says why', () => {
    const rendered = renderPage({ members: [member()] }, { role: 'member' });

    expect(rendered.html).toContain('data-members-denial="owner-only"');
    expect(rendered.text).toContain('owner-only');
    expect(rendered.text).toContain('ada@example.com');
    expect(rendered.html).not.toContain('data-invite-form="true"');
    expect(rendered.html).not.toContain('data-member-controls="true"');
    expect(rendered.html).toContain('data-keys-denial="owner-only"');
    expect(rendered.html).not.toContain('data-create-key-form="true"');
  });
});

describe('invitations', () => {
  const pending = invitationRow(
    {
      id: 'inv-1',
      email: 'new@example.com',
      role: 'member',
      created_at: NOW.toISOString(),
      expires_at: '2026-11-05T12:00:00.000Z',
    },
    NOW,
  );

  it('shows a pending invitation with its expiry sentence and exact instant', () => {
    const rendered = renderPage({ invitations: [pending] });

    expect(rendered.html).toContain('data-invitation-id="inv-1"');
    expect(rendered.html).toContain('data-invitation-expiry');
    expect(rendered.text).toContain('Expires in 7 days');
    expect(rendered.html).toContain('<time datetime="2026-11-05T12:00:00.000Z">');
  });

  it('shows the one-time acceptance token and says it cannot be shown again', () => {
    const created = createdInvitation(
      {
        invitation: {
          id: 'inv-1',
          email: 'new@example.com',
          role: 'member',
          created_at: NOW.toISOString(),
          expires_at: '2026-11-05T12:00:00.000Z',
        },
        token: 'invite-token-1',
      },
      NOW,
    );
    const rendered = renderPage({ invitations: [pending], createdInvitation: created });

    expect(rendered.html).toContain('data-invitation-result="true"');
    expect(rendered.html).toContain('data-invitation-token');
    expect(rendered.text).toContain('invite-token-1');
    expect(rendered.text).toContain(INVITATION_TOKEN_WARNING);
    expect(rendered.text).toContain('Expires in 7 days');
    expect(rendered.text).toContain('Copy acceptance token');
  });

  it('offers the invite form to an owner', () => {
    const rendered = renderPage();
    expect(rendered.html).toContain('data-invite-form="true"');
    expect(rendered.text).toContain('Send the invitation');
    expect(rendered.html).toContain('data-invitations-state="empty"');
  });
});

describe('API keys: the secret is shown once', () => {
  const created = createdApiKey(
    {
      api_key: {
        id: 'key-1',
        name: 'nightly',
        prefix: 'tp_live_abc',
        scopes: ['projects:read', 'runs:dispatch'],
        status: 'active',
        created_at: NOW.toISOString(),
      },
      secret: 'tp_live_abc.the-secret',
    },
    NOW,
  );

  it('shows the secret in a copyable block with copying as the obvious next action', () => {
    const rendered = renderPage({ createdKey: created, keys: [created.key] });

    expect(rendered.html).toContain('data-key-secret-panel="true"');
    expect(rendered.html).toContain('data-key-secret');
    expect(rendered.text).toContain('tp_live_abc.the-secret');
    expect(rendered.text).toContain(API_KEY_SECRET_WARNING);
    expect(rendered.text).toContain('will not be shown again');
    expect(rendered.html).toContain('data-copy-secret');

    // The copy button is primary and comes before the dismiss control.
    expect(rendered.html).toMatch(/class="tp-button"[^>]*data-copy-secret/);
    const copyAt = rendered.html.indexOf('data-copy-secret');
    const dismissAt = rendered.html.indexOf('I have copied the secret');
    expect(copyAt).toBeGreaterThan(-1);
    expect(dismissAt).toBeGreaterThan(copyAt);

    // The create form is replaced, so the secret cannot be minted twice by accident.
    expect(rendered.html).not.toContain('data-create-key-form="true"');
  });

  it('does not keep the secret once the panel is dismissed', () => {
    // The same state minus `createdKey` is what dismissal produces.
    const rendered = renderPage({ keys: [created.key] });

    expect(rendered.html).not.toContain('data-key-secret');
    expect(rendered.text).not.toContain('tp_live_abc.the-secret');
    expect(rendered.html).toContain('data-create-key-form="true"');
    // The key remains, with its prefix, so it is still manageable.
    expect(rendered.text).toContain('tp_live_abc');
  });

  it('shows the prefix, scopes, creation time and last used for an active key', () => {
    const rendered = renderPage({ keys: [key()] });

    expect(rendered.html).toContain('data-key-prefix');
    expect(rendered.text).toContain('tp_live_abc');
    expect(rendered.text).toContain('Read projects');
    expect(rendered.text).toContain('Dispatch runs');
    expect(rendered.text).toContain('Never used');
    expect(rendered.html).toContain('<time datetime="2026-10-28T09:00:00.000Z">');
    expect(rendered.text).toContain(REVOKE_KEY_CONSEQUENCE);
  });
});

describe('API keys: revocation is marked, not hidden', () => {
  const revoked = key({
    id: 'key-2',
    name: 'old-nightly',
    status: API_KEY_STATUS_DESCRIPTORS.revoked,
    revokedAt: '2026-10-29T11:00:00.000Z',
  });

  it('keeps a revoked key in the list, marked revoked with its revocation time', () => {
    const rendered = renderPage({ keys: [key(), revoked] });

    expect(rendered.html).toContain('data-api-key-id="key-1"');
    expect(rendered.html).toContain('data-api-key-id="key-2"');
    expect(rendered.html).toContain('data-key-status="revoked"');
    expect(rendered.text).toContain('Revoked');
    expect(rendered.html).toContain('data-key-revoked-note="true"');
    expect(rendered.html).toContain('data-key-revoked-at');
    expect(rendered.html).toContain('<time datetime="2026-10-29T11:00:00.000Z">');
  });

  it('offers no revoke action on a key that is already revoked', () => {
    const rendered = renderComponent(ApiKeyCard, { row: revoked, initialConfirming: false });
    expect(rendered.text).not.toContain('Revoke key');
    expect(rendered.text).toContain('stays here for the audit trail');
  });

  it('restates the consequence at the revoke confirm step', () => {
    const rendered = renderComponent(ApiKeyCard, {
      row: key(),
      initialConfirming: true,
    });
    expect(rendered.text).toContain(
      'Confirm: nightly stops authenticating immediately. This cannot be undone.',
    );
    expect(rendered.text).toContain('Revoke key nightly');
  });
});

describe('the settings page states', () => {
  it('says there is no pool rather than showing empty panels', () => {
    const rendered = renderComponent(SettingsPage, {});
    expect(rendered.html).toContain('data-settings-state="unconfigured"');
    expect(rendered.text).toContain('No pool is configured');
  });

  it('asks for a session when a pool is configured without one', () => {
    const rendered = renderComponent(SettingsPage, { pool: POOL });
    expect(rendered.html).toContain('data-settings-state="signed-out"');
    expect(rendered.text).toContain('Sign in to manage');
  });

  it('renders a successful change as a status notice', () => {
    const rendered = renderPage({ notice: 'Invitation created for new@example.com.' });
    expect(rendered.html).toContain('data-settings-notice="true"');
    expect(rendered.html).toContain('role="status"');
    expect(rendered.text).toContain('Invitation created');
  });
});

describe('the settings page in the app shell', () => {
  it('is wired to the signed-in session and renders both panels', () => {
    const rendered = renderComponent(App, {
      pool: POOL,
      path: '/settings',
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
    expect(rendered.html).toContain('data-settings-page="true"');
    expect(rendered.text).toContain('The pool URL, the palette');
    expect(rendered.text).toContain('Members');
    expect(rendered.text).toContain('API keys');
    expect(rendered.html).toContain('data-invite-form="true"');
    expect(rendered.html).toContain('data-create-key-form="true"');
  });
});
