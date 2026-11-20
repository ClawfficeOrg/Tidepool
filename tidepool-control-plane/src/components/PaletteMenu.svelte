<script lang="ts">
  /**
   * The palette dropdown in the header, top right.
   *
   * One compact control instead of a block of the page: a native `<select>`,
   * so the keyboard, the screen reader and the platform picker all work
   * without extra state. Every palette is named in words and the current one is
   * the selected option, so the choice is never carried by colour alone; the
   * small swatch beside it is decoration.
   *
   * Picking an option calls `onSelect` at once. The shell repaints from the new
   * palette and the root remembers the choice, so nothing needs a reload.
   */
  import {
    DEFAULT_PALETTE,
    PALETTES,
    PALETTE_NAMES,
    type PaletteName,
  } from '$lib/design/tokens';
  import { paletteChoice } from '$lib/theme';

  interface Props {
    /** The palette currently rendered. */
    palette?: PaletteName;
    /** Called with the chosen palette. */
    onSelect?: (name: PaletteName) => void;
  }

  let { palette = DEFAULT_PALETTE, onSelect }: Props = $props();

  /** Adopt a choice from the select; a value no palette defines is ignored. */
  function choose(event: Event): void {
    const choice = paletteChoice((event.currentTarget as HTMLSelectElement).value);
    if (!choice) return;
    onSelect?.(choice);
  }
</script>

<label class="tp-palette" data-palette-control data-palette-selected={palette}>
  <span class="tp-palette__label">Palette</span>
  <span
    class="tp-palette__swatch"
    aria-hidden="true"
    style={`background:${PALETTES[palette].roles.windowBackground};border-color:${PALETTES[palette].roles.primary}`}
  ></span>
  <select class="tp-palette__select" name="palette" onchange={choose} data-palette-select>
    {#each PALETTE_NAMES as name (name)}
      <option value={name} selected={name === palette} data-palette-option={name}>
        {PALETTES[name].displayName}{name === DEFAULT_PALETTE ? ' (default)' : ''}
      </option>
    {/each}
  </select>
</label>

<style>
  .tp-palette {
    display: inline-flex;
    align-items: center;
    gap: var(--tp-space-1);
    min-width: 0;
    color: var(--tp-text-muted);
    font-size: var(--tp-font-caption);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .tp-palette__swatch {
    width: 10px;
    height: 18px;
    display: block;
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-chip);
  }

  .tp-palette__select {
    max-width: 11rem;
    padding: var(--tp-space-1) var(--tp-space-2);
    border: 1px solid var(--tp-border-strong);
    border-radius: var(--tp-radius-control);
    background: var(--tp-terminal-background);
    color: var(--tp-text-strong);
    font: inherit;
    font-size: var(--tp-font-body);
    letter-spacing: normal;
    text-transform: none;
    cursor: pointer;
  }

  .tp-palette__select:hover {
    border-color: var(--tp-focus);
  }
</style>
