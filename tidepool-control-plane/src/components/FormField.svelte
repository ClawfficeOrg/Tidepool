<script lang="ts">
  /**
   * One field, laid out the one way.
   *
   * Every form on the control plane uses this component instead of writing its
   * own label, hint and error markup. The order is fixed: label above the
   * control, the control, help text under it, the error under that. The label
   * always says whether the field is required or optional, so "no marker" can
   * never be read as "optional", and the error is an alert tied to the control
   * through `aria-describedby`.
   *
   * The control itself is supplied by the caller as a snippet, because an
   * input, a select and a textarea need different props. The snippet receives
   * the control id, which the caller must put on its element; `describedBy`
   * from `./form` produces the matching `aria-describedby`.
   *
   * The wrapper's rules are global on purpose: the control is compiled in the
   * calling component, so a scoped selector here would not reach it.
   */
  import { describedBy, errorId, fieldId, helpId, requirementMark } from './form';
  import type { Snippet } from 'svelte';

  interface Props {
    /** The form this field belongs to, used to build the control id. */
    form: string;
    /** The field name the validator uses, e.g. `filters.connection`. */
    field: string;
    /** The label shown above the control. */
    label: string;
    /** True for a field the form cannot be submitted without. */
    required?: boolean;
    /** Help text, shown under the control. */
    help?: string | null;
    /** Extra help markup, shown under the help text in the same block. */
    extra?: Snippet | null;
    /** The error for this field, shown under the help text. */
    error?: string | null;
    /** Take the whole width of a multi-column form. */
    wide?: boolean;
    /** A fixed control id, for markup that already links to it. */
    id?: string;
    /**
     * A group of controls rather than one control: checkboxes or radios that
     * share a label. The label names the group instead of pointing at one
     * element, and the wrapper is a group for assistive technology.
     */
    group?: boolean;
    /** The control, given the id it must carry and its `aria-describedby`. */
    children: Snippet<[string, string | undefined]>;
  }

  let {
    form,
    field,
    label,
    required = false,
    help = null,
    error = null,
    wide = false,
    id,
    group = false,
    extra = null,
    children,
  }: Props = $props();

  const controlId = $derived(id ?? fieldId(form, field));
  const labelId = $derived(group ? `${controlId}-label` : undefined);
  const described = $derived(describedBy(controlId, help, error));
</script>

{#if group}
  <div
    class="tp-field"
    class:tp-field--wide={wide}
    data-field={field}
    data-required={required ? 'true' : 'false'}
    role="group"
    aria-labelledby={labelId}
  >
    <span class="tp-field__label" id={labelId}>
      <span class="tp-field__name">{label}</span>
      <span class="tp-field__requirement">{requirementMark(required)}</span>
    </span>
    {@render children(controlId, described)}
    {#if help}
      <p class="tp-field__help" id={helpId(controlId)}>{help}</p>
    {/if}
    {#if extra}
      {@render extra()}
    {/if}
    {#if error}
      <p class="tp-field__error" id={errorId(controlId)} role="alert">{error}</p>
    {/if}
  </div>
{:else}
  <div
    class="tp-field"
    class:tp-field--wide={wide}
    data-field={field}
    data-required={required ? 'true' : 'false'}
  >
    <label class="tp-field__label" for={controlId}>
      <span class="tp-field__name">{label}</span>
      <span class="tp-field__requirement">{requirementMark(required)}</span>
    </label>
    {@render children(controlId, described)}
    {#if help}
      <p class="tp-field__help" id={helpId(controlId)}>{help}</p>
    {/if}
    {#if extra}
      {@render extra()}
    {/if}
    {#if error}
      <p class="tp-field__error" id={errorId(controlId)} role="alert">{error}</p>
    {/if}
  </div>
{/if}

<style>
  :global(.tp-field) {
    display: grid;
    gap: var(--tp-space-1);
    align-content: start;
    min-width: 0;
  }

  :global(.tp-field--wide) {
    grid-column: 1 / -1;
  }

  :global(.tp-field__label) {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--tp-space-1);
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  :global(.tp-field__name) {
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  /* Required and optional are both said, in the same place, in the same size. */
  :global(.tp-field__requirement) {
    padding: 0 var(--tp-space-1);
    border: 1px solid var(--tp-border);
    border-radius: var(--tp-radius-chip);
    color: var(--tp-text-strong);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.02em;
    text-transform: none;
  }

  :global(.tp-field[data-required='true'] .tp-field__requirement) {
    border-color: var(--tp-border-strong);
  }

  :global(.tp-field__control) {
    width: 100%;
    padding: var(--tp-space-1) var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    background: var(--tp-window-background);
    color: var(--tp-text-strong);
    font: inherit;
  }

  :global(.tp-field__control[aria-invalid='true']) {
    border-color: var(--tp-status-danger);
  }

  :global(.tp-field__help) {
    margin: 0;
    max-width: 68ch;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  /* The error sits under the help text, in the same column as the control. */
  :global(.tp-field__error) {
    margin: 0;
    max-width: 68ch;
    padding-left: var(--tp-space-2);
    border-left: 3px solid var(--tp-status-danger);
    color: var(--tp-text-strong);
    font-size: var(--tp-font-caption);
  }

  /* The form's actions: quiet controls first, the primary action last, right. */
  :global(.tp-field__actions) {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    align-items: center;
    gap: var(--tp-space-2);
  }
</style>
