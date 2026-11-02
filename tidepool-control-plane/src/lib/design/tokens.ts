/**
 * clawffice-design tokens for the Tidepool control plane.
 *
 * The design system is the source of truth; this module reads the pinned
 * snapshot at `clawffice-design.snapshot.json` and turns it into the CSS custom
 * properties every component reads. `design.drift.test.ts` fails when the
 * snapshot and a sibling `clawffice-design` checkout disagree, and
 * `npm run design:snapshot` regenerates it.
 *
 * Suite rule 8: state is never colour alone. These tokens provide the colours;
 * `$lib/status` provides the icon and the word that go beside them.
 */

import snapshot from './clawffice-design.snapshot.json';

/** The five Clawffice palettes, in presentation order. */
export const PALETTE_NAMES = ['biolume', 'deep-tide', 'sea-glass', 'phosphor', 'boiled'] as const;

/** A palette the control plane can render. */
export type PaletteName = (typeof PALETTE_NAMES)[number];

/** A palette's native light/dark scheme. */
export type Scheme = 'light' | 'dark';

/** Biolume is the suite default and therefore this app's default. */
export const DEFAULT_PALETTE: PaletteName = 'biolume';

/** The semantic colour roles a palette must fill. */
export interface PaletteRoles {
  /** Behind the window. */
  windowBackground: string;
  /** Raised panels, cards and popovers. */
  surface: string;
  /** The darkest ground, used behind code and terminals. */
  terminalBackground: string;
  /** Hairline separation. */
  border: string;
  /** Emphasised separation and inactive control outlines. */
  borderStrong: string;
  /** Body text. */
  text: string;
  /** Secondary text; contrast-checked at 4.6:1 or better. */
  textMuted: string;
  /** Titles and emphasis. */
  textStrong: string;
  /** The palette's primary action colour. */
  primary: string;
  /** Text drawn on `primary`. */
  primaryText: string;
  /** The palette's secondary accent. */
  second: string;
}

/** The status colours a palette must fill. Always paired with an icon and a word. */
export interface StatusColors {
  /** Completed, connected. */
  success: string;
  /** Needs attention, degraded. */
  warning: string;
  /** Failed, revoked, refused. */
  danger: string;
  /** Neutral information. */
  info: string;
}

/** One palette, as rendered by this app. */
export interface Palette {
  /** Stable id, matching the token file name in `clawffice-design`. */
  name: PaletteName;
  /** Human-readable name from the token file. */
  displayName: string;
  /** The palette's native scheme. */
  scheme: Scheme;
  /** One-line description from the token file. */
  summary: string;
  /** Semantic colour roles. */
  roles: PaletteRoles;
  /** Status colours. */
  status: StatusColors;
}

/** Tidepool's product hue per scheme, from `tokens/apps/tidepool.json`. */
export const PRODUCT_HUE: Readonly<Record<Scheme, string>> = {
  dark: snapshot.app.color.product.dark,
  light: snapshot.app.color.product.light,
};

/** Shared geometry and type scale, from `tokens/core.json`. */
export const CORE = {
  /** Space scale, ascending. */
  space: snapshot.core.space,
  /** Corner radii by purpose. */
  radius: snapshot.core.radius,
  /** Font sizes by purpose. */
  fontSize: snapshot.core.font.size,
  /** Motion timings. */
  motion: snapshot.core.motion,
  /** The 12-cell sprite grid. */
  cellGrid: snapshot.core.logo['cell-grid'],
} as const;

type RawPalette = (typeof snapshot.palettes)[PaletteName];

/** Convert a token file into a `Palette`. */
function fromTokenFile(name: PaletteName, raw: RawPalette): Palette {
  return {
    name,
    displayName: raw.name,
    scheme: raw.scheme === 'light' ? 'light' : 'dark',
    summary: raw.summary,
    roles: {
      windowBackground: raw.role['window-background'],
      surface: raw.role.surface,
      terminalBackground: raw.role['terminal-background'],
      border: raw.role.border,
      borderStrong: raw.role['border-strong'],
      text: raw.role.text,
      textMuted: raw.role['text-muted'],
      textStrong: raw.role['text-strong'],
      primary: raw.role.primary,
      primaryText: raw.role['primary-text'],
      second: raw.role.second,
    },
    status: {
      success: raw['status-color'].success,
      warning: raw['status-color'].warning,
      danger: raw['status-color'].danger,
      info: raw['status-color'].info,
    },
  };
}

/** Every palette, keyed by name. */
export const PALETTES: Readonly<Record<PaletteName, Palette>> = Object.freeze({
  biolume: fromTokenFile('biolume', snapshot.palettes.biolume),
  'deep-tide': fromTokenFile('deep-tide', snapshot.palettes['deep-tide']),
  'sea-glass': fromTokenFile('sea-glass', snapshot.palettes['sea-glass']),
  phosphor: fromTokenFile('phosphor', snapshot.palettes.phosphor),
  boiled: fromTokenFile('boiled', snapshot.palettes.boiled),
} satisfies Record<PaletteName, Palette>);

/** True when `value` names a known palette. */
export function isPaletteName(value: unknown): value is PaletteName {
  return typeof value === 'string' && (PALETTE_NAMES as readonly string[]).includes(value);
}

/** Look a palette up by name, falling back to Biolume rather than throwing. */
export function paletteOf(name: string | null | undefined): Palette {
  return isPaletteName(name) ? PALETTES[name] : PALETTES[DEFAULT_PALETTE];
}

/** The scheme a palette renders in. */
export function schemeOf(name: PaletteName): Scheme {
  return PALETTES[name].scheme;
}

/** The tide product hue for a scheme: it drives focus and selection. */
export function productHue(scheme: Scheme): string {
  return PRODUCT_HUE[scheme];
}

/** `#rrggbb` to `rgba(r, g, b, alpha)`; used for the selection tint. */
export function withAlpha(hex: string, alpha: number): string {
  const value = hex.replace('#', '');
  const red = Number.parseInt(value.slice(0, 2), 16);
  const green = Number.parseInt(value.slice(2, 4), 16);
  const blue = Number.parseInt(value.slice(4, 6), 16);
  return `rgba(${red}, ${green}, ${blue}, ${alpha})`;
}

/**
 * The CSS custom properties for a palette.
 *
 * Focus and selection come from Tidepool's product hue (`tide`), not from the
 * palette's primary colour, so the app keeps its identity across all five.
 */
export function themeVars(name: PaletteName): Record<string, string> {
  const palette = PALETTES[name];
  const hue = productHue(palette.scheme);
  const vars: Record<string, string> = {
    '--tp-window-background': palette.roles.windowBackground,
    '--tp-surface': palette.roles.surface,
    '--tp-terminal-background': palette.roles.terminalBackground,
    '--tp-border': palette.roles.border,
    '--tp-border-strong': palette.roles.borderStrong,
    '--tp-text': palette.roles.text,
    '--tp-text-muted': palette.roles.textMuted,
    '--tp-text-strong': palette.roles.textStrong,
    '--tp-primary': palette.roles.primary,
    '--tp-primary-text': palette.roles.primaryText,
    '--tp-second': palette.roles.second,
    '--tp-status-success': palette.status.success,
    '--tp-status-warning': palette.status.warning,
    '--tp-status-danger': palette.status.danger,
    '--tp-status-info': palette.status.info,
    '--tp-focus': hue,
    '--tp-selection': withAlpha(hue, 0.35),
    '--tp-selection-text': palette.roles.textStrong,
    '--tp-color-scheme': palette.scheme,
    '--tp-cell-grid': String(CORE.cellGrid),
  };
  const radius: Record<string, string> = {
    cell: String(CORE.radius.cell),
    chip: String(CORE.radius.chip),
    control: String(CORE.radius.control),
    panel: String(CORE.radius.panel),
  };
  for (const [key, value] of Object.entries(radius)) vars[`--tp-radius-${key}`] = `${value}px`;
  const sizes: Record<string, number> = { ...CORE.fontSize };
  for (const [key, value] of Object.entries(sizes)) vars[`--tp-font-${key}`] = `${value}px`;
  CORE.space.forEach((value, index) => {
    vars[`--tp-space-${index + 1}`] = `${value}px`;
  });
  vars['--tp-motion-hover'] = `${CORE.motion['hover-ms']}ms`;
  vars['--tp-motion-pane'] = `${CORE.motion['pane-ms']}ms`;
  vars['--tp-easing'] = CORE.motion.easing;
  return vars;
}

/** The CSS custom properties for a palette, serialised for a `style` attribute. */
export function themeStyle(name: PaletteName): string {
  return Object.entries(themeVars(name))
    .map(([property, value]) => `${property}: ${value}`)
    .join('; ');
}
