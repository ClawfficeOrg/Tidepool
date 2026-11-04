/**
 * The shared field component, rendered on the server.
 *
 * The point of these tests is the pattern itself, so they render the component
 * with a minimal control and check the things every form then inherits: the
 * label above the control, the control, help under it, the error under that,
 * the required/optional word, and the accessibly-described wiring.
 */

import { createRawSnippet } from 'svelte';
import { describe, expect, it } from 'vitest';
import FormField from './FormField.svelte';
import { renderComponent } from '$lib/testing/render';

/** A plain text input that takes the id and `aria-describedby` it is given. */
function control(): ReturnType<typeof createRawSnippet> {
  return createRawSnippet((id: () => string, described: () => string | undefined) => ({
    render: () =>
      `<input class="tp-field__control" type="text" id="${id()}"${
        described() === undefined ? '' : ` aria-describedby="${described()}"`
      } />`,
  }));
}

/** A group of two checkboxes, for the group variant. */
function groupControls(): ReturnType<typeof createRawSnippet> {
  return createRawSnippet(() => ({
    render: () =>
      '<label><input type="checkbox" name="a" /> A</label><label><input type="checkbox" name="b" /> B</label>',
  }));
}

describe('one field, one order', () => {
  it('puts the label above the control, the help under it and the error last', () => {
    const { html } = renderComponent(FormField, {
      form: 'connection',
      field: 'slug',
      label: 'Slug',
      required: true,
      help: 'Lowercased, with non-alphanumerics collapsed.',
      error: 'Enter a slug.',
      children: control(),
    });

    const label = html.indexOf('tp-field__label');
    const input = html.indexOf('<input');
    const help = html.indexOf('tp-field__help');
    const error = html.indexOf('tp-field__error');

    expect(label).toBeGreaterThan(-1);
    expect(input).toBeGreaterThan(label);
    expect(help).toBeGreaterThan(input);
    expect(error).toBeGreaterThan(help);
  });

  it('names the field and whether it is required for later assertions', () => {
    const required = renderComponent(FormField, {
      form: 'invite',
      field: 'email',
      label: 'Email',
      required: true,
      children: control(),
    });
    expect(required.html).toContain('data-field="email"');
    expect(required.html).toContain('data-required="true"');

    const optional = renderComponent(FormField, {
      form: 'invite',
      field: 'note',
      label: 'Note',
      children: control(),
    });
    expect(optional.html).toContain('data-required="false"');
  });
});

describe('required and optional are both said', () => {
  it('marks a required field with the one required word', () => {
    const { html } = renderComponent(FormField, {
      form: 'signin',
      field: 'email',
      label: 'Email',
      required: true,
      children: control(),
    });
    expect(html).toContain('tp-field__requirement');
    expect(html).toContain('Required');
    expect(html).not.toContain('Optional');
  });

  it('marks an optional field with the one optional word', () => {
    const { html } = renderComponent(FormField, {
      form: 'connection',
      field: 'channel',
      label: 'Channel',
      children: control(),
    });
    expect(html).toContain('Optional');
    expect(html).not.toContain('Required');
  });
});

describe('the label and the control are wired together', () => {
  it('points the label at the control id', () => {
    const { html } = renderComponent(FormField, {
      form: 'setup',
      field: 'pool_name',
      label: 'Pool name',
      children: control(),
    });
    expect(html).toContain('for="setup-pool-name"');
    expect(html).toContain('id="setup-pool-name"');
  });

  it('uses a fixed id when the markup already links to one', () => {
    const { html } = renderComponent(FormField, {
      form: 'connection',
      field: 'provider',
      label: 'Provider',
      id: 'tp-connection-provider',
      children: control(),
    });
    expect(html).toContain('for="tp-connection-provider"');
    expect(html).toContain('id="tp-connection-provider"');
  });

  it('describes the control by the help and the error that exist', () => {
    const both = renderComponent(FormField, {
      form: 'connection',
      field: 'slug',
      label: 'Slug',
      help: 'The callback path segment.',
      error: 'Enter a slug.',
      children: control(),
    });
    expect(both.html).toContain('id="connection-slug-help"');
    expect(both.html).toContain('id="connection-slug-error"');
    expect(both.html).toContain('aria-describedby="connection-slug-help connection-slug-error"');

    const neither = renderComponent(FormField, {
      form: 'connection',
      field: 'slug',
      label: 'Slug',
      children: control(),
    });
    expect(neither.html).not.toContain('aria-describedby');
    expect(neither.html).not.toContain('tp-field__help');
    expect(neither.html).not.toContain('tp-field__error');
  });

  it('announces the error as an alert', () => {
    const { html, text } = renderComponent(FormField, {
      form: 'invite',
      field: 'email',
      label: 'Email',
      error: 'Enter an email address.',
      children: control(),
    });
    expect(html).toContain('role="alert"');
    expect(text).toContain('Enter an email address.');
  });
});

describe('the group and wide variants', () => {
  it('names a group of controls with a span, not a label', () => {
    const { html } = renderComponent(FormField, {
      form: 'create-key',
      field: 'scopes',
      label: 'Scopes',
      group: true,
      children: groupControls(),
    });
    expect(html).toContain('role="group"');
    expect(html).toContain('aria-labelledby="create-key-scopes-label"');
    expect(html).toContain('id="create-key-scopes-label"');
    expect(html).not.toContain('<label class="tp-field__label"');
    expect(html).toContain('type="checkbox"');
  });

  it('takes the whole width of a multi-column form when asked', () => {
    const { html } = renderComponent(FormField, {
      form: 'connection',
      field: 'credential_ref',
      label: 'Credential reference',
      wide: true,
      children: control(),
    });
    expect(html).toContain('tp-field--wide');
  });

  it('renders extra help markup under the help text', () => {
    const extra = createRawSnippet(() => ({
      render: () => '<p data-extra="true">In effect: https://pool.example.com</p>',
    }));
    const { html } = renderComponent(FormField, {
      form: 'pool_url',
      field: 'pool_url',
      label: 'Pool URL',
      help: 'Leave empty to use the pool that served this page.',
      extra,
      children: control(),
    });
    expect(html.indexOf('tp-field__help')).toBeLessThan(html.indexOf('data-extra="true"'));
  });
});
