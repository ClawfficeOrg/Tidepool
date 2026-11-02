/**
 * Render a Svelte component on the server, for tests.
 *
 * Component tests run in the `node` environment: `svelte/server` renders the
 * component to HTML, which is enough to assert what a person would see, what a
 * screen reader would read, and which tokens the markup carries, without a DOM.
 */

import { render } from 'svelte/server';
import type { Component } from 'svelte';

/** A rendered component. */
export interface RenderedComponent {
  /** The raw HTML. */
  html: string;
  /** The visible text, with tags and comments removed. */
  text: string;
}

/** Render a component to HTML and text. */
export function renderComponent(
  component: Component<never>,
  props: Record<string, unknown> = {},
): RenderedComponent {
  const result = render(component as Component<Record<string, unknown>>, {
    props,
  });
  return { html: result.body, text: textOf(result.body) };
}

/** Strip tags, comments and entities so text assertions read like the page. */
export function textOf(html: string): string {
  return html
    .replace(/<!--[\s\S]*?-->/g, ' ')
    .replace(/<[^>]+>/g, ' ')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/\s+/g, ' ')
    .trim();
}
