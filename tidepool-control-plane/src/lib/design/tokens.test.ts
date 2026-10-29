import { describe, expect, it } from 'vitest';
import {
  CORE,
  DEFAULT_PALETTE,
  PALETTES,
  PALETTE_NAMES,
  PRODUCT_HUE,
  isPaletteName,
  paletteOf,
  productHue,
  schemeOf,
  themeStyle,
  themeVars,
  withAlpha,
} from './tokens';

describe('clawffice-design palettes', () => {
  it('has exactly the five Clawffice palettes', () => {
    expect(PALETTE_NAMES).toEqual(['biolume', 'deep-tide', 'sea-glass', 'phosphor', 'boiled']);
    expect(Object.keys(PALETTES).sort()).toEqual([...PALETTE_NAMES].sort());
  });

  it('defaults to Biolume', () => {
    expect(DEFAULT_PALETTE).toBe('biolume');
    expect(paletteOf(undefined).name).toBe('biolume');
    expect(paletteOf('not-a-palette').name).toBe('biolume');
  });

  it('gives every palette a name, a summary, a scheme and every role', () => {
    for (const name of PALETTE_NAMES) {
      const palette = PALETTES[name];
      expect(palette.name).toBe(name);
      expect(palette.displayName.length).toBeGreaterThan(0);
      expect(palette.summary.length).toBeGreaterThan(0);
      expect(['light', 'dark']).toContain(palette.scheme);
      for (const value of Object.values(palette.roles)) {
        expect(value).toMatch(/^#[0-9A-Fa-f]{6}$/);
      }
      for (const value of Object.values(palette.status)) {
        expect(value).toMatch(/^#[0-9A-Fa-f]{6}$/);
      }
    }
  });

  it('covers light and dark: Sea Glass is the light palette', () => {
    const schemes = new Set(PALETTE_NAMES.map((name) => schemeOf(name)));
    expect(schemes).toEqual(new Set(['light', 'dark']));
    expect(schemeOf('sea-glass')).toBe('light');
  });

  it('carries the palettes clawffice-design defines', () => {
    expect(PALETTES.biolume.roles.primary).toBe('#C6F24E');
    expect(PALETTES['deep-tide'].roles.windowBackground).toBe('#0B1A2A');
    expect(PALETTES['sea-glass'].roles.windowBackground).toBe('#EEF3F1');
    expect(PALETTES.phosphor.roles.windowBackground).toBe('#0A0C0A');
    expect(PALETTES.boiled.roles.windowBackground).toBe('#1B1218');
  });
});

describe('Tidepool product hue', () => {
  it('uses the tide hue for each scheme', () => {
    expect(PRODUCT_HUE.dark).toBe('#4A9FE0');
    expect(PRODUCT_HUE.light).toBe('#1864AB');
    expect(productHue('dark')).toBe('#4A9FE0');
    expect(productHue('light')).toBe('#1864AB');
  });

  it('drives focus and selection for every palette, not the palette primary', () => {
    for (const name of PALETTE_NAMES) {
      const vars = themeVars(name);
      const scheme = schemeOf(name);
      expect(vars['--tp-focus']).toBe(productHue(scheme));
      expect(vars['--tp-selection']).toBe(withAlpha(productHue(scheme), 0.35));
      expect(vars['--tp-selection-text']).toBe(PALETTES[name].roles.textStrong);
    }
  });

  it('tints selection with the hue and a translucent alpha', () => {
    expect(withAlpha('#4A9FE0', 0.35)).toBe('rgba(74, 159, 224, 0.35)');
    expect(withAlpha('#1864AB', 0.5)).toBe('rgba(24, 100, 171, 0.5)');
  });
});

describe('theme variables', () => {
  it('exposes the palette roles as CSS custom properties', () => {
    const vars = themeVars('biolume');
    expect(vars['--tp-window-background']).toBe('#06141B');
    expect(vars['--tp-surface']).toBe('#0C2029');
    expect(vars['--tp-text']).toBe('#BFD6D6');
    expect(vars['--tp-status-success']).toBe('#4CC38A');
    expect(vars['--tp-color-scheme']).toBe('dark');
  });

  it('carries the core scale so components never invent a value', () => {
    const vars = themeVars('sea-glass');
    expect(vars['--tp-radius-panel']).toBe(`${CORE.radius.panel}px`);
    expect(vars['--tp-font-body']).toBe(`${CORE.fontSize.body}px`);
    expect(vars['--tp-space-1']).toBe(`${CORE.space[0]}px`);
    expect(vars['--tp-cell-grid']).toBe(String(CORE.cellGrid));
  });

  it('serialises to a style attribute with one declaration per variable', () => {
    const style = themeStyle('phosphor');
    const declarations = style.split('; ');
    expect(declarations.length).toBeGreaterThan(20);
    expect(style).toContain('--tp-focus: #4A9FE0');
    expect(style).not.toContain('undefined');
  });
});

describe('palette names', () => {
  it('accepts only known palettes', () => {
    expect(isPaletteName('biolume')).toBe(true);
    expect(isPaletteName('Biolume')).toBe(false);
    expect(isPaletteName(null)).toBe(false);
    expect(isPaletteName(3)).toBe(false);
  });
});
