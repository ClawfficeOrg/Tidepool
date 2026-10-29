import { describe, expect, it } from 'vitest';
import { ProblemError } from '@clawffice/tidepool-sdk';
import App from './App.svelte';
import { PALETTES, PALETTE_NAMES, DEFAULT_PALETTE, productHue } from '$lib/design/tokens';
import { presentProblem } from '$lib/api/problem';
import { renderComponent } from '$lib/testing/render';

describe('App shell', () => {
  it('runs with no pool configured and says so', () => {
    const { html, text } = renderComponent(App, {});
    expect(html).toContain('data-pool="unconfigured"');
    expect(text).toContain('No pool is configured yet');
    expect(text).toContain('Set the pool URL');
  });

  it('renders every page in the navigation', () => {
    const { text } = renderComponent(App, {});
    for (const label of ['Overview', 'Hosts', 'Runs', 'Settings']) {
      expect(text).toContain(label);
    }
  });

  it('renders all five palettes and defaults to Biolume', () => {
    const defaultRender = renderComponent(App, {});
    expect(defaultRender.html).toContain('data-palette="biolume"');
    expect(defaultRender.html).toContain('data-scheme="dark"');
    expect(defaultRender.html).toContain('--tp-focus: #4A9FE0');
    expect(defaultRender.text).toContain('Biolume');

    for (const name of PALETTE_NAMES) {
      const { html, text } = renderComponent(App, { palette: name });
      expect(html).toContain(`data-palette="${name}"`);
      expect(html).toContain(`data-scheme="${PALETTES[name].scheme}"`);
      expect(html).toContain(`--tp-focus: ${productHue(PALETTES[name].scheme)}`);
      expect(html).toContain(`--tp-window-background: ${PALETTES[name].roles.windowBackground}`);
      expect(text).toContain(PALETTES[name].displayName);
    }
  });

  it('shows the palette switch on the shell, with the default marked', () => {
    const { html } = renderComponent(App, {});
    expect(html).toContain(`data-palette-option="${DEFAULT_PALETTE}" aria-pressed="true"`);
    expect(html.match(/data-palette-option=/g)).toHaveLength(5);
  });

  it('switches palette from the shell without losing the page', () => {
    const overview = renderComponent(App, { palette: 'biolume', path: '/hosts' });
    const switched = renderComponent(App, { palette: 'phosphor', path: '/hosts' });
    expect(overview.text).toContain('Machines enrolled with this pool');
    expect(switched.text).toContain('Machines enrolled with this pool');
    expect(switched.html).toContain('data-palette="phosphor"');
    expect(switched.html).toContain('--tp-focus: #4A9FE0');
  });

  it('renders light and dark palettes with their own scheme', () => {
    const light = renderComponent(App, { palette: 'sea-glass' });
    const dark = renderComponent(App, { palette: 'biolume' });
    expect(light.html).toContain('data-scheme="light"');
    expect(light.html).toContain('--tp-focus: #1864AB');
    expect(dark.html).toContain('data-scheme="dark"');
    expect(dark.html).toContain('--tp-focus: #4A9FE0');
  });

  it('renders a pool failure in the shell with its request id', () => {
    const problem = presentProblem(
      new ProblemError('request failed', {
        status: 503,
        code: 'database_unavailable',
        title: 'Database unavailable',
        detail: 'The pool has no database connection.',
        requestId: 'req_01HSAMPLE',
      }),
    );
    const { html, text } = renderComponent(App, {
      pool: { baseUrl: 'https://pool.example.com' },
      problem,
    });
    expect(html).toContain('role="alert"');
    expect(text).toContain('Database unavailable');
    expect(text).toContain('The pool has no database connection.');
    expect(text).toContain('Request id req_01HSAMPLE');
    // A failure replaces the page and the no-pool notice, so nothing is hidden behind it.
    expect(text).not.toContain('No pool is configured yet');
    expect(text).not.toContain('This control plane is scaffolded');
  });

  it('routes to each page and reports an unknown path', () => {
    expect(renderComponent(App, { path: '/' }).text).toContain('This control plane is scaffolded');
    expect(renderComponent(App, { path: '/runs' }).text).toContain('What arrived');
    expect(renderComponent(App, { path: '/settings' }).text).toContain('The pool URL, the palette');
    const missing = renderComponent(App, { path: '/nowhere' });
    expect(missing.text).toContain('No such page');
    expect(missing.text).toContain('/nowhere');
  });

  it('marks the current page in the navigation', () => {
    const { html } = renderComponent(App, { path: '/hosts' });
    expect(html).toMatch(/data-route="hosts"[^>]*aria-current="page"|aria-current="page"[^>]*data-route="hosts"/);
    expect(html).toContain('href="#/hosts"');
  });
});
