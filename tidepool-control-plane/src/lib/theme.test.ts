import { describe, expect, it } from 'vitest';
import { get } from 'svelte/store';
import {
  THEME_STORAGE_KEY,
  createTheme,
  paletteFromStorage,
  type ThemeStorage,
} from './theme';
import { DEFAULT_PALETTE, PALETTE_NAMES } from './design/tokens';

/** An in-memory stand-in for `localStorage`. */
function memoryStorage(seed: Record<string, string> = {}): ThemeStorage & {
  readonly written: string[];
} {
  const data = new Map(Object.entries(seed));
  const written: string[] = [];
  return {
    written,
    getItem: (key) => data.get(key) ?? null,
    setItem: (key, value) => {
      written.push(key);
      data.set(key, value);
    },
  };
}

describe('theme store', () => {
  it('starts on Biolume with no storage', () => {
    const theme = createTheme(null);
    expect(get(theme)).toBe(DEFAULT_PALETTE);
  });

  it('switches palette for every one of the five', () => {
    const theme = createTheme(memoryStorage());
    for (const name of PALETTE_NAMES) {
      theme.setPalette(name);
      expect(get(theme)).toBe(name);
    }
  });

  it('remembers the choice under its own key', () => {
    const storage = memoryStorage();
    const theme = createTheme(storage);
    theme.setPalette('sea-glass');
    expect(storage.getItem(THEME_STORAGE_KEY)).toBe('sea-glass');
    expect(storage.written).toEqual([THEME_STORAGE_KEY]);
  });

  it('restores a remembered choice on reload', () => {
    const storage = memoryStorage({ [THEME_STORAGE_KEY]: 'phosphor' });
    expect(paletteFromStorage(storage)).toBe('phosphor');
    expect(get(createTheme(storage))).toBe('phosphor');
  });

  it('falls back to Biolume for an unknown or hostile stored value', () => {
    expect(paletteFromStorage(memoryStorage({ [THEME_STORAGE_KEY]: 'neon' }))).toBe(DEFAULT_PALETTE);
    expect(paletteFromStorage(memoryStorage({ [THEME_STORAGE_KEY]: '' }))).toBe(DEFAULT_PALETTE);
    expect(paletteFromStorage(null)).toBe(DEFAULT_PALETTE);
  });

  it('ignores a palette name the design system does not define', () => {
    const storage = memoryStorage();
    const theme = createTheme(storage);
    theme.setPalette('builtin-dark');
    expect(get(theme)).toBe(DEFAULT_PALETTE);
    expect(storage.written).toEqual([]);
  });

  it('resets to Biolume', () => {
    const theme = createTheme(memoryStorage());
    theme.setPalette('boiled');
    theme.reset();
    expect(get(theme)).toBe(DEFAULT_PALETTE);
  });

  it('keeps working when storage throws', () => {
    const hostile: ThemeStorage = {
      getItem: () => {
        throw new Error('storage disabled');
      },
      setItem: () => {
        throw new Error('storage disabled');
      },
    };
    expect(paletteFromStorage(hostile)).toBe(DEFAULT_PALETTE);
    const theme = createTheme(hostile);
    theme.setPalette('deep-tide');
    expect(get(theme)).toBe('deep-tide');
  });
});
