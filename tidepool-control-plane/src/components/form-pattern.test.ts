/**
 * The form pattern, checked across every component.
 *
 * One shared field component is only worth something if no page spells its own
 * label, hint or input. These tests read the sources and fail on the spellings
 * the migration removed: a local field wrapper, a local uppercase label, a
 * local hint, a control class that is not the shared one, and a `<form>` with
 * no actions row. They are deliberately structural, so a new page inherits the
 * check without anyone remembering to write one.
 */

import { readFileSync, readdirSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const SRC = fileURLToPath(new URL('..', import.meta.url));
const COMPONENTS = join(SRC, 'components');

/** Every `.svelte` source under `src`, with its path relative to `src`. */
function svelteSources(): { path: string; source: string }[] {
  return readdirSync(SRC, { recursive: true, encoding: 'utf8' })
    .filter((entry) => entry.endsWith('.svelte'))
    .map((entry) => {
      const path = join(SRC, entry);
      return { path: relative(SRC, path).split(sep).join('/'), source: readFileSync(path, 'utf8') };
    });
}

/** The sources that are allowed to define the pattern. */
const PATTERN_FILES = new Set(['components/FormField.svelte', 'components/form.ts']);

describe('no component spells its own field', () => {
  it('has exactly one definition of the field classes', () => {
    const definers = svelteSources()
      .filter((file) => !PATTERN_FILES.has(file.path))
      .filter((file) =>
        // A local rule or a local class name that claims the field vocabulary.
        /\.tp-field\s*\{|\.tp-field__label\s*\{|\.tp-field__input\s*\{|\.tp-field__hint\s*\{/.test(
          file.source,
        ),
      )
      .map((file) => file.path);
    expect(definers).toEqual([]);
  });

  it('has no page-local field, label or hint classes', () => {
    // These were the five spellings the migration removed. A page that
    // reintroduces one is a page that has left the pattern.
    const forbidden = [
      'tp-enrol__field',
      'tp-enrol__hint',
      'tp-enrol__input',
      'tp-secret__label',
      'tp-secret__input',
      'tp-secret__help',
      'tp-secret__actions',
      'tp-prefs__field',
      'tp-prefs__input',
      'tp-prefs__label',
      'tp-member__field',
      'tp-member__label',
      'tp-member__select',
      'tp-connection__input',
      'tp-connection__select',
      'run-field',
      'tp-field__hint',
      'tp-field__input',
      'tp-editor__label',
      'tp-editor__input',
    ];

    const offenders: string[] = [];
    for (const file of svelteSources()) {
      for (const name of forbidden) {
        if (file.source.includes(name)) offenders.push(`${file.path}: ${name}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('marks every field required or optional, never neither', () => {
    // A `FormField` with no `required` gets "Optional", so the marker always
    // appears; this checks the pages actually reach the component rather than
    // writing a bare `<label>` beside a control.
    const offenders = svelteSources()
      .filter((file) => file.path.startsWith('routes/'))
      .filter((file) => /<label[^>]*>\s*<span class="tp-field__label"/.test(file.source))
      .map((file) => file.path);
    expect(offenders).toEqual([]);
  });
});

describe('every form ends in one actions row', () => {
  it('gives each form an actions row holding its primary action last', () => {
    const offenders: string[] = [];
    for (const file of svelteSources()) {
      const openForms = file.source.match(/<form\b[\s\S]*?<\/form>/g) ?? [];
      for (const form of openForms) {
        if (!form.includes('tp-field__actions')) {
          offenders.push(`${file.path}: form with no tp-field__actions`);
          continue;
        }
        // The submit control is the last button in the actions row.
        const actions = form.slice(form.indexOf('tp-field__actions'));
        const buttons = actions.match(/<button\b[\s\S]*?<\/button>/g) ?? [];
        const primary = buttons.filter((button) => button.includes('type="submit"'));
        if (primary.length === 0) {
          offenders.push(`${file.path}: actions row with no submit button`);
          continue;
        }
        // Primary last: nothing but the submit button may follow it.
        const last = buttons[buttons.length - 1] ?? '';
        if (!last.includes('type="submit"')) {
          offenders.push(`${file.path}: the submit button is not the last action`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });

  it('marks each form with a data-form name, so the pattern is greppable', () => {
    const offenders: string[] = [];
    for (const file of svelteSources()) {
      const openForms = file.source.match(/<form\b[\s\S]*?<\/form>/g) ?? [];
      for (const form of openForms) {
        if (!/data-form="[^"]+"/.test(form)) offenders.push(file.path);
      }
    }
    expect([...new Set(offenders)]).toEqual([]);
  });
});

describe('the shared field component carries the whole pattern', () => {
  it('lives beside the module that names its classes', () => {
    const field = readFileSync(join(COMPONENTS, 'FormField.svelte'), 'utf8');
    expect(field).toContain("from './form'");
    expect(field).toContain('describedBy');
    expect(field).toContain('requirementMark');
    expect(field).toContain('role="alert"');
  });

  it('orders the label, the control, the help and the error in one place', () => {
    const field = readFileSync(join(COMPONENTS, 'FormField.svelte'), 'utf8');
    const label = field.indexOf('tp-field__label');
    const children = field.indexOf('{@render children(');
    const help = field.indexOf('tp-field__help');
    const error = field.indexOf('tp-field__error');
    expect(label).toBeGreaterThan(-1);
    expect(children).toBeGreaterThan(label);
    expect(help).toBeGreaterThan(children);
    expect(error).toBeGreaterThan(help);
  });
});
