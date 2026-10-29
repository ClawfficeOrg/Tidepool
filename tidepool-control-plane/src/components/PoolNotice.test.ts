import { describe, expect, it } from 'vitest';
import PoolNotice from './PoolNotice.svelte';
import { NO_POOL_MESSAGE } from '$lib/api/pool';
import { renderComponent } from '$lib/testing/render';

describe('PoolNotice', () => {
  it('states that no pool is configured, in words', () => {
    const { html, text } = renderComponent(PoolNotice, { config: { baseUrl: null } });
    expect(text).toContain(NO_POOL_MESSAGE);
    expect(text).toContain('No pool is configured yet');
    expect(html).toContain('data-configured="false"');
    expect(html).toContain('role="status"');
    expect(text).toContain('Warning');
    expect(text).toContain('Set the pool URL');
  });

  it('names the pool once one is configured', () => {
    const { html, text } = renderComponent(PoolNotice, {
      config: { baseUrl: 'https://pool.example.com' },
    });
    expect(html).toContain('data-configured="true"');
    expect(text).toContain('https://pool.example.com');
    expect(text).not.toContain(NO_POOL_MESSAGE);
    expect(text).toContain('Info');
  });

  it('treats an unusable URL as unconfigured', () => {
    const { html } = renderComponent(PoolNotice, { config: { baseUrl: 'not a url' } });
    expect(html).toContain('data-configured="false"');
  });
});
