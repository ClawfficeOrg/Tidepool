<script lang="ts">
  /**
   * The browser root: the only component that reads browser state.
   *
   * It subscribes the theme store to a reactive `palette`, follows the hash
   * route, and owns the auth controller. Keeping the controller here means
   * `App` stays a function of its props and can be rendered in tests without a
   * DOM; the controller itself is DOM-free and is tested directly.
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

  interface Props {
    /** Where the palette choice is remembered. */
    storage?: ThemeStorage | null;
    /** The pool this app is pointed at. */
    pool?: PoolConfig;
    /** Where the session is remembered; defaults to `localStorage`. */
    sessionStorage?: SessionStorage | null;
    /** The auth gateway, injectable for tests. */
    authGateway?: AuthGateway | null;
    /** The path to show before the hash is read. */
    initialPath?: string;
  }

  let {
    storage = null,
    pool = { baseUrl: null },
    sessionStorage = null,
    authGateway = null,
    initialPath = '/',
  }: Props = $props();

  /** `localStorage`, or null where it is unavailable (private mode, tests). */
  function browserSessionStorage(): SessionStorage | null {
    try {
      return typeof window === 'undefined' ? null : window.localStorage;
    } catch {
      return null;
    }
  }

  const configured = isPoolConfigured(pool);
  const gateway: AuthGateway | null =
    authGateway ?? (configured ? createPoolAuthGateway(pool) : null);
  const controller: AuthController | null = gateway
    ? new AuthController({ gateway, storage: sessionStorage ?? browserSessionStorage() })
    : null;

  // Both are read once, on mount: the store owns them from here.
  // svelte-ignore state_referenced_locally
  const theme = createTheme(storage);
  let palette = $state<PaletteName>(get(theme));
  // svelte-ignore state_referenced_locally
  let path = $state(initialPath);
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

  /** Switch palette and remember it. */
  function selectPalette(name: PaletteName): void {
    theme.setPalette(name);
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

<App
  {palette}
  {path}
  {pool}
  {auth}
  onPaletteSelect={selectPalette}
  onSignIn={signIn}
  onSetup={completeSetup}
  onSignOut={signOut}
/>
