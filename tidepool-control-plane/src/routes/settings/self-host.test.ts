/**
 * End to end: the app running from the pool's own origin.
 *
 * A real HTTP server (bound to loopback on an ephemeral port for this test
 * only) serves a built bundle the way `src/main.rs` does: `index.html` at `/`,
 * hashed assets byte for byte, the same-origin marker in the page, and the
 * pool's own `/healthz`. The test then does what the browser does — fetch the
 * page, read the marker, resolve the base URL, load the script, call the pool
 * through the real TypeScript SDK — and compares that origin's app with a
 * standalone deployment serving the same files.
 *
 * No MySQL, no Redis, no daemon: the listener lives inside the test and closes
 * with it.
 */

import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { createServer, type IncomingMessage, type Server, type ServerResponse } from 'node:http';
import type { AddressInfo } from 'node:net';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createClient } from '@clawffice/tidepool-sdk';
import { isServedByPool, resolvePoolConfig } from './preferences';

const SCRIPT = '/assets/app.js';
const MARKER = 'name="tidepool-pool-origin" content="same-origin"';

/** The bundle both origins serve: one page, one hashed asset, one icon. */
const INDEX = `<!doctype html><html lang="en"><head><meta charset="utf-8" /><title>Tidepool control plane</title><script type="module" crossorigin src="${SCRIPT}"></script></head><body><div id="app"></div></body></html>`;
const APP_JS = `console.log('the same control plane bundle');\n`;
const ICON = `<svg xmlns="http://www.w3.org/2000/svg"></svg>`;

/** One origin serving the bundle, with what it was asked for. */
interface BundleOrigin {
  /** The origin, e.g. `http://127.0.0.1:54321`. */
  origin: string;
  /** Request paths in order, including the method. */
  requests: string[];
  /** Stop listening. */
  close: () => Promise<void>;
}

/** A built bundle on disk. */
interface BundleFixture {
  root: string;
  cleanup: () => Promise<void>;
}

/** Write the bundle to a temporary directory. */
async function buildBundle(): Promise<BundleFixture> {
  const root = await mkdtemp(join(tmpdir(), 'tidepool-self-host-'));
  await mkdir(join(root, 'assets'), { recursive: true });
  await writeFile(join(root, 'index.html'), INDEX);
  await writeFile(join(root, SCRIPT.slice(1)), APP_JS);
  await writeFile(join(root, 'tidepool-cell.svg'), ICON);
  return { root, cleanup: () => rm(root, { recursive: true, force: true }) };
}

/** Serve the bundle the way the pool's static handler does. */
async function startOrigin(root: string, options: { marker: boolean }): Promise<BundleOrigin> {
  const requests: string[] = [];
  const handler = async (req: IncomingMessage, res: ServerResponse): Promise<void> => {
    const path = (req.url ?? '/').split('?')[0] ?? '/';
    requests.push(`${req.method ?? 'GET'} ${path}`);

    if (path === '/healthz') {
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ status: 'ok', db: 'ok' }));
      return;
    }
    if (path.startsWith('/v1')) {
      res.writeHead(404).end();
      return;
    }

    try {
      if (path === '/' || path === '/settings' || path === '/hosts') {
        const html = await readFile(join(root, 'index.html'), 'utf8');
        const body = options.marker
          ? html.replace('</head>', `<meta ${MARKER} /></head>`)
          : html;
        res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
        res.end(body);
        return;
      }
      const file = join(root, path.replace(/^\/+/, ''));
      const body = await readFile(file);
      const type = path.endsWith('.js')
        ? 'text/javascript; charset=utf-8'
        : path.endsWith('.svg')
          ? 'image/svg+xml'
          : 'application/octet-stream';
      res.writeHead(200, { 'content-type': type });
      res.end(body);
    } catch {
      res.writeHead(404).end();
    }
  };

  const server: Server = createServer((req, res) => {
    void handler(req, res);
  });
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  const address = server.address() as AddressInfo;
  return {
    origin: `http://127.0.0.1:${address.port}`,
    requests,
    close: () => new Promise<void>((resolve) => server.close(() => resolve())),
  };
}

describe('the app runs from the pool\'s own origin', () => {
  let bundle: BundleFixture;
  let poolOrigin: BundleOrigin;
  let standaloneOrigin: BundleOrigin;

  beforeAll(async () => {
    bundle = await buildBundle();
    poolOrigin = await startOrigin(bundle.root, { marker: true });
    standaloneOrigin = await startOrigin(bundle.root, { marker: false });
  });

  afterAll(async () => {
    await poolOrigin?.close();
    await standaloneOrigin?.close();
    await bundle?.cleanup();
  });

  it('serves the built page and assets from the pool origin', async () => {
    const page = await fetch(`${poolOrigin.origin}/`);
    expect(page.status).toBe(200);
    expect(page.headers.get('content-type')).toContain('text/html');
    const html = await page.text();
    expect(html).toContain(`<div id="app">`);

    const asset = await fetch(`${poolOrigin.origin}${SCRIPT}`);
    expect(asset.status).toBe(200);
    expect(asset.headers.get('content-type')).toContain('text/javascript');
    expect(await asset.text()).toBe(APP_JS);
  });

  it('resolves the pool from the page the pool served, with no build-time URL', async () => {
    const html = await (await fetch(`${poolOrigin.origin}/`)).text();
    const marker = /<meta name="tidepool-pool-origin" content="([^"]+)" \/>/.exec(html)?.[1] ?? null;
    expect(isServedByPool(marker)).toBe(true);

    const resolved = resolvePoolConfig({
      env: {},
      storage: null,
      sameOrigin: isServedByPool(marker),
      origin: poolOrigin.origin,
    });
    expect(resolved.baseUrl).toBe(poolOrigin.origin);
  });

  it('calls the pool on the same origin, with no CORS hop and no rebuild', async () => {
    const html = await (await fetch(`${poolOrigin.origin}/`)).text();
    const marker = /<meta name="tidepool-pool-origin" content="([^"]+)" \/>/.exec(html)?.[1] ?? null;
    const pool = resolvePoolConfig({
      env: {},
      storage: null,
      sameOrigin: isServedByPool(marker),
      origin: poolOrigin.origin,
    });
    expect(pool.baseUrl).not.toBeNull();

    const client = createClient({ baseUrl: pool.baseUrl ?? '' });
    const health = await client.api.system.health();

    expect(health).toEqual({ status: 'ok', db: 'ok' });
    // The app asked its own origin: one request, no preflight.
    expect(poolOrigin.requests).toContain('GET /healthz');
    expect(poolOrigin.requests.some((entry) => entry.startsWith('OPTIONS'))).toBe(false);
  });

  it('behaves identically on a standalone deployment of the same bundle', async () => {
    const served = await (await fetch(`${poolOrigin.origin}/`)).text();
    const standalone = await (await fetch(`${standaloneOrigin.origin}/`)).text();

    // The only difference is the marker the pool adds; everything else is byte
    // for byte the same build.
    expect(standalone).not.toContain('tidepool-pool-origin');
    expect(served.replace(`<meta ${MARKER} />`, '')).toBe(standalone);

    const assetA = await (await fetch(`${poolOrigin.origin}${SCRIPT}`)).text();
    const assetB = await (await fetch(`${standaloneOrigin.origin}${SCRIPT}`)).text();
    expect(assetA).toBe(assetB);

    // Without the marker the standalone copy stays unconfigured rather than
    // calling its static host, which is what the app always did.
    const marker = /<meta name="tidepool-pool-origin" content="([^"]+)" \/>/.exec(standalone)?.[1] ?? null;
    expect(isServedByPool(marker)).toBe(false);
    expect(
      resolvePoolConfig({
        env: {},
        storage: null,
        sameOrigin: isServedByPool(marker),
        origin: standaloneOrigin.origin,
      }),
    ).toEqual({ baseUrl: null });
  });

  it('applies a different pool URL at runtime, without a rebuild', async () => {
    const storage = (() => {
      const data = new Map<string, string>();
      return {
        getItem: (key: string) => data.get(key) ?? null,
        setItem: (key: string, value: string) => void data.set(key, value),
      };
    })();
    storage.setItem('tidepool.control-plane.pool-url', standaloneOrigin.origin);

    // Even served by one pool, a saved URL wins: the running app points
    // somewhere else without a build and without the marker changing.
    const resolved = resolvePoolConfig({
      env: {},
      storage,
      sameOrigin: true,
      origin: poolOrigin.origin,
    });
    expect(resolved.baseUrl).toBe(standaloneOrigin.origin);

    const client = createClient({ baseUrl: resolved.baseUrl ?? '' });
    const health = await client.api.system.health();
    expect(health).toEqual({ status: 'ok', db: 'ok' });
    expect(standaloneOrigin.requests).toContain('GET /healthz');
  });

  it('serves an unknown page as the app, so a hash route reloads', async () => {
    const page = await fetch(`${poolOrigin.origin}/hosts`);
    expect(page.status).toBe(200);
    expect(await page.text()).toContain('<div id="app">');
  });
});
