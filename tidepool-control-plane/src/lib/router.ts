/**
 * The control plane's routes.
 *
 * A tiny hash router rather than a routing library: the app has one shell and a
 * handful of top-level pages, the table is data a test can read, and a hash
 * keeps a built app servable from any static origin (and from the pool itself)
 * without server rewrite rules.
 */

/** One top-level page. */
export interface RouteDefinition {
  /** Stable id, used to pick the page component. */
  id: 'overview' | 'hosts' | 'connections' | 'runs' | 'settings';
  /** Absolute in-app path. */
  path: string;
  /** Navigation label. */
  label: string;
  /** One-line purpose, shown under the label in the shell. */
  description: string;
}

/** The route table, in navigation order. */
export const ROUTES: readonly RouteDefinition[] = [
  {
    id: 'overview',
    path: '/',
    label: 'Overview',
    description: 'Pool health and what needs attention',
  },
  {
    id: 'hosts',
    path: '/hosts',
    label: 'Hosts',
    description: 'Machines enrolled with this pool',
  },
  {
    id: 'connections',
    path: '/connections',
    label: 'Connections',
    description: 'Provider accounts this pool receives events from',
  },
  {
    id: 'runs',
    path: '/runs',
    label: 'Runs',
    description: 'What arrived, what ran, and what it produced',
  },
  {
    id: 'settings',
    path: '/settings',
    label: 'Settings',
    description: 'Palette, pool URL and preferences',
  },
];

/** The result of resolving a location. */
export type ResolvedRoute =
  | { found: true; route: RouteDefinition; path: string }
  | { found: false; route: null; path: string };

/** Strip a hash, a query string and any trailing slash, leaving an app path. */
export function normalisePath(href: string): string {
  let path = href.trim();
  const hash = path.indexOf('#');
  if (hash !== -1) path = path.slice(hash + 1);
  path = path.split('?')[0] ?? '';
  if (!path.startsWith('/')) path = `/${path}`;
  if (path.length > 1 && path.endsWith('/')) path = path.slice(0, -1);
  return path === '' ? '/' : path;
}

/** Resolve a location to a route, or to the not-found state. */
export function resolveRoute(href: string): ResolvedRoute {
  const path = normalisePath(href);
  const route = ROUTES.find((candidate) => candidate.path === path);
  return route ? { found: true, route, path } : { found: false, route: null, path };
}

/** The `href` for a route id, usable in an anchor. */
export function routeHref(path: string): string {
  return `#${normalisePath(path)}`;
}
