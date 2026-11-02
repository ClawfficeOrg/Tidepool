import { describe, expect, it } from 'vitest';
import { ProblemError } from '@clawffice/tidepool-sdk';
import App from '../App.svelte';
import { presentProblem } from '$lib/api/problem';
import { renderComponent } from '$lib/testing/render';
import { CONTROLS, mutatingControls } from './roles';
import type { AuthSession, AuthState } from './types';

const POOL = { baseUrl: 'https://pool.example.com' };

const OWNER: AuthSession = {
  token: 'token-1',
  expiresAt: 9_000_000,
  user: {
    id: 'user-1',
    email: 'owner@example.com',
    display_name: 'Ada',
    role: 'owner',
  },
  organization: { id: 'org-1', name: 'Rock Pool', created_at: '2026-10-29T00:00:00Z' },
  mustChangePassword: false,
};

const VIEWER: AuthSession = {
  ...OWNER,
  user: { ...OWNER.user, id: 'user-2', display_name: 'Vic', role: 'viewer' },
};

const SETUP_STATUS = { needs_setup: true, bootstrap_configured: false };

/** Render the shell with one auth state. */
function renderAuth(auth: AuthState) {
  return renderComponent(App, { pool: POOL, auth, path: '/' });
}

describe('the auth gate', () => {
  it('shows the sign-in form, and no pages, when signed out', () => {
    const { html, text } = renderAuth({
      status: 'signed-out',
      problem: null,
      notice: 'Your session expired. Sign in again.',
    });
    expect(html).toContain('data-auth-page="signin"');
    expect(text).toContain('Sign in');
    expect(text).toContain('Your session expired');
    expect(html).not.toContain('href="#/hosts"');
    expect(text).not.toContain('This control plane is scaffolded');
  });

  it('shows first-run setup, not sign-in, when the pool needs setup', () => {
    const { html, text } = renderAuth({
      status: 'setup',
      setup: SETUP_STATUS,
      problem: null,
      notice: null,
    });
    expect(html).toContain('data-auth-page="setup"');
    expect(text).toContain('Set up this pool');
    expect(text).toContain('Create the pool and sign in');
    expect(text).toContain('At least 12 characters');
    expect(html).not.toContain('data-auth-page="signin"');
  });

  it('says it is checking the session before deciding', () => {
    const { html, text } = renderAuth({ status: 'loading' });
    expect(html).toContain('data-auth-page="loading"');
    expect(text).toContain('Checking your session');
    expect(html).not.toContain('data-auth-page="signin"');
  });

  it('renders a sign-in failure with its request id', () => {
    const problem = presentProblem(
      new ProblemError('bad credentials', {
        status: 401,
        code: 'unauthorized',
        title: 'The pool did not accept those credentials',
        detail: 'invalid credentials',
        requestId: 'req_signin',
      }),
    );
    const { text } = renderAuth({ status: 'signed-out', problem, notice: null });
    expect(text).toContain('The pool did not accept those credentials');
    expect(text).toContain('Request id req_signin');
  });
});

describe('the signed-in shell', () => {
  it('shows the person, role and sign-out control', () => {
    const { html, text } = renderAuth({ status: 'signed-in', session: OWNER });
    expect(text).toContain('Ada');
    expect(text).toContain('Owner');
    expect(text).toContain('Rock Pool');
    expect(html).toContain('type="button"');
    expect(text).toContain('Sign out');
    expect(html).toContain('href="#/hosts"');
  });

  it('offers an owner the owner-only controls', () => {
    const { html } = renderAuth({ status: 'signed-in', session: OWNER });
    for (const control of CONTROLS) {
      expect(html).toContain(`data-control="${control.id}"`);
    }
    expect(html).not.toContain('data-hidden-control=');
  });

  it('never offers a viewer a mutating control, and says why', () => {
    const { html, text } = renderAuth({ status: 'signed-in', session: VIEWER });
    for (const control of mutatingControls()) {
      expect(html).not.toContain(`data-control="${control.id}"`);
      expect(html).toContain(`data-hidden-control="${control.id}"`);
    }
    expect(html).toContain('data-control="view-hosts"');
    expect(text).toContain('the viewer role is read-only');
    expect(text).toContain('actions are hidden for this role');
  });

  it('does not render a mutating control for a signed-out person either', () => {
    const { html } = renderAuth({ status: 'signed-out', problem: null, notice: null });
    for (const control of mutatingControls()) {
      expect(html).not.toContain(`data-control="${control.id}"`);
    }
  });
});
