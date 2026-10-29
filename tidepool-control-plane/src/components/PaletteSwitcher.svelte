<script lang="ts">
  /**
   * The palette switch.
   *
   * All five Clawffice palettes are offered; Biolume is the default and says so.
   * Each option previews the palette's ground, primary and second colour as
   * decoration only: the palette is named in words and the current one is
   * marked with `aria-pressed`, so the control is not a row of unexplained
   * swatches.
   */
  import {
    DEFAULT_PALETTE,
    PALETTE_NAMES,
    PALETTES,
    type PaletteName,
  } from '$lib/design/tokens';

  interface Props {
    /** The palette currently rendered. */
    palette?: PaletteName;
    /** Called with the chosen palette. */
    onSelect?: (name: PaletteName) => void;
  }

  let { palette = DEFAULT_PALETTE, onSelect }: Props = $props();
</script>

<fieldset class="tp-palettes">
  <legend class="tp-palettes__legend">Palette</legend>
  {#each PALETTE_NAMES as name (name)}
    {@const option = PALETTES[name]}
    <button
      type="button"
      class="tp-palettes__option"
      data-palette-option={name}
      aria-pressed={name === palette}
      onclick={() => onSelect?.(name)}
    >
      <span class="tp-palettes__swatches" aria-hidden="true">
        <span style={`background:${option.roles.windowBackground}`}></span>
        <span style={`background:${option.roles.primary}`}></span>
        <span style={`background:${option.roles.second}`}></span>
      </span>
      <span class="tp-palettes__name">{option.displayName}</span>
      <span class="tp-palettes__scheme">{option.scheme}</span>
      {#if name === DEFAULT_PALETTE}
        <span class="tp-palettes__default">default</span>
      {/if}
    </button>
  {/each}
</fieldset>
