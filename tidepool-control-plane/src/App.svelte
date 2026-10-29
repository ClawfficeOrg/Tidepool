<script lang="ts">
  /**
   * The app shell.
   *
   * It owns the palette (and therefore every token the pages read), the
   * navigation, the pool notice and the error boundary. Everything it renders
   * is a pure function of its props, so the shell can be rendered and asserted
   * on without a browser.
   */
  import PaletteSwitcher from './components/PaletteSwitcher.svelte';
  import Nav from './components/Nav.svelte';
  import PoolNotice from './components/PoolNotice.svelte';
  import ProblemDetails from './components/ProblemDetails.svelte';
  import Hosts from './routes/Hosts.svelte';
  import NotFound from './routes/NotFound.svelte';
  import Overview from './routes/Overview.svelte';
  import Runs from './routes/Runs.svelte';
  import Settings from './routes/Settings.svelte';
  import { DEFAULT_PALETTE, PALETTES, themeStyle, type PaletteName } from '$lib/design/tokens';
  import { presentProblem, type PoolProblem } from '$lib/api/problem';
  import { isPoolConfigured, type PoolConfig } from '$lib/api/pool';
  import { ROUTES, resolveRoute } from '$lib/router';

  interface Props {
    /** The Clawffice palette to render. Biolume unless chosen otherwise. */
    palette?: PaletteName;
    /** The app path currently shown. */
    path?: string;
    /** The pool this control plane is pointed at. */
    pool?: PoolConfig;
    /** A failure to render in the boundary, when one has been caught. */
    problem?: PoolProblem | null;
    /** Called when a palette is chosen. */
    onPaletteSelect?: (name: PaletteName) => void;
    /** Called when a person asks to set the pool URL. */
    onConfigurePool?: () => void;
    /** Called to retry after a failure. */
    onRetry?: () => void;
  }

  let {
    palette = DEFAULT_PALETTE,
    path = '/',
    pool = { baseUrl: null },
    problem = null,
    onPaletteSelect,
    onConfigurePool,
    onRetry,
  }: Props = $props();

  const current = $derived(resolveRoute(path));
  const configured = $derived(isPoolConfigured(pool));
  const scheme = $derived(PALETTES[palette].scheme);
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
    <PaletteSwitcher {palette} onSelect={onPaletteSelect} />
  </header>

  <div class="tp-shell">
    <Nav routes={ROUTES} current={current.path} />

    <main class="tp-main" id="main" tabindex="-1">
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
            <Overview />
          {:else if current.found && current.route.id === 'hosts'}
            <Hosts />
          {:else if current.found && current.route.id === 'runs'}
            <Runs />
          {:else if current.found && current.route.id === 'settings'}
            <Settings />
          {:else}
            <NotFound path={current.path} />
          {/if}
        </svelte:boundary>
      {/if}
    </main>
  </div>
</div>
