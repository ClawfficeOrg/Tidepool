import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

const here = (path: string): string => fileURLToPath(new URL(path, import.meta.url));

/**
 * Vite configuration for the Tidepool control plane.
 *
 * The TypeScript SDK lives in this repository (`sdk/ts`), so it is aliased to
 * its source rather than installed: a checkout can build the app without a
 * package install, and the alias is what `package.json` would resolve to after
 * `npm install` anyway.
 */
export default defineConfig({
  plugins: [svelte()],
  resolve: {
    alias: {
      '@clawffice/tidepool-sdk': here('../sdk/ts/src/browser.js'),
      $lib: here('./src/lib'),
    },
  },
  build: {
    target: 'es2022',
    sourcemap: true,
  },
  test: {
    // Component tests render through `svelte/server`, so no DOM is needed and
    // the suite runs without jsdom or a browser.
    environment: 'node',
    include: ['src/**/*.test.ts'],
    reporters: ['default'],
  },
});
