/**
 * The settings page's model: everything a person can set, and the rules for
 * remembering it.
 *
 * The preferences live in the browser's storage, not in a build: changing the
 * pool URL or the palette writes a key and the running app reacts, so a
 * deployment does not need a rebuild to point somewhere else. Storage is
 * injected so every rule is testable without a browser.
 *
 * The three rules this module owns:
 *
 * - Biolume is the default palette and an unknown stored value falls back to it;
 * - a pool URL must be an absolute `http`/`https` origin, and an empty value
 *   means "use whatever this deployment was told", not "broken";
 * - a stale or unreadable storage never leaves the app unstyled or unconfigured.
 */

import pkg from '../../../package.json';
import { CORE, DEFAULT_PALETTE, isPaletteName, type PaletteName } from '$lib/design/tokens';
import { THEME_STORAGE_KEY } from '$lib/theme';
import { POOL_URL_STORAGE_KEY, normalisePoolUrl, type PoolConfig } from '$lib/api/pool';

/** The control plane's own version, from `package.json`. */
export const CONTROL_PLANE_VERSION: string = pkg.version;

/**
 * The hub contract this build speaks.
 *
 * Kept beside the app version so the about panel can show both: the pool may be
 * older or newer, and a person comparing the two numbers knows which mismatch
 * they have. Matches `CONTRACT_VERSION` in `src/api/openapi.rs`.
 */
export const HUB_CONTRACT_VERSION = '1.0.0';

/** Storage key for the 12/24-hour choice. */
export const TIME_FORMAT_STORAGE_KEY = 'tidepool.control-plane.time-format';

/** Storage key for the local/UTC choice. */
export const TIME_ZONE_STORAGE_KEY = 'tidepool.control-plane.time-zone';

/** Storage key for the comfortable/compact choice. */
export const DENSITY_STORAGE_KEY = 'tidepool.control-plane.density';

/** The time formats offered. */
export const TIME_FORMATS = ['24h', '12h'] as const;

/** A clock preference. */
export type TimeFormat = (typeof TIME_FORMATS)[number];

/** The time zones offered. */
export const TIME_ZONES = ['local', 'utc'] as const;

/** A time-zone preference. */
export type TimeZonePreference = (typeof TIME_ZONES)[number];

/** The densities offered. */
export const DENSITIES = ['comfortable', 'compact'] as const;

/** A density preference. */
export type Density = (typeof DENSITIES)[number];

/** Everything the settings page remembers for this browser. */
export interface ControlPlanePreferences {
  /** A saved pool URL, or null to use the deployment's own value. */
  poolUrl: string | null;
  /** The Clawffice palette to render. */
  palette: PaletteName;
  /** Twelve- or twenty-four-hour clock. */
  timeFormat: TimeFormat;
  /** Local time or UTC. */
  timeZone: TimeZonePreference;
  /** Comfortable spacing or compact. */
  density: Density;
}

/** The subset of `Storage` the preferences need. */
export interface PreferenceStorage {
  /** Read a saved value. */
  getItem(key: string): string | null;
  /** Write a saved value. */
  setItem(key: string, value: string): void;
}

/** The defaults: Biolume, 24-hour local time and comfortable spacing. */
export const DEFAULT_PREFERENCES: ControlPlanePreferences = Object.freeze({
  poolUrl: null,
  palette: DEFAULT_PALETTE,
  timeFormat: '24h',
  timeZone: 'local',
  density: 'comfortable',
});

/** The sentence shown when a typed pool URL is not one. */
export const POOL_URL_INVALID_REASON =
  'Enter an absolute pool URL such as https://pool.example.com — scheme included, no path beyond the pool.';

/** The sentence shown when saving worked. */
export const POOL_URL_SAVED_NOTICE = 'Pool URL saved. The app now uses it without a rebuild.';

/** Read one value, falling back to a default for anything unrecognised. */
function pick<T extends string>(value: string | null, allowed: readonly T[], fallback: T): T {
  return value !== null && (allowed as readonly string[]).includes(value)
    ? (value as T)
    : fallback;
}

/** Read a saved value without letting a blocked storage break the page. */
function read(storage: PreferenceStorage | null, key: string): string | null {
  if (!storage) return null;
  try {
    return storage.getItem(key);
  } catch {
    return null;
  }
}

/**
 * Read every preference, restoring the palette from the same key the theme
 * store uses so a reload and the theme store never disagree.
 */
export function readPreferences(
  storage: PreferenceStorage | null,
  defaults: ControlPlanePreferences = DEFAULT_PREFERENCES,
): ControlPlanePreferences {
  const savedPalette = read(storage, THEME_STORAGE_KEY);
  return {
    poolUrl: normalisePoolUrl(read(storage, POOL_URL_STORAGE_KEY)),
    palette: isPaletteName(savedPalette) ? savedPalette : defaults.palette,
    timeFormat: pick(read(storage, TIME_FORMAT_STORAGE_KEY), TIME_FORMATS, defaults.timeFormat),
    timeZone: pick(read(storage, TIME_ZONE_STORAGE_KEY), TIME_ZONES, defaults.timeZone),
    density: pick(read(storage, DENSITY_STORAGE_KEY), DENSITIES, defaults.density),
  };
}

/**
 * Write every preference.
 *
 * A blocked or full storage is reported as `false` rather than thrown: the app
 * still renders the change, it just cannot remember it. A null pool URL is
 * written as an empty value so clearing it beats a stale saved one.
 */
export function savePreferences(
  storage: PreferenceStorage | null,
  preferences: ControlPlanePreferences,
): boolean {
  if (!storage) return false;
  try {
    storage.setItem(POOL_URL_STORAGE_KEY, preferences.poolUrl ?? '');
    storage.setItem(THEME_STORAGE_KEY, preferences.palette);
    storage.setItem(TIME_FORMAT_STORAGE_KEY, preferences.timeFormat);
    storage.setItem(TIME_ZONE_STORAGE_KEY, preferences.timeZone);
    storage.setItem(DENSITY_STORAGE_KEY, preferences.density);
    return true;
  } catch {
    return false;
  }
}

/** The result of applying a typed pool URL. */
export type PoolUrlOutcome =
  | { ok: true; preferences: ControlPlanePreferences; pool: PoolConfig }
  | { ok: false; reason: string };

/**
 * Apply a typed pool URL.
 *
 * An empty value clears the saved URL, which returns the app to whatever the
 * deployment provides (a baked-in `VITE_POOL_URL` or, when the pool serves the
 * app itself, the pool's own origin). Anything else must be absolute http(s).
 */
export function applyPoolUrl(preferences: ControlPlanePreferences, raw: string): PoolUrlOutcome {
  const trimmed = raw.trim();
  if (trimmed === '') {
    return {
      ok: true,
      preferences: { ...preferences, poolUrl: null },
      pool: { baseUrl: null },
    };
  }
  const normalised = normalisePoolUrl(trimmed);
  if (!normalised) {
    return { ok: false, reason: POOL_URL_INVALID_REASON };
  }
  return {
    ok: true,
    preferences: { ...preferences, poolUrl: normalised },
    pool: { baseUrl: normalised },
  };
}

/**
 * Resolve the pool this build talks to.
 *
 * Order: a build-time `VITE_POOL_URL`, then the URL saved here, then — only when
 * the page says it was served by a pool — that same origin. The last step is
 * what lets a self-hosted pool serve its own control plane with no build-time
 * URL and no CORS, while a standalone deployment with no marker stays
 * unconfigured instead of calling its static host.
 */
export function resolvePoolConfig(sources: {
  env?: Record<string, string | undefined>;
  storage?: PreferenceStorage | null;
  sameOrigin?: boolean;
  origin?: string | null;
}): PoolConfig {
  const configured = normalisePoolUrl(sources.env?.['VITE_POOL_URL']);
  if (configured) return { baseUrl: configured };
  const saved = normalisePoolUrl(read(sources.storage ?? null, POOL_URL_STORAGE_KEY));
  if (saved) return { baseUrl: saved };
  if (!sources.sameOrigin) return { baseUrl: null };
  return { baseUrl: normalisePoolUrl(sources.origin ?? null) };
}

/** True when the page was served by the pool itself, per its meta marker. */
export function isServedByPool(markerValue: string | null | undefined): boolean {
  return markerValue === 'same-origin';
}

/** The CSS custom properties that turn comfortable spacing into compact. */
export function densityStyle(density: Density): string {
  if (density === 'comfortable') return '';
  return CORE.space
    .map((value, index) => `--tp-space-${index + 1}: ${Math.max(2, Math.round(value * 0.75))}px`)
    .join('; ');
}

/** The label a density option shows. */
export function densityLabel(density: Density): string {
  return density === 'compact' ? 'Compact' : 'Comfortable';
}

/** The label a time format shows. */
export function timeFormatLabel(format: TimeFormat): string {
  return format === '12h' ? '12-hour (2:05 PM)' : '24-hour (14:05)';
}

/** The label a time zone shows. */
export function timeZoneLabel(zone: TimeZonePreference): string {
  return zone === 'utc' ? 'UTC' : 'Local time';
}

/**
 * Format an instant for display using the saved preferences.
 *
 * The formatter is explicit rather than `Intl` so the same instant renders the
 * same way in every browser and in a test. An unparseable value is returned
 * unchanged: the pool's own text is better than "Invalid Date".
 */
export function formatTime(
  iso: string,
  preferences: Pick<ControlPlanePreferences, 'timeFormat' | 'timeZone'>,
  options: { seconds?: boolean; date?: boolean } = {},
): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;
  const utc = preferences.timeZone === 'utc';
  const hours24 = utc ? at.getUTCHours() : at.getHours();
  const minutes = pad(utc ? at.getUTCMinutes() : at.getMinutes());
  const seconds = pad(utc ? at.getUTCSeconds() : at.getSeconds());
  const clock12 = preferences.timeFormat === '12h';
  const hourText = clock12
    ? String(hours24 % 12 === 0 ? 12 : hours24 % 12)
    : pad(hours24);
  const withSeconds = options.seconds ? `${hourText}:${minutes}:${seconds}` : `${hourText}:${minutes}`;
  const suffix = clock12 ? (hours24 < 12 ? ' AM' : ' PM') : '';
  const time = utc ? `${withSeconds}${suffix} UTC` : `${withSeconds}${suffix}`;
  if (!options.date) return time;
  const year = utc ? at.getUTCFullYear() : at.getFullYear();
  const month = pad((utc ? at.getUTCMonth() : at.getMonth()) + 1);
  const day = pad(utc ? at.getUTCDate() : at.getDate());
  return `${year}-${month}-${day} ${time}`;
}

/** Zero-pad a clock field. */
function pad(value: number): string {
  return String(value).padStart(2, '0');
}
