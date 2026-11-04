import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import Nav from './Nav.svelte';
import App from '../App.svelte';
import { ROUTES, resolveRoute, routeHref } from '$lib/router';
import { renderComponent } from '$lib/testing/render';

describe('Nav', () => {
  it('renders the same list whether it is the sidebar or the menu', () => {
    // One component, one list: the menu cannot fall behind the sidebar.
    const { html } = renderComponent(Nav, { routes: ROUTES, current: '/' });
    expect(html.match(/class="tp-nav__link"/g)).toHaveLength(ROUTES.length);
    for (const route of ROUTES) {
      expect(html).toContain(`href="${routeHref(route.path)}"`);
      expect(html).toContain(`data-route="${route.id}"`);
    }
  });

  it('reaches every page from the menu', () => {
    const { html, text } = renderComponent(Nav, { routes: ROUTES, current: '/hosts' });
    // The panel the button controls...
    const controls = /aria-controls="([^"]+)"/.exec(html)?.[1];
    expect(controls).toBeTruthy();
    expect(html).toContain(`<nav class="tp-nav" id="${controls}"`);
    // ...holds a link for every route, named in words.
    const panel = html.slice(html.indexOf(`id="${controls}"`));
    for (const route of ROUTES) {
      expect(panel).toContain(`href="${routeHref(route.path)}"`);
      expect(text).toContain(route.label);
    }
    expect(panel.match(/class="tp-nav__link"/g)).toHaveLength(ROUTES.length);
  });

  it('offers a menu button that is closed and labelled to open', () => {
    const { html, text } = renderComponent(Nav, { routes: ROUTES, current: '/' });
    expect(html).toContain('data-menu-toggle');
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain('aria-label="Open navigation"');
    expect(html).toContain('type="button"');
    expect(text).toContain('Menu');
  });

  it('keeps the panel closed until the button is pressed', () => {
    const { html } = renderComponent(Nav, { routes: ROUTES, current: '/' });
    expect(html).toContain('data-open="false"');
    expect(html).not.toContain('data-open="true"');
  });

  it('draws the hamburger as decoration, so the button is named once', () => {
    const { html } = renderComponent(Nav, { routes: ROUTES, current: '/' });
    expect(html).toMatch(/<span class="tp-menu-toggle__bars" aria-hidden="true">/);
    const button = /<button[^>]*data-menu-toggle[^>]*>/.exec(html)?.[0] ?? '';
    expect(button.match(/aria-label=/g)).toHaveLength(1);
    // The only other landmark name is the navigation itself.
    expect(html.match(/aria-label=/g)).toHaveLength(2);
    expect(html).toContain('aria-label="Control plane"');
  });

  it('marks the current page and leaves the others unmarked', () => {
    const { html } = renderComponent(Nav, { routes: ROUTES, current: '/triggers' });
    const marked = html.match(/aria-current="page"[^>]*data-route="([^"]+)"/g) ?? [];
    expect(marked).toHaveLength(1);
    expect(html).toMatch(/aria-current="page"[^>]*data-route="triggers"/);
  });

  it('labels the navigation as the control plane', () => {
    const { html } = renderComponent(Nav, { routes: ROUTES, current: '/' });
    expect(html).toContain('aria-label="Control plane"');
  });

  it('wires the button and Escape to the menu, and focus back to the button', () => {
    // The handlers cannot fire in a server render, so the wiring itself is
    // checked at the source: press on the button, keydown on the window, focus
    // on the button when the menu asks for it.
    const source = readFileSync(fileURLToPath(new URL('./Nav.svelte', import.meta.url)), 'utf8');
    expect(source).toContain('onclick={press}');
    expect(source).toContain('<svelte:window onkeydown={onKeydown} />');
    expect(source).toContain('bind:this={toggle}');
    expect(source).toContain('toggle?.focus()');
    expect(source).toContain("dispatch({ type: 'toggle' })");
  });

  it('points every menu link at a page the shell actually renders', () => {
    // The menu is the only way to navigate below tablet width, so every one of
    // its links must resolve to a route and that route must render a page.
    const { html } = renderComponent(Nav, { routes: ROUTES, current: '/' });
    const hrefs = [...html.matchAll(/href="([^"]+)"/g)].map((match) => match[1] ?? '');
    expect(hrefs).toHaveLength(ROUTES.length);
    for (const [index, href] of hrefs.entries()) {
      const route = ROUTES[index];
      expect(route).toBeDefined();
      expect(href).toBe(routeHref(route?.path ?? ''));
      const resolved = resolveRoute(href);
      expect(resolved.found).toBe(true);
      expect(resolved.route?.id).toBe(route?.id);
      const page = renderComponent(App, { path: resolved.path });
      expect(page.text).not.toContain('No such page');
    }
  });
});
