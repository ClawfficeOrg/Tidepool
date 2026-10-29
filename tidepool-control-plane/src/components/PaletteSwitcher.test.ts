import { describe, expect, it } from 'vitest';
import PaletteSwitcher from './PaletteSwitcher.svelte';
import { PALETTES, PALETTE_NAMES, DEFAULT_PALETTE } from '$lib/design/tokens';
import { renderComponent } from '$lib/testing/render';

describe('PaletteSwitcher', () => {
  it('offers all five Clawffice palettes by name', () => {
    const { html, text } = renderComponent(PaletteSwitcher, {});
    for (const name of PALETTE_NAMES) {
      expect(html).toContain(`data-palette-option="${name}"`);
      expect(text).toContain(PALETTES[name].displayName);
    }
    expect(html.match(/data-palette-option=/g)).toHaveLength(PALETTE_NAMES.length);
  });

  it('marks Biolume as the default and as the current palette', () => {
    const { html, text } = renderComponent(PaletteSwitcher, {});
    expect(text).toContain('default');
    expect(html).toContain(`data-palette-option="${DEFAULT_PALETTE}" aria-pressed="true"`);
  });

  it('marks exactly the chosen palette as pressed', () => {
    for (const name of PALETTE_NAMES) {
      const { html } = renderComponent(PaletteSwitcher, { palette: name });
      const pressed = [...html.matchAll(/data-palette-option="([^"]+)" aria-pressed="true"/g)].map(
        (match) => match[1],
      );
      expect(pressed).toEqual([name]);
    }
  });

  it('switches the rendered palette when the state changes', () => {
    const before = renderComponent(PaletteSwitcher, { palette: 'biolume' });
    const after = renderComponent(PaletteSwitcher, { palette: 'sea-glass' });
    expect(before.html).toContain('data-palette-option="biolume" aria-pressed="true"');
    expect(after.html).toContain('data-palette-option="sea-glass" aria-pressed="true"');
    expect(after.html).toContain('data-palette-option="biolume" aria-pressed="false"');
  });

  it('previews each palette as decoration only, with the scheme in words', () => {
    for (const name of PALETTE_NAMES) {
      const { html, text } = renderComponent(PaletteSwitcher, { palette: name });
      expect(html).toContain('aria-hidden="true"');
      expect(text).toContain(PALETTES[name].scheme);
      expect(html).toContain(`background:${PALETTES[name].roles.primary}`);
    }
  });
});
