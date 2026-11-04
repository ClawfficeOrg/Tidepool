import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import PaletteMenu from './PaletteMenu.svelte';
import { DEFAULT_PALETTE, PALETTES, PALETTE_NAMES } from '$lib/design/tokens';
import { paletteChoice } from '$lib/theme';
import { renderComponent } from '$lib/testing/render';

/** The markup of the one `<select>` the header holds. */
function selectOf(html: string): string {
  return /<select[^>]*>[\s\S]*?<\/select>/.exec(html)?.[0] ?? '';
}

/** The option values in document order. */
function optionValues(html: string): string[] {
  return [...html.matchAll(/<option[^>]*value="([^"]+)"/g)].map((match) => match[1] ?? '');
}

/** The one option marked selected, or null. */
function selectedValue(html: string): string | null {
  return /<option[^>]*value="([^"]+)"[^>]*selected/.exec(html)?.[1] ?? null;
}

describe('PaletteMenu', () => {
  it('is one compact dropdown, not a block of options', () => {
    const { html } = renderComponent(PaletteMenu, {});
    expect(html.match(/<select/g)).toHaveLength(1);
    expect(html).toContain('data-palette-control');
    expect(html).toContain('data-palette-select');
    // The old switcher's row of pressed buttons is gone.
    expect(html).not.toContain('aria-pressed');
    expect(html).not.toContain('tp-palettes');
    expect(html).toMatch(/<label class="tp-palette [^"]*"[^>]*data-palette-control/);
  });

  it('offers all five Clawffice palettes by name', () => {
    const { html, text } = renderComponent(PaletteMenu, {});
    expect(optionValues(html)).toEqual([...PALETTE_NAMES]);
    for (const name of PALETTE_NAMES) {
      expect(html).toContain(`data-palette-option="${name}"`);
      expect(text).toContain(PALETTES[name].displayName);
    }
  });

  it('names Biolume as the default in words', () => {
    const { text } = renderComponent(PaletteMenu, {});
    const options = renderComponent(PaletteMenu, {}).html;
    expect(selectedValue(options)).toBe(DEFAULT_PALETTE);
    expect(text).toContain(`${PALETTES[DEFAULT_PALETTE].displayName} (default)`);
  });

  it('selects exactly the palette that is rendered', () => {
    for (const name of PALETTE_NAMES) {
      const { html } = renderComponent(PaletteMenu, { palette: name });
      expect(selectedValue(html)).toBe(name);
      expect(html.match(/<option[^>]*selected/g)).toHaveLength(1);
      expect(html).toContain(`data-palette-selected="${name}"`);
    }
  });

  it('previews the current palette as decoration only', () => {
    for (const name of PALETTE_NAMES) {
      const { html } = renderComponent(PaletteMenu, { palette: name });
      expect(html).toContain('aria-hidden="true"');
      expect(html).toContain(`background:${PALETTES[name].roles.windowBackground}`);
      expect(html).toContain(`border-color:${PALETTES[name].roles.primary}`);
    }
  });

  it('labels the control in words, so colour is never the only cue', () => {
    const { text } = renderComponent(PaletteMenu, {});
    expect(text).toContain('Palette');
  });

  it('wires the change to the shell so a choice applies at once', () => {
    // A server render cannot fire a change event, so the wiring is checked at
    // the source: the select's change handler is the function that calls
    // `onSelect`, and the value is validated before it is adopted.
    const source = readFileSync(fileURLToPath(new URL('./PaletteMenu.svelte', import.meta.url)), 'utf8');
    expect(source).toContain('onchange={choose}');
    expect(source).toContain('onSelect?.(choice)');
    expect(source).toContain('paletteChoice(');
  });

  it('leaves no palette panel in the stylesheet', () => {
    const css = readFileSync(fileURLToPath(new URL('../app.css', import.meta.url)), 'utf8');
    expect(css).not.toContain('tp-palettes');
    // The tool group is the header's right-hand edge, so the dropdown sits top right.
    expect(/\.tp-header__tools \{([^}]*)\}/.exec(css)?.[1] ?? '').toContain(
      'justify-content: flex-end',
    );
  });
  it('is the shell header control, and the shell hands the choice to the theme', () => {
    const app = readFileSync(fileURLToPath(new URL('../App.svelte', import.meta.url)), 'utf8');
    // Exactly one palette control, rendered in the header's tool group.
    expect(app.match(/<PaletteMenu/g)).toHaveLength(1);
    const tools = /<div class="tp-header__tools">([\s\S]*?)<\/div>/.exec(app)?.[1] ?? '';
    expect(tools).toContain('<PaletteMenu');
    expect(app).toContain('onSelect={onPaletteSelect}');

    // The root repaints the running page and writes the choice, so no reload
    // is needed to apply it and the next load restores it.
    const root = readFileSync(fileURLToPath(new URL('../Root.svelte', import.meta.url)), 'utf8');
    expect(root).toContain('onPaletteSelect={selectPalette}');
    expect(/function selectPalette[\s\S]*?theme\.setPalette\(name\);/.test(root)).toBe(true);
    expect(/function selectPalette[\s\S]*?remember\(preferences\);/.test(root)).toBe(true);
  });
});

describe('a palette choice read from a control', () => {
  it('accepts every palette the design system defines', () => {
    for (const name of PALETTE_NAMES) {
      expect(paletteChoice(name)).toBe(name);
    }
  });

  it('ignores a value no palette defines', () => {
    expect(paletteChoice('neon')).toBeNull();
    expect(paletteChoice('')).toBeNull();
    expect(paletteChoice(undefined)).toBeNull();
    expect(paletteChoice(7)).toBeNull();
  });
});
