import { describe, expect, it } from 'vitest';
import StatusBadge from './StatusBadge.svelte';
import { STATUS_DESCRIPTORS, STATUS_KINDS } from '$lib/status';
import { renderComponent } from '$lib/testing/render';

describe('StatusBadge', () => {
  it('renders a word beside the icon for every status', () => {
    for (const kind of STATUS_KINDS) {
      const { html, text } = renderComponent(StatusBadge, { kind });
      const expected = STATUS_DESCRIPTORS[kind];
      expect(html).toContain(`data-status="${kind}"`);
      expect(text).toContain(expected.word);
      expect(html).toContain(`title="${expected.description}"`);
    }
  });

  it('marks the icon decorative so the word is what is announced', () => {
    const { html } = renderComponent(StatusBadge, { kind: 'danger' });
    expect(html).toMatch(/<svg[^>]*aria-hidden="true"/);
    expect(html).toContain('role="presentation"');
    expect(html).not.toMatch(/<svg[^>]*aria-label/);
  });

  it('draws the status icon with a path, not a colour block', () => {
    const { html } = renderComponent(StatusBadge, { kind: 'warning' });
    expect(html).toContain('<path');
    expect(html).toContain(`d="${STATUS_DESCRIPTORS.warning.icon}"`);
    expect(html).toContain('stroke="currentColor"');
  });

  it('falls back to Unknown for a status it cannot read', () => {
    const { text, html } = renderComponent(StatusBadge, { kind: 'gremlin' });
    expect(text).toContain('Unknown');
    expect(html).toContain('data-status="unknown"');
  });
});
