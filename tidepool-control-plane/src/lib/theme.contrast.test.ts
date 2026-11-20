/**
 * The palette contrast audit.
 *
 * Task 2.4.5: every text and control colour must clear WCAG AA in every
 * palette, light and dark. These tests read the CSS custom properties the app
 * actually ships (`themeVars`), so a regression in a design token, in the
 * repair step or in the token plumbing fails here rather than shipping as an
 * unreadable control.
 *
 * The audits are driven by `contrastReport`, which is also what the app's own
 * module documents, so the tests and the repair share one definition of the
 * floors.
 */

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { PALETTES, PALETTE_NAMES, PRODUCT_HUE, themeVars } from './design/tokens';
import {
  BODY_TEXT_MIN,
  CONTROL_MIN,
  SELECTION_TINT,
  contrastRatio,
  contrastReport,
  meetsContrast,
  readableColor,
  relativeLuminance,
  blend,
  composite,
} from './theme.contrast';

/** Every palette's shipped variables, paired with its name. */
const SHIPPED = PALETTE_NAMES.map((name) => ({ name, vars: themeVars(name) }));

describe('WCAG contrast maths', () => {
  it('measures luminance at the two extremes', () => {
    expect(relativeLuminance('#000000')).toBeCloseTo(0, 5);
    expect(relativeLuminance('#FFFFFF')).toBeCloseTo(1, 5);
  });

  it('gives the canonical ratios black and white are known by', () => {
    expect(contrastRatio('#000000', '#FFFFFF')).toBeCloseTo(21, 2);
    expect(contrastRatio('#FFFFFF', '#FFFFFF')).toBeCloseTo(1, 5);
    // The order of the arguments must not matter.
    expect(contrastRatio('#FFFFFF', '#000000')).toBeCloseTo(21, 2);
  });

  it('matches the published ratio for a mid grey on white', () => {
    // #767676 is the classic "smallest pass on white" example (4.54:1).
    expect(contrastRatio('#767676', '#FFFFFF')).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio('#777777', '#FFFFFF')).toBeLessThan(4.54);
  });

  it('reports NaN for a colour it cannot parse rather than guessing', () => {
    expect(Number.isNaN(contrastRatio('not-a-colour', '#FFFFFF'))).toBe(true);
    expect(Number.isNaN(relativeLuminance('#FFF'))).toBe(true);
  });

  it('checks a foreground against every background it is drawn on', () => {
    expect(meetsContrast('#FFFFFF', ['#000000', '#101010'], 4.5)).toBe(true);
    expect(meetsContrast('#FFFFFF', ['#000000', '#FFFFFF'], 4.5)).toBe(false);
    expect(meetsContrast('#FFFFFF', [], 4.5)).toBe(true);
  });

  it('blends and composites within the two inputs', () => {
    expect(blend('#000000', '#FFFFFF', 0)).toBe('#000000');
    expect(blend('#000000', '#FFFFFF', 1)).toBe('#FFFFFF');
    expect(blend('#000000', '#FFFFFF', 0.5)).toBe('#808080');
    // A fully opaque composite is the overlay; a fully transparent one the ground.
    expect(composite('#FFFFFF', 1, '#000000')).toBe('#FFFFFF');
    expect(composite('#FFFFFF', 0, '#000000')).toBe('#000000');
  });
});

describe('readableColor repairs only what fails', () => {
  it('returns a passing colour untouched', () => {
    expect(readableColor('#FFFFFF', ['#000000'], 4.5)).toBe('#FFFFFF');
    expect(readableColor('#4A9FE0', ['#11263B'], CONTROL_MIN)).toBe('#4A9FE0');
  });

  it('shades a failing colour until it passes, and no further', () => {
    const repaired = readableColor('#24505E', ['#0C2029', '#06141B'], CONTROL_MIN, '#E9FBF6');
    expect(contrastRatio(repaired, '#0C2029')).toBeGreaterThanOrEqual(CONTROL_MIN);
    expect(contrastRatio(repaired, '#06141B')).toBeGreaterThanOrEqual(CONTROL_MIN);
    // The smallest sufficient correction stays recognisably the same hue.
    expect(repaired).not.toBe('#E9FBF6');
    expect(repaired).not.toBe('#24505E');
  });

  it('picks white or black when no direction is given', () => {
    expect(readableColor('#777777', ['#FFFFFF'], 4.5)).not.toBe('#777777');
    expect(contrastRatio(readableColor('#777777', ['#000000'], 4.5), '#000000')).toBeGreaterThanOrEqual(4.5);
  });

  it('leaves an unparseable colour alone', () => {
    expect(readableColor('rebeccapurple', ['#000000'], 4.5)).toBe('rebeccapurple');
    expect(readableColor('#24505E', [], 4.5)).toBe('#24505E');
  });
});

describe('every palette clears WCAG AA', () => {
  it('has a named palette for each of the five', () => {
    expect(SHIPPED).toHaveLength(5);
    expect(SHIPPED.map((entry) => entry.name)).toEqual([...PALETTE_NAMES]);
  });

  it('covers both a light and a dark palette', () => {
    const schemes = new Set(SHIPPED.map((entry) => PALETTES[entry.name].scheme));
    expect(schemes).toEqual(new Set(['light', 'dark']));
  });

  it.each(PALETTE_NAMES)('reports no failure in %s', (name) => {
    const failures = contrastReport(themeVars(name)).filter((finding) => !finding.ok);
    // Name the culprits so a failure is actionable without opening a browser.
    const summary = failures
      .map(
        (finding) =>
          `${finding.variable} ${finding.color} is ${finding.ratio.toFixed(2)}:1, needs ${finding.minimum}:1`,
      )
      .join('; ');
    expect(failures.map((finding) => finding.variable)).toEqual([]);
    expect(summary).toBe('');
  });

  it.each(PALETTE_NAMES)('holds body text at 4.5:1 in %s', (name) => {
    const vars = themeVars(name);
    for (const variable of ['--tp-text', '--tp-text-muted', '--tp-text-strong']) {
      for (const ground of ['--tp-window-background', '--tp-surface', '--tp-terminal-background']) {
        expect(contrastRatio(vars[variable] as string, vars[ground] as string)).toBeGreaterThanOrEqual(
          BODY_TEXT_MIN,
        );
      }
    }
  });

  it.each(PALETTE_NAMES)('holds controls at 3:1 in %s', (name) => {
    const vars = themeVars(name);
    for (const ground of ['--tp-window-background', '--tp-surface', '--tp-terminal-background']) {
      expect(
        contrastRatio(vars['--tp-border-strong'] as string, vars[ground] as string),
      ).toBeGreaterThanOrEqual(CONTROL_MIN);
      expect(contrastRatio(vars['--tp-focus'] as string, vars[ground] as string)).toBeGreaterThanOrEqual(
        CONTROL_MIN,
      );
    }
  });

  it.each(PALETTE_NAMES)('keeps the label on a primary button readable in %s', (name) => {
    const vars = themeVars(name);
    expect(contrastRatio(vars['--tp-primary-text'] as string, vars['--tp-primary'] as string)).toBeGreaterThanOrEqual(
      BODY_TEXT_MIN,
    );
  });

  it.each(PALETTE_NAMES)('keeps selected text readable in %s', (name) => {
    const vars = themeVars(name);
    const hue = vars['--tp-focus'] as string;
    for (const ground of ['--tp-window-background', '--tp-surface', '--tp-terminal-background']) {
      const painted = composite(hue, SELECTION_TINT, vars[ground] as string);
      expect(contrastRatio(vars['--tp-selection-text'] as string, painted)).toBeGreaterThanOrEqual(
        BODY_TEXT_MIN,
      );
    }
  });

  it.each(PALETTE_NAMES)('keeps every status colour readable in %s', (name) => {
    const vars = themeVars(name);
    for (const variable of [
      '--tp-status-success',
      '--tp-status-warning',
      '--tp-status-danger',
      '--tp-status-info',
    ]) {
      for (const ground of ['--tp-window-background', '--tp-surface', '--tp-terminal-background']) {
        expect(contrastRatio(vars[variable] as string, vars[ground] as string)).toBeGreaterThanOrEqual(
          BODY_TEXT_MIN,
        );
      }
    }
  });

  it('keeps the design identity: a passing role is the design-system value', () => {
    // The repair must not repaint palettes that already pass.
    expect(themeVars('biolume')['--tp-primary']).toBe(PALETTES.biolume.roles.primary);
    expect(themeVars('biolume')['--tp-text']).toBe(PALETTES.biolume.roles.text);
    expect(themeVars('sea-glass')['--tp-text']).toBe(PALETTES['sea-glass'].roles.text);
    expect(themeVars('phosphor')['--tp-focus']).toBe(PRODUCT_HUE.dark);
  });

  it('repairs the control edge the design tokens do not make legible', () => {
    // Regression guard for the failure this task fixed. If the design system
    // ever raises border-strong past 3:1, the token value comes through again;
    // what must never return is a shipped value below the floor.
    for (const name of PALETTE_NAMES) {
      const raw = PALETTES[name].roles.borderStrong;
      const shipped = themeVars(name)['--tp-border-strong'] as string;
      expect(contrastRatio(shipped, themeVars(name)['--tp-surface'] as string)).toBeGreaterThanOrEqual(
        CONTROL_MIN,
      );
      if (!meetsContrast(raw, [themeVars(name)['--tp-surface'] as string], CONTROL_MIN)) {
        expect(shipped).not.toBe(raw);
      }
    }
  });
});

describe('contrastReport', () => {
  it('names every checked property and its floor', () => {
    const findings = contrastReport(themeVars('biolume'));
    const variables = findings.map((finding) => finding.variable);
    const minimum = Object.fromEntries(findings.map((finding) => [finding.variable, finding.minimum]));
    expect(variables).toContain('--tp-text');
    expect(variables).toContain('--tp-border-strong');
    expect(variables).toContain('--tp-focus');
    expect(variables).toContain('--tp-selection-text');
    expect(minimum['--tp-text']).toBe(BODY_TEXT_MIN);
    expect(minimum['--tp-border-strong']).toBe(CONTROL_MIN);
    expect(minimum['--tp-focus']).toBe(CONTROL_MIN);
  });

  it('flags a colour below its floor instead of passing it silently', () => {
    const vars = { ...themeVars('biolume'), '--tp-border-strong': '#24505E' };
    const finding = contrastReport(vars).find((entry) => entry.variable === '--tp-border-strong');
    expect(finding?.ok).toBe(false);
    expect(finding?.ratio).toBeLessThan(CONTROL_MIN);
  });

  it('flags body text that falls below 4.5:1', () => {
    const vars = { ...themeVars('sea-glass'), '--tp-text-muted': '#BBBBBB' };
    const finding = contrastReport(vars).find((entry) => entry.variable === '--tp-text-muted');
    expect(finding?.ok).toBe(false);
  });

  it('flags a selection tint that leaves selected text unreadable', () => {
    const vars = { ...themeVars('sea-glass'), '--tp-selection-text': '#9AA0A0' };
    const finding = contrastReport(vars).find((entry) => entry.variable === '--tp-selection-text');
    expect(finding?.ok).toBe(false);
  });
});

describe('app.css paints only audited colours', () => {
  /** The stylesheet, read once. */
  const stylesheet = readFileSync(resolve(process.cwd(), 'src', 'app.css'), 'utf8');

  it('spells every colour as a token, apart from the pre-mount fallbacks', () => {
    // One literal hex per fallback, and no others: any new literal is a colour
    // that escaped the palette audit.
    const literals = stylesheet.match(/#[0-9A-Fa-f]{3,8}\b/g) ?? [];
    expect(literals).toEqual(['#06141b', '#bfd6d6']);
  });

  it('keeps the fallback pair legible before a palette is applied', () => {
    // These paint the first frame, before `themeStyle` sets the variables.
    expect(contrastRatio('#06141b', '#bfd6d6')).toBeGreaterThanOrEqual(BODY_TEXT_MIN);
  });

  it('draws a control border and a focus ring from tokens', () => {
    // The two rules that make an interactive edge visible must come from the
    // repaired metrics, so they cannot drift back to an unreadable literal.
    expect(stylesheet).toContain('border: 1px solid var(--tp-border-strong)');
    expect(stylesheet).toContain('outline: 2px solid var(--tp-focus)');
  });
});
