/**
 * WCAG AA contrast maths for the control plane's palette colours, and the
 * audit that keeps every rendered palette legible.
 *
 * The clawffice-design palettes are the source of truth for hue and identity,
 * but a design token is not automatically readable. `border-strong`, the
 * outline of every input, select and button, sat between 1.7:1 and 2.7:1
 * against the panel grounds: the controls had no visible edge. Rather than
 * edit the design system, `$lib/design/tokens` shades a failing role toward
 * `text-strong` just far enough to clear its floor and leaves a passing role
 * exactly as designed.
 *
 * Floors (WCAG 2.2, AA):
 * - body text: 4.5:1 (SC 1.4.3)
 * - controls, focus indicators and large text: 3:1 (SC 1.4.11, SC 1.4.3)
 *
 * Everything here is pure and DOM-free, so the audit runs in the test suite
 * against the CSS custom properties the app actually ships, rather than
 * needing a rendered page.
 */

/** Body text floor: 4.5:1. */
export const BODY_TEXT_MIN = 4.5;

/** Control, focus-ring and large-text floor: 3:1. */
export const CONTROL_MIN = 3;

/** How much of the product hue a text selection is tinted with. */
export const SELECTION_TINT = 0.35;

/** A colour written as `#rrggbb`. */
const HEX_PATTERN = /^#[0-9A-Fa-f]{6}$/;

/** The sRGB channels of a `#rrggbb` colour, or null when the value is not one. */
function channels(hex: string): [number, number, number] | null {
  if (!HEX_PATTERN.test(hex)) return null;
  return [
    Number.parseInt(hex.slice(1, 3), 16),
    Number.parseInt(hex.slice(3, 5), 16),
    Number.parseInt(hex.slice(5, 7), 16),
  ];
}

/** The WCAG relative luminance of a `#rrggbb` colour. */
export function relativeLuminance(hex: string): number {
  const rgb = channels(hex);
  if (!rgb) return Number.NaN;
  const linear = rgb.map((value) => {
    const scaled = value / 255;
    return scaled <= 0.04045 ? scaled / 12.92 : ((scaled + 0.055) / 1.055) ** 2.4;
  });
  const [red, green, blue] = linear as [number, number, number];
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

/**
 * The WCAG contrast ratio between two colours, from 1:1 to 21:1.
 *
 * An unparseable colour yields NaN, so a comparison against it is false and a
 * caller cannot mistake "could not check" for "readable".
 */
export function contrastRatio(first: string, second: string): number {
  const one = relativeLuminance(first);
  const two = relativeLuminance(second);
  if (Number.isNaN(one) || Number.isNaN(two)) return Number.NaN;
  const lighter = Math.max(one, two);
  const darker = Math.min(one, two);
  return (lighter + 0.05) / (darker + 0.05);
}

/** True when `foreground` reaches `minimum` against every background. */
export function meetsContrast(
  foreground: string,
  backgrounds: readonly string[],
  minimum: number,
): boolean {
  return backgrounds.every((background) => contrastRatio(foreground, background) >= minimum);
}

/**
 * Mix two colours, `weight` of the way from `from` to `to`.
 *
 * Both must be `#rrggbb`; a value that cannot be parsed is returned unchanged
 * so the caller keeps whatever it had rather than gaining a surprise colour.
 */
export function blend(from: string, to: string, weight: number): string {
  const start = channels(from);
  const end = channels(to);
  if (!start || !end) return from;
  const clamped = Math.min(1, Math.max(0, weight));
  const mixed = [0, 1, 2].map(
    (index) => (start[index] as number) + ((end[index] as number) - (start[index] as number)) * clamped,
  );
  return `#${mixed.map((value) => Math.round(value).toString(16).padStart(2, '0')).join('')}`.toUpperCase();
}

/** Paint a translucent colour over an opaque one, as the browser would. */
export function composite(overlay: string, alpha: number, ground: string): string {
  return blend(ground, overlay, alpha);
}

/**
 * The extreme a failing colour is shaded toward when the caller names none.
 *
 * White and black are the two ends of the scale, so whichever has more headroom
 * against the darkest level in play gives the shorter correction.
 */
function defaultAnchor(backgrounds: readonly string[]): string {
  const worstWhite = Math.min(...backgrounds.map((ground) => contrastRatio('#FFFFFF', ground)));
  const worstBlack = Math.min(...backgrounds.map((ground) => contrastRatio('#000000', ground)));
  return worstWhite >= worstBlack ? '#FFFFFF' : '#000000';
}

/**
 * A colour that reaches `minimum` against every background.
 *
 * The colour is returned unchanged when it already passes, so a legible palette
 * keeps its exact design-system values. When it fails it is mixed toward
 * `toward` in 1% steps and the first passing value is returned: the smallest
 * change that clears the floor. With no `toward`, or one equal to the colour
 * itself, white or black is used, whichever needs the shorter move.
 */
export function readableColor(
  color: string,
  backgrounds: readonly string[],
  minimum: number,
  toward?: string,
): string {
  if (!channels(color) || backgrounds.length === 0) return color;
  if (meetsContrast(color, backgrounds, minimum)) return color;
  const named = toward && channels(toward) ? toward.toUpperCase() : null;
  const anchor =
    named !== null && named !== color.toUpperCase() ? named : defaultAnchor(backgrounds);
  for (let step = 1; step <= 100; step += 1) {
    const candidate = blend(color, anchor, step / 100);
    if (meetsContrast(candidate, backgrounds, minimum)) return candidate;
  }
  return anchor;
}

/**
 * One colour the audit checked.
 *
 * `ratio` is the worst ratio found against any of its grounds, because that is
 * the one that decides whether the colour is readable everywhere it is used.
 */
export interface ContrastFinding {
  /** The CSS custom property that carries the colour. */
  variable: string;
  /** The colour actually rendered, as `#rrggbb`. */
  color: string;
  /** The floor this property must clear. */
  minimum: number;
  /** The worst ratio against the property's grounds. */
  ratio: number;
  /** True when `ratio` reaches `minimum`. */
  ok: boolean;
}

/** The three opaque grounds a palette paints content on. */
const GROUNDS = ['--tp-window-background', '--tp-surface', '--tp-terminal-background'] as const;

/** Roles read as body text, so they take body measurements. */
const TEXT_VARIABLES = [
  '--tp-text',
  '--tp-text-muted',
  '--tp-text-strong',
  '--tp-primary',
  '--tp-second',
] as const;

/** Roles that draw a control edge or a focus ring. */
const CONTROL_VARIABLES = ['--tp-border-strong'] as const;

/** Status colours, which the app also prints as words on many pages. */
const STATUS_VARIABLES = [
  '--tp-status-success',
  '--tp-status-warning',
  '--tp-status-danger',
  '--tp-status-info',
] as const;

/**
 * Audit a palette's rendered CSS custom properties against WCAG AA.
 *
 * It reads the variables the app actually ships, not the design tokens behind
 * them, so a value repaired on the way out is checked as repaired. `--tp-border`
 * is deliberately absent: a hairline that separates two panels is decoration,
 * not a control boundary, and WCAG sets no ratio for it.
 *
 * The returned list is ordered so a failure names the property, its colour and
 * the worst ground ratio in one line.
 */
export function contrastReport(
  vars: Readonly<Record<string, string>>,
  tint: number = SELECTION_TINT,
): ContrastFinding[] {
  const grounds = GROUNDS.map((variable) => vars[variable] ?? '');
  const findings: ContrastFinding[] = [];

  /** Record one property against its grounds. */
  const check = (variable: string, minimum: number, against: readonly string[]): void => {
    const color = vars[variable] ?? '';
    const ratios = against.map((ground) => contrastRatio(color, ground));
    const ratio = ratios.length === 0 ? Number.NaN : Math.min(...ratios);
    findings.push({ variable, color, minimum, ratio, ok: ratio >= minimum });
  };

  for (const variable of TEXT_VARIABLES) check(variable, BODY_TEXT_MIN, grounds);
  for (const variable of CONTROL_VARIABLES) check(variable, CONTROL_MIN, grounds);
  for (const variable of STATUS_VARIABLES) check(variable, BODY_TEXT_MIN, grounds);
  check('--tp-focus', CONTROL_MIN, grounds);
  check('--tp-primary-text', BODY_TEXT_MIN, [vars['--tp-primary'] ?? '']);

  // A selection is a translucent tint of the product hue, so selected text is
  // measured against each ground as painted, not against the bare canvas.
  const hue = vars['--tp-focus'] ?? '';
  const selectedGrounds = grounds.map((ground) => composite(hue, tint, ground));
  check('--tp-selection-text', BODY_TEXT_MIN, selectedGrounds);

  return findings;
}
