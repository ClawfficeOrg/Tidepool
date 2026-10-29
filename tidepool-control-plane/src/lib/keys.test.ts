/**
 * The members and API keys model.
 *
 * These tests pin the two rules the task is judged on — the last owner cannot
 * be removed or demoted, and a key's secret is held exactly once — plus the
 * expiry, scope and revoked-key behaviour the page renders. No DOM, no network,
 * no pool: the controller takes its transport by injection.
 */

import { describe, expect, it } from 'vitest';
import { SCOPES } from '@clawffice/tidepool-sdk';
import { ProblemError } from '@clawffice/tidepool-sdk';
import {
  API_KEY_SCOPE_NAMES,
  API_KEY_SECRET_WARNING,
  API_KEY_STATUS_DESCRIPTORS,
  LAST_OWNER_REASON,
  MembersKeysController,
  REVOKE_KEY_CONSEQUENCE,
  REMOVE_MEMBER_CONSEQUENCE,
  apiKeyRow,
  copySecret,
  createdApiKey,
  createdInvitation,
  emptyMembersKeysState,
  expiryView,
  invitationRow,
  isApiKeyScope,
  keysSummary,
  memberRow,
  membersKeysCounts,
  membersSummary,
  memberLoginView,
  keyUsageView,
  normaliseScopes,
  ownerCount,
  removalRule,
  roleChangeRule,
  sortApiKeyRows,
  sortMemberRows,
  validateInviteInput,
  validateKeyInput,
  type ApiKeyRow,
  type MemberRow,
  type MembersKeysTransport,
} from '$lib/keys';

const NOW = new Date('2026-10-29T12:00:00.000Z');

/** One member row, overridable per test. */
function member(overrides: Partial<MemberRow> = {}): MemberRow {
  return {
    ...memberRow(
      {
        id: 'user-1',
        email: 'ada@example.com',
        display_name: 'Ada',
        role: 'owner',
        password_change_required: false,
        last_login_at: null,
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

/** A transport whose every call fails unless a test overrides it. */
function transport(overrides: Partial<MembersKeysTransport> = {}): MembersKeysTransport {
  const unexpected = (name: string) => () =>
    Promise.reject(new Error(`the model should not call ${name} here`));
  return {
    listMembers: async () => ({ members: [] }),
    listInvitations: async () => ({ invitations: [] }),
    listKeys: async () => ({ api_keys: [] }),
    inviteMember: unexpected('inviteMember'),
    changeMemberRole: unexpected('changeMemberRole'),
    removeMember: unexpected('removeMember'),
    revokeInvitation: unexpected('revokeInvitation'),
    createKey: unexpected('createKey'),
    revokeKey: unexpected('revokeKey'),
    ...overrides,
  };
}

describe('API key scopes', () => {
  it('is the contract’s scope set, in the contract’s order', () => {
    expect([...API_KEY_SCOPE_NAMES]).toEqual([...SCOPES]);
  });

  it('knows every contract scope and rejects anything else', () => {
    for (const scope of SCOPES) expect(isApiKeyScope(scope)).toBe(true);
    expect(isApiKeyScope('projects:write')).toBe(false);
    expect(isApiKeyScope(null)).toBe(false);
  });

  it('normalises duplicates away and restores the contract order', () => {
    expect(normaliseScopes(['runs:dispatch', 'projects:read', 'runs:dispatch'])).toEqual([
      'projects:read',
      'runs:dispatch',
    ]);
    expect(normaliseScopes(['not:a:scope'])).toEqual([]);
  });
});

describe('form validation', () => {
  it('requires a real email address and a known role', () => {
    expect(validateInviteInput({ email: '', role: 'member' })).toMatchObject({
      ok: false,
      field: 'email',
    });
    expect(validateInviteInput({ email: 'ada.example.com', role: 'member' })).toMatchObject({
      ok: false,
      field: 'email',
    });
    expect(validateInviteInput({ email: 'ada@example.com', role: 'superuser' })).toMatchObject({
      ok: false,
      field: 'role',
    });
    expect(validateInviteInput({ email: '  ada@example.com ', role: 'admin' })).toEqual({
      ok: true,
      input: { email: 'ada@example.com', role: 'admin' },
    });
  });

  it('requires a name and at least one known scope', () => {
    expect(validateKeyInput({ name: '  ', scopes: ['projects:read'] })).toMatchObject({
      ok: false,
      field: 'name',
    });
    expect(validateKeyInput({ name: 'x'.repeat(81), scopes: ['projects:read'] })).toMatchObject({
      ok: false,
      field: 'name',
    });
    expect(validateKeyInput({ name: 'nightly', scopes: [] })).toMatchObject({
      ok: false,
      field: 'scopes',
    });
    expect(validateKeyInput({ name: 'nightly', scopes: ['every:thing'] })).toMatchObject({
      ok: false,
      field: 'scopes',
    });
    expect(validateKeyInput({ name: ' nightly ', scopes: ['runs:dispatch'] })).toEqual({
      ok: true,
      input: { name: 'nightly', scopes: ['runs:dispatch'] },
    });
  });
});

describe('the last-owner rule', () => {
  it('blocks removing the only owner and says why', () => {
    const owner = member();
    const rule = removalRule(owner, [owner]);
    expect(rule.isLastOwner).toBe(true);
    expect(rule.reason).toBe(LAST_OWNER_REASON);
    expect(rule.reason).toContain('last owner');
  });

  it('blocks demoting the only owner, but never blocks promoting them', () => {
    const owner = member();
    const members = [owner];
    expect(roleChangeRule(owner, members, 'admin').reason).toBe(LAST_OWNER_REASON);
    expect(roleChangeRule(owner, members, 'viewer').reason).toBe(LAST_OWNER_REASON);
    expect(roleChangeRule(owner, members, 'owner').reason).toBeNull();
  });

  it('allows demoting or removing an owner once another owner exists', () => {
    const first = member();
    const second = member({ id: 'user-2', email: 'grace@example.com', displayName: 'Grace' });
    const members = [first, second];
    expect(ownerCount(members)).toBe(2);
    expect(removalRule(first, members)).toEqual({ isLastOwner: false, reason: null });
    expect(roleChangeRule(first, members, 'member')).toEqual({
      isLastOwner: false,
      reason: null,
    });
  });

  it('never blocks a non-owner from changing or being removed', () => {
    const owner = member();
    const viewer = member({ id: 'user-2', role: 'viewer', roleLabel: 'Viewer' });
    expect(removalRule(viewer, [owner, viewer]).reason).toBeNull();
    expect(roleChangeRule(viewer, [owner, viewer], 'admin').reason).toBeNull();
  });

  it('orders owners before the other roles', () => {
    const rows = [
      member({ id: 'v', role: 'viewer', roleLabel: 'Viewer', displayName: 'Zoe' }),
      member({ id: 'o', displayName: 'Ada' }),
      member({ id: 'a', role: 'admin', roleLabel: 'Admin', displayName: 'Bo' }),
    ];
    expect(sortMemberRows(rows).map((row) => row.id)).toEqual(['o', 'a', 'v']);
  });
});

describe('expiry and moments', () => {
  it('shows an invitation’s expiry as a sentence and an instant', () => {
    const view = expiryView('2026-11-05T12:00:00.000Z', NOW);
    expect(view.text).toBe('Expires in 7 days');
    expect(view.expired).toBe(false);
    expect(view.at).toBe('2026-11-05T12:00:00.000Z');
  });

  it('marks an invitation that has already lapsed', () => {
    const view = expiryView('2026-10-29T10:00:00.000Z', NOW);
    expect(view.text).toBe('Expired 2 hours ago');
    expect(view.expired).toBe(true);
  });

  it('says when no expiry was reported rather than inventing one', () => {
    expect(expiryView(null, NOW)).toEqual({
      text: 'Expiry not reported',
      expired: false,
      at: null,
    });
  });

  it('reads a key that has never been used, and one that has', () => {
    expect(keyUsageView(null, NOW).text).toBe('Never used');
    expect(keyUsageView('2026-10-29T10:00:00.000Z', NOW).text).toBe('Last used 2 hours ago');
    expect(memberLoginView(undefined, NOW).text).toBe('Never signed in');
  });

  it('keeps the invitation expiry on the row', () => {
    const row = invitationRow(
      {
        id: 'inv-1',
        email: 'new@example.com',
        role: 'member',
        created_at: NOW.toISOString(),
        expires_at: '2026-11-05T12:00:00.000Z',
      },
      NOW,
    );
    expect(row.expiry.text).toBe('Expires in 7 days');
    expect(row.expiresAt).toBe('2026-11-05T12:00:00.000Z');
  });
});

describe('API key rows', () => {
  it('carries the prefix, labelled scopes, creation time and never a secret', () => {
    const row = key();
    expect(row.prefix).toBe('tp_live_abc');
    expect(row.scopes.map((scope) => scope.label)).toEqual(['Read projects', 'Dispatch runs']);
    expect(row.createdAt).toBe('2026-10-28T09:00:00.000Z');
    expect(row.lastUsed.text).toBe('Never used');
    expect(row.status.status).toBe('active');
    expect(JSON.stringify(row)).not.toContain('secret');
  });

  it('keeps an unknown scope visible rather than hiding power', () => {
    const row = apiKeyRow(
      {
        id: 'key-2',
        name: 'future',
        prefix: 'tp_live_future',
        scopes: ['projects:read', 'future:scope'],
        status: 'active',
      },
      NOW,
    );
    expect(row.scopes.map((scope) => scope.scope)).toEqual(['projects:read', 'future:scope']);
    expect(row.scopes[1]?.known).toBe(false);
    expect(row.scopes[1]?.label).toBe('future:scope');
  });

  it('marks a revoked key as revoked and keeps it in the sorted list', () => {
    const active = key({ id: 'key-active', name: 'alpha' });
    const revoked = key({
      id: 'key-revoked',
      name: 'beta',
      status: API_KEY_STATUS_DESCRIPTORS.revoked,
      revokedAt: '2026-10-29T11:00:00.000Z',
    });
    const sorted = sortApiKeyRows([revoked, active]);
    expect(sorted.map((row) => row.id)).toEqual(['key-active', 'key-revoked']);
    expect(sorted[1]?.status.word).toBe('Revoked');
    expect(sorted[1]?.revokedAt).toBe('2026-10-29T11:00:00.000Z');
  });

  it('orders active keys newest first', () => {
    const older = key({ id: 'older', createdAt: '2026-10-01T00:00:00.000Z' });
    const newer = key({ id: 'newer', createdAt: '2026-10-20T00:00:00.000Z' });
    expect(sortApiKeyRows([older, newer]).map((row) => row.id)).toEqual(['newer', 'older']);
  });

  it('says the secret will not be shown again, and keeps it verbatim', () => {
    const created = createdApiKey(
      {
        api_key: {
          id: 'key-1',
          name: 'nightly',
          prefix: 'tp_live_abc',
          scopes: ['projects:read'],
          status: 'active',
          created_at: '2026-10-29T12:00:00.000Z',
        },
        secret: 'tp_live_abc.the-secret',
      },
      NOW,
    );
    expect(created.secret).toBe('tp_live_abc.the-secret');
    expect(created.warning).toBe(API_KEY_SECRET_WARNING);
    expect(created.warning).toContain('will not be shown again');
    expect(created.nextAction).toContain('Copy the secret');
  });

  it('warns that an invitation token is shown once and carries its expiry', () => {
    const created = createdInvitation(
      {
        invitation: {
          id: 'inv-1',
          email: 'new@example.com',
          role: 'member',
          created_at: NOW.toISOString(),
          expires_at: '2026-11-05T12:00:00.000Z',
        },
        token: 'invite-token',
      },
      NOW,
    );
    expect(created.token).toBe('invite-token');
    expect(created.warning).toContain('shown once');
    expect(created.invitation.expiry.text).toBe('Expires in 7 days');
  });
});

describe('copying a secret', () => {
  it('writes the exact secret and reports success', async () => {
    const written: string[] = [];
    const ok = await copySecret('tp_live_abc.the-secret', {
      writeText: async (text) => {
        written.push(text);
      },
    });
    expect(ok).toBe(true);
    expect(written).toEqual(['tp_live_abc.the-secret']);
  });

  it('reports a refused clipboard instead of throwing', async () => {
    await expect(
      copySecret('tp_live_abc.the-secret', {
        writeText: () => Promise.reject(new Error('denied')),
      }),
    ).resolves.toBe(false);
    await expect(copySecret('tp_live_abc.the-secret', null)).resolves.toBe(false);
  });
});

describe('summaries', () => {
  it('counts owners, invitations and keys', () => {
    const state = {
      ...emptyMembersKeysState(),
      members: [member(), member({ id: 'user-2', role: 'viewer', roleLabel: 'Viewer' })],
      keys: [key(), key({ id: 'key-2', status: { ...key().status } })],
    };
    const counts = membersKeysCounts(state);
    expect(counts).toMatchObject({ owners: 1, members: 2, activeKeys: 2, revokedKeys: 0 });
    expect(membersSummary(counts)).toContain('2 operators, 1 owner');
    expect(keysSummary(counts)).toBe('2 active keys.');
  });
});

describe('the members and keys controller', () => {
  it('loads members, invitations and keys', async () => {
    const controller = new MembersKeysController({
      now: () => NOW,
      transport: transport({
        listMembers: async () => ({
          members: [
            {
              id: 'user-1',
              email: 'ada@example.com',
              display_name: 'Ada',
              role: 'owner',
              password_change_required: false,
              created_at: '2026-10-01T00:00:00.000Z',
            },
          ],
        }),
        listInvitations: async () => ({
          invitations: [
            {
              id: 'inv-1',
              email: 'new@example.com',
              role: 'member',
              created_at: NOW.toISOString(),
              expires_at: '2026-11-05T12:00:00.000Z',
            },
          ],
        }),
        listKeys: async () => ({
          api_keys: [
            {
              id: 'key-1',
              name: 'nightly',
              prefix: 'tp_live_abc',
              scopes: ['projects:read'],
              status: 'active',
              created_at: '2026-10-28T00:00:00.000Z',
            },
          ],
        }),
      }),
    });
    const state = await controller.load();
    expect(state.phase).toBe('ready');
    expect(state.members).toHaveLength(1);
    expect(state.invitations[0]?.expiry.text).toBe('Expires in 7 days');
    expect(state.keys[0]?.prefix).toBe('tp_live_abc');
  });

  it('does not ask for owner-only data when the role is not an owner', async () => {
    const calls: string[] = [];
    const controller = new MembersKeysController({
      now: () => NOW,
      transport: transport({
        listMembers: async () => {
          calls.push('members');
          return {
            members: [
              {
                id: 'user-1',
                email: 'ada@example.com',
                display_name: 'Ada',
                role: 'owner',
                password_change_required: false,
                created_at: '2026-10-01T00:00:00.000Z',
              },
            ],
          };
        },
        listInvitations: async () => {
          calls.push('invitations');
          return { invitations: [] };
        },
        listKeys: async () => {
          calls.push('keys');
          return { api_keys: [] };
        },
      }),
    });
    const state = await controller.load({ includeOwnerData: false });
    expect(calls).toEqual(['members']);
    expect(state.members).toHaveLength(1);
    expect(state.keys).toEqual([]);
  });

  it('surfaces a pool failure as problem details rather than throwing', async () => {
    const controller = new MembersKeysController({
      now: () => NOW,
      transport: transport({
        listMembers: () =>
          Promise.reject(
            new ProblemError('forbidden', {
              status: 403,
              code: 'forbidden',
              title: 'That is not allowed for your role',
              detail: 'owner role required',
              requestId: 'req_members_1',
            }),
          ),
      }),
    });
    const state = await controller.load();
    expect(state.phase).toBe('error');
    expect(state.problem?.message).toBe('owner role required');
    expect(state.problem?.requestId).toBe('req_members_1');
  });

  it('keeps the key secret once and drops it when dismissed', async () => {
    const controller = new MembersKeysController({
      now: () => NOW,
      transport: transport({
        createKey: async () => ({
          api_key: {
            id: 'key-1',
            name: 'nightly',
            prefix: 'tp_live_abc',
            scopes: ['runs:dispatch'],
            status: 'active',
            created_at: NOW.toISOString(),
          },
          secret: 'tp_live_abc.the-secret',
        }),
      }),
    });
    const created = await controller.createKey({ name: 'nightly', scopes: ['runs:dispatch'] });
    expect(created.createdKey?.secret).toBe('tp_live_abc.the-secret');
    expect(created.keys).toHaveLength(1);

    const dismissed = controller.dismissCreatedKey();
    expect(dismissed.createdKey).toBeNull();
    // The key itself remains, with its prefix and metadata, but never the secret.
    expect(dismissed.keys[0]?.prefix).toBe('tp_live_abc');
    expect(JSON.stringify(dismissed)).not.toContain('the-secret');
  });

  it('drops a secret on reload, because the pool cannot return it again', async () => {
    const controller = new MembersKeysController({
      now: () => NOW,
      transport: transport({
        createKey: async () => ({
          api_key: {
            id: 'key-1',
            name: 'nightly',
            prefix: 'tp_live_abc',
            scopes: ['projects:read'],
            status: 'active',
            created_at: NOW.toISOString(),
          },
          secret: 'tp_live_abc.the-secret',
        }),
      }),
    });
    await controller.createKey({ name: 'nightly', scopes: ['projects:read'] });
    const reloaded = await controller.load();
    expect(reloaded.createdKey).toBeNull();
  });

  it('refuses to create a key with no scopes without calling the pool', async () => {
    let called = false;
    const controller = new MembersKeysController({
      now: () => NOW,
      transport: transport({
        createKey: async () => {
          called = true;
          return { api_key: {} as never, secret: '' };
        },
      }),
    });
    const state = await controller.createKey({ name: 'nightly', scopes: [] });
    expect(called).toBe(false);
    expect(state.problem?.code).toBe('invalid_key_input');
  });

  it('invites a member, keeps the one-time token and shows the expiry', async () => {
    const controller = new MembersKeysController({
      now: () => NOW,
      transport: transport({
        inviteMember: async (input) => ({
          invitation: {
            id: 'inv-1',
            email: input.email,
            role: input.role,
            created_at: NOW.toISOString(),
            expires_at: '2026-11-05T12:00:00.000Z',
          },
          token: 'invite-token',
        }),
      }),
    });
    const state = await controller.invite({ email: 'new@example.com', role: 'member' });
    expect(state.createdInvitation?.token).toBe('invite-token');
    expect(state.createdInvitation?.invitation.expiry.text).toBe('Expires in 7 days');
    expect(state.invitations[0]?.email).toBe('new@example.com');
    expect(state.notice).toContain('Invitation created for new@example.com');
    expect(controller.dismissCreatedInvitation().createdInvitation).toBeNull();
  });

  it('refuses an invalid invitation without calling the pool', async () => {
    let called = false;
    const controller = new MembersKeysController({
      now: () => NOW,
      transport: transport({
        inviteMember: async () => {
          called = true;
          return { invitation: {} as never, token: '' };
        },
      }),
    });
    const state = await controller.invite({ email: 'not-an-email', role: 'member' });
    expect(called).toBe(false);
    expect(state.problem?.code).toBe('invalid_invite_input');
  });

  it('refuses to remove or demote the last owner, with the reason', async () => {
    const calls: string[] = [];
    const owner = {
      id: 'user-1',
      email: 'ada@example.com',
      display_name: 'Ada',
      role: 'owner' as const,
      password_change_required: false,
      created_at: '2026-10-01T00:00:00.000Z',
    };
    const controller = new MembersKeysController({
      now: () => NOW,
      transport: transport({
        listMembers: async () => ({ members: [owner] }),
        removeMember: async () => {
          calls.push('remove');
        },
        changeMemberRole: async () => {
          calls.push('change');
          return owner;
        },
      }),
    });
    await controller.load();

    const removed = await controller.removeMember('user-1');
    expect(removed.problem?.title).toContain('last owner');
    expect(removed.problem?.message).toBe(LAST_OWNER_REASON);
    expect(removed.members).toHaveLength(1);

    const demoted = await controller.changeRole('user-1', 'viewer');
    expect(demoted.problem?.message).toBe(LAST_OWNER_REASON);
    expect(calls).toEqual([]);
  });

  it('changes a role and removes a member when another owner exists', async () => {
    const owner = {
      id: 'user-1',
      email: 'ada@example.com',
      display_name: 'Ada',
      role: 'owner' as const,
      password_change_required: false,
      created_at: '2026-10-01T00:00:00.000Z',
    };
    const second = {
      id: 'user-2',
      email: 'grace@example.com',
      display_name: 'Grace',
      role: 'owner' as const,
      password_change_required: false,
      created_at: '2026-10-02T00:00:00.000Z',
    };
    const controller = new MembersKeysController({
      now: () => NOW,
      transport: transport({
        listMembers: async () => ({ members: [owner, second] }),
        changeMemberRole: async (id, role) => ({ ...second, id, role }),
        removeMember: async () => undefined,
      }),
    });
    await controller.load();
    const changed = await controller.changeRole('user-2', 'admin');
    expect(changed.members.find((row) => row.id === 'user-2')?.role).toBe('admin');
    expect(changed.notice).toContain('Grace is now admin');

    const removed = await controller.removeMember('user-2');
    expect(removed.members.map((row) => row.id)).toEqual(['user-1']);
    expect(removed.notice).toContain('no longer has access');
  });

  it('revokes a key and keeps its row, marked revoked', async () => {
    const controller = new MembersKeysController({
      now: () => NOW,
      transport: transport({
        listKeys: async () => ({
          api_keys: [
            {
              id: 'key-1',
              name: 'nightly',
              prefix: 'tp_live_abc',
              scopes: ['projects:read'],
              status: 'active',
              created_at: '2026-10-28T00:00:00.000Z',
            },
          ],
        }),
        revokeKey: async () => undefined,
      }),
    });
    await controller.load();
    const state = await controller.revokeKey('key-1');
    expect(state.keys).toHaveLength(1);
    expect(state.keys[0]?.status.status).toBe('revoked');
    expect(state.keys[0]?.status.word).toBe('Revoked');
    expect(state.keys[0]?.revokedAt).toBe(NOW.toISOString());
    expect(state.notice).toContain('no longer authenticates');
    expect(REVOKE_KEY_CONSEQUENCE).toContain('marked revoked');
  });

  it('states the consequence of removing a member', () => {
    expect(REMOVE_MEMBER_CONSEQUENCE).toContain('ends their access immediately');
  });
});
