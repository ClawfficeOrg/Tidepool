import { describe, expect, it } from 'vitest';
import {
  NO_POOL_MESSAGE,
  POOL_URL_STORAGE_KEY,
  PoolNotConfiguredError,
  fetchPoolHealth,
  isPoolConfigured,
  normalisePoolUrl,
  poolClient,
  poolConfigFrom,
} from './pool';

/** A fetch that answers one canned response and records what it was asked for. */
function fakeFetch(response: () => Response): typeof fetch & { readonly calls: string[] } {
  const calls: string[] = [];
  const impl = async (input: RequestInfo | URL): Promise<Response> => {
    calls.push(typeof input === 'string' ? input : input instanceof URL ? input.href : input.url);
    return response();
  };
  return Object.assign(impl as typeof fetch, { calls });
}

/** A JSON response. */
function json(body: unknown, status = 200, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json', ...headers },
  });
}

describe('pool URL', () => {
  it('accepts absolute http and https URLs and strips trailing slashes', () => {
    expect(normalisePoolUrl('https://pool.example.com')).toBe('https://pool.example.com');
    expect(normalisePoolUrl('  https://pool.example.com/  ')).toBe('https://pool.example.com');
    expect(normalisePoolUrl('http://localhost:8080///')).toBe('http://localhost:8080');
    expect(normalisePoolUrl('https://example.com/tidepool/')).toBe('https://example.com/tidepool');
  });

  it('treats an empty, relative or non-http value as unconfigured', () => {
    expect(normalisePoolUrl('')).toBeNull();
    expect(normalisePoolUrl('   ')).toBeNull();
    expect(normalisePoolUrl('pool.example.com')).toBeNull();
    expect(normalisePoolUrl('/v1/hosts')).toBeNull();
    expect(normalisePoolUrl('ftp://pool.example.com')).toBeNull();
    expect(normalisePoolUrl(null)).toBeNull();
    expect(normalisePoolUrl(undefined)).toBeNull();
  });

  it('reports the unconfigured state rather than guessing', () => {
    expect(isPoolConfigured({ baseUrl: null })).toBe(false);
    expect(isPoolConfigured({ baseUrl: 'nonsense' })).toBe(false);
    expect(isPoolConfigured({ baseUrl: 'https://pool.example.com' })).toBe(true);
  });

  it('prefers the build-time pool URL over the saved one', () => {
    const storage = { getItem: () => 'https://saved.example.com' };
    expect(poolConfigFrom({ env: { VITE_POOL_URL: 'https://built.example.com' }, storage })).toEqual({
      baseUrl: 'https://built.example.com',
    });
    expect(poolConfigFrom({ env: {}, storage })).toEqual({ baseUrl: 'https://saved.example.com' });
    expect(poolConfigFrom({ env: {}, storage: { getItem: () => 'not a url' } })).toEqual({ baseUrl: null });
    expect(poolConfigFrom({ env: {}, storage: null })).toEqual({ baseUrl: null });
  });

  it('reads the saved URL from its own key', () => {
    const storage = {
      getItem: (key: string) => (key === POOL_URL_STORAGE_KEY ? 'https://pool.example.com' : null),
    };
    expect(poolConfigFrom({ storage })).toEqual({ baseUrl: 'https://pool.example.com' });
  });
});

describe('pool client', () => {
  it('refuses to build a client with no pool, with a code and a readable message', () => {
    let caught: unknown;
    try {
      poolClient({ baseUrl: null });
    } catch (error) {
      caught = error;
    }
    expect(caught).toBeInstanceOf(PoolNotConfiguredError);
    expect(caught).toBeInstanceOf(Error);
    expect((caught as PoolNotConfiguredError).code).toBe('pool_not_configured');
    expect((caught as PoolNotConfiguredError).message).toBe(NO_POOL_MESSAGE);
  });

  it('builds the typed SDK client against the configured origin', () => {
    const client = poolClient({ baseUrl: 'https://pool.example.com/' });
    expect(client.baseUrl).toBe('https://pool.example.com');
    expect(typeof client.api.system.health).toBe('function');
    expect(client.token).toBeNull();
  });
});

describe('pool health', () => {
  it('says the app is unconfigured instead of making a request', async () => {
    const transport = fakeFetch(() => json({}));
    const result = await fetchPoolHealth({ baseUrl: null }, { fetch: transport });
    expect(result.ok).toBe(false);
    if (result.ok) throw new Error('expected a failure');
    expect(result.problem.title).toBe('No pool is configured');
    expect(result.problem.message).toBe(NO_POOL_MESSAGE);
    expect(result.problem.requestId).toBeNull();
    expect(transport.calls).toEqual([]);
  });

  it('asks the pool health route and returns what it said', async () => {
    const transport = fakeFetch(() => json({ status: 'ok', database: 'ok' }));
    const result = await fetchPoolHealth({ baseUrl: 'https://pool.example.com' }, { fetch: transport });
    expect(result.ok).toBe(true);
    if (!result.ok) throw new Error('expected health');
    expect(result.health).toEqual({ status: 'ok', database: 'ok' });
    expect(transport.calls).toEqual(['https://pool.example.com/healthz']);
  });

  it('renders a pool failure with the request id the pool sent', async () => {
    const transport = fakeFetch(() =>
      json(
        {
          type: 'about:blank',
          title: 'Database unavailable',
          status: 503,
          code: 'database_unavailable',
          detail: 'The pool has no database connection.',
        },
        503,
        { 'content-type': 'application/problem+json', 'x-request-id': 'req_01HSAMPLE' },
      ),
    );
    const result = await fetchPoolHealth({ baseUrl: 'https://pool.example.com' }, { fetch: transport });
    expect(result.ok).toBe(false);
    if (result.ok) throw new Error('expected a failure');
    expect(result.problem.title).toBe('Database unavailable');
    expect(result.problem.message).toBe('The pool has no database connection.');
    expect(result.problem.status).toBe(503);
    expect(result.problem.code).toBe('database_unavailable');
    expect(result.problem.requestId).toBe('req_01HSAMPLE');
  });
});
