<script lang="ts">
  /**
   * The browser root: the only component that reads browser state.
   *
   * It subscribes the theme store to a reactive `palette`, follows the hash
   * route, and hands both to the pure shell. Keeping this here means `App`
   * stays a function of its props and can be rendered in tests without a DOM.
   */
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import App from './App.svelte';
  import { createTheme, type ThemeStorage } from '$lib/theme';
  import type { PoolConfig } from '$lib/api/pool';
  import type { PaletteName } from '$lib/design/tokens';

  interface Props {
    /** Where the palette choice is remembered. */
    storage?: ThemeStorage | null;
    /** The pool this app is pointed at. */
    pool?: PoolConfig;
    /** The path to show before the hash is read. */
    initialPath?: string;
  }

  let { storage = null, pool = { baseUrl: null }, initialPath = '/' }: Props = $props();

  // Both are read once, on mount: the store owns them from here.
  // svelte-ignore state_referenced_locally
  const theme = createTheme(storage);
  let palette = $state<PaletteName>(get(theme));
  // svelte-ignore state_referenced_locally
  let path = $state(initialPath);

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
    return () => {
      unsubscribe();
      window.removeEventListener('hashchange', onHashChange);
    };
  });

  /** Switch palette and remember it. */
  function selectPalette(name: PaletteName): void {
    theme.setPalette(name);
  }
</script>

<App {palette} {path} {pool} onPaletteSelect={selectPalette} />
