/**
 * The form pattern's rules, asserted without a DOM.
 *
 * These are the parts every form must agree on: the class names that make a
 * field, the two marker words, the id and `aria-describedby` wiring, and how a
 * validator's one failure becomes the field's error. A form that imports these
 * cannot invent a fifth spelling of "required".
 */

import { describe, expect, it } from 'vitest';
import {
  FIELD,
  OPTIONAL_MARK,
  REQUIRED_MARK,
  describedBy,
  errorForField,
  errorId,
  errorsFor,
  fieldId,
  helpId,
  requirementMark,
} from './form';

describe('the one field pattern', () => {
  it('names the parts in the one order a field is read', () => {
    expect(FIELD.root).toBe('tp-field');
    expect(FIELD.label).toBe('tp-field__label');
    expect(FIELD.requirement).toBe('tp-field__requirement');
    expect(FIELD.control).toBe('tp-field__control');
    expect(FIELD.help).toBe('tp-field__help');
    expect(FIELD.error).toBe('tp-field__error');
    expect(FIELD.actions).toBe('tp-field__actions');
  });

  it('marks required and optional in words, never by absence', () => {
    expect(requirementMark(true)).toBe('Required');
    expect(requirementMark(false)).toBe('Optional');
    // The two words differ, so "no marker" can never be read as optional.
    expect(REQUIRED_MARK).not.toBe(OPTIONAL_MARK);
  });
});

describe('field ids', () => {
  it('builds a stable id from the form and the field', () => {
    expect(fieldId('attach_host', 'display_name')).toBe('attach-host-display-name');
    expect(fieldId('trigger', 'filters.from_users')).toBe('trigger-filters-from-users');
  });

  it('leaves no leading, trailing or repeated separator', () => {
    for (const [form, field] of [
      ['Sign In', 'email'],
      ['_pool_', '__url__'],
      ['a', 'b'],
    ] as const) {
      const id = fieldId(form, field);
      expect(id.startsWith('-')).toBe(false);
      expect(id.endsWith('-')).toBe(false);
      expect(id.includes('--')).toBe(false);
    }
  });

  it('points the help and the error at the control id', () => {
    expect(helpId('trigger-name')).toBe('trigger-name-help');
    expect(errorId('trigger-name')).toBe('trigger-name-error');
  });

  it('describes a control by the help and error that are present', () => {
    expect(describedBy('f', null, null)).toBeUndefined();
    expect(describedBy('f', 'help', null)).toBe('f-help');
    expect(describedBy('f', null, 'oops')).toBe('f-error');
    expect(describedBy('f', 'help', 'oops')).toBe('f-help f-error');
  });
});

describe('one validator failure, one field', () => {
  it('finds the message the validator tied to a field', () => {
    const errors = errorsFor({ field: 'email', message: 'Enter an email address.' });
    expect(errorForField(errors, 'email')).toBe('Enter an email address.');
    expect(errorForField(errors, 'role')).toBeNull();
  });

  it('has no errors when the validator passed', () => {
    expect(errorsFor(null)).toEqual([]);
    expect(errorForField(errorsFor(null), 'email')).toBeNull();
  });
});
