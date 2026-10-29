/**
 * The browser entry point.
 *
 * It mounts the root with the saved palette storage, the pool configuration and
 * the current hash. No request is made at startup: with no pool configured the
 * app renders the notice and stays quiet.
 */

import { mount } from 'svelte';
import Root from './Root.svelte';
import { poolConfigFrom, type KeyValueStorage } from '$lib/api/pool';
import type { ThemeStorage } from '$lib/theme';

/** `localStorage`, or null where it is unavailable (private mode, tests). */
function browserStorage(): (ThemeStorage & KeyValueStorage) | null {
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

const storage = browserStorage();
const pool = poolConfigFrom({ env: import.meta.env as Record<string, string | undefined>, storage });

const target = document.getElementById('app');
if (!target) throw new Error('The control plane needs an element with id="app" to mount into.');

mount(Root, {
  target,
  props: { storage, pool, initialPath: currentPath() },
});
