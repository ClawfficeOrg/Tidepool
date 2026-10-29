<script lang="ts">
  /**
   * The app shell.
   *
   * It owns the palette (and therefore every token the pages read), the
   * navigation, the pool notice and the error boundary. When an auth state is
   * passed it also owns the gate: setup, sign-in, the loading state and the
   * signed-in session bar. Everything it renders is a pure function of its
   * props, so the shell can be rendered and asserted on without a browser.
   */
  import PaletteSwitcher from './components/PaletteSwitcher.svelte';
  import Nav from './components/Nav.svelte';
  import PoolNotice from './components/PoolNotice.svelte';
  import ProblemDetails from './components/ProblemDetails.svelte';
  import SessionBar from './auth/SessionBar.svelte';
  import AuthLoading from './routes/AuthLoading.svelte';
  import Connections from './routes/Connections.svelte';
  import Hosts from './routes/Hosts.svelte';
  import NotFound from './routes/NotFound.svelte';
  import Overview from './routes/Overview.svelte';
  import Projects from './routes/projects/ProjectsPage.svelte';
  import Runs from './routes/Runs.svelte';
  import Triggers from './routes/Triggers.svelte';
  import Settings from './routes/Settings.svelte';
  import Setup from './routes/Setup.svelte';
  import SignIn from './routes/SignIn.svelte';
  import { DEFAULT_PALETTE, PALETTES, themeStyle, type PaletteName } from '$lib/design/tokens';
  import { presentProblem, type PoolProblem } from '$lib/api/problem';
  import { isPoolConfigured, type PoolConfig } from '$lib/api/pool';
  import { ROUTES, resolveRoute } from '$lib/router';
  import type { AuthState, LoginInput, SetupInput } from './auth/types';

  interface Props {
    /** The Clawffice palette to render. Biolume unless chosen otherwise. */
    palette?: PaletteName;
    /** The app path currently shown. */
    path?: string;
    /** The pool this control plane is pointed at. */
    pool?: PoolConfig;
    /** A failure to render in the boundary, when one has been caught. */
    problem?: PoolProblem | null;
    /** Who is signed in, or what the next step is. Null means no auth gate. */
    auth?: AuthState | null;
    /** Called when a palette is chosen. */
    onPaletteSelect?: (name: PaletteName) => void;
    /** Called when a person asks to set the pool URL. */
    onConfigurePool?: () => void;
    /** Called with credentials from the sign-in form. */
    onSignIn?: (input: LoginInput) => void | Promise<void>;
    /** Called with the first-run setup form. */
    onSetup?: (input: SetupInput) => void | Promise<void>;
    /** Called when the signed-in person asks to sign out. */
    onSignOut?: () => void | Promise<void>;
    /** Called to retry after a failure. */
    onRetry?: () => void;
  }

  let {
    palette = DEFAULT_PALETTE,
    path = '/',
    pool = { baseUrl: null },
    problem = null,
    auth = null,
    onPaletteSelect,
    onConfigurePool,
    onSignIn,
    onSetup,
    onSignOut,
    onRetry,
  }: Props = $props();

  const current = $derived(resolveRoute(path));
  const configured = $derived(isPoolConfigured(pool));
  const scheme = $derived(PALETTES[palette].scheme);
  const session = $derived(auth?.status === 'signed-in' ? auth.session : null);
  const gate = $derived(auth !== null && auth.status !== 'signed-in' ? auth : null);
</script>

<svelte:head>
  <title>Tidepool control plane</title>
</svelte:head>

<div
  class="tp-app"
  data-palette={palette}
  data-scheme={scheme}
  data-pool={configured ? 'configured' : 'unconfigured'}
  style={themeStyle(palette)}
>
  <a class="tp-skip-link" href="#main">Skip to content</a>

  <header class="tp-header">
    <div class="tp-brand">
      <img class="tp-brand__mark" src="/tidepool-mark.svg" alt="" width="40" height="40" />
      <span class="tp-brand__wordmark">tide<span class="tp-brand__wordmark-accent">pool</span></span>
      <img
        class="tp-brand__cell"
        src="/tidepool-cell.svg"
        alt=""
        aria-hidden="true"
        width="24"
        height="24"
      />
      <span class="tp-brand__role">control plane</span>
    </div>
    <div class="tp-header__tools">
      {#if session}
        <SessionBar {session} {onSignOut} />
      {/if}
      <PaletteSwitcher {palette} onSelect={onPaletteSelect} />
    </div>
  </header>

  <div class="tp-shell" data-gated={gate ? 'true' : 'false'}>
    {#if !gate}
      <Nav routes={ROUTES} current={current.path} />
    {/if}

    <main class="tp-main" id="main" tabindex="-1">
      {#if gate}
        <svelte:boundary>
          {#snippet failed(error)}
            <ProblemDetails problem={presentProblem(error)} onRetry={onRetry} />
          {/snippet}

          {#if gate.status === 'loading'}
            <AuthLoading />
          {:else if gate.status === 'setup'}
            <Setup
              poolUrl={pool.baseUrl}
              notice={gate.notice}
              problem={gate.problem}
              onSubmit={onSetup}
            />
          {:else if gate.status === 'signed-out'}
            <SignIn
              poolUrl={pool.baseUrl}
              notice={gate.notice}
              problem={gate.problem}
              onSubmit={onSignIn}
            />
          {/if}
        </svelte:boundary>
      {:else}
        {#if !configured && !problem}
          <PoolNotice config={pool} onConfigure={onConfigurePool} />
        {/if}

        {#if problem}
          <ProblemDetails {problem} onRetry={onRetry} />
        {:else}
          <svelte:boundary>
            {#snippet failed(error)}
              <ProblemDetails problem={presentProblem(error)} onRetry={onRetry} />
            {/snippet}

            {#if current.found && current.route.id === 'overview'}
              <Overview
                role={session?.user.role ?? null}
                name={session?.user.display_name ?? ''}
                organization={session?.organization?.name ?? null}
              />
            {:else if current.found && current.route.id === 'hosts'}
              <Hosts
                pool={pool}
                token={session?.token ?? null}
                role={session?.user.role ?? null}
              />
            {:else if current.found && current.route.id === 'connections'}
              <Connections
                pool={pool}
                token={session?.token ?? null}
                role={session?.user.role ?? null}
              />
            {:else if current.found && current.route.id === 'projects'}
              <Projects
                pool={pool}
                token={session?.token ?? null}
                role={session?.user.role ?? null}
              />
            {:else if current.found && current.route.id === 'triggers'}
              <Triggers
                pool={pool}
                token={session?.token ?? null}
                role={session?.user.role ?? null}
              />
            {:else if current.found && current.route.id === 'runs'}
              <Runs
                pool={pool}
                token={session?.token ?? null}
                role={session?.user.role ?? null}
              />
            {:else if current.found && current.route.id === 'settings'}
              <Settings
                pool={pool}
                token={session?.token ?? null}
                role={session?.user.role ?? null}
              />
            {:else}
              <NotFound path={current.path} />
            {/if}
          </svelte:boundary>
        {/if}
      {/if}
    </main>
  </div>
</div>
