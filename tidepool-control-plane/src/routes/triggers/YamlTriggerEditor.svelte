<script lang="ts">
  /**
   * The YAML view.
   *
   * It is a text editor over the same document the form shows. While the text
   * parses, the form and the YAML describe one document. When it does not, the
   * error is stated plainly and the form is not allowed to claim the broken
   * text: the last parsed document is what the form still describes.
   */
  interface Props {
    /** The authored YAML text. */
    value: string;
    /** The parse failure, or null. */
    error: string | null;
    /** False for a viewer. */
    disabled?: boolean;
    /** Called with the exact text the author typed. */
    onInput: (text: string) => void;
  }

  let { value, error, disabled = false, onInput }: Props = $props();

  function textOf(event: Event): string {
    return (event.currentTarget as HTMLTextAreaElement).value;
  }
</script>

<section class="tp-yaml" data-yaml-view="true" aria-labelledby="tp-yaml-title">
  <h3 class="tp-yaml__title" id="tp-yaml-title">YAML</h3>
  <p class="tp-yaml__hint">
    The same trigger document as the form, in the pool's field names. Switching
    to the form adopts this text only when it parses; a pattern, channel or id
    is never rewritten.
  </p>
  <textarea
    class="tp-input tp-yaml__editor"
    rows="24"
    spellcheck="false"
    value={value}
    disabled={disabled}
    aria-invalid={error !== null}
    aria-describedby={error ? 'tp-yaml-error' : undefined}
    data-yaml-editor="true"
    oninput={(event) => onInput(textOf(event))}
  ></textarea>
  {#if error}
    <p class="tp-yaml__error" id="tp-yaml-error" role="alert" data-yaml-error="true">
      {error}
    </p>
    <p class="tp-yaml__hint">
      The form still describes the last YAML that parsed. Fix the text above
      before switching views.
    </p>
  {/if}
</section>

<style>
  .tp-yaml {
    display: grid;
    gap: var(--tp-space-1);
  }

  .tp-yaml__title {
    margin: 0;
    font-family: var(--tp-font-heading);
    font-size: var(--tp-font-body);
  }

  .tp-yaml__hint {
    margin: 0;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
  }

  .tp-yaml__editor {
    min-height: 24rem;
    font-family: var(--tp-font-code);
    font-size: var(--tp-font-caption);
    line-height: 1.5;
    resize: vertical;
  }

  .tp-yaml__error {
    margin: 0;
    color: var(--tp-status-danger);
  }
</style>
