/**
 * The control plane's theme state: which Clawffice palette is rendered.
 *
 * Biolume is the default. The palette choice is written to storage so a reload
 * keeps it, and an unknown stored value falls back to the default rather than
 * leaving the app unstyled. Storage is injected so the store is testable
 * without a browser.
 */

import { writable, type Readable, type Writable } from 'svelte/store';
import {
  DEFAULT_PALETTE,
  isPaletteName,
  type PaletteName,
} from '$lib/design/tokens';

/** The storage key the palette choice is saved under. */
export const THEME_STORAGE_KEY = 'tidepool.control-plane.palette';

/** The subset of `Storage` the theme needs. */
export interface ThemeStorage {
  /** Read a saved value. */
  getItem(key: string): string | null;
  /** Write a saved value. */
  setItem(key: string, value: string): void;
}

/** The theme store the app renders from. */
export interface ThemeStore extends Readable<PaletteName> {
  /** Switch to a palette, ignoring names the design system does not define. */
  setPalette(name: string): void;
  /** Return to Biolume. */
  reset(): void;
}

/** Read a saved palette, falling back to the default. */
export function paletteFromStorage(storage: ThemeStorage | null): PaletteName {
  if (!storage) return DEFAULT_PALETTE;
  try {
    const saved = storage.getItem(THEME_STORAGE_KEY);
    return isPaletteName(saved) ? saved : DEFAULT_PALETTE;
  } catch {
    return DEFAULT_PALETTE;
  }
}

/** Create the theme store, restoring the saved palette when there is one. */
export function createTheme(
  storage: ThemeStorage | null = null,
  initial: PaletteName = paletteFromStorage(storage),
): ThemeStore {
  const store: Writable<PaletteName> = writable(initial);
  const setPalette = (name: string): void => {
    if (!isPaletteName(name)) return;
    store.set(name);
    if (!storage) return;
    try {
      storage.setItem(THEME_STORAGE_KEY, name);
    } catch {
      // A full or blocked storage must not break the palette switch.
    }
  };
  return {
    subscribe: store.subscribe,
    setPalette,
    reset(): void {
      setPalette(DEFAULT_PALETTE);
    },
  };
}
