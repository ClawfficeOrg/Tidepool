/**
 * The settings persistence, and the palette across every page.
 *
 * Server-rendered component tests plus the preferences module directly: no DOM
 * and no network. The one thing a browser is needed for — watching a real
 * reload paint a palette — cannot be done here; what is asserted is the whole
 * chain behind it: a choice writes its key, a fresh read restores it, and the
 * shell renders every route in that palette.
 */

import { describe, expect, it } from 'vitest';
import { get } from 'svelte/store';
import App from '../../App.svelte';
import { renderComponent } from '$lib/testing/render';
import { createTheme, THEME_STORAGE_KEY, type ThemeStorage } from '$lib/theme';
import { DEFAULT_PALETTE, PALETTE_NAMES, PALETTES, type PaletteName } from '$lib/design/tokens';
import { POOL_URL_STORAGE_KEY } from '$lib/api/pool';
import { ROUTES } from '$lib/router';
import {
  CONTROL_PLANE_VERSION,
  DEFAULT_PREFERENCES,
  DENSITY_STORAGE_KEY,
  HUB_CONTRACT_VERSION,
  POOL_URL_INVALID_REASON,
  TIME_FORMAT_STORAGE_KEY,
  TIME_ZONE_STORAGE_KEY,
  applyPoolUrl,
  densityStyle,
  loadRuntimePoolUrl,
  formatTime,
  readPreferences,
  resolvePoolConfig,
  savePreferences,
  timeFormatLabel,
  type ControlPlanePreferences,
  type PreferenceStorage,
} from './preferences';
import PreferencesPanel from './PreferencesPanel.svelte';
import packageJson from '../../../package.json';

/** An in-memory `localStorage` that records what it was asked. */
function memoryStorage(seed: Record<string, string> = {}): PreferenceStorage & ThemeStorage & {
  readonly written: string[];
  readonly data: Map<string, string>;
} {
  const data = new Map(Object.entries(seed));
  const written: string[] = [];
  return {
    written,
    data,
    getItem: (key) => data.get(key) ?? null,
    setItem: (key, value) => {
      written.push(key);
      data.set(key, value);
    },
  };
}

/** Preferences with overrides. */
function prefs(overrides: Partial<ControlPlanePreferences> = {}): ControlPlanePreferences {
  return { ...DEFAULT_PREFERENCES, ...overrides };
}

describe('the palette is remembered', () => {
  it('defaults to Biolume on a browser that has never chosen', () => {
    expect(readPreferences(memoryStorage()).palette).toBe(DEFAULT_PALETTE);
    expect(DEFAULT_PALETTE).toBe('biolume');
    expect(readPreferences(null).palette).toBe(DEFAULT_PALETTE);
  });

  it('remembers every one of the five palettes and restores it after a reload', () => {
    for (const name of PALETTE_NAMES) {
      const storage = memoryStorage();
      const theme = createTheme(storage);
      theme.setPalette(name);
      // The same call `savePreferences` makes, from the shell.
      savePreferences(storage, prefs({ palette: name }));

      // A reload is a fresh store and a fresh read against the same storage.
      expect(storage.getItem(THEME_STORAGE_KEY)).toBe(name);
      expect(get(createTheme(storage))).toBe(name);
      expect(readPreferences(storage).palette).toBe(name);
    }
  });

  it('falls back to Biolume for an unknown stored palette', () => {
    const storage = memoryStorage({ [THEME_STORAGE_KEY]: 'neon' });
    expect(readPreferences(storage).palette).toBe(DEFAULT_PALETTE);
  });

  it('survives a storage that throws', () => {
    const hostile: PreferenceStorage = {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('blocked');
      },
    };
    expect(readPreferences(hostile).palette).toBe(DEFAULT_PALETTE);
    expect(savePreferences(hostile, prefs({ palette: 'boiled' }))).toBe(false);
  });
});

describe('the palette renders every page after a reload', () => {
  it('renders each route in a restored palette, unchanged by the reload', () => {
    const storage = memoryStorage();
    createTheme(storage).setPalette('boiled');

    // Reload: no in-memory state survives, only storage.
    const restored = createTheme(storage);
    const reloadedPalette = get(restored);
    expect(reloadedPalette).toBe('boiled');
    expect(readPreferences(storage).palette).toBe(reloadedPalette);

    for (const route of ROUTES) {
      const rendered = renderComponent(App, {
        palette: reloadedPalette,
        path: route.path,
        preferences: readPreferences(storage),
      });
      expect(rendered.html).toContain(`data-palette="${reloadedPalette}"`);
      expect(rendered.html).toContain(`data-scheme="${PALETTES[reloadedPalette].scheme}"`);
      expect(rendered.html).toContain(
        `--tp-window-background: ${PALETTES[reloadedPalette].roles.windowBackground}`,
      );
      // The page itself is rendered, not only the shell.
      expect(rendered.text).toContain(route.label);
      expect(rendered.html).toContain(`data-palette-option="${reloadedPalette}" aria-pressed="true"`);
    }
  });

  it('renders the settings page in every palette, with the default marked', () => {
    for (const name of PALETTE_NAMES) {
      const rendered = renderComponent(App, { palette: name, path: '/settings' });
      expect(rendered.html).toContain(`data-palette="${name}"`);
      expect(rendered.html).toContain('data-settings-preferences="true"');
      expect(rendered.html).toContain(`data-palette-option="${name}" aria-pressed="true"`);
      expect(rendered.html).toContain('data-palette-option="biolume"');
      expect(rendered.text).toContain('default');
    }
  });
});

describe('the pool URL changes without a rebuild', () => {
  it('saves an absolute URL and a fresh read uses it', () => {
    const storage = memoryStorage();
    const outcome = applyPoolUrl(prefs(), '  https://other.example.com/  ');
    expect(outcome.ok).toBe(true);
    if (!outcome.ok) return;
    expect(outcome.pool.baseUrl).toBe('https://other.example.com');
    expect(savePreferences(storage, outcome.preferences)).toBe(true);

    // No rebuild: the same build reads the new value at startup and at runtime.
    expect(storage.getItem(POOL_URL_STORAGE_KEY)).toBe('https://other.example.com');
    expect(resolvePoolConfig({ env: {}, storage })).toEqual({
      baseUrl: 'https://other.example.com',
    });
    expect(readPreferences(storage).poolUrl).toBe('https://other.example.com');
  });

  it('refuses a relative or non-http URL in words', () => {
    for (const raw of ['pool.example.com', '/v1/hosts', 'ftp://pool.example.com', '   x  ']) {
      const outcome = applyPoolUrl(prefs(), raw);
      expect(outcome.ok).toBe(false);
      if (outcome.ok) continue;
      expect(outcome.reason).toBe(POOL_URL_INVALID_REASON);
    }
  });

  it('clears back to the deployment value on an empty URL', () => {
    const storage = memoryStorage({ [POOL_URL_STORAGE_KEY]: 'https://old.example.com' });
    const outcome = applyPoolUrl(prefs({ poolUrl: 'https://old.example.com' }), '   ');
    expect(outcome.ok).toBe(true);
    if (!outcome.ok) return;
    expect(outcome.preferences.poolUrl).toBeNull();
    savePreferences(storage, outcome.preferences);
    expect(storage.getItem(POOL_URL_STORAGE_KEY)).toBe('');
    expect(resolvePoolConfig({ env: {}, storage })).toEqual({ baseUrl: null });
  });

  it('keeps a build-time URL ahead of the saved one and the served origin last', () => {
    const storage = memoryStorage({ [POOL_URL_STORAGE_KEY]: 'https://saved.example.com' });
    expect(
      resolvePoolConfig({
        env: { VITE_POOL_URL: 'https://built.example.com' },
        storage,
        sameOrigin: true,
        origin: 'https://pool.example.com',
      }),
    ).toEqual({ baseUrl: 'https://built.example.com' });
    expect(
      resolvePoolConfig({ env: {}, storage, sameOrigin: true, origin: 'https://pool.example.com' }),
    ).toEqual({ baseUrl: 'https://saved.example.com' });
    expect(
      resolvePoolConfig({ env: {}, storage: null, sameOrigin: true, origin: 'https://pool.example.com' }),
    ).toEqual({ baseUrl: 'https://pool.example.com' });
    expect(resolvePoolConfig({ env: {}, storage: null, sameOrigin: false })).toEqual({
      baseUrl: null,
    });
  });
});

describe('clock and density preferences', () => {
  it('default to 24-hour local time and comfortable spacing', () => {
    const read = readPreferences(memoryStorage());
    expect(read.timeFormat).toBe('24h');
    expect(read.timeZone).toBe('local');
    expect(read.density).toBe('comfortable');
    expect(densityStyle('comfortable')).toBe('');
  });

  it('round-trip every option through storage', () => {
    const storage = memoryStorage();
    savePreferences(storage, prefs({ timeFormat: '12h', timeZone: 'utc', density: 'compact' }));

    expect(storage.getItem(TIME_FORMAT_STORAGE_KEY)).toBe('12h');
    expect(storage.getItem(TIME_ZONE_STORAGE_KEY)).toBe('utc');
    expect(storage.getItem(DENSITY_STORAGE_KEY)).toBe('compact');

    const reloaded = readPreferences(storage);
    expect(reloaded.timeFormat).toBe('12h');
    expect(reloaded.timeZone).toBe('utc');
    expect(reloaded.density).toBe('compact');
  });

  it('falls back on unrecognised stored options', () => {
    const storage = memoryStorage({
      [TIME_FORMAT_STORAGE_KEY]: 'martian',
      [TIME_ZONE_STORAGE_KEY]: 'europe/paris',
      [DENSITY_STORAGE_KEY]: 'roomy',
    });
    const read = readPreferences(storage);
    expect(read.timeFormat).toBe('24h');
    expect(read.timeZone).toBe('local');
    expect(read.density).toBe('comfortable');
  });

  it('formats the same instant for each clock preference', () => {
    const instant = '2026-10-29T14:05:09.000Z';
    expect(formatTime(instant, { timeFormat: '24h', timeZone: 'utc' })).toBe('14:05 UTC');
    expect(formatTime(instant, { timeFormat: '12h', timeZone: 'utc' })).toBe('2:05 PM UTC');
    expect(
      formatTime(instant, { timeFormat: '12h', timeZone: 'utc' }, { seconds: true, date: true }),
    ).toBe('2026-10-29 2:05:09 PM UTC');
    expect(formatTime('not a date', { timeFormat: '24h', timeZone: 'utc' })).toBe('not a date');
    expect(timeFormatLabel('12h')).toContain('12-hour');
  });

  it('turns comfortable spacing into a compact override on the shell', () => {
    expect(densityStyle('compact')).toContain('--tp-space-1: 3px');
    expect(densityStyle('compact')).toContain('--tp-space-8: 48px');

    const compact = renderComponent(App, {
      path: '/hosts',
      preferences: prefs({ density: 'compact' }),
    });
    expect(compact.html).toContain('data-density="compact"');
    expect(compact.html).toContain('--tp-space-1: 3px');
    expect(compact.text).toContain('Machines enrolled with this pool');

    const comfortable = renderComponent(App, {
      path: '/hosts',
      preferences: prefs({ density: 'comfortable' }),
    });
    expect(comfortable.html).toContain('data-density="comfortable"');
    expect(comfortable.html).not.toContain('--tp-space-1: 3px');
  });
});

describe('the panel and its about block', () => {
  it('shows every preference, the effective pool and both versions', () => {
    const rendered = renderComponent(PreferencesPanel, {
      preferences: prefs({ palette: 'sea-glass', timeFormat: '12h', density: 'compact' }),
      effectivePoolUrl: 'https://pool.example.com',
      servedByPool: true,
    });

    expect(rendered.html).toContain('data-prefs-group="pool"');
    expect(rendered.html).toContain('data-effective-pool-url');
    expect(rendered.text).toContain('https://pool.example.com');
    expect(rendered.html).toContain('data-prefs-group="palette"');
    expect(rendered.html.match(/data-palette-option=/g)).toHaveLength(5);
    expect(rendered.html).toContain('data-palette-option="sea-glass" aria-pressed="true"');
    expect(rendered.html).toContain('data-time-format-option="12h"');
    expect(rendered.html).toContain('data-time-zone-option="utc"');
    expect(rendered.html).toContain('data-density-option="compact"');
    expect(rendered.html).toContain('data-control-plane-version');
    expect(rendered.text).toContain(CONTROL_PLANE_VERSION);
    expect(rendered.text).toContain(HUB_CONTRACT_VERSION);
    expect(rendered.text).toContain('Served by');
    expect(rendered.text).toContain('no CORS');
  });

  it('reports the version from package.json', () => {
    expect(CONTROL_PLANE_VERSION).toBe(packageJson.version);
  });

  it('shows a rejected URL in words and a saved one as a status', () => {
    const rejected = renderComponent(PreferencesPanel, {
      preferences: prefs(),
      poolUrlProblem: POOL_URL_INVALID_REASON,
    });
    expect(rejected.html).toContain('data-pool-url-problem="true"');
    expect(rejected.text).toContain(POOL_URL_INVALID_REASON);

    const saved = renderComponent(PreferencesPanel, {
      preferences: prefs(),
      poolUrlNotice: 'Pool URL saved.',
    });
    expect(saved.html).not.toContain('data-pool-url-notice="true"');
    // The notice appears only after a submit; a fresh render has not submitted.
    expect(saved.text).not.toContain('Pool URL saved.');
  });

  it('says the standalone copy and the pool-served copy are the same app', () => {
    const standalone = renderComponent(PreferencesPanel, { preferences: prefs() });
    expect(standalone.html).toContain('data-served-by="standalone"');
    expect(standalone.text).toContain('same built app');
  });
});

describe('the palette name type is the five Clawffice palettes', () => {
  it('offers exactly the design system list', () => {
    const names: PaletteName[] = [...PALETTE_NAMES];
    expect(names).toHaveLength(5);
    expect(DEFAULT_PALETTE).toBe('biolume');
  });
});

describe('the settings page exposes the preferences in the shell', () => {
  it('renders the pool form and the palette with no pool, so first run can set one', () => {
    const rendered = renderComponent(App, { path: '/settings', auth: null });
    expect(rendered.html).toContain('data-settings-preferences="true"');
    expect(rendered.html).toContain('data-pool-url-input="true"');
    expect(rendered.html).toContain('Save pool URL');
    // Two switches: the shell's header and the settings panel's.
    expect(rendered.html.match(/data-palette-option=/g)?.length).toBeGreaterThanOrEqual(5);
    // Members and keys still say the pool is missing rather than showing empties.
    expect(rendered.html).toContain('data-settings-state="unconfigured"');
  });

  it('shows the saved pool URL and the served-from state', () => {
    const rendered = renderComponent(App, {
      path: '/settings',
      pool: { baseUrl: 'https://saved.example.com' },
      servedByPool: true,
      preferences: prefs({ poolUrl: 'https://saved.example.com', density: 'compact' }),
    });
    expect(rendered.html).toContain('value="https://saved.example.com"');
    expect(rendered.text).toContain('https://saved.example.com');
    expect(rendered.html).toContain('data-served-by="pool"');
    expect(rendered.html).toContain('data-density="compact"');
  });
});

describe('runtime pool URL from /config.json', () => {
  const reply = (body: string, status = 200): typeof fetch =>
    (async () => new Response(body, { status })) as unknown as typeof fetch;

  it('wins over the saved and build-time URLs and is not stored', async () => {
    const storage = memoryStorage({ [POOL_URL_STORAGE_KEY]: 'https://saved.example.com' });
    const runtime = await loadRuntimePoolUrl(reply('{"poolUrl":"https://run.example.com"}'));
    expect(runtime).toBe('https://run.example.com');
    expect(
      resolvePoolConfig({ runtime, env: { VITE_POOL_URL: 'https://built.example.com' }, storage }),
    ).toEqual({ baseUrl: 'https://run.example.com' });
    expect(storage.getItem(POOL_URL_STORAGE_KEY)).toBe('https://saved.example.com');
  });

  it('falls through on a missing file, an empty value or an empty object', async () => {
    const storage = memoryStorage({ [POOL_URL_STORAGE_KEY]: 'https://saved.example.com' });
    for (const fetchImpl of [reply('', 404), reply('{"poolUrl":""}'), reply('{}')]) {
      const runtime = await loadRuntimePoolUrl(fetchImpl);
      expect(runtime).toBeNull();
      expect(resolvePoolConfig({ runtime, env: {}, storage })).toEqual({
        baseUrl: 'https://saved.example.com',
      });
    }
  });

  it('falls through on invalid JSON or a failed request', async () => {
    expect(await loadRuntimePoolUrl(reply('<html>spa fallback</html>'))).toBeNull();
    expect(
      await loadRuntimePoolUrl((async () => {
        throw new Error('offline');
      }) as unknown as typeof fetch),
    ).toBeNull();
  });

  it('rejects a non-http(s) poolUrl and falls through', async () => {
    const storage = memoryStorage({ [POOL_URL_STORAGE_KEY]: 'https://saved.example.com' });
    for (const bad of ['ftp://pool.example.com', 'javascript:alert(1)', 'pool.example.com', 42]) {
      const runtime = await loadRuntimePoolUrl(reply(JSON.stringify({ poolUrl: bad })));
      expect(runtime).toBeNull();
      expect(resolvePoolConfig({ runtime, env: {}, storage })).toEqual({
        baseUrl: 'https://saved.example.com',
      });
    }
  });
});
