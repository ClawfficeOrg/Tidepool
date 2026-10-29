<script lang="ts">
  /**
   * The browser root: the only component that reads browser state.
   *
   * It subscribes the theme store to a reactive `palette`, follows the hash
   * route, owns the auth controller and owns the settings that are remembered
   * in this browser. Keeping them here means `App` stays a function of its
   * props and can be rendered in tests without a DOM; the preferences module is
   * DOM-free and is tested directly.
   *
   * A changed pool URL rebuilds the auth controller against the new pool, so
   * pointing the app elsewhere takes effect at once and never needs a rebuild.
   */
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import App from './App.svelte';
  import { AuthController } from './auth/controller';
  import { createPoolAuthGateway, type AuthGateway } from './auth/gateway';
  import type { SessionStorage } from './auth/session';
  import type { AuthState, LoginInput, SetupInput } from './auth/types';
  import { createTheme, type ThemeStorage } from '$lib/theme';
  import { isPoolConfigured, type PoolConfig } from '$lib/api/pool';
  import type { PaletteName } from '$lib/design/tokens';
  import {
    POOL_URL_SAVED_NOTICE,
    applyPoolUrl,
    readPreferences,
    resolvePoolConfig,
    savePreferences,
    type ControlPlanePreferences,
  } from './routes/settings/preferences';

  interface Props {
    /** Where the palette choice and preferences are remembered. */
    storage?: ThemeStorage | null;
    /** The pool this app is pointed at. */
    pool?: PoolConfig;
    /** Where the session is remembered; defaults to `localStorage`. */
    sessionStorage?: SessionStorage | null;
    /** The auth gateway, injectable for tests. */
    authGateway?: AuthGateway | null;
    /** The path to show before the hash is read. */
    initialPath?: string;
    /** True when the pool itself served this page. */
    servedByPool?: boolean;
  }

  let {
    storage = null,
    pool: initialPool = { baseUrl: null },
    sessionStorage = null,
    authGateway = null,
    initialPath = '/',
    servedByPool = false,
  }: Props = $props();

  /** `localStorage`, or null where it is unavailable (private mode, tests). */
  function browserSessionStorage(): SessionStorage | null {
    try {
      return typeof window === 'undefined' ? null : window.localStorage;
    } catch {
      return null;
    }
  }

  /** The origin this page was opened from, or null outside a browser. */
  function pageOrigin(): string | null {
    try {
      return typeof window === 'undefined' ? null : window.location.origin;
    } catch {
      return null;
    }
  }

  /** Resolve the pool again after a preference changes. */
  function resolveFromStorage(): PoolConfig {
    return resolvePoolConfig({
      env: import.meta.env as Record<string, string | undefined>,
      storage,
      sameOrigin: servedByPool,
      origin: pageOrigin(),
    });
  }

  /** Build the auth controller for a pool, or null when there is none. */
  function buildController(config: PoolConfig): AuthController | null {
    if (authGateway) {
      return new AuthController({ gateway: authGateway, storage: sessionStorage ?? browserSessionStorage() });
    }
    if (!isPoolConfigured(config)) return null;
    return new AuthController({
      gateway: createPoolAuthGateway(config),
      storage: sessionStorage ?? browserSessionStorage(),
    });
  }

  // Read once, on mount: the stores own them from here.
  // svelte-ignore state_referenced_locally
  const theme = createTheme(storage);
  let palette = $state<PaletteName>(get(theme));
  // svelte-ignore state_referenced_locally
  let pool = $state<PoolConfig>(initialPool);
  // svelte-ignore state_referenced_locally
  let preferences = $state<ControlPlanePreferences>(readPreferences(storage));
  let poolUrlProblem = $state<string | null>(null);
  let poolUrlNotice = $state<string | null>(null);
  // svelte-ignore state_referenced_locally
  let path = $state(initialPath);
  // svelte-ignore state_referenced_locally
  let controller = $state<AuthController | null>(buildController(initialPool));
  // svelte-ignore state_referenced_locally
  let auth = $state<AuthState | null>(controller ? { status: 'loading' } : null);

  /** Read the current hash route, e.g. `/hosts`. */
  function readPath(): string {
    return window.location.hash.replace(/^#/, '') || '/';
  }

  onMount(() => {
    path = readPath();
    const unsubscribe = theme.subscribe((next) => {
      palette = next;
      document.documentElement.dataset['palette'] = next;
    });
    const onHashChange = (): void => {
      path = readPath();
    };
    window.addEventListener('hashchange', onHashChange);
    if (controller) {
      void controller.start().then((next) => {
        auth = next;
      });
    }
    return () => {
      unsubscribe();
      window.removeEventListener('hashchange', onHashChange);
    };
  });

  /** Remember preferences, tolerating a storage that refuses. */
  function remember(next: ControlPlanePreferences): void {
    savePreferences(storage, next);
  }

  /** Switch palette and remember it. */
  function selectPalette(name: PaletteName): void {
    theme.setPalette(name);
    preferences = { ...preferences, palette: name };
    remember(preferences);
  }

  /** Change one non-palette preference and remember it. */
  function changePreference(patch: Partial<ControlPlanePreferences>): void {
    preferences = { ...preferences, ...patch };
    remember(preferences);
    if (patch.palette) theme.setPalette(patch.palette);
  }

  /** Point the app at a pool without rebuilding it. */
  function savePoolUrl(url: string): void {
    const outcome = applyPoolUrl(preferences, url);
    if (!outcome.ok) {
      poolUrlProblem = outcome.reason;
      poolUrlNotice = null;
      return;
    }
    poolUrlProblem = null;
    preferences = outcome.preferences;
    remember(preferences);
    const next = resolveFromStorage();
    pool = next;
    poolUrlNotice = POOL_URL_SAVED_NOTICE;

    controller = buildController(next);
    if (!controller) {
      auth = null;
      return;
    }
    auth = { status: 'loading' };
    void controller.start().then((state) => {
      auth = state;
    });
  }

  /** Sign in and adopt the resulting state. */
  async function signIn(input: LoginInput): Promise<void> {
    if (!controller) return;
    auth = await controller.signIn(input);
  }

  /** Walk first-run setup and the sign-in that follows it. */
  async function completeSetup(input: SetupInput): Promise<void> {
    if (!controller) return;
    auth = await controller.completeSetup(input);
  }

  /** End the session, on the pool and here. */
  async function signOut(): Promise<void> {
    if (!controller) return;
    auth = await controller.signOut();
  }
</script>

<!-- A pool change remounts the shell so every page rebuilds its controller
     against the new pool; Root keeps the preferences and the session. -->
{#key pool.baseUrl}
  <App
    {palette}
    {path}
    {pool}
    {auth}
    {preferences}
    {servedByPool}
    {poolUrlProblem}
    {poolUrlNotice}
    onPaletteSelect={selectPalette}
    onSavePoolUrl={savePoolUrl}
    onPreferenceChange={changePreference}
    onSignIn={signIn}
    onSetup={completeSetup}
    onSignOut={signOut}
  />
{/key}
