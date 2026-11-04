/**
 * The one form pattern.
 *
 * Every field on the control plane is rendered by `FormField.svelte`, which
 * lays the parts out in one order: label above the control, help text under the
 * control, error under that, and each field marked either required or optional
 * in the same words. The class names and the marker words live here so a form
 * cannot invent a fifth spelling of "required": it imports these, and the
 * pattern test fails if a form builds its own label or hint.
 *
 * The module is plain TypeScript so the rules can be asserted without a DOM.
 */

/** The class names that make up one field. */
export const FIELD = {
  /** The field wrapper: a grid, so the parts stack in source order. */
  root: 'tp-field',
  /** A field that takes the whole width of a multi-column form. */
  wide: 'tp-field--wide',
  /** The label, above the control. */
  label: 'tp-field__label',
  /** The required or optional marker beside the label. */
  requirement: 'tp-field__requirement',
  /** The control itself: input, select or textarea. */
  control: 'tp-field__control',
  /** Help text, under the control. */
  help: 'tp-field__help',
  /** The field's error, under the help text. */
  error: 'tp-field__error',
  /** The row of actions at the end of a form. */
  actions: 'tp-field__actions',
} as const;

/** The word a required field carries. */
export const REQUIRED_MARK = 'Required';

/** The word an optional field carries, so "no marker" never means optional. */
export const OPTIONAL_MARK = 'Optional';

/** One validation failure, tied to the field it belongs to. */
export interface FieldError {
  /** The field name the validator named. */
  field: string;
  /** What the person must change, in words. */
  message: string;
}

/** The id of a control, derived from the form and field it belongs to. */
export function fieldId(form: string, field: string): string {
  const clean = (value: string): string =>
    value
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '');
  return [clean(form), clean(field)].filter((part) => part !== '').join('-');
}

/** The id of a field's help text, so a control can point at it. */
export function helpId(controlId: string): string {
  return `${controlId}-help`;
}

/** The id of a field's error, so a control can point at it. */
export function errorId(controlId: string): string {
  return `${controlId}-error`;
}

/** The marker word for a field: required is said out loud, so is optional. */
export function requirementMark(required: boolean): string {
  return required ? REQUIRED_MARK : OPTIONAL_MARK;
}

/** What `aria-describedby` points at, given the help and error present. */
export function describedBy(controlId: string, help: string | null, error: string | null): string | undefined {
  const ids = [help !== null ? helpId(controlId) : null, error !== null ? errorId(controlId) : null].filter(
    (id): id is string => id !== null,
  );
  return ids.length === 0 ? undefined : ids.join(' ');
}

/** The message for one field, or null when the validator did not name it. */
export function errorForField(errors: readonly FieldError[], field: string): string | null {
  return errors.find((entry) => entry.field === field)?.message ?? null;
}

/** Turn one validator failure into the list a form passes to its fields. */
export function errorsFor(failure: { field: string; message: string } | null): FieldError[] {
  return failure === null ? [] : [{ field: failure.field, message: failure.message }];
}
