/**
 * The browser entry point.
 *
 * It mounts the root with the saved palette and session storage, the pool
 * configuration and the current hash. With no pool configured the app renders
 * the notice and stays quiet; with one, the root asks for the session state.
 *
 * When the pool serves this built app itself, the served page carries a
 * `tidepool-pool-origin` marker and the pool's own origin becomes the base URL:
 * one origin, no build-time URL and no CORS. A standalone deployment has no
 * marker and keeps using `VITE_POOL_URL` or the URL saved in settings.
 *
 * A container deployment can also set the URL at start: `/config.json`, written
 * from `POOL_URL`, wins on a fresh load and is never saved to storage.
 */

import { mount } from 'svelte';
import Root from './Root.svelte';
import { isServedByPool, loadRuntimePoolUrl, resolvePoolConfig } from './routes/settings/preferences';

/** `localStorage`, or null where it is unavailable (private mode, tests). */
function browserStorage(): Storage | null {
  try {
    return typeof window === 'undefined' ? null : window.localStorage;
  } catch {
    return null;
  }
}

/** The current hash route, e.g. `/hosts`. */
function currentPath(): string {
  return typeof window === 'undefined' ? '/' : window.location.hash.replace(/^#/, '') || '/';
}

/** True when the page carries the pool's same-origin marker. */
function servedByPool(): boolean {
  if (typeof document === 'undefined') return false;
  const marker = document.querySelector('meta[name="tidepool-pool-origin"]');
  return isServedByPool(marker?.getAttribute('content'));
}

const storage = browserStorage();
const servedFromPool = servedByPool();
const pool = resolvePoolConfig({
  runtime: await loadRuntimePoolUrl(),
  env: import.meta.env as Record<string, string | undefined>,
  storage,
  sameOrigin: servedFromPool,
  origin: typeof window === 'undefined' ? null : window.location.origin,
});

const target = document.getElementById('app');
if (!target) throw new Error('The control plane needs an element with id="app" to mount into.');

mount(Root, {
  target,
  props: {
    storage,
    sessionStorage: storage,
    pool,
    servedByPool: servedFromPool,
    initialPath: currentPath(),
  },
});
