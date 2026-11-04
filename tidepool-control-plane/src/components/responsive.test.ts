/**
 * The responsive shell, asserted against the stylesheet itself.
 *
 * Layout is measured by a browser, so these tests check the rules that make
 * "no sideways scroll at phone width" true rather than a pixel count: a 16px
 * side gutter below tablet width, one column, no fixed track wider than the
 * viewport, wrapping for long words, and images that shrink. They also check
 * that the menu button is hidden only where the sidebar is not.
 */

import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { renderComponent } from '$lib/testing/render';
import App from '../App.svelte';

const css = readFileSync(fileURLToPath(new URL('../app.css', import.meta.url)), 'utf8');

/** Every component in `src`, so a page rule cannot escape the shell's checks. */
function svelteSources(): string[] {
  const root = fileURLToPath(new URL('../', import.meta.url));
  return readdirSync(root, { recursive: true, encoding: 'utf8' })
    .filter((entry) => entry.endsWith('.svelte'))
    .map((entry) => join(root, entry));
}

/** The body of the first block whose selector list matches. */
function ruleBody(selector: string): string | null {
  const pattern = new RegExp(`${selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\{([^}]*)\\}`);
  return pattern.exec(css)?.[1] ?? null;
}

/** Every `@media (max-width: ...)` breakpoint in the stylesheet, in rem. */
function narrowBreakpoints(): number[] {
  const found = new Set<number>();
  for (const match of css.matchAll(/@media \(max-width: ([\d.]+)rem\)/g)) {
    found.add(Number(match[1]));
  }
  return [...found].sort((a, b) => a - b);
}

/** Every `@media (max-width: <rem>)` block body in the stylesheet. */
function mediaBlocks(rem: number): string[] {
  const header = `@media (max-width: ${rem}rem)`;
  const blocks: string[] = [];
  let from = 0;
  for (;;) {
    const start = css.indexOf(header, from);
    if (start === -1) return blocks;
    const open = css.indexOf('{', start);
    if (open === -1) return blocks;
    let depth = 0;
    let end = css.length;
    for (let index = open; index < css.length; index += 1) {
      if (css[index] === '{') depth += 1;
      else if (css[index] === '}') {
        depth -= 1;
        if (depth === 0) {
          end = index;
          break;
        }
      }
    }
    blocks.push(css.slice(open + 1, end));
    from = end + 1;
  }
}

/** The block that mentions `needle`, or the empty string. */
function mediaBlockWith(rem: number, needle: string): string {
  return mediaBlocks(rem).find((block) => block.includes(needle)) ?? '';
}

describe('responsive shell', () => {
  it('collapses the shell to one column below tablet width', () => {
    // 52rem is 832px: wider than a tablet in portrait, narrower than a desk.
    expect(narrowBreakpoints()).toContain(52);
    const body = ruleBody('.tp-shell');
    expect(body).toContain('grid-template-columns: minmax(14rem, 16rem) minmax(0, 1fr)');
  });

  it('uses a 16px side gutter below tablet width', () => {
    const media = mediaBlockWith(52, '.tp-shell') || mediaBlocks(52)[0] || '';
    const shell = /\.tp-shell \{([^}]*)\}/.exec(media)?.[1] ?? '';
    expect(shell).toContain('grid-template-columns: minmax(0, 1fr)');
    // --tp-space-4 is the 16px step of the clawffice space scale.
    expect(shell).toContain('padding: var(--tp-space-4)');
  });

  it('never lets the page scroll sideways', () => {
    const app = ruleBody('.tp-app');
    expect(app).toContain('overflow-x: clip');
    // Long words, URLs and ids wrap instead of pushing the box wider.
    expect(ruleBody('.tp-main')).toContain('overflow-wrap: break-word');
    expect(ruleBody('.tp-main img')).toContain('max-width: 100%');
    // A grid or flex child may shrink below its content's minimum.
    expect(ruleBody('.tp-shell')).toContain('min-width: 0');
    expect(ruleBody('.tp-main')).toContain('min-width: 0');
    expect(ruleBody('.tp-sidebar')).toContain('min-width: 0');
  });

  it('has no fixed track or width wider than a phone viewport', () => {
    // 375px is 23.4375rem. A fixed track or an unshrinkable minimum wider than
    // that is exactly what causes a sideways scroll on a phone.
    const files = [css, ...svelteSources().map((path) => readFileSync(path, 'utf8'))];
    const tooWide = (value: string, unit: string): boolean => {
      const size = Number(value) / (unit === 'rem' ? 1 : 16);
      return size > 23.4375;
    };
    for (const source of files) {
      // `min-width` and plain `width`; `max-width` may exceed the viewport.
      for (const match of source.matchAll(/(?:^|[\s;{])(min-width|width):\s*([\d.]+)(rem|px)/g)) {
        expect(tooWide(match[2] ?? '0', match[3] ?? 'px')).toBe(false);
      }
      // A fixed grid track cannot shrink either. `minmax()` tracks can.
      for (const match of source.matchAll(/grid-template-columns:([^;]+);/g)) {
        const fixed = (match[1] ?? '').replace(/minmax\([^)]*\)/g, ' ');
        for (const track of fixed.matchAll(/([\d.]+)(rem|px)/g)) {
          expect(tooWide(track[1] ?? '0', track[2] ?? 'px')).toBe(false);
        }
      }
    }
  });

  it('shows the menu button only where the sidebar is gone', () => {
    expect(ruleBody('.tp-menu-toggle')).toContain('display: none');
    const media = mediaBlockWith(52, '.tp-menu-toggle');
    expect(media).not.toBe('');
    expect(media).toMatch(/\.tp-menu-toggle \{\s*display: inline-flex;/);
    // Closed panel is not rendered, so it cannot be tabbed into or read.
    expect(media).toContain(".tp-nav[data-open='false']");
    expect(media).toContain('display: none');
  });

  it('renders the menu button in the shell', () => {
    const { html } = renderComponent(App, { path: '/hosts' });
    expect(html).toContain('data-menu-toggle');
    expect(html).toContain('aria-expanded="false"');
  });
});
