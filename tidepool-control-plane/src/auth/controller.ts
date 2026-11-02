/**
 * The session lifecycle, independent of Svelte and the DOM.
 *
 * The controller is the one place that decides what the shell shows:
 *
 * - first run: ask the pool whether it needs setup, and walk setup then sign-in;
 * - reload: restore the saved session, verify it with the pool, and drop a token
 *   the pool no longer accepts;
 * - expiry: a token past its lifetime returns to sign-in without a request;
 * - sign-out: ask the pool to end the session, then forget it locally.
 *
 * Everything it depends on is injected — the gateway, storage and the clock —
 * so the whole lifecycle is exercised headlessly against a local HTTP pool in
 * `controller.test.ts` and `e2e.test.ts`.
 */

import type { PoolProblem } from '../lib/api/problem';
import { describeAuthFailure, isCredentialRejection } from './problem';
import {
  clearSession,
  isSessionExpired,
  readStoredSession,
  saveSession,
  sessionFromLogin,
  validateSetupInput,
  withIdentity,
} from './session';
import type { SessionStorage } from './session';
import {
  SESSION_EXPIRED_NOTICE,
  SIGNED_OUT_NOTICE,
  type AuthSession,
  type AuthState,
  type LoginInput,
  type SetupInput,
  type SetupStatus,
} from './types';
import type { AuthGateway } from './gateway';

/** What the controller needs to run. */
export interface AuthControllerDeps {
  /** The pool's auth surface. */
  gateway: AuthGateway;
  /** Where a session is remembered, or null for memory-only. */
  storage: SessionStorage | null;
  /** Clock, injectable for expiry tests. */
  now?: () => number;
}

/** The notice when the pool ended a session the browser still remembered. */
export const SESSION_REJECTED_NOTICE =
  'The pool no longer accepts this session. Sign in again to continue.';

/** The notice when setup succeeded but the automatic sign-in did not. */
export const SETUP_COMPLETE_NOTICE =
  'Setup finished. Sign in with the email and password you just chose.';

/** The notice when sign-out could not be confirmed by the pool. */
export const SIGNED_OUT_UNCONFIRMED_NOTICE =
  'Signed out on this device, but the pool could not confirm the session ended.';

/** The notice when a saved session could not be read. */
export const MALFORMED_SESSION_NOTICE =
  'The saved session could not be read. Sign in again.';

/** Drives setup, sign-in, reload, expiry and sign-out. */
export class AuthController {
  private readonly gateway: AuthGateway;
  private readonly storage: SessionStorage | null;
  private readonly now: () => number;
  private stateValue: AuthState = { status: 'loading' };
  private setupStatus: SetupStatus = { needs_setup: true, bootstrap_configured: false };

  constructor(deps: AuthControllerDeps) {
    this.gateway = deps.gateway;
    this.storage = deps.storage;
    this.now = deps.now ?? (() => Date.now());
  }

  /** The current state, for the shell to render. */
  get state(): AuthState {
    return this.stateValue;
  }

  /** Restore or establish the starting state: setup, sign-in or signed-in. */
  async start(): Promise<AuthState> {
    this.setState({ status: 'loading' });
    const now = this.now();
    const stored = readStoredSession(this.storage, now);
    let notice: string | null = null;
    if (stored.expired) notice = SESSION_EXPIRED_NOTICE;
    if (stored.malformed) notice = MALFORMED_SESSION_NOTICE;

    if (stored.session) {
      try {
        const identity = await this.gateway.me(stored.session.token);
        const session = withIdentity(stored.session, identity);
        saveSession(this.storage, session);
        return this.setState({ status: 'signed-in', session });
      } catch (error) {
        const problem = describeAuthFailure(error);
        if (isCredentialRejection(problem)) {
          clearSession(this.storage);
          notice = SESSION_REJECTED_NOTICE;
        } else {
          // The pool is unreachable or failed: keep the remembered session so a
          // reload does not throw a person out over a transient network blip.
          return this.setState({ status: 'signed-in', session: stored.session });
        }
      }
    }

    try {
      const setup = await this.gateway.setupStatus();
      this.setupStatus = setup;
      if (setup.needs_setup) {
        return this.setState({ status: 'setup', setup, problem: null, notice });
      }
      return this.setState({ status: 'signed-out', problem: null, notice });
    } catch (error) {
      return this.setState({
        status: 'signed-out',
        problem: describeAuthFailure(error),
        notice,
      });
    }
  }

  /** Sign in with an email and password, remembering the session. */
  async signIn(input: LoginInput): Promise<AuthState> {
    try {
      const result = await this.gateway.login({
        email: input.email.trim(),
        password: input.password,
      });
      const session = sessionFromLogin(result, this.now());
      saveSession(this.storage, session);
      return this.setState({ status: 'signed-in', session });
    } catch (error) {
      return this.setState({
        status: 'signed-out',
        problem: describeAuthFailure(error),
        notice: null,
      });
    }
  }

  /**
   * Walk first-run setup and then sign the new owner in.
   *
   * The pool's setup endpoint creates the organization and owner but does not
   * issue a token, so the controller performs the sign-in itself with the
   * credentials the person just chose. A setup that succeeds while the
   * follow-up sign-in fails leaves a signed-out state with an explicit notice:
   * the pool is initialized, and the person only needs to sign in.
   */
  async completeSetup(input: SetupInput): Promise<AuthState> {
    const invalid = validateSetupInput(input);
    if (invalid !== null) {
      return this.setState({
        status: 'setup',
        setup: this.setupStatus,
        problem: null,
        notice: invalid,
      });
    }

    const normalized: SetupInput = {
      organization_name: input.organization_name.trim(),
      display_name: input.display_name.trim(),
      email: input.email.trim(),
      password: input.password,
    };

    try {
      await this.gateway.setup(normalized);
      this.setupStatus = { ...this.setupStatus, needs_setup: false };
    } catch (error) {
      return this.setState({
        status: 'setup',
        setup: this.setupStatus,
        problem: describeAuthFailure(error),
        notice: null,
      });
    }

    try {
      const result = await this.gateway.login({
        email: normalized.email,
        password: normalized.password,
      });
      const session = sessionFromLogin(result, this.now());
      saveSession(this.storage, session);
      return this.setState({ status: 'signed-in', session });
    } catch (error) {
      return this.setState({
        status: 'signed-out',
        problem: describeAuthFailure(error),
        notice: SETUP_COMPLETE_NOTICE,
      });
    }
  }

  /**
   * End the session.
   *
   * The pool is asked first so the token stops working for anyone who has it;
   * the local session is cleared either way, because keeping a token on a
   * shared machine is worse than an unconfirmed sign-out. The failure is shown
   * rather than hidden when the pool did not confirm.
   */
  async signOut(): Promise<AuthState> {
    const session = this.currentSession();
    clearSession(this.storage);
    if (!session) {
      return this.setState({ status: 'signed-out', problem: null, notice: SIGNED_OUT_NOTICE });
    }

    try {
      await this.gateway.logout(session.token);
      return this.setState({ status: 'signed-out', problem: null, notice: SIGNED_OUT_NOTICE });
    } catch (error) {
      return this.setState({
        status: 'signed-out',
        problem: describeAuthFailure(error),
        notice: SIGNED_OUT_UNCONFIRMED_NOTICE,
      });
    }
  }

  /** The session to sign out, from state or from what a reload would restore. */
  private currentSession(): AuthSession | null {
    if (this.stateValue.status === 'signed-in') return this.stateValue.session;
    const stored = readStoredSession(this.storage, this.now());
    if (stored.session && !isSessionExpired(stored.session, this.now())) {
      return stored.session;
    }
    return null;
  }

  /** Store and return the new state. */
  private setState(next: AuthState): AuthState {
    this.stateValue = next;
    return next;
  }
}

/** Build a controller for one gateway and storage. */
export function createAuthController(deps: AuthControllerDeps): AuthController {
  return new AuthController(deps);
}

/** Narrow an auth state to a problem, for a caller that wants to render one. */
export function stateProblem(state: AuthState): PoolProblem | null {
  if (state.status === 'setup' || state.status === 'signed-out') return state.problem;
  return null;
}
