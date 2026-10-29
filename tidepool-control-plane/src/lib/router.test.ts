import { describe, expect, it } from 'vitest';
import { ROUTES, normalisePath, resolveRoute, routeHref } from './router';

describe('routes', () => {
  it('has the eight top-level pages, with unique ids and paths', () => {
    expect(ROUTES.map((route) => route.id)).toEqual([
      'overview',
      'hosts',
      'connections',
      'projects',
      'triggers',
      'runs',
      'rooms',
      'settings',
    ]);
    expect(new Set(ROUTES.map((route) => route.path)).size).toBe(ROUTES.length);
    for (const route of ROUTES) {
      expect(route.path.startsWith('/')).toBe(true);
      expect(route.label.length).toBeGreaterThan(0);
      expect(route.description.length).toBeGreaterThan(0);
    }
  });

  it('normalises hashes, queries and trailing slashes', () => {
    expect(normalisePath('#/hosts')).toBe('/hosts');
    expect(normalisePath('/hosts/')).toBe('/hosts');
    expect(normalisePath('/hosts?tab=enrolled')).toBe('/hosts');
    expect(normalisePath('hosts')).toBe('/hosts');
    expect(normalisePath('')).toBe('/');
    expect(normalisePath('#')).toBe('/');
    expect(normalisePath('/')).toBe('/');
  });

  it('resolves every route from its own href', () => {
    for (const route of ROUTES) {
      const resolved = resolveRoute(routeHref(route.path));
      expect(resolved.found).toBe(true);
      expect(resolved.route?.id).toBe(route.id);
      expect(resolved.path).toBe(route.path);
    }
  });

  it('reports an unknown path as not found, keeping the path for the message', () => {
    const resolved = resolveRoute('#/nowhere/deep');
    expect(resolved.found).toBe(false);
    expect(resolved.route).toBeNull();
    expect(resolved.path).toBe('/nowhere/deep');
  });

  it('builds hash hrefs that survive a reload without a server rewrite', () => {
    expect(routeHref('hosts')).toBe('#/hosts');
    expect(routeHref('/runs/')).toBe('#/runs');
    expect(routeHref('/')).toBe('#/');
  });
});
