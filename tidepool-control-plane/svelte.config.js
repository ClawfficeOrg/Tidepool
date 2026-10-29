import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** Svelte configuration: TypeScript in components, no other preprocessing. */
export default {
  preprocess: vitePreprocess(),
};
